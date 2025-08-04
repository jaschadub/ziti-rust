//! Session lifecycle management
//!
//! Manages API and network sessions with automatic renewal.

use super::{api_session, ApiSession, NetworkSession};
use crate::error::ZitiResult;
use crate::identity::IdentityManager;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session manager for handling session lifecycle
#[derive(Clone)]
pub struct SessionManager {
    identity_manager: Arc<IdentityManager>,
    current_api_session: Arc<RwLock<Option<ApiSession>>>,
}

impl SessionManager {
    /// Create a new SessionManager with the provided identity manager
    pub fn new(identity_manager: IdentityManager) -> Self {
        Self {
            identity_manager: Arc::new(identity_manager),
            current_api_session: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid API session, authenticating if necessary
    ///
    /// This method checks if there's a current valid session, and if not,
    /// it will authenticate with the controller to obtain a new one.
    ///
    /// # Returns
    ///
    /// * `ZitiResult<ApiSession>` - A valid API session
    ///
    /// # Example
    ///
    /// ```rust
    /// use ziti_sdk::session::SessionManager;
    /// use ziti_sdk::identity::load_from_file;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let identity = load_from_file("identity.json").await?;
    ///     let session_manager = SessionManager::new(identity);
    ///     let api_session = session_manager.get_api_session().await?;
    ///     println!("Got API session: {}", api_session.id);
    ///     Ok(())
    /// }
    /// ```
    pub async fn get_api_session(&self) -> ZitiResult<ApiSession> {
        // Check if we have a current valid session
        {
            let session_guard = self.current_api_session.read().await;
            if let Some(ref session) = *session_guard {
                if !session.is_expired() {
                    return Ok(session.clone());
                }
            }
        }

        // Session is expired or doesn't exist, authenticate
        let new_session = api_session::authenticate(&self.identity_manager).await?;

        // Store the new session
        {
            let mut session_guard = self.current_api_session.write().await;
            *session_guard = Some(new_session.clone());
        }

        Ok(new_session)
    }

    /// Get a network session for a specific service
    ///
    /// This method will obtain an API session first (if needed), then request
    /// a network session for the specified service.
    ///
    /// # Arguments
    ///
    /// * `service_id` - The ID of the service to get a session for
    ///
    /// # Returns
    ///
    /// * `ZitiResult<NetworkSession>` - A network session for the service
    ///
    /// # Note
    ///
    /// This is a placeholder implementation. The actual network session
    /// functionality will be implemented in a future task.
    pub async fn get_network_session(&self, _service_id: &str) -> ZitiResult<NetworkSession> {
        // For now, ensure we have a valid API session
        let _api_session = self.get_api_session().await?;

        // TODO: Implement network session creation using the API session
        todo!("Network session implementation will be added in a future task")
    }

    /// Clear the current API session
    ///
    /// This method clears the stored API session, forcing the next call to
    /// `get_api_session()` to authenticate again.
    pub async fn clear_api_session(&self) {
        let mut session_guard = self.current_api_session.write().await;
        *session_guard = None;
    }

    /// Check if the current API session is valid
    ///
    /// # Returns
    ///
    /// * `bool` - True if there's a valid (non-expired) API session
    pub async fn has_valid_api_session(&self) -> bool {
        let session_guard = self.current_api_session.read().await;
        if let Some(ref session) = *session_guard {
            !session.is_expired()
        } else {
            false
        }
    }

    /// Get the identity manager associated with this session manager
    pub fn identity_manager(&self) -> &IdentityManager {
        &self.identity_manager
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        // Note: This will not be very useful since we need an IdentityManager
        // This is mainly here to satisfy trait bounds if needed
        panic!("SessionManager requires an IdentityManager and cannot be created with default()")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_default_panic() {
        // Test that default() properly panics
        let result = std::panic::catch_unwind(SessionManager::default);
        assert!(result.is_err());
    }

    // Note: More comprehensive tests would require proper identity setup
    // which is beyond the scope of this initial implementation
}
