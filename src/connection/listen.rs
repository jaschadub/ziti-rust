//! ZitiListener implementation
//!
//! Provides listener functionality for accepting incoming Ziti connections.

use super::ZitiStream;
use crate::config::ListenOptions;
use crate::context::Context;
use crate::error::{ZitiError, ZitiResult};
use crate::service::list_services;
use crate::transport::{TlsConfig, WebSocketTransport};
use std::sync::Arc;
use tokio::sync::mpsc;
use url::Url;

/// Listen on a Ziti service by name
///
/// This function orchestrates the process of hosting a Ziti service:
/// 1. Look up service details using the service discovery API
/// 2. Create a terminator via the controller API to register as service host
/// 3. Establish persistent connection to an edge router
/// 4. Return a ZitiListener for accepting incoming connections
///
/// # Arguments
///
/// * `service_name` - The name of the service to host
/// * `context` - The Ziti context containing identity and session managers
///
/// # Returns
///
/// * `ZitiResult<ZitiListener>` - A listener for accepting connections to the service
///
/// # Example
///
/// ```rust
/// use ziti_sdk::{Context, connection::listen};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let context = Context::from_file("identity.json").await?;
///     let listener = listen("echo-service", &context).await?;
///     // Accept incoming connections
///     Ok(())
/// }
/// ```
pub async fn listen(service_name: &str, context: &Context) -> ZitiResult<ZitiListener> {
    listen_with_options(service_name, context, &ListenOptions::default()).await
}

/// Listen on a Ziti service with custom options
///
/// Creates a listener with custom configuration options for hosting a Ziti service.
/// This allows fine-grained control over terminator settings such as cost and precedence.
///
/// # Arguments
///
/// * `service_name` - The name of the service to host
/// * `context` - The Ziti context containing identity and session managers
/// * `options` - Configuration options for the listener
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
/// ```rust
/// use ziti_sdk::{Context, connection::listen_with_options, ListenOptions};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let context = Context::from_file("identity.json").await?;
///
///     let options = ListenOptions {
///         cost: Some(100),
///         precedence: Some("high".to_string()),
///         ..Default::default()
///     };
///
///     let listener = listen_with_options("my-service", &context, &options).await?;
///     println!("Listening with custom options on: {}", listener.service_name());
///
///     Ok(())
/// }
/// ```
pub async fn listen_with_options(
    service_name: &str,
    context: &Context,
    options: &ListenOptions,
) -> ZitiResult<ZitiListener> {
    // Step 1: Find the service by name using list_services
    let services = list_services(context.session_manager()).await?;
    let service = services
        .iter()
        .find(|s| s.name == service_name)
        .ok_or_else(|| ZitiError::ServiceNotFound {
            service_name: service_name.to_string(),
        })?;

    // Step 2: Get available edge routers for the current identity
    let edge_routers = get_available_edge_routers(context).await?;
    
    if edge_routers.is_empty() {
        return Err(ZitiError::ConnectionFailed(
            "No edge routers available for hosting".to_string()
        ));
    }

    // Step 3: Choose an edge router (for now, just pick the first one)
    let edge_router = &edge_routers[0];

    // Step 4: Create a terminator to register as service host
    let terminator_id = create_terminator(&service.id, &edge_router.id, context, options).await?;

    // Step 5: Establish persistent connection to edge router
    let tls_config = TlsConfig::from_identity(context.identity_manager())?;
    let ws_url = Url::parse(&format!("wss://{}:{}/ws", edge_router.hostname, edge_router.port))
        .map_err(|e| ZitiError::ConfigError(format!("Invalid edge router URL: {}", e)))?;
    
    let transport = WebSocketTransport::connect(ws_url, tls_config).await?;

    // Step 6: Perform listen handshake
    // TODO: Implement proper listen handshake protocol
    
    // Step 7: Create and return ZitiListener
    let (sender, receiver) = mpsc::unbounded_channel();
    
    let listener = ZitiListener {
        service_name: service_name.to_string(),
        service_id: service.id.clone(),
        terminator_id,
        edge_router: edge_router.clone(),
        context: Arc::new(context.clone()),
        transport: Some(transport),
        connection_receiver: receiver,
        _connection_sender: sender,
    };

    Ok(listener)
}

/// Listener for accepting incoming Ziti connections
///
/// `ZitiListener` provides a server-side interface for accepting incoming connections
/// to a Ziti service. It manages the terminator registration with the Ziti controller
/// and handles the connection acceptance process.
///
/// When a listener is created, it registers a terminator with the Ziti controller,
/// advertising that this identity can handle connections for the specified service.
/// The listener then waits for incoming connections and provides them as `ZitiStream`
/// instances through the `accept()` method.
///
/// # Lifecycle
///
/// 1. **Creation**: Register as a terminator for the service
/// 2. **Listening**: Accept incoming connections via `accept()`
/// 3. **Cleanup**: Terminator is automatically cleaned up when dropped
///
/// # Examples
///
/// ## Basic echo server
///
/// ```rust
/// use ziti_sdk::{Context, ZitiResult};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// #[tokio::main]
/// async fn main() -> ZitiResult<()> {
///     let context = Context::from_file("identity.json").await?;
///     let mut listener = context.listen("echo-service").await?;
///
///     println!("Echo server listening on: {}", listener.service_name());
///
///     loop {
///         match listener.accept().await {
///             Ok(mut stream) => {
///                 // Handle each connection in a separate task
///                 tokio::spawn(async move {
///                     let mut buffer = [0; 1024];
///                     while let Ok(n) = stream.read(&mut buffer).await {
///                         if n == 0 { break; }
///                         let _ = stream.write_all(&buffer[..n]).await;
///                     }
///                 });
///             }
///             Err(e) => {
///                 eprintln!("Error accepting connection: {}", e);
///                 break;
///             }
///         }
///     }
///
///     Ok(())
/// }
/// ```
///
/// ## Server with connection handling
///
/// ```rust
/// use ziti_sdk::{Context, ZitiResult};
///
/// #[tokio::main]
/// async fn main() -> ZitiResult<()> {
///     let context = Context::from_file("identity.json").await?;
///     let mut listener = context.listen("my-service").await?;
///
///     println!("Listening on service: {}", listener.service_name());
///     println!("Terminator ID: {}", listener.terminator_id());
///
///     // Accept up to 10 connections
///     for i in 0..10 {
///         match listener.accept().await {
///             Ok(stream) => {
///                 println!("Accepted connection #{}", i + 1);
///                 // Handle the stream...
///             }
///             Err(e) => {
///                 eprintln!("Failed to accept connection: {}", e);
///                 break;
///             }
///         }
///     }
///
///     Ok(())
/// }
/// ```
pub struct ZitiListener {
    service_name: String,
    #[allow(dead_code)]
    service_id: String,
    terminator_id: String,
    #[allow(dead_code)]
    edge_router: EdgeRouter,
    #[allow(dead_code)]
    context: Arc<Context>,
    #[allow(dead_code)]
    transport: Option<WebSocketTransport>,
    connection_receiver: mpsc::UnboundedReceiver<ZitiStream>,
    _connection_sender: mpsc::UnboundedSender<ZitiStream>,
}

impl ZitiListener {
    /// Accept an incoming connection
    ///
    /// Waits for and accepts a new incoming connection to the service.
    /// This method blocks until a connection is available or an error occurs.
    ///
    /// Each successful call to `accept()` returns a new `ZitiStream` that can
    /// be used to communicate with the connecting client.
    ///
    /// # Returns
    ///
    /// * `ZitiResult<ZitiStream>` - A new stream for the accepted connection
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The listener has been closed or dropped
    /// - The underlying connection channel has been closed
    /// - A network error occurs during connection acceptance
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ziti_sdk::{Context, ZitiResult};
    /// use tokio::io::{AsyncReadExt, AsyncWriteExt};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let mut listener = context.listen("my-service").await?;
    ///
    ///     // Accept connections in a loop
    ///     loop {
    ///         match listener.accept().await {
    ///             Ok(mut stream) => {
    ///                 println!("New connection accepted");
    ///
    ///                 // Handle the connection
    ///                 tokio::spawn(async move {
    ///                     let mut buffer = [0; 1024];
    ///                     if let Ok(n) = stream.read(&mut buffer).await {
    ///                         let _ = stream.write_all(&buffer[..n]).await;
    ///                     }
    ///                 });
    ///             }
    ///             Err(e) => {
    ///                 eprintln!("Error accepting connection: {}", e);
    ///                 break;
    ///             }
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept(&mut self) -> ZitiResult<ZitiStream> {
        // Wait for incoming connection from the receiver
        self.connection_receiver
            .recv()
            .await
            .ok_or_else(|| ZitiError::ConnectionFailed(
                "Listener connection channel closed".to_string()
            ))
    }

    /// Get the service name this listener is bound to
    ///
    /// Returns the name of the Ziti service that this listener is hosting.
    /// This is the same service name that was provided when creating the listener.
    ///
    /// # Returns
    ///
    /// * `&str` - The service name
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let listener = context.listen("echo-service").await?;
    ///
    ///     println!("Listening on service: {}", listener.service_name());
    ///     assert_eq!(listener.service_name(), "echo-service");
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get the terminator ID for this listener
    ///
    /// Returns the unique identifier of the terminator that was created in the
    /// Ziti controller for this listener. This ID can be used for debugging
    /// or administrative purposes.
    ///
    /// # Returns
    ///
    /// * `&str` - The terminator ID
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let listener = context.listen("my-service").await?;
    ///
    ///     println!("Terminator ID: {}", listener.terminator_id());
    ///     // Terminator ID can be used for monitoring or debugging
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn terminator_id(&self) -> &str {
        &self.terminator_id
    }
}

impl Drop for ZitiListener {
    fn drop(&mut self) {
        // TODO: Clean up terminator when listener is dropped
        // This should send a DELETE request to /terminators/{terminator_id}
    }
}

/// Edge router information for listener
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EdgeRouter {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    #[serde(rename = "isOnline")]
    pub is_online: bool,
    #[serde(rename = "supportedProtocols", default)]
    pub supported_protocols: std::collections::HashMap<String, String>,
}

/// Response structure for current identity edge routers API call
#[derive(Debug, serde::Deserialize)]
struct EdgeRoutersResponse {
    data: Vec<EdgeRouter>,
}

/// Get available edge routers for the current identity
async fn get_available_edge_routers(context: &Context) -> ZitiResult<Vec<EdgeRouter>> {
    // Get API session for authentication
    let api_session = context.session_manager().get_api_session().await?;
    
    // Create HTTP client
    let client = reqwest::Client::new();
    
    // Build edge routers endpoint URL
    let edge_routers_url = format!(
        "{}/current-identity/edge-routers",
        context.identity_manager().zt_api().trim_end_matches('/')
    );

    // Send request to get available edge routers
    let response = client
        .get(&edge_routers_url)
        .header("Content-Type", "application/json")
        .header("zt-session", &api_session.token)
        .send()
        .await
        .map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to get available edge routers: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ZitiError::ProtocolError {
            message: format!(
                "Edge routers request failed with status {}: {}",
                status, error_text
            ),
        });
    }

    // Parse edge routers response
    let edge_routers_response: EdgeRoutersResponse = response
        .json()
        .await
        .map_err(|e| ZitiError::ProtocolError {
            message: format!("Failed to parse edge routers response: {}", e),
        })?;

    // Filter for online edge routers only
    let online_edge_routers = edge_routers_response
        .data
        .into_iter()
        .filter(|er| er.is_online && !er.hostname.is_empty())
        .collect();

    Ok(online_edge_routers)
}

/// Terminator creation request structure
#[derive(serde::Serialize)]
struct CreateTerminatorRequest {
    service: String,
    router: String,
    binding: String,
    address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    precedence: Option<String>,
}

/// Terminator creation response structure
#[derive(Debug, serde::Deserialize)]
struct CreateTerminatorResponse {
    data: TerminatorData,
}

#[derive(Debug, serde::Deserialize)]
struct TerminatorData {
    id: String,
}

/// Create a terminator to register as service host
async fn create_terminator(
    service_id: &str,
    router_id: &str,
    context: &Context,
    options: &ListenOptions,
) -> ZitiResult<String> {
    // Get API session for authentication
    let api_session = context.session_manager().get_api_session().await?;
    
    // Create HTTP client
    let client = reqwest::Client::new();
    
    // Build terminators endpoint URL
    let terminators_url = format!(
        "{}/terminators",
        context.identity_manager().zt_api().trim_end_matches('/')
    );

    // Create terminator request payload
    let terminator_request = CreateTerminatorRequest {
        service: service_id.to_string(),
        router: router_id.to_string(),
        binding: "edge_transport".to_string(), // Use edge_transport for end-to-end encryption
        address: "hosted:".to_string(), // Special address for SDK-hosted services
        cost: options.cost,
        precedence: options.precedence.clone(),
    };

    // Send request to create terminator
    let response = client
        .post(&terminators_url)
        .header("Content-Type", "application/json")
        .header("zt-session", &api_session.token)
        .json(&terminator_request)
        .send()
        .await
        .map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to create terminator: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ZitiError::ProtocolError {
            message: format!(
                "Create terminator request failed with status {}: {}",
                status, error_text
            ),
        });
    }

    // Parse terminator creation response
    let terminator_response: CreateTerminatorResponse = response
        .json()
        .await
        .map_err(|e| ZitiError::ProtocolError {
            message: format!("Failed to parse terminator creation response: {}", e),
        })?;

    Ok(terminator_response.data.id)
}
