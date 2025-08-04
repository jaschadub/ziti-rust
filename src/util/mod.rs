//! Utility module
//!
//! Provides common utilities including retry mechanisms and timeout handling
//! for robust network operations.

pub mod retry;
pub mod time;

pub use retry::RetryPolicy;
pub use time::TimeoutConfig;
