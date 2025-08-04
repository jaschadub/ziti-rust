//! Retry mechanism implementation
//!
//! Provides configurable retry policies for network operations.

use std::time::Duration;

/// Retry policy configuration
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_retry_policy_custom() {
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_delay: Duration::from_secs(1),
        };
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_policy_edge_cases() {
        let policy = RetryPolicy {
            max_attempts: 0,
            initial_delay: Duration::from_millis(0),
        };
        assert_eq!(policy.max_attempts, 0);
        assert_eq!(policy.initial_delay, Duration::from_millis(0));

        let policy = RetryPolicy {
            max_attempts: u32::MAX,
            initial_delay: Duration::from_secs(3600),
        };
        assert_eq!(policy.max_attempts, u32::MAX);
        assert_eq!(policy.initial_delay, Duration::from_secs(3600));
    }
}
