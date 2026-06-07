//! Service discovery implementation
//!
//! Handles service lookup and discovery operations for querying
//! the Ziti controller for available services.

use crate::error::{ZitiError, ZitiResult};
use crate::session::SessionManager;
use crate::transport::http::controller_client;
use serde::Deserialize;
use std::time::Duration;

/// Represents a Ziti service returned from the controller
#[derive(Debug, Clone, Deserialize)]
pub struct Service {
    /// Unique identifier for the service
    pub id: String,
    /// Human-readable name of the service
    pub name: String,
    /// Service configuration type references
    #[serde(rename = "configs", default)]
    pub configs: Vec<String>,
    /// Service configuration
    #[serde(rename = "config", default)]
    pub config: serde_json::Value,
    /// Encryption required flag
    #[serde(rename = "encryptionRequired", default)]
    pub encryption_required: bool,
    /// Service permissions
    #[serde(rename = "permissions", default)]
    pub permissions: Vec<String>,
    /// Service tags
    #[serde(default)]
    pub tags: serde_json::Value,
}

/// Response structure for services API call
#[derive(Debug, Deserialize)]
struct ServicesResponse {
    data: Vec<Service>,
}

/// List services available to the current identity
///
/// This function uses the provided SessionManager to obtain an authenticated
/// API session, then queries the Ziti controller's `/services` endpoint to
/// retrieve a list of services the identity is authorized to access.
///
/// # Arguments
///
/// * `session_manager` - The session manager to use for authentication
///
/// # Returns
///
/// * `ZitiResult<Vec<Service>>` - List of available services
///
/// # Example
///
/// ```rust,no_run
/// use ziti_sdk::service::list_services;
/// use ziti_sdk::session::SessionManager;
/// use ziti_sdk::identity::load_from_file;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let identity = load_from_file("identity.json").await?;
///     let session_manager = SessionManager::new(identity);
///     let services = list_services(&session_manager).await?;
///     println!("Found {} services", services.len());
///     for service in services {
///         println!("Service: {} ({})", service.name, service.id);
///     }
///     Ok(())
/// }
/// ```
pub async fn list_services(session_manager: &SessionManager) -> ZitiResult<Vec<Service>> {
    let api_session = session_manager.get_api_session().await?;
    let identity_manager = session_manager.identity_manager();
    let client = controller_client(identity_manager, Duration::from_secs(30)).await?;

    let services_url = format!(
        "{}/services",
        identity_manager.zt_api().trim_end_matches('/')
    );

    // Send authenticated request to services endpoint
    let response = client
        .get(&services_url)
        .header("Content-Type", "application/json")
        .header("zt-session", &api_session.token)
        .send()
        .await
        .map_err(|e| {
            ZitiError::ConnectionFailed(format!("Failed to send services request: {}", e))
        })?;

    // Check response status
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ZitiError::ProtocolError {
            message: format!(
                "Services request failed with status {}: {}",
                status, error_text
            ),
        });
    }

    // Parse response
    let services_response: ServicesResponse =
        response
            .json()
            .await
            .map_err(|e| ZitiError::ProtocolError {
                message: format!("Failed to parse services response: {}", e),
            })?;

    Ok(services_response.data)
}

/// Service discovery manager
#[derive(Default)]
pub struct ServiceDiscovery;

impl ServiceDiscovery {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = Service {
            id: "service-123".to_string(),
            name: "test-service".to_string(),
            configs: vec!["config1".to_string(), "config2".to_string()],
            config: serde_json::json!({"host": "localhost", "port": 8080}),
            encryption_required: true,
            permissions: vec!["Dial".to_string()],
            tags: serde_json::json!({"environment": "test"}),
        };

        assert_eq!(service.id, "service-123");
        assert_eq!(service.name, "test-service");
        assert_eq!(service.configs.len(), 2);
        assert!(service.encryption_required);
        assert_eq!(service.permissions, vec!["Dial"]);
    }

    #[test]
    fn test_service_clone() {
        let service = Service {
            id: "clone-test".to_string(),
            name: "clone-service".to_string(),
            configs: vec![],
            config: serde_json::Value::Null,
            encryption_required: false,
            permissions: vec![],
            tags: serde_json::Value::Null,
        };

        let cloned_service = service.clone();
        assert_eq!(service.id, cloned_service.id);
        assert_eq!(service.name, cloned_service.name);
        assert_eq!(service.encryption_required, cloned_service.encryption_required);
    }

    #[test]
    fn test_service_deserialization() {
        let json_data = r#"
        {
            "id": "service-456",
            "name": "test-service",
            "configs": ["config1"],
            "config": {"host": "example.com"},
            "encryptionRequired": true,
            "permissions": ["Dial", "Bind"],
            "tags": {"env": "prod"}
        }
        "#;

        let result: Result<Service, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let service = result.unwrap();
        assert_eq!(service.id, "service-456");
        assert_eq!(service.name, "test-service");
        assert_eq!(service.configs, vec!["config1"]);
        assert!(service.encryption_required);
        assert_eq!(service.permissions, vec!["Dial", "Bind"]);
    }

    #[test]
    fn test_service_deserialization_with_defaults() {
        let json_data = r#"
        {
            "id": "minimal-service",
            "name": "minimal"
        }
        "#;

        let result: Result<Service, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let service = result.unwrap();
        assert_eq!(service.id, "minimal-service");
        assert_eq!(service.name, "minimal");
        assert_eq!(service.configs, Vec::<String>::new());
        assert!(!service.encryption_required);
        assert_eq!(service.permissions, Vec::<String>::new());
    }

    #[test]
    fn test_services_response_deserialization() {
        let json_data = r#"
        {
            "data": [
                {
                    "id": "service-1",
                    "name": "service-one"
                },
                {
                    "id": "service-2",
                    "name": "service-two",
                    "encryptionRequired": true
                }
            ]
        }
        "#;

        let result: Result<ServicesResponse, _> = serde_json::from_str(json_data);
        assert!(result.is_ok());

        let services_response = result.unwrap();
        assert_eq!(services_response.data.len(), 2);
        assert_eq!(services_response.data[0].id, "service-1");
        assert_eq!(services_response.data[1].id, "service-2");
        assert!(services_response.data[1].encryption_required);
    }

    #[test]
    fn test_service_discovery_creation() {
        let discovery = ServiceDiscovery::new();
        // Since ServiceDiscovery is a unit struct, we can only test creation
        // In a real implementation, we would test methods that perform operations
        let _discovery_ref = &discovery;
    }

    #[test]
    fn test_service_discovery_default() {
        let discovery = ServiceDiscovery;
        let new_discovery = ServiceDiscovery::new();
        // Both should be equivalent since ServiceDiscovery is a unit struct
        // Unit structs are always equivalent, so we just test they can be created
        let _discovery_ref = &discovery;
        let _new_discovery_ref = &new_discovery;
    }

    #[test]
    fn test_service_debug_format() {
        let service = Service {
            id: "debug-service".to_string(),
            name: "debug-test".to_string(),
            configs: vec![],
            config: serde_json::Value::Null,
            encryption_required: false,
            permissions: vec![],
            tags: serde_json::Value::Null,
        };

        let debug_str = format!("{:?}", service);
        assert!(debug_str.contains("Service"));
        assert!(debug_str.contains("debug-service"));
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_service_with_complex_config() {
        let complex_config = serde_json::json!({
            "protocols": ["tcp", "udp"],
            "interceptors": {
                "http": {
                    "addresses": ["example.com"],
                    "portRanges": [{"low": 80, "high": 80}]
                }
            }
        });

        let service = Service {
            id: "complex-service".to_string(),
            name: "complex-test".to_string(),
            configs: vec!["intercept.v1".to_string(), "host.v1".to_string()],
            config: complex_config.clone(),
            encryption_required: true,
            permissions: vec!["Dial".to_string(), "Bind".to_string()],
            tags: serde_json::json!({"type": "web-service", "priority": "high"}),
        };

        assert_eq!(service.config, complex_config);
        assert_eq!(service.configs.len(), 2);
        assert!(service.encryption_required);
        assert_eq!(service.permissions.len(), 2);
    }
}
