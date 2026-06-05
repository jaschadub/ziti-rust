//! Main Context implementation
//!
//! Provides the core Context struct and its implementation for Ziti SDK operations.

use crate::config::ZitiConfig;
use crate::connection::ZitiStream;
use crate::error::ZitiResult;
use crate::identity::credentials::Credentials;
use crate::identity::{self, IdentityConfig, IdentityManager};
use crate::session::SessionManager;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Default timeout applied to controller API requests.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Main Context struct for Ziti SDK operations
///
/// The `Context` is the primary interface for interacting with the Ziti network.
/// It manages identity authentication, session handling, and provides methods
/// for establishing both outbound connections (dial) and inbound listeners (listen).
///
/// # Examples
///
/// ## Creating a Context from an identity file
///
/// ```rust,no_run
/// use ziti_sdk::{Context, ZitiResult};
///
/// #[tokio::main]
/// async fn main() -> ZitiResult<()> {
///     let context = Context::from_file("identity.json").await?;
///     // Context is now ready for use
///     Ok(())
/// }
/// ```
///
/// ## Using the Context to dial a service
///
/// ```rust,no_run
/// use ziti_sdk::Context;
/// use tokio::io::AsyncWriteExt;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let context = Context::from_file("identity.json").await?;
///     let mut stream = context.dial("my-service").await?;
///     stream.write_all(b"Hello Ziti!").await?;
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Context {
    identity_manager: Arc<IdentityManager>,
    session_manager: SessionManager,
    connect_timeout: Duration,
}

impl Context {
    /// Create a new Context from configuration
    ///
    /// Builds a Context directly from a [`ZitiConfig`] that already holds loaded
    /// identity credentials (certificate chain, private key and CA store).
    ///
    /// # Arguments
    ///
    /// * `config` - The Ziti configuration containing the controller URL and identity
    ///
    /// # Returns
    ///
    /// * `ZitiResult<Self>` - A new Context instance on success
    pub async fn new(config: ZitiConfig) -> ZitiResult<Self> {
        let identity_config = IdentityConfig::new(
            config.controller_url.to_string(),
            config.identity.id.clone(),
            String::new(),
            String::new(),
            String::new(),
        );

        let credentials = Credentials::new(
            config.identity.certificate_chain.clone(),
            config.identity.private_key.clone_key(),
            config.identity.ca_store.clone(),
        );

        let identity_manager = IdentityManager::new(identity_config, credentials);
        let session_manager = SessionManager::new(identity_manager.clone());

        Ok(Self {
            identity_manager: Arc::new(identity_manager),
            session_manager,
            connect_timeout: config.connect_timeout,
        })
    }

    /// Create a new Context with existing IdentityManager and SessionManager
    ///
    /// This is an advanced constructor that allows you to provide pre-configured
    /// identity and session managers. This is useful for testing or when you need
    /// fine-grained control over the context creation process.
    ///
    /// # Arguments
    ///
    /// * `identity_manager` - Pre-configured identity manager
    /// * `session_manager` - Pre-configured session manager
    ///
    /// # Returns
    ///
    /// * `Self` - A new Context instance
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ziti_sdk::{Context, IdentityManager, SessionManager};
    ///
    /// async fn create_context_from_managers() -> ziti_sdk::ZitiResult<Context> {
    ///     let identity_manager = IdentityManager::load_from_file("identity.json").await?;
    ///     let session_manager = SessionManager::new(identity_manager.clone());
    ///     let context = Context::from_managers(identity_manager, session_manager);
    ///     Ok(context)
    /// }
    /// ```
    pub fn from_managers(identity_manager: IdentityManager, session_manager: SessionManager) -> Self {
        Self {
            identity_manager: Arc::new(identity_manager),
            session_manager,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Create context from identity file
    ///
    /// Loads a Ziti identity from a JSON file and creates a new context.
    /// This is the most common way to create a context for production use.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the identity JSON file
    ///
    /// # Returns
    ///
    /// * `ZitiResult<Self>` - A new Context instance on success
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The identity file cannot be read
    /// - The identity file contains invalid JSON
    /// - The identity contains invalid certificates or keys
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("./my-identity.json").await?;
    ///     println!("Context created successfully!");
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_file<P: AsRef<Path>>(path: P) -> ZitiResult<Self> {
        // Load identity from file
        let identity_manager = identity::load_from_file(path).await?;
        
        // Create session manager with the identity
        let session_manager = SessionManager::new(identity_manager.clone());

        Ok(Self {
            identity_manager: Arc::new(identity_manager),
            session_manager,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        })
    }

    /// Override the timeout used for controller API requests.
    pub(crate) fn set_connect_timeout(&mut self, timeout: Duration) {
        self.connect_timeout = timeout;
    }

    /// The timeout applied to controller API requests made by this context.
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// Dial a service by name
    ///
    /// Establishes an outbound connection to a Ziti service. This method performs
    /// the complete Ziti connection process including service discovery, edge router
    /// selection, and protocol handshake.
    ///
    /// # Arguments
    ///
    /// * `service_name` - The name of the service to connect to
    ///
    /// # Returns
    ///
    /// * `ZitiResult<ZitiStream>` - A bidirectional stream for communication
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The service is not found or not accessible
    /// - No edge routers are available for the service
    /// - The connection handshake fails
    /// - Authentication or authorization fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::Context;
    /// use tokio::io::{AsyncReadExt, AsyncWriteExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let mut stream = context.dial("echo-service").await?;
    ///
    ///     // Send data to the service
    ///     stream.write_all(b"Hello, World!").await?;
    ///
    ///     // Read response
    ///     let mut buffer = [0; 1024];
    ///     let n = stream.read(&mut buffer).await?;
    ///     println!("Response: {}", String::from_utf8_lossy(&buffer[..n]));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn dial(&self, service_name: &str) -> ZitiResult<ZitiStream> {
        crate::connection::dial(service_name, self).await
    }

    /// Listen on a service
    ///
    /// Creates a listener that can accept incoming connections for a Ziti service.
    /// This method registers the current identity as a host for the specified service
    /// and returns a listener that can accept incoming connections.
    ///
    /// # Arguments
    ///
    /// * `service_name` - The name of the service to host
    ///
    /// # Returns
    ///
    /// * `ZitiResult<ZitiListener>` - A listener for accepting incoming connections
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The service is not found or not accessible for hosting
    /// - No edge routers are available for hosting
    /// - Terminator creation fails
    /// - Connection to edge router fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult};
    /// use tokio::io::{AsyncReadExt, AsyncWriteExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let mut listener = context.listen("echo-service").await?;
    ///
    ///     println!("Listening on service: {}", listener.service_name());
    ///
    ///     // Accept incoming connections
    ///     while let Ok(mut stream) = listener.accept().await {
    ///         tokio::spawn(async move {
    ///             let mut buffer = [0; 1024];
    ///             if let Ok(n) = stream.read(&mut buffer).await {
    ///                 let _ = stream.write_all(&buffer[..n]).await;
    ///             }
    ///         });
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn listen(&self, service_name: &str) -> ZitiResult<crate::connection::ZitiListener> {
        crate::connection::listen(service_name, self).await
    }

    /// Listen on a service with custom options
    ///
    /// Creates a listener with custom configuration options for hosting a Ziti service.
    /// This allows fine-grained control over terminator settings such as cost and precedence.
    ///
    /// # Arguments
    ///
    /// * `service_name` - The name of the service to host
    /// * `options` - Configuration options for the listener
    ///
    /// # Returns
    ///
    /// * `ZitiResult<ZitiListener>` - A listener for accepting incoming connections
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult, ListenOptions};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///
    ///     let options = ListenOptions {
    ///         cost: Some(100),
    ///         precedence: Some("high".to_string()),
    ///         ..Default::default()
    ///     };
    ///
    ///     let listener = context.listen_with_options("my-service", &options).await?;
    ///     println!("Listening with custom options on: {}", listener.service_name());
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn listen_with_options(
        &self,
        service_name: &str,
        options: &crate::config::ListenOptions,
    ) -> ZitiResult<crate::connection::ZitiListener> {
        crate::connection::listen_with_options(service_name, self, options).await
    }

    /// Get the identity manager
    ///
    /// Returns a reference to the underlying identity manager, which handles
    /// certificate management and authentication with the Ziti controller.
    ///
    /// # Returns
    ///
    /// * `&IdentityManager` - Reference to the identity manager
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let identity = context.identity_manager();
    ///     println!("Identity ID: {}", identity.id());
    ///     Ok(())
    /// }
    /// ```
    pub fn identity_manager(&self) -> &IdentityManager {
        &self.identity_manager
    }

    /// Get the session manager
    ///
    /// Returns a reference to the underlying session manager, which handles
    /// API session management and network session lifecycle.
    ///
    /// # Returns
    ///
    /// * `&SessionManager` - Reference to the session manager
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let session_mgr = context.session_manager();
    ///     // Session manager can be used for advanced session operations
    ///     Ok(())
    /// }
    /// ```
    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{credentials::Credentials, IdentityConfig};
    use rustls::RootCertStore;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    fn create_test_identity_manager() -> IdentityManager {
        let config = IdentityConfig::new(
            "https://controller.example.com".to_string(),
            "test-identity".to_string(),
            "cert.pem".to_string(),
            "key.pem".to_string(),
            "ca.pem".to_string(),
        );
        
        let cert_data = b"test certificate data";
        let key_data = b"test private key data";
        
        let credentials = Credentials::new(
            vec![CertificateDer::from(cert_data.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_data.to_vec())),
            RootCertStore::empty(),
        );
        
        IdentityManager::new(config, credentials)
    }

    #[test]
    fn test_context_from_managers() {
        let identity_manager = create_test_identity_manager();
        let session_manager = SessionManager::new(identity_manager.clone());
        
        let context = Context::from_managers(identity_manager.clone(), session_manager);
        
        // Test that we can access the managers
        assert_eq!(context.identity_manager().id(), identity_manager.id());
    }

    #[test]
    fn test_context_clone() {
        let identity_manager = create_test_identity_manager();
        let session_manager = SessionManager::new(identity_manager.clone());
        
        let context = Context::from_managers(identity_manager, session_manager);
        let cloned_context = context.clone();
        
        // Both contexts should have the same identity manager
        assert_eq!(
            context.identity_manager().id(),
            cloned_context.identity_manager().id()
        );
    }

    #[test]
    fn test_context_getters() {
        let identity_manager = create_test_identity_manager();
        let session_manager = SessionManager::new(identity_manager.clone());
        
        let context = Context::from_managers(identity_manager.clone(), session_manager);
        
        // Test getter methods
        let _identity_mgr = context.identity_manager();
        let _session_mgr = context.session_manager();
        
        assert_eq!(context.identity_manager().id(), identity_manager.id());
    }

}
