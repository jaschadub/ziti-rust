//! API session implementation
//!
//! Handles controller API sessions for authentication.

use crate::error::{ZitiError, ZitiResult};
use crate::identity::IdentityManager;
use crate::transport::http::controller_client;
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
    let client = controller_client(identity_manager, Duration::from_secs(30)).await?;

    // The Ziti edge /authenticate endpoint requires the auth method as
    // a query parameter. `cert` tells the controller to authenticate
    // via the mTLS client certificate presented by the SDK.
    let auth_url = format!(
        "{}/authenticate?method=cert",
        identity_manager.zt_api().trim_end_matches('/')
    );

    // Send authentication request. An empty JSON object body keeps
    // the controller happy when it parses Content-Type: application/json.
    let response = client
        .post(&auth_url)
        .header("Content-Type", "application/json")
        .body("{}")
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
