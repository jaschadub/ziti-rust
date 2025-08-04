//! Identity configuration
//!
//! Configuration structures and types for identity management.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Identity configuration from Ziti JSON files
///
/// This structure represents the standard Ziti identity file format
/// which contains the identity metadata and paths to certificate files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Ziti API endpoint URL
    #[serde(rename = "ztAPI")]
    pub zt_api: String,

    /// Identity ID
    pub id: String,

    /// Path to client certificate file
    pub cert: String,

    /// Path to private key file
    pub key: String,

    /// Path to CA certificate file
    pub ca: String,
}

impl Config {
    /// Create a new Config instance
    pub fn new(zt_api: String, id: String, cert: String, key: String, ca: String) -> Self {
        Self {
            zt_api,
            id,
            cert,
            key,
            ca,
        }
    }

    /// Get the certificate file path as PathBuf
    pub fn cert_path(&self) -> PathBuf {
        PathBuf::from(&self.cert)
    }

    /// Get the private key file path as PathBuf
    pub fn key_path(&self) -> PathBuf {
        PathBuf::from(&self.key)
    }

    /// Get the CA certificate file path as PathBuf
    pub fn ca_path(&self) -> PathBuf {
        PathBuf::from(&self.ca)
    }
}

// Keep the old struct for backwards compatibility during transition
/// Legacy identity configuration structure
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    pub id: String,
    pub cert: String,
    pub key: String,
    pub ca: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_config_new() {
        let config = Config::new(
            "https://controller.example.com".to_string(),
            "test-id".to_string(),
            "/path/to/cert.pem".to_string(),
            "/path/to/key.pem".to_string(),
            "/path/to/ca.pem".to_string(),
        );

        assert_eq!(config.zt_api, "https://controller.example.com");
        assert_eq!(config.id, "test-id");
        assert_eq!(config.cert, "/path/to/cert.pem");
        assert_eq!(config.key, "/path/to/key.pem");
        assert_eq!(config.ca, "/path/to/ca.pem");
    }

    #[test]
    fn test_config_path_methods() {
        let config = Config::new(
            "https://controller.example.com".to_string(),
            "test-id".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        );

        assert_eq!(config.cert_path(), PathBuf::from("cert.pem"));
        assert_eq!(config.key_path(), PathBuf::from("key.pem"));
        assert_eq!(config.ca_path(), PathBuf::from("ca.pem"));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::new(
            "https://controller.example.com".to_string(),
            "test-id".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        );

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.zt_api, config.zt_api);
        assert_eq!(deserialized.id, config.id);
        assert_eq!(deserialized.cert, config.cert);
        assert_eq!(deserialized.key, config.key);
        assert_eq!(deserialized.ca, config.ca);
    }

    #[test]
    fn test_config_with_zt_api_field_name() {
        // Test the serde rename attribute
        let json = r#"{
            "ztAPI": "https://controller.example.com",
            "id": "test-id",
            "cert": "cert.pem",
            "key": "key.pem",
            "ca": "ca.pem"
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.zt_api, "https://controller.example.com");
    }

    #[test]
    fn test_legacy_identity_config() {
        let legacy_config = IdentityConfig {
            id: "legacy-id".to_string(),
            cert: "legacy-cert.pem".to_string(),
            key: "legacy-key.pem".to_string(),
            ca: "legacy-ca.pem".to_string(),
        };

        assert_eq!(legacy_config.id, "legacy-id");
        assert_eq!(legacy_config.cert, "legacy-cert.pem");
        assert_eq!(legacy_config.key, "legacy-key.pem");
        assert_eq!(legacy_config.ca, "legacy-ca.pem");
    }

    #[test]
    fn test_config_clone() {
        let original = Config::new(
            "https://controller.example.com".to_string(),
            "test-id".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        );

        let cloned = original.clone();
        assert_eq!(cloned.zt_api, original.zt_api);
        assert_eq!(cloned.id, original.id);
        assert_eq!(cloned.cert, original.cert);
        assert_eq!(cloned.key, original.key);
        assert_eq!(cloned.ca, original.ca);
    }
}
