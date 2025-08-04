//! Test utilities for integration tests
//!
//! Common helper functions and setup code for integration tests.

use std::env;
use ziti_sdk::Context;

/// Get the identity file path from environment variable
pub fn get_identity_file() -> Option<String> {
    env::var("ZITI_IDENTITY_FILE").ok()
}

/// Check if integration tests should be run
pub fn should_run_integration_tests() -> bool {
    get_identity_file().is_some()
}

/// Create a Context for testing from environment
pub async fn create_test_context() -> Result<Context, Box<dyn std::error::Error>> {
    let identity_file = get_identity_file()
        .expect("ZITI_IDENTITY_FILE environment variable must be set for integration tests");
    
    Ok(Context::from_file(&identity_file).await?)
}

/// Skip test if no identity file is available
#[macro_export]
macro_rules! skip_if_no_identity {
    () => {
        if !$crate::utils::should_run_integration_tests() {
            println!("Skipping integration test - ZITI_IDENTITY_FILE not set");
            return;
        }
    };
}

/// Get test service name from environment or use default
pub fn get_test_service() -> String {
    env::var("ZITI_TEST_SERVICE").unwrap_or_else(|_| "echo-service".to_string())
}

/// Get test listen service name from environment or use default
pub fn get_test_listen_service() -> String {
    env::var("ZITI_TEST_LISTEN_SERVICE").unwrap_or_else(|_| "test-listen-service".to_string())
}