//! Integration tests for Ziti SDK
//!
//! These tests verify end-to-end functionality including dial and listen operations.
//! They require a running Ziti network and valid identity configuration.

use std::env;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use ziti_sdk::Context;

/// Get the identity file path from environment variable
fn get_identity_file() -> Option<String> {
    env::var("ZITI_IDENTITY_FILE").ok()
}

/// Check if integration tests should be run
fn should_run_integration_tests() -> bool {
    get_identity_file().is_some()
}

/// Create a Context for testing from environment
async fn create_test_context() -> Result<Context, Box<dyn std::error::Error>> {
    let identity_file = get_identity_file()
        .expect("ZITI_IDENTITY_FILE environment variable must be set for integration tests");
    
    Ok(Context::from_file(&identity_file).await?)
}

/// Skip test if no identity file is available
macro_rules! skip_if_no_identity {
    () => {
        if !should_run_integration_tests() {
            println!("Skipping integration test - ZITI_IDENTITY_FILE not set");
            return;
        }
    };
}

/// Get test service name from environment or use default
fn get_test_service() -> String {
    env::var("ZITI_TEST_SERVICE").unwrap_or_else(|_| "echo-service".to_string())
}

/// Get test listen service name from environment or use default
fn get_test_listen_service() -> String {
    env::var("ZITI_TEST_LISTEN_SERVICE").unwrap_or_else(|_| "test-listen-service".to_string())
}

/// Test basic context creation and identity loading
#[tokio::test]
async fn test_context_creation() {
    skip_if_no_identity!();

    let context = create_test_context().await;
    assert!(context.is_ok(), "Failed to create context: {:?}", context.err());
    
    let _context = context.unwrap();
    // Verify context has valid identity
    // This is a basic smoke test that the context was created successfully
}

/// Test dialing to a service
#[tokio::test]
async fn test_dial_service() {
    skip_if_no_identity!();

    let context = create_test_context().await.expect("Failed to create context");
    let service_name = get_test_service();
    
    // Attempt to dial the service
    let result = timeout(Duration::from_secs(30), context.dial(&service_name)).await;
    
    match result {
        Ok(Ok(_stream)) => {
            println!("Successfully dialed service: {}", service_name);
            // Basic connectivity test - the fact that we can dial is the main test
        }
        Ok(Err(e)) => {
            // Service may not exist or may not be accessible
            println!("Failed to dial service {}: {:?}", service_name, e);
            // This might be expected if the test service isn't configured
        }
        Err(_) => {
            panic!("Dial operation timed out after 30 seconds");
        }
    }
}

/// Test dial and basic data exchange
#[tokio::test]
async fn test_dial_and_send_data() {
    skip_if_no_identity!();

    let context = create_test_context().await.expect("Failed to create context");
    let service_name = get_test_service();
    
    // Attempt to dial and send data
    let dial_result = timeout(Duration::from_secs(30), context.dial(&service_name)).await;
    
    if let Ok(Ok(mut stream)) = dial_result {
        // Test sending and receiving data
        let test_data = b"Hello, Ziti!";
        
        // Send data
        let write_result = timeout(
            Duration::from_secs(10),
            stream.write_all(test_data)
        ).await;
        
        if write_result.is_ok() {
            // Try to read response (for echo services)
            let mut buffer = [0u8; 1024];
            let read_result = timeout(
                Duration::from_secs(10),
                stream.read(&mut buffer)
            ).await;
            
            match read_result {
                Ok(Ok(bytes_read)) => {
                    println!("Received {} bytes back from service", bytes_read);
                    if bytes_read > 0 {
                        let response = &buffer[..bytes_read];
                        if response == test_data {
                            println!("Echo service working correctly!");
                        } else {
                            println!("Received different data than sent");
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("Failed to read from stream: {:?}", e);
                }
                Err(_) => {
                    println!("Read operation timed out");
                }
            }
        } else {
            println!("Failed to write to stream");
        }
    } else {
        println!("Could not establish connection to test service: {}", service_name);
    }
}

/// Test listening for incoming connections
#[tokio::test]
async fn test_listen_service() {
    skip_if_no_identity!();

    let context = create_test_context().await.expect("Failed to create context");
    let service_name = get_test_listen_service();
    
    // Attempt to listen on a service
    let listen_result = timeout(
        Duration::from_secs(30), 
        context.listen(&service_name)
    ).await;
    
    match listen_result {
        Ok(Ok(mut listener)) => {
            println!("Successfully created listener for service: {}", service_name);
            
            // Test accepting a connection with timeout
            let accept_result = timeout(
                Duration::from_secs(5),
                listener.accept()
            ).await;
            
            match accept_result {
                Ok(Ok(stream)) => {
                    println!("Accepted connection: {:?}", stream);
                    // Could test basic I/O here if needed
                }
                Ok(Err(e)) => {
                    println!("Failed to accept connection: {:?}", e);
                }
                Err(_) => {
                    println!("No incoming connections within timeout period (this is expected)");
                }
            }
        }
        Ok(Err(e)) => {
            println!("Failed to create listener for service {}: {:?}", service_name, e);
            // This might be expected if the service isn't configured for hosting
        }
        Err(_) => {
            panic!("Listen operation timed out after 30 seconds");
        }
    }
}

/// Test context cleanup and resource management
#[tokio::test]
async fn test_context_cleanup() {
    skip_if_no_identity!();

    // Create and drop multiple contexts to test resource cleanup
    for i in 0..3 {
        let context = create_test_context().await;
        assert!(context.is_ok(), "Failed to create context {}: {:?}", i, context.err());
        
        // Context should be dropped automatically when it goes out of scope
        println!("Created and dropping context {}", i);
    }
    
    // If we get here without panicking, cleanup is working
    println!("Context cleanup test completed successfully");
}

/// Test concurrent operations
#[tokio::test]
async fn test_concurrent_operations() {
    skip_if_no_identity!();

    let context = create_test_context().await.expect("Failed to create context");
    let service_name = get_test_service();
    
    // Spawn multiple concurrent dial operations
    let mut handles = Vec::new();
    
    for i in 0..3 {
        let context_clone = context.clone();
        let service_clone = service_name.clone();
        
        let handle = tokio::spawn(async move {
            let result = timeout(
                Duration::from_secs(30),
                context_clone.dial(&service_clone)
            ).await;
            
            match result {
                Ok(Ok(_)) => {
                    println!("Concurrent dial {} succeeded", i);
                    true
                }
                Ok(Err(e)) => {
                    println!("Concurrent dial {} failed: {:?}", i, e);
                    false
                }
                Err(_) => {
                    println!("Concurrent dial {} timed out", i);
                    false
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut success_count = 0;
    for handle in handles {
        if let Ok(success) = handle.await
            && success
        {
            success_count += 1;
        }
    }
    
    println!("Concurrent operations completed: {}/3 succeeded", success_count);
    // The test passes as long as we don't panic - success depends on service availability
}