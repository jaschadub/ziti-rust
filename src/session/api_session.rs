//! API session implementation
//!
//! Handles controller API sessions for authentication.

use crate::error::{ZitiError, ZitiResult};
use crate::identity::IdentityManager;
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

/// API session for controller authentication
#[derive(Debug, Clone)]
pub struct ApiSession {
    pub id: String,
    pub token: String,
    pub expires_at: SystemTime,
}

impl ApiSession {
    pub fn new(id: String, token: String, expires_at: SystemTime) -> Self {
        Self {
            id,
            token,
            expires_at,
        }
    }

    /// Check if the session is expired or will expire soon
    pub fn is_expired(&self) -> bool {
        self.is_expired_with_buffer(Duration::from_secs(30))
    }

    /// Check if the session is expired with a time buffer
    pub fn is_expired_with_buffer(&self, buffer: Duration) -> bool {
        SystemTime::now() + buffer >= self.expires_at
    }
}

/// Response structure for authentication API call
#[derive(Debug, Deserialize)]
struct AuthResponse {
    data: AuthData,
}

#[derive(Debug, Deserialize)]
struct AuthData {
    id: String,
    token: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
}

/// Authenticate with the Ziti controller using client certificate
///
/// This function creates an HTTP client configured with the identity's client certificate
/// for mutual TLS authentication, then sends an authentication request to the controller's
/// ztAPI endpoint.
///
/// # Arguments
///
/// * `identity_manager` - The identity manager containing client credentials
///
/// # Returns
///
/// * `ZitiResult<ApiSession>` - API session with token and expiration
///
/// # Example
///
/// ```rust,no_run
/// use ziti_sdk::session::authenticate;
/// use ziti_sdk::identity::load_from_file;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let identity = load_from_file("identity.json").await?;
///     let session = authenticate(&identity).await?;
///     println!("Authenticated with session: {}", session.id);
///     Ok(())
/// }
/// ```
pub async fn authenticate(identity_manager: &IdentityManager) -> ZitiResult<ApiSession> {
    // Create reqwest client with TLS configuration
    let client = create_authenticated_client(identity_manager)?;

    // Build authentication endpoint URL
    let auth_url = format!(
        "{}/authenticate",
        identity_manager.zt_api().trim_end_matches('/')
    );

    // Send authentication request
    let response = client
        .post(&auth_url)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| ZitiError::AuthenticationFailed {
            reason: format!("Failed to send authentication request: {}", e),
        })?;

    // Check response status
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ZitiError::AuthenticationFailed {
            reason: format!(
                "Authentication failed with status {}: {}",
                status, error_text
            ),
        });
    }

    // Parse response
    let auth_response: AuthResponse =
        response
            .json()
            .await
            .map_err(|e| ZitiError::AuthenticationFailed {
                reason: format!("Failed to parse authentication response: {}", e),
            })?;

    // Parse expiration timestamp
    let expires_at = parse_ziti_timestamp(&auth_response.data.expires_at)?;

    Ok(ApiSession::new(
        auth_response.data.id,
        auth_response.data.token,
        expires_at,
    ))
}

/// Create an HTTP client configured with identity credentials for mTLS
fn create_authenticated_client(identity_manager: &IdentityManager) -> ZitiResult<Client> {
    let identity = identity_manager.identity();

    // Convert rustls certificate chain to PEM format for reqwest
    let cert_pem = serialize_cert_chain(&identity.certificate_chain)?;
    let key_pem = serialize_private_key(&identity.private_key)?;

    // Combine cert and key into single PEM buffer
    let mut pem_buffer = cert_pem;
    pem_buffer.extend_from_slice(&key_pem);

    // Create TLS identity for reqwest
    let tls_identity = reqwest::Identity::from_pem(&pem_buffer)
        .map_err(|e| ZitiError::ConfigError(format!("Failed to create TLS identity: {}", e)))?;

    // Create reqwest client with TLS configuration
    let client = Client::builder()
        .identity(tls_identity)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| ZitiError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

    Ok(client)
}

/// Parse Ziti timestamp format to SystemTime
fn parse_ziti_timestamp(timestamp: &str) -> ZitiResult<SystemTime> {
    // Parse ISO 8601 timestamp (e.g., "2023-12-07T10:30:00.000Z")
    let offset_dt = OffsetDateTime::parse(
        timestamp,
        &time::format_description::well_known::Iso8601::DEFAULT,
    )
    .map_err(|e| {
        ZitiError::ConfigError(format!("Failed to parse timestamp {}: {}", timestamp, e))
    })?;

    let unix_seconds = offset_dt.unix_timestamp();
    if unix_seconds < 0 {
        return Err(ZitiError::ConfigError(format!(
            "Timestamp {} predates the Unix epoch",
            timestamp
        )));
    }

    let duration_since_epoch = Duration::from_secs(unix_seconds as u64);
    Ok(UNIX_EPOCH + duration_since_epoch)
}

/// Serialize certificate chain to PEM format for reqwest
fn serialize_cert_chain(cert_chain: &[rustls::pki_types::CertificateDer]) -> ZitiResult<Vec<u8>> {
    let mut pem_data = Vec::new();

    for cert in cert_chain {
        pem_data.extend_from_slice(b"-----BEGIN CERTIFICATE-----\n");
        let cert_b64 = base64_encode(cert.as_ref());
        for chunk in cert_b64.as_bytes().chunks(64) {
            pem_data.extend_from_slice(chunk);
            pem_data.push(b'\n');
        }
        pem_data.extend_from_slice(b"-----END CERTIFICATE-----\n");
    }

    Ok(pem_data)
}

/// Serialize private key to PEM format for reqwest
fn serialize_private_key(private_key: &rustls::pki_types::PrivateKeyDer) -> ZitiResult<Vec<u8>> {
    use rustls::pki_types::PrivateKeyDer;

    // The PEM label must match the key's DER encoding, otherwise reqwest
    // cannot parse the identity.
    let label = match private_key {
        PrivateKeyDer::Pkcs1(_) => "RSA PRIVATE KEY",
        PrivateKeyDer::Sec1(_) => "EC PRIVATE KEY",
        _ => "PRIVATE KEY",
    };

    let mut pem_data = Vec::new();

    pem_data.extend_from_slice(format!("-----BEGIN {}-----\n", label).as_bytes());
    let key_b64 = base64_encode(private_key.secret_der());
    for chunk in key_b64.as_bytes().chunks(64) {
        pem_data.extend_from_slice(chunk);
        pem_data.push(b'\n');
    }
    pem_data.extend_from_slice(format!("-----END {}-----\n", label).as_bytes());

    Ok(pem_data)
}

/// Simple base64 encoder implementation
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;
    while i < input.len() {
        let b1 = input[i];
        let b2 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b3 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let bitmap = ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);

        result.push(CHARS[((bitmap >> 18) & 63) as usize] as char);
        result.push(CHARS[((bitmap >> 12) & 63) as usize] as char);
        result.push(if i + 1 < input.len() {
            CHARS[((bitmap >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if i + 2 < input.len() {
            CHARS[(bitmap & 63) as usize] as char
        } else {
            '='
        });

        i += 3;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn test_api_session_creation() {
        let id = "test-session-id".to_string();
        let token = "test-token".to_string();
        let expires_at = SystemTime::now() + Duration::from_secs(3600);

        let session = ApiSession::new(id.clone(), token.clone(), expires_at);

        assert_eq!(session.id, id);
        assert_eq!(session.token, token);
        assert_eq!(session.expires_at, expires_at);
    }

    #[test]
    fn test_api_session_is_expired() {
        // Test non-expired session
        let future_time = SystemTime::now() + Duration::from_secs(3600);
        let session = ApiSession::new(
            "test-id".to_string(),
            "test-token".to_string(),
            future_time,
        );
        assert!(!session.is_expired());

        // Test expired session
        let past_time = SystemTime::now() - Duration::from_secs(3600);
        let expired_session = ApiSession::new(
            "test-id".to_string(),
            "test-token".to_string(),
            past_time,
        );
        assert!(expired_session.is_expired());
    }

    #[test]
    fn test_api_session_is_expired_with_buffer() {
        let future_time = SystemTime::now() + Duration::from_secs(60);
        let session = ApiSession::new(
            "test-id".to_string(),
            "test-token".to_string(),
            future_time,
        );

        // With small buffer, should not be expired
        assert!(!session.is_expired_with_buffer(Duration::from_secs(10)));

        // With large buffer, should be considered expired
        assert!(session.is_expired_with_buffer(Duration::from_secs(120)));
    }

    #[test]
    fn test_parse_ziti_timestamp() {
        let timestamp = "2023-12-07T10:30:00.000Z";
        let result = parse_ziti_timestamp(timestamp);
        assert!(result.is_ok());

        let system_time = result.unwrap();
        let duration_since_epoch = system_time.duration_since(UNIX_EPOCH).unwrap();
        assert!(duration_since_epoch.as_secs() > 0);
    }

    #[test]
    fn test_parse_invalid_timestamp() {
        let invalid_timestamp = "invalid-timestamp";
        let result = parse_ziti_timestamp(invalid_timestamp);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ZitiError::ConfigError(_)));
    }

    #[test]
    fn test_base64_encode() {
        // Test empty input
        assert_eq!(base64_encode(b""), "");

        // Test single character
        assert_eq!(base64_encode(b"M"), "TQ==");

        // Test two characters
        assert_eq!(base64_encode(b"Ma"), "TWE=");

        // Test three characters
        assert_eq!(base64_encode(b"Man"), "TWFu");

        // Test longer string
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hello World"), "SGVsbG8gV29ybGQ=");
    }

    #[test]
    fn test_serialize_cert_chain() {
        let cert_data = b"test certificate data";
        let cert = rustls::pki_types::CertificateDer::from(cert_data.to_vec());
        let cert_chain = vec![cert];

        let result = serialize_cert_chain(&cert_chain);
        assert!(result.is_ok());

        let pem_data = result.unwrap();
        let pem_string = String::from_utf8(pem_data).unwrap();
        assert!(pem_string.contains("-----BEGIN CERTIFICATE-----"));
        assert!(pem_string.contains("-----END CERTIFICATE-----"));
        assert!(pem_string.contains(&base64_encode(cert_data)));
    }

    #[test]
    fn test_serialize_private_key() {
        let key_data = b"test private key data";
        let private_key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_data.to_vec()),
        );

        let result = serialize_private_key(&private_key);
        assert!(result.is_ok());

        let pem_data = result.unwrap();
        let pem_string = String::from_utf8(pem_data).unwrap();
        assert!(pem_string.contains("-----BEGIN PRIVATE KEY-----"));
        assert!(pem_string.contains("-----END PRIVATE KEY-----"));
        assert!(pem_string.contains(&base64_encode(key_data)));
    }

    #[test]
    fn test_auth_response_deserialization() {
        let json_data = r#"
        {
            "data": {
                "id": "session-123",
                "token": "token-abc",
                "expiresAt": "2023-12-07T10:30:00.000Z"
            }
        }
        "#;

        let result: Result<AuthResponse, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let auth_response = result.unwrap();
        assert_eq!(auth_response.data.id, "session-123");
        assert_eq!(auth_response.data.token, "token-abc");
        assert_eq!(auth_response.data.expires_at, "2023-12-07T10:30:00.000Z");
    }
}
