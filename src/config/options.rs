//! Dial and Listen options
//!
//! Configuration options for dial and listen operations.

use std::time::Duration;

/// Options for dial operations
///
/// `DialOptions` allows customization of outbound connection behavior
/// when establishing connections to Ziti services. These options provide
/// fine-grained control over connection parameters.
///
/// # Examples
///
/// ## Basic usage with default options
///
/// ```rust
/// use ziti_sdk::DialOptions;
///
/// let options = DialOptions::default();
/// // Uses default timeout and identity settings
/// ```
///
/// ## Custom timeout configuration
///
/// ```rust
/// use ziti_sdk::DialOptions;
/// use std::time::Duration;
///
/// let options = DialOptions {
///     timeout: Some(Duration::from_secs(30)),
///     identity: None,
/// };
/// ```
///
/// ## Using specific identity
///
/// ```rust
/// use ziti_sdk::DialOptions;
/// use std::time::Duration;
///
/// let options = DialOptions {
///     timeout: Some(Duration::from_secs(60)),
///     identity: Some("client-identity".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct DialOptions {
    /// Connection timeout duration
    ///
    /// Specifies the maximum time to wait for a connection to be established.
    /// If `None`, the SDK will use its default timeout value.
    pub timeout: Option<Duration>,
    
    /// Specific identity to use for the connection
    ///
    /// If specified, this identity will be used instead of the default identity
    /// from the context. This is useful when multiple identities are available.
    pub identity: Option<String>,
}

/// Options for listen operations
///
/// `ListenOptions` allows customization of service hosting behavior
/// when creating listeners for Ziti services. These options control
/// how the terminator is registered with the Ziti controller.
///
/// # Examples
///
/// ## Basic usage with default options
///
/// ```rust
/// use ziti_sdk::ListenOptions;
///
/// let options = ListenOptions::default();
/// // Uses default cost, precedence, and identity settings
/// ```
///
/// ## High-priority service with custom cost
///
/// ```rust
/// use ziti_sdk::ListenOptions;
///
/// let options = ListenOptions {
///     identity: None,
///     cost: Some(50),  // Lower cost = higher preference
///     precedence: Some("high".to_string()),
/// };
/// ```
///
/// ## Service with specific identity and precedence
///
/// ```rust
/// use ziti_sdk::ListenOptions;
///
/// let options = ListenOptions {
///     identity: Some("server-identity".to_string()),
///     cost: Some(100),
///     precedence: Some("required".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct ListenOptions {
    /// Specific identity to use for hosting
    ///
    /// If specified, this identity will be used instead of the default identity
    /// from the context when creating the terminator.
    pub identity: Option<String>,
    
    /// Cost value for the terminator
    ///
    /// Lower cost values indicate higher preference when multiple terminators
    /// are available for a service. If `None`, the controller will assign
    /// a default cost value.
    pub cost: Option<u16>,
    
    /// Precedence requirement for the terminator
    ///
    /// Determines the routing precedence for this terminator. Common values
    /// include "default", "required", "high", etc. If `None`, the controller
    /// will use the default precedence.
    pub precedence: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_dial_options_default() {
        let options = DialOptions::default();
        assert_eq!(options.timeout, None);
        assert_eq!(options.identity, None);
    }

    #[test]
    fn test_dial_options_custom() {
        let options = DialOptions {
            timeout: Some(Duration::from_secs(60)),
            identity: Some("my-identity".to_string()),
        };
        assert_eq!(options.timeout, Some(Duration::from_secs(60)));
        assert_eq!(options.identity, Some("my-identity".to_string()));
    }

    #[test]
    fn test_dial_options_partial() {
        let options = DialOptions {
            timeout: Some(Duration::from_millis(5000)),
            identity: None,
        };
        assert_eq!(options.timeout, Some(Duration::from_millis(5000)));
        assert_eq!(options.identity, None);
    }

    #[test]
    fn test_listen_options_default() {
        let options = ListenOptions::default();
        assert_eq!(options.identity, None);
        assert_eq!(options.cost, None);
        assert_eq!(options.precedence, None);
    }

    #[test]
    fn test_listen_options_custom() {
        let options = ListenOptions {
            identity: Some("server-identity".to_string()),
            cost: Some(100),
            precedence: Some("high".to_string()),
        };
        assert_eq!(options.identity, Some("server-identity".to_string()));
        assert_eq!(options.cost, Some(100));
        assert_eq!(options.precedence, Some("high".to_string()));
    }

    #[test]
    fn test_listen_options_partial() {
        let options = ListenOptions {
            identity: None,
            cost: Some(50),
            precedence: None,
        };
        assert_eq!(options.identity, None);
        assert_eq!(options.cost, Some(50));
        assert_eq!(options.precedence, None);
    }

    #[test]
    fn test_dial_options_clone() {
        let original = DialOptions {
            timeout: Some(Duration::from_secs(30)),
            identity: Some("test".to_string()),
        };
        let cloned = original.clone();
        assert_eq!(cloned.timeout, original.timeout);
        assert_eq!(cloned.identity, original.identity);
    }

    #[test]
    fn test_listen_options_clone() {
        let original = ListenOptions {
            identity: Some("test".to_string()),
            cost: Some(200),
            precedence: Some("normal".to_string()),
        };
        let cloned = original.clone();
        assert_eq!(cloned.identity, original.identity);
        assert_eq!(cloned.cost, original.cost);
        assert_eq!(cloned.precedence, original.precedence);
    }
}
