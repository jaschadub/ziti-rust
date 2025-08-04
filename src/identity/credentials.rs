//! Identity credentials management
//!
//! Handles certificate chains, private keys, and CA stores for Ziti identity.

use crate::error::{ZitiError, ZitiResult};
use crate::identity::config::Config;
use rustls::{Certificate, PrivateKey, RootCertStore};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tokio::fs;

/// Credentials structure to hold client certificate and private key
#[derive(Debug, Clone)]
pub struct Credentials {
    /// Client certificate chain
    pub certificate_chain: Vec<Certificate>,
    /// Private key for the client certificate
    pub private_key: PrivateKey,
    /// CA certificate store for validation
    pub ca_store: RootCertStore,
}

impl Credentials {
    /// Create new Credentials instance
    pub fn new(
        certificate_chain: Vec<Certificate>,
        private_key: PrivateKey,
        ca_store: RootCertStore,
    ) -> Self {
        Self {
            certificate_chain,
            private_key,
            ca_store,
        }
    }

    /// Load credentials from file paths provided in the identity configuration
    pub async fn load_from_config(config: &Config) -> ZitiResult<Self> {
        // Load certificate chain
        let cert_chain = Self::load_certificates(&config.cert_path()).await?;

        // Load private key
        let private_key = Self::load_private_key(&config.key_path()).await?;

        // Load CA certificates
        let ca_store = Self::load_ca_certificates(&config.ca_path()).await?;

        Ok(Self::new(cert_chain, private_key, ca_store))
    }

    /// Load certificates from a PEM file
    async fn load_certificates(cert_path: &Path) -> ZitiResult<Vec<Certificate>> {
        let cert_file = fs::File::open(cert_path).await.map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to open certificate file {:?}: {}",
                cert_path, e
            ))
        })?;

        let cert_file = cert_file.into_std().await;
        let mut cert_reader = BufReader::new(cert_file);

        let cert_chain = certs(&mut cert_reader).map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to parse certificate file {:?}: {}",
                cert_path, e
            ))
        })?;

        if cert_chain.is_empty() {
            return Err(ZitiError::ConfigError(format!(
                "No certificates found in file {:?}",
                cert_path
            )));
        }

        Ok(cert_chain.into_iter().map(Certificate).collect())
    }

    /// Load private key from a PEM file
    async fn load_private_key(key_path: &Path) -> ZitiResult<PrivateKey> {
        let key_file = fs::File::open(key_path).await.map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to open private key file {:?}: {}",
                key_path, e
            ))
        })?;

        let key_file = key_file.into_std().await;
        let mut key_reader = BufReader::new(key_file);

        // Try PKCS8 format first
        if let Ok(mut keys) = pkcs8_private_keys(&mut key_reader) {
            if !keys.is_empty() {
                return Ok(PrivateKey(keys.remove(0)));
            }
        }

        // Reset reader and try RSA format
        let key_file = File::open(key_path).map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to reopen private key file {:?}: {}",
                key_path, e
            ))
        })?;
        let mut key_reader = BufReader::new(key_file);

        let keys = rsa_private_keys(&mut key_reader).map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to parse private key file {:?}: {}",
                key_path, e
            ))
        })?;

        if keys.is_empty() {
            return Err(ZitiError::ConfigError(format!(
                "No private keys found in file {:?}",
                key_path
            )));
        }

        Ok(PrivateKey(keys[0].clone()))
    }

    /// Load CA certificates from a PEM file
    async fn load_ca_certificates(ca_path: &Path) -> ZitiResult<RootCertStore> {
        let ca_file = fs::File::open(ca_path).await.map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to open CA certificate file {:?}: {}",
                ca_path, e
            ))
        })?;

        let ca_file = ca_file.into_std().await;
        let mut ca_reader = BufReader::new(ca_file);

        let ca_certs = certs(&mut ca_reader).map_err(|e| {
            ZitiError::ConfigError(format!(
                "Failed to parse CA certificate file {:?}: {}",
                ca_path, e
            ))
        })?;

        if ca_certs.is_empty() {
            return Err(ZitiError::ConfigError(format!(
                "No CA certificates found in file {:?}",
                ca_path
            )));
        }

        let mut ca_store = RootCertStore::empty();
        for cert in ca_certs {
            ca_store.add(&Certificate(cert)).map_err(|e| {
                ZitiError::ConfigError(format!("Failed to add CA certificate: {}", e))
            })?;
        }

        Ok(ca_store)
    }
}

/// Identity credentials for Ziti authentication
#[derive(Debug, Clone)]
pub struct Identity {
    pub id: String,
    pub certificate_chain: Vec<Certificate>,
    pub private_key: PrivateKey,
    pub ca_store: RootCertStore,
}

impl Identity {
    /// Create a new Identity
    pub fn new(
        id: String,
        certificate_chain: Vec<Certificate>,
        private_key: PrivateKey,
        ca_store: RootCertStore,
    ) -> Self {
        Self {
            id,
            certificate_chain,
            private_key,
            ca_store,
        }
    }

    /// Create Identity from Config and Credentials
    pub fn from_config_and_credentials(config: &Config, credentials: Credentials) -> Self {
        Self {
            id: config.id.clone(),
            certificate_chain: credentials.certificate_chain,
            private_key: credentials.private_key,
            ca_store: credentials.ca_store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{Certificate, PrivateKey, RootCertStore};

    fn create_mock_certificate() -> Certificate {
        Certificate(vec![0u8; 32]) // Mock certificate data
    }

    fn create_mock_private_key() -> PrivateKey {
        PrivateKey(vec![0u8; 32]) // Mock private key data
    }

    fn create_mock_ca_store() -> RootCertStore {
        RootCertStore::empty()
    }

    #[test]
    fn test_credentials_new() {
        let cert_chain = vec![create_mock_certificate()];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let credentials = Credentials::new(cert_chain.clone(), private_key.clone(), ca_store);

        assert_eq!(credentials.certificate_chain.len(), 1);
        assert_eq!(credentials.private_key.0, private_key.0);
    }

    #[test]
    fn test_credentials_clone() {
        let cert_chain = vec![create_mock_certificate()];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let original = Credentials::new(cert_chain, private_key, ca_store);
        let cloned = original.clone();

        assert_eq!(cloned.certificate_chain.len(), original.certificate_chain.len());
        assert_eq!(cloned.private_key.0, original.private_key.0);
    }

    #[test]
    fn test_identity_new() {
        let id = "test-identity".to_string();
        let cert_chain = vec![create_mock_certificate()];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let identity = Identity::new(id.clone(), cert_chain.clone(), private_key.clone(), ca_store);

        assert_eq!(identity.id, id);
        assert_eq!(identity.certificate_chain.len(), 1);
        assert_eq!(identity.private_key.0, private_key.0);
    }

    #[test]
    fn test_identity_clone() {
        let id = "test-identity".to_string();
        let cert_chain = vec![create_mock_certificate()];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let original = Identity::new(id, cert_chain, private_key, ca_store);
        let cloned = original.clone();

        assert_eq!(cloned.id, original.id);
        assert_eq!(cloned.certificate_chain.len(), original.certificate_chain.len());
        assert_eq!(cloned.private_key.0, original.private_key.0);
    }

    #[test]
    fn test_identity_from_config_and_credentials() {
        let config = Config::new(
            "https://controller.example.com".to_string(),
            "config-id".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        );

        let cert_chain = vec![create_mock_certificate()];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();
        let credentials = Credentials::new(cert_chain.clone(), private_key.clone(), ca_store);

        let identity = Identity::from_config_and_credentials(&config, credentials);

        assert_eq!(identity.id, "config-id");
        assert_eq!(identity.certificate_chain.len(), 1);
        assert_eq!(identity.private_key.0, private_key.0);
    }

    #[test]
    fn test_credentials_with_multiple_certificates() {
        let cert_chain = vec![
            create_mock_certificate(),
            create_mock_certificate(),
            create_mock_certificate(),
        ];
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let credentials = Credentials::new(cert_chain, private_key, ca_store);

        assert_eq!(credentials.certificate_chain.len(), 3);
    }

    #[test]
    fn test_identity_with_empty_certificate_chain() {
        let id = "test-identity".to_string();
        let cert_chain = vec![]; // Empty certificate chain
        let private_key = create_mock_private_key();
        let ca_store = create_mock_ca_store();

        let identity = Identity::new(id.clone(), cert_chain, private_key, ca_store);

        assert_eq!(identity.id, id);
        assert_eq!(identity.certificate_chain.len(), 0);
    }
}
