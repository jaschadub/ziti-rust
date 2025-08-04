//! Service discovery and management module
//!
//! Handles service lookup, policy evaluation, and terminator management
//! for Ziti service access control.

pub mod discovery;
pub mod policy;
pub mod terminator;

pub use discovery::{list_services, Service, ServiceDiscovery};
pub use policy::PolicyEngine;
pub use terminator::TerminatorManager;
