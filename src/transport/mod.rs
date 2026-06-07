//! Network transport layer module
//!
//! Handles WebSocket transport, TLS connections, and Ziti protocol implementation
//! for secure communication with edge routers.

pub mod http;
pub mod protocol;
pub mod tls;
pub mod websocket;

// Re-export key types and functions for convenient access
pub use protocol::{
    ContentType, MessageHeader, ZitiMessage, ZitiProtocol,
};
pub use tls::{TlsConfig, build_client_config};
pub use websocket::{WebSocketTransport, connect_websocket};

// Additional convenience exports
pub use protocol::ZitiProtocol as Protocol;
pub use tls::TlsConfig as Config;
pub use websocket::WebSocketTransport as Transport;
