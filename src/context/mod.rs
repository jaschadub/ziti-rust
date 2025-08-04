//! Context module for the Ziti SDK
//!
//! Provides the main Context interface and builder pattern for configuring
//! and creating Ziti SDK instances.

pub mod builder;
#[allow(clippy::module_inception)]
pub mod context;

pub use builder::ContextBuilder;
pub use context::Context;
