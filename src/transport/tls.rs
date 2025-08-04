//! TLS configuration and handling
//!
//! Manages TLS connections and certificate validation for mTLS with Ziti Edge Routers.

use crate::error::{ZitiError, ZitiResult};
use crate::identity::IdentityManager;
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use webpki_roots;

/// TLS configuration manager for Ziti connections
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Rustls client configuration
    client_config: Arc<ClientConfig>,
}

impl TlsConfig {
    /// Create a new TLS configuration from an identity manager
    ///
    /// This builds a rustls ClientConfig with:
    /// - Client certificate from the identity for mTLS authentication
    /// - CA bundle from the identity to verify the server certificate
    /// - Additional system root CAs for fallback verification
    ///
    /// # Arguments
    ///
    /// * `identity` - IdentityManager containing the client certificate and CA bundle
    ///
    /// # Returns
    ///
    /// * `ZitiResult<TlsConfig>` - Configured TLS manager ready for WebSocket connections
    pub fn from_identity(identity: &IdentityManager) -> ZitiResult<Self> {
        // Start with the identity's CA store
        let mut root_store = identity.credentials().ca_store.clone();
        
        // Add system root CAs as fallback
        for cert in webpki_roots::TLS_SERVER_ROOTS {
            root_store.add(&rustls::Certificate(cert.spki.to_vec()))
                .map_err(|e| ZitiError::ConfigError(format!("Failed to add root CA: {}", e)))?;
        }

        // Create client configuration with mTLS
        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_client_auth_cert(
                identity.credentials().certificate_chain.clone(),
                identity.credentials().private_key.clone(),
            )
            .map_err(|e| ZitiError::ConfigError(format!("Failed to create TLS config: {}", e)))?;

        Ok(Self {
            client_config: Arc::new(client_config),
        })
    }

    /// Get the rustls client configuration
    pub fn client_config(&self) -> Arc<ClientConfig> {
        self.client_config.clone()
    }

    /// Create a new empty TLS config (for testing)
    pub fn new() -> Self {
        let root_store = RootCertStore::empty();
        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Self {
            client_config: Arc::new(client_config),
        }
    }
}

/// Build a rustls client configuration for Ziti connections
///
/// This is a convenience function that creates a TLS configuration
/// from an identity manager.
///
/// # Arguments
///
/// * `identity` - IdentityManager containing certificates and keys
///
/// # Returns
///
/// * `ZitiResult<Arc<ClientConfig>>` - Rustls client configuration
pub fn build_client_config(identity: &IdentityManager) -> ZitiResult<Arc<ClientConfig>> {
    let tls_config = TlsConfig::from_identity(identity)?;
    Ok(tls_config.client_config())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_new() {
        let config = TlsConfig::new();
        // Basic test to ensure config is created successfully
        assert!(config.client_config.clone().alpn_protocols.is_empty());
    }
}
