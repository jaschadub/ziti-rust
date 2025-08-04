//! Network connection module
//!
//! Provides client dial connections, server listeners, and connection streams
//! for Ziti network communication.

pub mod dial;
pub mod listen;
pub mod stream;

// Re-export the main dial and listen functions and types
pub use dial::{dial, ZitiDial, EdgeRouter};
pub use listen::{listen, listen_with_options, ZitiListener};
pub use stream::ZitiStream;
