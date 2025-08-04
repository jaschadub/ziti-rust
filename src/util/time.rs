//! Timeout handling utilities
//!
//! Provides timeout configuration and handling for async operations.

use std::time::Duration;

/// Timeout configuration for operations
pub struct TimeoutConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Option<Duration>,
    pub write_timeout: Option<Duration>,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: None,
            write_timeout: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.read_timeout, None);
        assert_eq!(config.write_timeout, None);
    }

    #[test]
    fn test_timeout_config_custom() {
        let config = TimeoutConfig {
            connect_timeout: Duration::from_secs(60),
            read_timeout: Some(Duration::from_secs(10)),
            write_timeout: Some(Duration::from_secs(5)),
        };
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        assert_eq!(config.read_timeout, Some(Duration::from_secs(10)));
        assert_eq!(config.write_timeout, Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_timeout_config_partial() {
        let config = TimeoutConfig {
            connect_timeout: Duration::from_millis(5000),
            read_timeout: Some(Duration::from_millis(2000)),
            write_timeout: None,
        };
        assert_eq!(config.connect_timeout, Duration::from_millis(5000));
        assert_eq!(config.read_timeout, Some(Duration::from_millis(2000)));
        assert_eq!(config.write_timeout, None);
    }

    #[test]
    fn test_timeout_config_edge_cases() {
        let config = TimeoutConfig {
            connect_timeout: Duration::from_millis(0),
            read_timeout: Some(Duration::from_millis(0)),
            write_timeout: Some(Duration::from_secs(3600)),
        };
        assert_eq!(config.connect_timeout, Duration::from_millis(0));
        assert_eq!(config.read_timeout, Some(Duration::from_millis(0)));
        assert_eq!(config.write_timeout, Some(Duration::from_secs(3600)));
    }
}
