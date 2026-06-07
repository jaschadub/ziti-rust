//! ZitiListener implementation
//!
//! Provides listener functionality for accepting incoming Ziti connections.

use super::stream::OutboundFrame;
use super::ZitiStream;
use crate::config::ListenOptions;
use crate::context::Context;
use crate::error::{ZitiError, ZitiResult};
use crate::service::list_services;
use crate::transport::http::controller_client;
use crate::transport::protocol::{ContentType, ZitiMessage};
use crate::transport::{TlsConfig, WebSocketTransport};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
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
/// ```rust,no_run
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
/// ```rust,no_run
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
    
    let mut transport = WebSocketTransport::connect(ws_url, tls_config).await?;

    // Step 6: Perform the bind handshake so the edge router routes connections
    // for this terminator to us.
    perform_listen_handshake(&mut transport, &service.id, &terminator_id).await?;

    // Step 7: Hand the bound WS to a demux task and return the listener.
    let (accept_tx, accept_rx) = mpsc::unbounded_channel();
    let demux_task = tokio::spawn(run_demux(transport, accept_tx));

    let listener = ZitiListener {
        service_name: service_name.to_string(),
        service_id: service.id.clone(),
        terminator_id,
        edge_router: edge_router.clone(),
        context: Arc::new(context.clone()),
        accept_rx,
        demux_task: Some(demux_task),
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
/// ```rust,no_run
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
/// ```rust,no_run
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
    context: Arc<Context>,
    accept_rx: mpsc::UnboundedReceiver<ZitiStream>,
    demux_task: Option<JoinHandle<()>>,
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
    /// ```rust,no_run
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
        self.accept_rx.recv().await.ok_or_else(|| {
            ZitiError::ConnectionFailed(
                "Listener accept channel closed (demux task exited)".to_string(),
            )
        })
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
    /// ```rust,no_run
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
    /// ```rust,no_run
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
        // Stop the demux task so the WS closes and any in-flight accept()s
        // observe the channel close.
        if let Some(handle) = self.demux_task.take() {
            handle.abort();
        }

        if self.terminator_id.is_empty() {
            return;
        }

        // Deleting the terminator requires an async HTTP request, which can't be
        // awaited in Drop. Spawn a best-effort cleanup task if a runtime is
        // available (the common case, since the listener was created in one).
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let context = self.context.clone();
            let terminator_id = self.terminator_id.clone();
            handle.spawn(async move {
                let _ = delete_terminator(&context, &terminator_id).await;
            });
        }
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

    let client = controller_client(context.identity_manager(), context.connect_timeout()).await?;

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

    let client = controller_client(context.identity_manager(), context.connect_timeout()).await?;

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

/// Perform the listen (bind) handshake with the edge router using the
/// binary [`ZitiMessage`] framing.
///
/// Announces to the edge router that this connection hosts the given service via
/// the created terminator, so inbound connections are routed here.
async fn perform_listen_handshake(
    transport: &mut WebSocketTransport,
    service_id: &str,
    terminator_id: &str,
) -> ZitiResult<()> {
    let mut bind = ZitiMessage::new(ContentType::Bind, 1, Bytes::new());
    bind.header
        .add_header("service_id".to_string(), service_id.to_string());
    bind.header
        .add_header("terminator_id".to_string(), terminator_id.to_string());
    bind.header
        .add_header("version".to_string(), "1.0".to_string());

    transport.send(Message::Binary(bind.serialize()?)).await?;

    match transport.receive().await? {
        Some(Message::Binary(data)) => check_response_status(
            ZitiMessage::deserialize(Bytes::copy_from_slice(&data))?,
            "bind",
        ),
        Some(_) => Err(ZitiError::ProtocolError {
            message: "Unexpected message type in bind handshake response".to_string(),
        }),
        None => Err(ZitiError::ConnectionFailed(
            "Connection closed during bind handshake".to_string(),
        )),
    }
}

/// Read the `status`/`error` headers from a handshake response.
pub(crate) fn check_response_status(msg: ZitiMessage, op: &str) -> ZitiResult<()> {
    match msg.header.get_header("status").map(String::as_str) {
        Some("ok") | Some("success") => Ok(()),
        Some(other) => {
            let detail = msg
                .header
                .get_header("error")
                .cloned()
                .unwrap_or_default();
            Err(ZitiError::ProtocolError {
                message: format!("{} handshake failed: status={} error={}", op, other, detail),
            })
        }
        None => Err(ZitiError::ProtocolError {
            message: format!("{} handshake response missing status header", op),
        }),
    }
}

/// What the demuxer should do with an inbound frame from the edge router.
#[derive(Debug)]
pub(crate) enum DemuxAction {
    /// A new inbound connection request. The demuxer accepts it by
    /// creating a stream and replying with `DialResponse(status=ok)`.
    NewInbound { conn_id: u32 },
    /// Payload bytes for an existing connection.
    Data { conn_id: u32, payload: Bytes },
    /// Peer closed a connection.
    Close { conn_id: u32 },
    /// Frame is well-formed but not actionable (ping, error, etc.).
    Ignore,
}

/// Decide what to do with a single inbound `ZitiMessage`.
pub(crate) fn classify_frame(msg: ZitiMessage) -> ZitiResult<DemuxAction> {
    let conn_id = match msg.header.get_header("conn_id") {
        Some(s) => s.parse::<u32>().map_err(|e| ZitiError::ProtocolError {
            message: format!("Invalid conn_id header '{}': {}", s, e),
        })?,
        None => {
            // Hello / Bind responses and pings have no conn_id.
            return Ok(DemuxAction::Ignore);
        }
    };

    match msg.content_type() {
        ContentType::Dial => Ok(DemuxAction::NewInbound { conn_id }),
        ContentType::Data => Ok(DemuxAction::Data {
            conn_id,
            payload: msg.payload.clone(),
        }),
        ContentType::Close => Ok(DemuxAction::Close { conn_id }),
        _ => Ok(DemuxAction::Ignore),
    }
}

/// Listener demux task: owns the bound WebSocket, fans inbound frames out
/// to per-conn channels, and serializes outbound frames from every
/// accepted [`ZitiStream`] back onto the same WebSocket.
async fn run_demux(
    mut transport: WebSocketTransport,
    accept_tx: mpsc::UnboundedSender<ZitiStream>,
) {
    let mut conns: HashMap<u32, mpsc::UnboundedSender<Bytes>> = HashMap::new();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<OutboundFrame>();
    let mut accept_seq: u32 = 1;

    loop {
        tokio::select! {
            // Outbound: a stream wants to send a Data or Close frame.
            maybe_out = out_rx.recv() => {
                let Some(frame) = maybe_out else { break };
                if frame.is_close {
                    conns.remove(&frame.conn_id);
                }
                if transport
                    .stream_mut()
                    .send(Message::Binary(frame.bytes))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            // Inbound: read the next frame from the edge router.
            maybe_in = transport.stream_mut().next() => {
                let Some(item) = maybe_in else { break };
                let msg = match item {
                    Ok(Message::Binary(data)) => {
                        match ZitiMessage::deserialize(Bytes::copy_from_slice(&data)) {
                            Ok(m) => m,
                            Err(_) => continue,
                        }
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                    Ok(Message::Text(_)) => continue,
                    Ok(Message::Close(_)) | Err(_) => break,
                };

                match classify_frame(msg) {
                    Ok(DemuxAction::NewInbound { conn_id }) => {
                        let (in_tx, in_rx) = mpsc::unbounded_channel();
                        conns.insert(conn_id, in_tx);

                        let stream = ZitiStream::from_channels(
                            conn_id,
                            in_rx,
                            out_tx.clone(),
                        );

                        // Accept the dial back to the edge router before
                        // surfacing the stream so the peer can start sending.
                        let mut resp = ZitiMessage::new(
                            ContentType::DialResponse,
                            accept_seq,
                            Bytes::new(),
                        );
                        accept_seq = accept_seq.wrapping_add(1).max(1);
                        resp.header
                            .add_header("conn_id".to_string(), conn_id.to_string());
                        resp.header
                            .add_header("status".to_string(), "ok".to_string());

                        if let Ok(serialized) = resp.serialize()
                            && transport
                                .stream_mut()
                                .send(Message::Binary(serialized))
                                .await
                                .is_err()
                        {
                            break;
                        }

                        if accept_tx.send(stream).is_err() {
                            break;
                        }
                    }
                    Ok(DemuxAction::Data { conn_id, payload }) => {
                        if let Some(tx) = conns.get(&conn_id)
                            && tx.send(payload).is_err()
                        {
                            conns.remove(&conn_id);
                        }
                    }
                    Ok(DemuxAction::Close { conn_id }) => {
                        conns.remove(&conn_id);
                    }
                    Ok(DemuxAction::Ignore) | Err(_) => {}
                }
            }
        }
    }

    // Closing the WS drops all per-conn senders, signaling EOF to every
    // outstanding ZitiStream. Best-effort flush.
    let _ = transport.close().await;
}

/// Delete a terminator from the controller (best-effort listener cleanup).
async fn delete_terminator(context: &Context, terminator_id: &str) -> ZitiResult<()> {
    let api_session = context.session_manager().get_api_session().await?;

    let client = controller_client(context.identity_manager(), context.connect_timeout()).await?;

    let url = format!(
        "{}/terminators/{}",
        context.identity_manager().zt_api().trim_end_matches('/'),
        terminator_id
    );

    let response = client
        .delete(&url)
        .header("zt-session", &api_session.token)
        .send()
        .await
        .map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to delete terminator: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ZitiError::ProtocolError {
            message: format!(
                "Delete terminator request failed with status {}: {}",
                status, error_text
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ct: ContentType, conn_id: Option<u32>, payload: &[u8]) -> ZitiMessage {
        let mut msg = ZitiMessage::new(ct, 1, Bytes::copy_from_slice(payload));
        if let Some(cid) = conn_id {
            msg.header
                .add_header("conn_id".to_string(), cid.to_string());
        }
        msg
    }

    #[test]
    fn classify_dial_returns_new_inbound() {
        let action = classify_frame(frame(ContentType::Dial, Some(7), &[])).unwrap();
        assert!(matches!(action, DemuxAction::NewInbound { conn_id: 7 }));
    }

    #[test]
    fn classify_data_routes_payload() {
        let action = classify_frame(frame(ContentType::Data, Some(3), b"hi")).unwrap();
        match action {
            DemuxAction::Data { conn_id, payload } => {
                assert_eq!(conn_id, 3);
                assert_eq!(payload.as_ref(), b"hi");
            }
            other => panic!("expected Data, got {:?}", other),
        }
    }

    #[test]
    fn classify_close_returns_close() {
        let action = classify_frame(frame(ContentType::Close, Some(11), &[])).unwrap();
        assert!(matches!(action, DemuxAction::Close { conn_id: 11 }));
    }

    #[test]
    fn classify_missing_conn_id_ignores() {
        let action = classify_frame(frame(ContentType::Data, None, b"x")).unwrap();
        assert!(matches!(action, DemuxAction::Ignore));
    }

    #[test]
    fn classify_invalid_conn_id_errors() {
        let mut msg = ZitiMessage::new(ContentType::Data, 1, Bytes::new());
        msg.header
            .add_header("conn_id".to_string(), "not-a-number".to_string());
        assert!(classify_frame(msg).is_err());
    }

    #[test]
    fn classify_unknown_content_type_ignores() {
        let action = classify_frame(frame(ContentType::Ping, Some(1), &[])).unwrap();
        assert!(matches!(action, DemuxAction::Ignore));
    }

    #[test]
    fn check_response_status_ok() {
        let mut msg = ZitiMessage::new(ContentType::Hello, 1, Bytes::new());
        msg.header
            .add_header("status".to_string(), "ok".to_string());
        assert!(check_response_status(msg, "test").is_ok());
    }

    #[test]
    fn check_response_status_carries_error_detail() {
        let mut msg = ZitiMessage::new(ContentType::Hello, 1, Bytes::new());
        msg.header
            .add_header("status".to_string(), "error".to_string());
        msg.header
            .add_header("error".to_string(), "denied".to_string());
        let err = check_response_status(msg, "bind").unwrap_err();
        let s = format!("{}", err);
        assert!(s.contains("denied"), "got: {}", s);
        assert!(s.contains("bind"), "got: {}", s);
    }
}
