//! Context builder implementation
//!
//! Provides a builder pattern for configuring and creating Context instances.

use super::Context;
use crate::error::{ZitiError, ZitiResult};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Builder for creating Context instances
pub struct ContextBuilder {
    identity_file: Option<PathBuf>,
    connect_timeout: Option<Duration>,
}

impl ContextBuilder {
    /// Create a new ContextBuilder
    pub fn new() -> Self {
        Self {
            identity_file: None,
            connect_timeout: None,
        }
    }

    /// Set the identity file path
    pub fn identity_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.identity_file = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the connection timeout
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Build the Context
    pub async fn build(self) -> ZitiResult<Context> {
        let identity_file = self.identity_file.ok_or_else(|| {
            ZitiError::ConfigError("an identity file path is required to build a Context".to_string())
        })?;

        let mut context = Context::from_file(identity_file).await?;
        if let Some(timeout) = self.connect_timeout {
            context.set_connect_timeout(timeout);
        }

        Ok(context)
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_builder_creation() {
        let builder = ContextBuilder::new();
        assert!(builder.identity_file.is_none());
        assert!(builder.connect_timeout.is_none());
    }

    #[test]
    fn test_context_builder_default() {
        let builder = ContextBuilder::default();
        assert!(builder.identity_file.is_none());
        assert!(builder.connect_timeout.is_none());
    }

    #[test]
    fn test_context_builder_sets_fields() {
        let builder = ContextBuilder::new()
            .identity_file("identity.json")
            .connect_timeout(Duration::from_secs(10));

        assert_eq!(builder.identity_file, Some(PathBuf::from("identity.json")));
        assert_eq!(builder.connect_timeout, Some(Duration::from_secs(10)));
    }

    #[tokio::test]
    async fn test_build_without_identity_file_errors() {
        let result = ContextBuilder::new().build().await;
        assert!(matches!(result, Err(ZitiError::ConfigError(_))));
    }
}
