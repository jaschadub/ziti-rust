//! Identity and authentication module
//!
//! Handles identity configuration, enrollment processes, and certificate management
//! for Ziti SDK authentication.

pub mod config;
pub mod credentials;

use crate::error::{ZitiError, ZitiResult};
use config::Config;
use credentials::{Credentials, Identity as IdentityCredentials};
use serde_json;
use std::path::Path;
use tokio::fs;

pub use config::{Config as IdentityConfig, IdentityConfig as LegacyIdentityConfig};
pub use credentials::Identity;

/// Primary Identity struct that encapsulates Config and Credentials
///
/// This struct represents a complete Ziti identity with both configuration
/// metadata and loaded cryptographic credentials.
#[derive(Debug, Clone)]
pub struct IdentityManager {
    /// Identity configuration from JSON file
    pub config: Config,
    /// Loaded cryptographic credentials
    pub credentials: Credentials,
    /// Combined identity for authentication
    pub identity: IdentityCredentials,
}

impl IdentityManager {
    /// Create a new IdentityManager
    pub fn new(config: Config, credentials: Credentials) -> Self {
        let identity =
            IdentityCredentials::from_config_and_credentials(&config, credentials.clone());

        Self {
            config,
            credentials,
            identity,
        }
    }

    /// Load an IdentityManager from a JSON identity file
    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> ZitiResult<Self> {
        load(path.as_ref()).await
    }

    /// Get the identity ID
    pub fn id(&self) -> &str {
        &self.config.id
    }

    /// Get the Ziti API endpoint
    pub fn zt_api(&self) -> &str {
        &self.config.zt_api
    }

    /// Get the identity credentials
    pub fn identity(&self) -> &IdentityCredentials {
        &self.identity
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the credentials
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }
}

/// Load a Ziti identity from a JSON configuration file
///
/// This function takes a path to an identity JSON file, deserializes it,
/// loads the associated credentials (certificate, private key, CA), and
/// returns a complete Identity instance ready for authentication.
///
/// # Arguments
///
/// * `path` - Path to the identity JSON file
///
/// # Returns
///
/// * `ZitiResult<IdentityManager>` - Complete identity manager with config and credentials
///
/// # Example
///
/// ```rust,no_run
/// use ziti_sdk::identity::load;
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let identity = load(Path::new("identity.json")).await?;
///     println!("Loaded identity: {}", identity.id());
///     Ok(())
/// }
/// ```
pub async fn load(path: &Path) -> ZitiResult<IdentityManager> {
    // Read and parse the identity JSON file
    let json_content = fs::read_to_string(path).await.map_err(|e| {
        ZitiError::ConfigError(format!("Failed to read identity file {:?}: {}", path, e))
    })?;

    let config: Config = serde_json::from_str(&json_content).map_err(|e| {
        ZitiError::ConfigError(format!(
            "Failed to parse identity JSON from {:?}: {}",
            path, e
        ))
    })?;

    // Load credentials based on paths in the config
    let credentials = Credentials::load_from_config(&config).await?;

    // Create and return the complete identity manager
    Ok(IdentityManager::new(config, credentials))
}

/// Load a Ziti identity from a JSON configuration file (convenience function)
///
/// This is a convenience wrapper around the `load` function that accepts
/// anything that can be converted to a Path reference.
pub async fn load_from_file<P: AsRef<Path>>(path: P) -> ZitiResult<IdentityManager> {
    load(path.as_ref()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::RootCertStore;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    fn create_mock_config() -> Config {
        Config::new(
            "https://controller.example.com".to_string(),
            "test-identity".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        )
    }

    fn create_mock_credentials() -> Credentials {
        Credentials::new(
            vec![CertificateDer::from(vec![0u8; 32])], // Mock certificate
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(vec![0u8; 32])), // Mock private key
            RootCertStore::empty(),                    // Empty CA store
        )
    }

    #[test]
    fn test_identity_manager_new() {
        let config = create_mock_config();
        let credentials = create_mock_credentials();
        
        let manager = IdentityManager::new(config.clone(), credentials);
        
        assert_eq!(manager.id(), "test-identity");
        assert_eq!(manager.zt_api(), "https://controller.example.com");
        assert_eq!(manager.config().id, config.id);
    }

    #[test]
    fn test_identity_manager_getters() {
        let config = create_mock_config();
        let credentials = create_mock_credentials();
        
        let manager = IdentityManager::new(config, credentials);
        
        // Test getter methods
        assert_eq!(manager.id(), "test-identity");
        assert_eq!(manager.zt_api(), "https://controller.example.com");
        assert!(manager.identity().id == "test-identity");
        assert!(manager.config().zt_api == "https://controller.example.com");
        assert!(manager.credentials().certificate_chain.len() == 1);
    }

    #[test]
    fn test_identity_manager_clone() {
        let config = create_mock_config();
        let credentials = create_mock_credentials();
        
        let original = IdentityManager::new(config, credentials);
        let cloned = original.clone();
        
        assert_eq!(cloned.id(), original.id());
        assert_eq!(cloned.zt_api(), original.zt_api());
        assert_eq!(cloned.config().id, original.config().id);
    }

    #[test]
    fn test_identity_manager_with_different_values() {
        let config = Config::new(
            "https://different.controller.com:8443".to_string(),
            "different-id".to_string(),
            "different-cert.pem".to_string(),
            "different-key.pem".to_string(),
            "different-ca.pem".to_string(),
        );
        let credentials = create_mock_credentials();
        
        let manager = IdentityManager::new(config, credentials);
        
        assert_eq!(manager.id(), "different-id");
        assert_eq!(manager.zt_api(), "https://different.controller.com:8443");
    }

    #[test]
    fn test_identity_credentials_from_manager() {
        let config = create_mock_config();
        let credentials = create_mock_credentials();
        
        let manager = IdentityManager::new(config, credentials);
        let identity = manager.identity();
        
        assert_eq!(identity.id, "test-identity");
        assert_eq!(identity.certificate_chain.len(), 1);
    }
}
