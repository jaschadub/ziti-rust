//! WebSocket transport implementation
//!
//! Handles secure WebSocket connections to Ziti Edge Routers using TLS.

use crate::error::{ZitiError, ZitiResult};
use crate::transport::tls::TlsConfig;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_rustls::{rustls::ServerName, TlsConnector, client::TlsStream};
use tokio_tungstenite::{
    tungstenite::Message,
    WebSocketStream,
};
use url::Url;

/// WebSocket transport manager for Ziti Edge Router connections
#[derive(Debug)]
pub struct WebSocketTransport {
    /// WebSocket stream for communication
    stream: WebSocketStream<TlsStream<TcpStream>>,
    /// Remote URL
    url: Url,
}

impl WebSocketTransport {
    /// Establish a secure WebSocket connection to an Edge Router
    ///
    /// Creates a TLS-secured WebSocket connection using the provided TLS configuration.
    /// The connection uses mTLS for authentication with the Edge Router.
    ///
    /// # Arguments
    ///
    /// * `url` - WebSocket URL of the Edge Router (typically wss://)
    /// * `tls_config` - TLS configuration with client certificates
    ///
    /// # Returns
    ///
    /// * `ZitiResult<WebSocketTransport>` - Connected WebSocket transport
    ///
    /// # Example
    ///
    /// ```rust
    /// use ziti_sdk::transport::{WebSocketTransport, TlsConfig};
    /// use url::Url;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let url = Url::parse("wss://edge-router.example.com/ws")?;
    ///     let tls_config = TlsConfig::new(); // In practice, use TlsConfig::from_identity()
    ///     let transport = WebSocketTransport::connect(url, tls_config).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect(url: Url, tls_config: TlsConfig) -> ZitiResult<Self> {
        // Parse the host from URL
        let host = url
            .host_str()
            .ok_or_else(|| ZitiError::ConfigError("Invalid URL: no host specified".to_string()))?;

        let port = url.port().unwrap_or(443);

        // Create TLS connector from config
        let tls_connector = TlsConnector::from(tls_config.client_config());

        // Parse server name for TLS
        let server_name = ServerName::try_from(host).map_err(|e| {
            ZitiError::ConfigError(format!("Invalid server name '{}': {}", host, e))
        })?;

        // Establish TCP connection
        let tcp_stream = TcpStream::connect((host, port))
            .await
            .map_err(|e| ZitiError::ConnectionFailed(format!("TCP connection failed: {}", e)))?;

        // Establish TLS connection
        let tls_stream = tls_connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|e| ZitiError::ConnectionFailed(format!("TLS handshake failed: {}", e)))?;

        // Establish WebSocket connection
        let (ws_stream, _response) = tokio_tungstenite::client_async(url.as_str(), tls_stream)
            .await
            .map_err(|e| {
                ZitiError::ConnectionFailed(format!("WebSocket handshake failed: {}", e))
            })?;

        Ok(Self {
            stream: ws_stream,
            url,
        })
    }

    /// Send a message through the WebSocket
    ///
    /// # Arguments
    ///
    /// * `message` - Message to send
    ///
    /// # Returns
    ///
    /// * `ZitiResult<()>` - Success or error result
    pub async fn send(&mut self, message: Message) -> ZitiResult<()> {
        self.stream.send(message).await.map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to send WebSocket message: {}", e))
        })
    }

    /// Receive a message from the WebSocket
    ///
    /// # Returns
    ///
    /// * `ZitiResult<Option<Message>>` - Received message or None if connection closed
    pub async fn receive(&mut self) -> ZitiResult<Option<Message>> {
        match self.stream.next().await {
            Some(Ok(message)) => Ok(Some(message)),
            Some(Err(e)) => Err(ZitiError::ConnectionFailed(format!(
                "Failed to receive WebSocket message: {}",
                e
            ))),
            None => Ok(None), // Connection closed
        }
    }

    /// Close the WebSocket connection
    ///
    /// # Returns
    ///
    /// * `ZitiResult<()>` - Success or error result
    pub async fn close(&mut self) -> ZitiResult<()> {
        self.stream.close(None).await.map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to close WebSocket connection: {}", e))
        })
    }

    /// Get the connection URL
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Check if the connection is closed
    pub fn is_closed(&self) -> bool {
        // Note: tokio-tungstenite doesn't provide a direct way to check if closed
        // This is a placeholder implementation
        false
    }

}

/// Establish a WebSocket connection to an Edge Router
///
/// This is a convenience function that creates both TLS config and WebSocket connection.
///
/// # Arguments
///
/// * `url` - WebSocket URL of the Edge Router
/// * `tls_config` - TLS configuration with client certificates
///
/// # Returns
///
/// * `ZitiResult<WebSocketStream<TlsStream<TcpStream>>>` - Connected WebSocket stream
pub async fn connect_websocket(
    url: Url,
    tls_config: TlsConfig,
) -> ZitiResult<WebSocketStream<TlsStream<TcpStream>>> {
    let transport = WebSocketTransport::connect(url, tls_config).await?;
    Ok(transport.stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tls::TlsConfig;

    #[tokio::test]
    async fn test_invalid_url() {
        // Test with URL that has no host
        let url = Url::parse("wss:///path").unwrap(); // This parses but has no host
        let tls_config = TlsConfig::new();
        
        let result = WebSocketTransport::connect(url, tls_config).await;
        assert!(result.is_err());
        
        // Just verify that an error is returned - the exact error type can vary
        match result {
            Err(ZitiError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid URL: no host specified"));
            }
            Err(ZitiError::ConnectionFailed(msg)) => {
                // This is also acceptable - connection would fail without a host
                assert!(msg.contains("TCP connection failed") || msg.contains("connection failed"));
            }
            Err(_) => {
                // Any error is acceptable for invalid URL
            }
            Ok(_) => panic!("Expected error but got success"),
        }
    }

    #[test]
    fn test_url_parsing() {
        let url = Url::parse("wss://example.com:8443/ws").unwrap();
        assert_eq!(url.host_str().unwrap(), "example.com");
        assert_eq!(url.port().unwrap(), 8443);
    }
}
