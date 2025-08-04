//! SDK settings and configuration
//!
//! Provides the main ZitiConfig struct and related configuration types.

use crate::identity::Identity;
use std::time::Duration;
use url::Url;

/// Main configuration struct for Ziti SDK
#[derive(Debug, Clone)]
pub struct ZitiConfig {
    pub controller_url: Url,
    pub identity: Identity,
    pub connect_timeout: Duration,
    pub enable_ha: bool,
    pub max_connections: Option<u32>,
}

impl ZitiConfig {
    /// Create a new ZitiConfig with default values
    pub fn new(controller_url: Url, identity: Identity) -> Self {
        Self {
            controller_url,
            identity,
            connect_timeout: Duration::from_secs(30),
            enable_ha: false,
            max_connections: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::credentials::Identity;
    use rustls::{Certificate, PrivateKey, RootCertStore};
    use std::time::Duration;
    use url::Url;

    fn create_mock_identity() -> Identity {
        Identity::new(
            "test-id".to_string(),
            vec![Certificate(vec![0u8; 32])], // Mock certificate
            PrivateKey(vec![0u8; 32]),        // Mock private key
            RootCertStore::empty(),           // Empty CA store
        )
    }

    #[test]
    fn test_ziti_config_new() {
        let controller_url = Url::parse("https://controller.example.com:443").unwrap();
        let identity = create_mock_identity();
        
        let config = ZitiConfig::new(controller_url.clone(), identity.clone());
        
        assert_eq!(config.controller_url, controller_url);
        assert_eq!(config.identity.id, "test-id");
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert!(!config.enable_ha);
        assert_eq!(config.max_connections, None);
    }

    #[test]
    fn test_ziti_config_clone() {
        let controller_url = Url::parse("https://controller.example.com:443").unwrap();
        let identity = create_mock_identity();
        
        let original = ZitiConfig::new(controller_url, identity);
        let cloned = original.clone();
        
        assert_eq!(cloned.controller_url, original.controller_url);
        assert_eq!(cloned.identity.id, original.identity.id);
        assert_eq!(cloned.connect_timeout, original.connect_timeout);
        assert_eq!(cloned.enable_ha, original.enable_ha);
        assert_eq!(cloned.max_connections, original.max_connections);
    }

    #[test]
    fn test_ziti_config_with_custom_values() {
        let controller_url = Url::parse("https://custom.controller.com:8443").unwrap();
        let mut identity = create_mock_identity();
        identity.id = "custom-identity".to_string();
        
        let mut config = ZitiConfig::new(controller_url.clone(), identity);
        config.connect_timeout = Duration::from_secs(60);
        config.enable_ha = true;
        config.max_connections = Some(100);
        
        assert_eq!(config.controller_url, controller_url);
        assert_eq!(config.identity.id, "custom-identity");
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        assert!(config.enable_ha);
        assert_eq!(config.max_connections, Some(100));
    }
}
