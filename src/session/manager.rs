//! Session lifecycle management
//!
//! Manages API and network sessions with automatic renewal.

use super::{api_session, ApiSession, NetworkSession};
use crate::error::{ZitiError, ZitiResult};
use crate::identity::IdentityManager;
use crate::transport::http::controller_client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Response structure for the create-session API call
#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    data: SessionData,
}

#[derive(Debug, Deserialize)]
struct SessionData {
    id: String,
}

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
    /// ```rust,no_run
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
            if let Some(ref session) = *session_guard
                && !session.is_expired()
            {
                return Ok(session.clone());
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
    pub async fn get_network_session(&self, service_id: &str) -> ZitiResult<NetworkSession> {
        // Ensure we have a valid API session to authorize the request.
        let api_session = self.get_api_session().await?;

        let client = controller_client(&self.identity_manager, Duration::from_secs(30)).await?;

        let sessions_url = format!(
            "{}/sessions",
            self.identity_manager.zt_api().trim_end_matches('/')
        );

        let request_body = serde_json::json!({
            "serviceId": service_id,
            "type": "Dial",
        });

        let response = client
            .post(&sessions_url)
            .header("Content-Type", "application/json")
            .header("zt-session", &api_session.token)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                ZitiError::ConnectionFailed(format!("Failed to create network session: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ZitiError::ProtocolError {
                message: format!(
                    "Network session request failed with status {}: {}",
                    status, error_text
                ),
            });
        }

        let session_response: CreateSessionResponse =
            response.json().await.map_err(|e| ZitiError::ProtocolError {
                message: format!("Failed to parse network session response: {}", e),
            })?;

        Ok(NetworkSession::new(
            session_response.data.id,
            service_id.to_string(),
        ))
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

#[cfg(test)]
mod tests {
    // Note: comprehensive tests require a controller and proper identity setup,
    // which is exercised by the integration tests rather than here.
}
