//! Session management module
//!
//! Handles API sessions, network sessions, and session lifecycle management
//! including automatic renewal and background tasks.

pub mod api_session;
pub mod manager;
pub mod network_session;

pub use api_session::{authenticate, ApiSession};
pub use manager::SessionManager;
pub use network_session::NetworkSession;
