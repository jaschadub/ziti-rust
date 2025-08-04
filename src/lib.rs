//! Ziti Rust SDK
//!
//! A high-performance, async-first Rust implementation that provides secure,
//! zero-trust networking capabilities through the OpenZiti platform.
//!
//! # Example
//!
//! ```no_run
//! use ziti_sdk::{Context, ZitiResult};
//!
//! #[tokio::main]
//! async fn main() -> ZitiResult<()> {
//!     let context = Context::from_file("identity.json").await?;
//!     let stream = context.dial("echo-service").await?;
//!     // Use stream for communication
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod connection;
pub mod context;
pub mod error;
pub mod identity;
pub mod service;
pub mod session;
pub mod transport;
pub mod util;

// Re-export primary types
pub use config::{DialOptions, ListenOptions, ZitiConfig};
pub use connection::{dial, listen, listen_with_options, ZitiListener, ZitiStream, EdgeRouter};
pub use context::{Context, ContextBuilder};
pub use error::{ZitiError, ZitiResult};
