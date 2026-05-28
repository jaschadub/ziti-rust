//! Error types for the Ziti SDK
//!
//! Defines comprehensive error types with proper categorization and recovery hints.

use std::time::Duration;
use thiserror::Error;

/// Result type for Ziti operations
pub type ZitiResult<T> = Result<T, ZitiError>;

/// Comprehensive error types for Ziti SDK operations
///
/// `ZitiError` provides a detailed categorization of errors that can occur
/// during Ziti SDK operations. Each error variant includes specific information
/// about the failure and provides methods to determine appropriate recovery strategies.
///
/// # Error Categories
///
/// - **Network Errors**: Connection failures, timeouts
/// - **Authentication Errors**: Identity verification, session management
/// - **Service Errors**: Service discovery, access control
/// - **Configuration Errors**: Invalid settings, malformed data
/// - **Protocol Errors**: Ziti protocol violations, unexpected responses
///
/// # Examples
///
/// ## Basic error handling
///
/// ```rust,no_run
/// use ziti_sdk::{Context, ZitiError, ZitiResult};
///
/// #[tokio::main]
/// async fn main() -> ZitiResult<()> {
///     let context = Context::from_file("identity.json").await?;
///
///     match context.dial("nonexistent-service").await {
///         Ok(stream) => {
///             // Handle successful connection
///         }
///         Err(ZitiError::ServiceNotFound { service_name }) => {
///             eprintln!("Service '{}' not found", service_name);
///         }
///         Err(ZitiError::ConnectionFailed(reason)) => {
///             eprintln!("Connection failed: {}", reason);
///         }
///         Err(e) => {
///             eprintln!("Other error: {}", e);
///         }
///     }
///
///     Ok(())
/// }
/// ```
///
/// ## Error recovery strategies
///
/// ```rust
/// use ziti_sdk::{ZitiError, ZitiResult};
///
/// fn handle_error(error: &ZitiError) {
///     if error.is_recoverable() {
///         println!("Error can be retried: {}", error);
///     } else if error.requires_session_renewal() {
///         println!("Session needs renewal: {}", error);
///     } else if error.is_configuration_error() {
///         println!("Check configuration: {}", error);
///     } else {
///         println!("Permanent error: {}", error);
///     }
/// }
/// ```
#[derive(Error, Debug)]
pub enum ZitiError {
    /// Network connection failure
    ///
    /// Indicates that a network connection could not be established or was lost.
    /// This is typically a recoverable error that may succeed on retry.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Connection timeout
    ///
    /// Indicates that a connection attempt timed out after the specified duration.
    /// This is typically a recoverable error that may succeed on retry.
    #[error("Connection timeout after {timeout:?}")]
    ConnectionTimeout {
        /// The timeout duration that was exceeded
        timeout: Duration
    },

    /// Authentication failure
    ///
    /// Indicates that identity authentication failed. This usually means
    /// invalid credentials or expired certificates.
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed {
        /// The specific reason for authentication failure
        reason: String
    },

    /// API session expired
    ///
    /// Indicates that the current API session has expired and needs to be renewed.
    /// The SDK typically handles this automatically, but applications may need
    /// to handle this for long-running operations.
    #[error("API session expired")]
    ApiSessionExpired,

    /// Service not found
    ///
    /// Indicates that the requested service was not found or is not accessible
    /// with the current identity's permissions.
    #[error("Service not found: {service_name}")]
    ServiceNotFound {
        /// The name of the service that was not found
        service_name: String
    },

    /// Service access denied
    ///
    /// Indicates that access to the requested service was denied due to
    /// insufficient permissions or policy restrictions.
    #[error("Service access denied: {service_name}")]
    ServiceAccessDenied {
        /// The name of the service that denied access
        service_name: String
    },

    /// Configuration error
    ///
    /// Indicates an error in configuration data, such as invalid JSON,
    /// missing required fields, or malformed identity files.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Protocol error
    ///
    /// Indicates a violation of the Ziti protocol, such as unexpected
    /// message types or malformed protocol data.
    #[error("Protocol error: {message}")]
    ProtocolError {
        /// Description of the protocol error
        message: String
    },
}

impl ZitiError {
    /// Returns true if this is a recoverable error that should trigger retry
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ZitiError::ConnectionFailed(_) | ZitiError::ConnectionTimeout { .. }
        )
    }

    /// Returns true if this error should trigger session renewal
    pub fn requires_session_renewal(&self) -> bool {
        matches!(self, ZitiError::ApiSessionExpired)
    }

    /// Returns true if this is a configuration or setup error
    pub fn is_configuration_error(&self) -> bool {
        matches!(self, ZitiError::ConfigError(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_connection_failed_error() {
        let error = ZitiError::ConnectionFailed("Network unreachable".to_string());
        assert_eq!(error.to_string(), "Connection failed: Network unreachable");
        assert!(error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_connection_timeout_error() {
        let timeout = Duration::from_secs(30);
        let error = ZitiError::ConnectionTimeout { timeout };
        assert_eq!(error.to_string(), "Connection timeout after 30s");
        assert!(error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_authentication_failed_error() {
        let error = ZitiError::AuthenticationFailed {
            reason: "Invalid credentials".to_string(),
        };
        assert_eq!(error.to_string(), "Authentication failed: Invalid credentials");
        assert!(!error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_api_session_expired_error() {
        let error = ZitiError::ApiSessionExpired;
        assert_eq!(error.to_string(), "API session expired");
        assert!(!error.is_recoverable());
        assert!(error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_service_not_found_error() {
        let error = ZitiError::ServiceNotFound {
            service_name: "my-service".to_string(),
        };
        assert_eq!(error.to_string(), "Service not found: my-service");
        assert!(!error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_service_access_denied_error() {
        let error = ZitiError::ServiceAccessDenied {
            service_name: "secure-service".to_string(),
        };
        assert_eq!(error.to_string(), "Service access denied: secure-service");
        assert!(!error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_config_error() {
        let error = ZitiError::ConfigError("Invalid JSON format".to_string());
        assert_eq!(error.to_string(), "Configuration error: Invalid JSON format");
        assert!(!error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(error.is_configuration_error());
    }

    #[test]
    fn test_protocol_error() {
        let error = ZitiError::ProtocolError {
            message: "Unexpected message type".to_string(),
        };
        assert_eq!(error.to_string(), "Protocol error: Unexpected message type");
        assert!(!error.is_recoverable());
        assert!(!error.requires_session_renewal());
        assert!(!error.is_configuration_error());
    }

    #[test]
    fn test_ziti_result_type() {
        let success: ZitiResult<i32> = Ok(42);
        assert!(success.is_ok());
        assert_eq!(42, 42);

        let failure_error = ZitiError::ApiSessionExpired;
        assert!(failure_error.requires_session_renewal());
        
        let failure: ZitiResult<i32> = Err(failure_error);
        assert!(failure.is_err());
    }
}
