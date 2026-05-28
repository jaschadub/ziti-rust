//! Main Context implementation
//!
//! Provides the core Context struct and its implementation for Ziti SDK operations.

use crate::config::ZitiConfig;
use crate::connection::ZitiStream;
use crate::error::{ZitiError, ZitiResult};
use crate::identity::{self, IdentityManager};
use crate::service::list_services;
use crate::session::SessionManager;
use crate::transport::{TlsConfig, WebSocketTransport};
use std::path::Path;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

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
}

impl Context {
    /// Create a new Context from configuration
    ///
    /// Creates a new Ziti context using the provided configuration.
    /// This method is currently not implemented and will be available in future versions.
    ///
    /// # Arguments
    ///
    /// * `_config` - The Ziti configuration containing connection details
    ///
    /// # Returns
    ///
    /// * `ZitiResult<Self>` - A new Context instance on success
    ///
    /// # Note
    ///
    /// This method is currently unimplemented. Use [`Context::from_file`] instead.
    pub async fn new(_config: ZitiConfig) -> ZitiResult<Self> {
        // For now, we'll implement a basic version that loads from identity file
        // In practice, ZitiConfig would contain the identity file path
        todo!("Implement Context::new - requires ZitiConfig structure definition")
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
        })
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
        // Step 1: Find the service by name using list_services
        let services = list_services(&self.session_manager).await?;
        let service = services
            .iter()
            .find(|s| s.name == service_name)
            .ok_or_else(|| ZitiError::ServiceNotFound {
                service_name: service_name.to_string(),
            })?;

        // Step 2: Request a network session from the controller
        // This will involve calling an API endpoint to get edge router info
        let edge_routers = self.get_service_terminators(&service.id).await?;
        
        if edge_routers.is_empty() {
            return Err(ZitiError::ConnectionFailed(
                format!("No edge routers available for service '{}'", service_name)
            ));
        }

        // Step 3: Choose an edge router (for now, just pick the first one)
        let edge_router = &edge_routers[0];
        
        // Step 4: Create TLS config from identity
        let tls_config = TlsConfig::from_identity(&self.identity_manager)?;
        
        // Step 5: Establish WebSocket connection to edge router
        let ws_url = Url::parse(&format!("wss://{}:{}/ws", edge_router.hostname, edge_router.port))
            .map_err(|e| ZitiError::ConfigError(format!("Invalid edge router URL: {}", e)))?;
        
        let mut transport = WebSocketTransport::connect(ws_url, tls_config).await?;
        
        // Step 6: Perform Ziti connection handshake
        self.perform_ziti_handshake(&mut transport, &service.id).await?;
        
        // Step 7: Return ZitiStream wrapping the transport
        Ok(ZitiStream::from_transport(transport))
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

    /// Get terminators (edge routers) for a service
    async fn get_service_terminators(&self, service_id: &str) -> ZitiResult<Vec<EdgeRouter>> {
        // Get API session for authentication
        let api_session = self.session_manager.get_api_session().await?;
        
        // Create HTTP client
        let client = reqwest::Client::new();
        
        // Build terminators endpoint URL
        let terminators_url = format!(
            "{}/services/{}/terminators",
            self.identity_manager.zt_api().trim_end_matches('/'),
            service_id
        );

        // Send request to get terminators
        let response = client
            .get(&terminators_url)
            .header("Content-Type", "application/json")
            .header("zt-session", &api_session.token)
            .send()
            .await
            .map_err(|e| {
                ZitiError::ConnectionFailed(format!("Failed to get service terminators: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ZitiError::ProtocolError {
                message: format!(
                    "Terminators request failed with status {}: {}",
                    status, error_text
                ),
            });
        }

        // Parse terminators response
        let terminators_response: TerminatorsResponse = response
            .json()
            .await
            .map_err(|e| ZitiError::ProtocolError {
                message: format!("Failed to parse terminators response: {}", e),
            })?;

        // Convert terminators to edge routers
        let edge_routers = terminators_response
            .data
            .into_iter()
            .filter_map(|t| t.router)
            .collect();

        Ok(edge_routers)
    }

    /// Perform Ziti connection handshake
    async fn perform_ziti_handshake(
        &self,
        transport: &mut WebSocketTransport,
        service_id: &str,
    ) -> ZitiResult<()> {
        // Create Hello message for Ziti handshake
        let hello_msg = create_hello_message(service_id)?;
        
        // Send Hello message
        transport.send(Message::Binary(hello_msg.into())).await?;
        
        // Wait for response
        let response = transport.receive().await?;
        
        match response {
            Some(Message::Binary(data)) => {
                // Parse and validate Hello response
                validate_hello_response(&data)?;
                Ok(())
            }
            Some(_) => Err(ZitiError::ProtocolError {
                message: "Unexpected message type in handshake response".to_string(),
            }),
            None => Err(ZitiError::ConnectionFailed(
                "Connection closed during handshake".to_string(),
            )),
        }
    }
}

/// Edge router information
///
/// Represents an edge router in the Ziti network that can be used for establishing
/// connections to services. Edge routers act as entry points into the Ziti network
/// and handle the secure tunneling of application traffic.
///
/// # Fields
///
/// * `hostname` - The hostname or IP address of the edge router
/// * `port` - The port number for WebSocket connections
/// * `supported_protocols` - List of protocols supported by this edge router
///
/// # Examples
///
/// ```rust
/// use ziti_sdk::EdgeRouter;
///
/// let json_data = r#"
/// {
///     "hostname": "edge-router-1.example.com",
///     "port": 10080,
///     "supported_protocols": ["tls", "ws"]
/// }
/// "#;
///
/// let edge_router: EdgeRouter = serde_json::from_str(json_data).unwrap();
/// println!("Edge router: {}:{}", edge_router.hostname, edge_router.port);
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EdgeRouter {
    /// The hostname or IP address of the edge router
    pub hostname: String,
    /// The port number for WebSocket connections
    pub port: u16,
    /// List of protocols supported by this edge router (defaults to empty if not specified)
    #[serde(default)]
    pub supported_protocols: Vec<String>,
}

/// Terminator information from the controller
#[derive(Debug, serde::Deserialize)]
struct Terminator {
    #[allow(dead_code)]
    pub id: String,
    pub router: Option<EdgeRouter>,
}

/// Response structure for terminators API call
#[derive(Debug, serde::Deserialize)]
struct TerminatorsResponse {
    data: Vec<Terminator>,
}

/// Create a Hello message for Ziti protocol handshake
fn create_hello_message(service_id: &str) -> ZitiResult<Vec<u8>> {
    // This is a simplified Hello message. In practice, this would be more complex
    // and follow the actual Ziti protocol specification
    use serde_json::json;
    
    let hello = json!({
        "type": "Hello",
        "service_id": service_id,
        "version": "1.0"
    });
    
    serde_json::to_vec(&hello).map_err(|e| {
        ZitiError::ProtocolError {
            message: format!("Failed to serialize Hello message: {}", e),
        }
    })
}

/// Validate Hello response from edge router
fn validate_hello_response(data: &[u8]) -> ZitiResult<()> {
    // Parse response as JSON
    let response: serde_json::Value = serde_json::from_slice(data).map_err(|e| {
        ZitiError::ProtocolError {
            message: format!("Failed to parse Hello response: {}", e),
        }
    })?;
    
    // Check if response indicates success
    if let Some(status) = response.get("status")
        && (status == "ok" || status == "success")
    {
        return Ok(());
    }
    
    // If we get here, the handshake failed
    Err(ZitiError::ProtocolError {
        message: format!("Hello handshake failed: {:?}", response),
    })
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

    #[test]
    fn test_edge_router_deserialization() {
        let json_data = r#"
        {
            "hostname": "edge-router-1.example.com",
            "port": 10080,
            "supported_protocols": ["tls", "ws"]
        }
        "#;

        let result: Result<EdgeRouter, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let edge_router = result.unwrap();
        assert_eq!(edge_router.hostname, "edge-router-1.example.com");
        assert_eq!(edge_router.port, 10080);
        assert_eq!(edge_router.supported_protocols, vec!["tls", "ws"]);
    }

    #[test]
    fn test_edge_router_deserialization_with_defaults() {
        let json_data = r#"
        {
            "hostname": "edge-router-2.example.com",
            "port": 443
        }
        "#;

        let result: Result<EdgeRouter, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let edge_router = result.unwrap();
        assert_eq!(edge_router.hostname, "edge-router-2.example.com");
        assert_eq!(edge_router.port, 443);
        assert_eq!(edge_router.supported_protocols, Vec::<String>::new());
    }

    #[test]
    fn test_edge_router_debug() {
        let edge_router = EdgeRouter {
            hostname: "test.example.com".to_string(),
            port: 8080,
            supported_protocols: vec!["tls".to_string()],
        };

        let debug_str = format!("{:?}", edge_router);
        assert!(debug_str.contains("EdgeRouter"));
        assert!(debug_str.contains("test.example.com"));
        assert!(debug_str.contains("8080"));
    }

    #[test]
    fn test_edge_router_clone() {
        let edge_router = EdgeRouter {
            hostname: "clone-test.example.com".to_string(),
            port: 9090,
            supported_protocols: vec!["ws".to_string(), "tls".to_string()],
        };

        let cloned_edge_router = edge_router.clone();
        assert_eq!(edge_router.hostname, cloned_edge_router.hostname);
        assert_eq!(edge_router.port, cloned_edge_router.port);
        assert_eq!(edge_router.supported_protocols, cloned_edge_router.supported_protocols);
    }

    #[test]
    fn test_create_hello_message() {
        let service_id = "test-service-123";
        let result = create_hello_message(service_id);
        
        assert!(result.is_ok());
        let message_bytes = result.unwrap();
        
        // Parse the message back to verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_slice(&message_bytes).unwrap();
        assert_eq!(parsed["type"], "Hello");
        assert_eq!(parsed["service_id"], service_id);
        assert_eq!(parsed["version"], "1.0");
    }

    #[test]
    fn test_validate_hello_response_success() {
        let success_response = serde_json::json!({
            "status": "ok",
            "message": "Connection established"
        });
        
        let response_bytes = serde_json::to_vec(&success_response).unwrap();
        let result = validate_hello_response(&response_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hello_response_success_alternate() {
        let success_response = serde_json::json!({
            "status": "success",
            "session_id": "12345"
        });
        
        let response_bytes = serde_json::to_vec(&success_response).unwrap();
        let result = validate_hello_response(&response_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_hello_response_failure() {
        let failure_response = serde_json::json!({
            "status": "error",
            "message": "Authentication failed"
        });
        
        let response_bytes = serde_json::to_vec(&failure_response).unwrap();
        let result = validate_hello_response(&response_bytes);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ZitiError::ProtocolError { .. }));
    }

    #[test]
    fn test_validate_hello_response_invalid_json() {
        let invalid_json = b"invalid json data";
        let result = validate_hello_response(invalid_json);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ZitiError::ProtocolError { .. }));
    }

    #[test]
    fn test_validate_hello_response_missing_status() {
        let response_without_status = serde_json::json!({
            "message": "Some message without status"
        });
        
        let response_bytes = serde_json::to_vec(&response_without_status).unwrap();
        let result = validate_hello_response(&response_bytes);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ZitiError::ProtocolError { .. }));
    }

    #[test]
    fn test_terminators_response_deserialization() {
        let json_data = r#"
        {
            "data": [
                {
                    "id": "terminator-1",
                    "router": {
                        "hostname": "edge-router-1.example.com",
                        "port": 10080
                    }
                },
                {
                    "id": "terminator-2",
                    "router": null
                }
            ]
        }
        "#;

        let result: Result<TerminatorsResponse, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "terminator-1");
        assert!(response.data[0].router.is_some());
        assert_eq!(response.data[1].id, "terminator-2");
        assert!(response.data[1].router.is_none());
    }

    #[test]
    fn test_create_hello_message_with_special_characters() {
        let service_id = "service-with-special-chars-!@#$%";
        let result = create_hello_message(service_id);
        
        assert!(result.is_ok());
        let message_bytes = result.unwrap();
        
        let parsed: serde_json::Value = serde_json::from_slice(&message_bytes).unwrap();
        assert_eq!(parsed["service_id"], service_id);
    }
}
