//! Ziti dial functionality
//!
//! Provides the main dial function for establishing connections to Ziti services.

use crate::context::Context;
use crate::connection::ZitiStream;
use crate::error::{ZitiError, ZitiResult};
use crate::service::list_services;
use crate::transport::{TlsConfig, WebSocketTransport};
use tokio_tungstenite::tungstenite::Message;
use url::Url;

/// Dial a Ziti service by name
///
/// This function orchestrates the complete process of connecting to a Ziti service:
/// 1. Look up service details using the service discovery API
/// 2. Request network session and obtain edge router information
/// 3. Choose an edge router and establish WebSocket connection
/// 4. Perform Ziti protocol handshake
/// 5. Return a ZitiStream for communication
///
/// # Arguments
///
/// * `service_name` - The name of the service to connect to
/// * `context` - The Ziti context containing identity and session managers
///
/// # Returns
///
/// * `ZitiResult<ZitiStream>` - A stream for communicating with the service
///
/// # Example
///
/// ```rust
/// use ziti_sdk::{Context, connection::dial};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let context = Context::from_file("identity.json").await?;
///     let stream = dial("echo-service", &context).await?;
///     // Use stream for communication
///     Ok(())
/// }
/// ```
pub async fn dial(service_name: &str, context: &Context) -> ZitiResult<ZitiStream> {
    // Step 1: Find the service by name using list_services
    let services = list_services(context.session_manager()).await?;
    let service = services
        .iter()
        .find(|s| s.name == service_name)
        .ok_or_else(|| ZitiError::ServiceNotFound {
            service_name: service_name.to_string(),
        })?;

    // Step 2: Request network session and get edge router information
    let edge_routers = get_service_terminators(&service.id, context).await?;
    
    if edge_routers.is_empty() {
        return Err(ZitiError::ConnectionFailed(
            format!("No edge routers available for service '{}'", service_name)
        ));
    }

    // Step 3: Choose an edge router (for now, just pick the first one)
    let edge_router = &edge_routers[0];
    
    // Step 4: Create TLS config from identity
    let tls_config = TlsConfig::from_identity(context.identity_manager())?;
    
    // Step 5: Establish WebSocket connection to edge router
    let ws_url = Url::parse(&format!("wss://{}:{}/ws", edge_router.hostname, edge_router.port))
        .map_err(|e| ZitiError::ConfigError(format!("Invalid edge router URL: {}", e)))?;
    
    let mut transport = WebSocketTransport::connect(ws_url, tls_config).await?;
    
    // Step 6: Perform Ziti connection handshake
    perform_ziti_handshake(&mut transport, &service.id).await?;
    
    // Step 7: Return ZitiStream wrapping the transport
    Ok(ZitiStream::from_transport(transport))
}

/// Edge router information
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EdgeRouter {
    pub hostname: String,
    pub port: u16,
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

/// Get terminators (edge routers) for a service
async fn get_service_terminators(service_id: &str, context: &Context) -> ZitiResult<Vec<EdgeRouter>> {
    // Get API session for authentication
    let api_session = context.session_manager().get_api_session().await?;
    
    // Create HTTP client
    let client = reqwest::Client::new();
    
    // Build terminators endpoint URL
    let terminators_url = format!(
        "{}/services/{}/terminators",
        context.identity_manager().zt_api().trim_end_matches('/'),
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
    transport: &mut WebSocketTransport,
    service_id: &str,
) -> ZitiResult<()> {
    // Create Hello message for Ziti handshake
    let hello_msg = create_hello_message(service_id)?;
    
    // Send Hello message
    transport.send(Message::Binary(hello_msg)).await?;
    
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
    if let Some(status) = response.get("status") {
        if status == "ok" || status == "success" {
            return Ok(());
        }
    }
    
    // If we get here, the handshake failed
    Err(ZitiError::ProtocolError {
        message: format!("Hello handshake failed: {:?}", response),
    })
}

/// Placeholder struct for dial functionality
pub struct ZitiDial;

impl ZitiDial {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ZitiDial {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_router_deserialization() {
        let json = r#"{
            "hostname": "edge-router.example.com",
            "port": 443,
            "supported_protocols": ["tls", "ws"]
        }"#;
        
        let edge_router: EdgeRouter = serde_json::from_str(json).unwrap();
        assert_eq!(edge_router.hostname, "edge-router.example.com");
        assert_eq!(edge_router.port, 443);
        assert_eq!(edge_router.supported_protocols, vec!["tls", "ws"]);
    }

    #[test]
    fn test_hello_message_creation() {
        let message = create_hello_message("test-service").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&message).unwrap();
        
        assert_eq!(parsed["type"], "Hello");
        assert_eq!(parsed["service_id"], "test-service");
        assert_eq!(parsed["version"], "1.0");
    }
}
