//! Simple Ziti client example
//!
//! This example demonstrates how to use the Ziti Rust SDK to create a
//! client that connects to a Ziti service and sends a message, then
//! prints the response.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example client -- <identity-file> <service-name> [message]
//! ```
//!
//! ## Examples
//!
//! ```bash
//! # Send default message
//! cargo run --example client -- client-identity.json echo-service
//!
//! # Send custom message
//! cargo run --example client -- client-identity.json echo-service "Hello, Ziti!"
//! ```

use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ziti_sdk::Context;

/// Default message to send if none is provided
const DEFAULT_MESSAGE: &str = "Hello from Ziti Rust SDK!";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: {} <identity-file> <service-name> [message]", args[0]);
        eprintln!("Examples:");
        eprintln!("  {} client-identity.json echo-service", args[0]);
        eprintln!("  {} client-identity.json echo-service \"Hello, Ziti!\"", args[0]);
        std::process::exit(1);
    }

    let identity_file = &args[1];
    let service_name = &args[2];
    let message = args.get(3).map(|s| s.as_str()).unwrap_or(DEFAULT_MESSAGE);

    println!("Starting Ziti client...");
    println!("Identity file: {}", identity_file);
    println!("Service name: {}", service_name);
    println!("Message to send: '{}'", message);
    println!();

    // Load Ziti identity from file
    println!("📋 Loading Ziti identity...");
    let context = Context::from_file(identity_file).await?;
    println!("✅ Identity loaded successfully");

    // Connect to the specified service
    println!("🔌 Connecting to service '{}'...", service_name);
    let mut stream = context.dial(service_name).await?;
    println!("✅ Connected to service successfully");

    // Send the message to the server
    println!("📤 Sending message...");
    stream.write_all(message.as_bytes()).await?;
    println!("✅ Message sent ({} bytes)", message.len());

    // Read the response from the server
    println!("📥 Waiting for response...");
    let mut buffer = [0u8; 4096];
    match stream.read(&mut buffer).await {
        Ok(0) => {
            println!("⚠️  Server closed connection without sending a response");
        }
        Ok(bytes_read) => {
            let response = String::from_utf8_lossy(&buffer[..bytes_read]);
            println!("✅ Received response ({} bytes): '{}'", bytes_read, response.trim());
            
            // Verify echo functionality
            if response.trim() == message {
                println!("🎉 Echo test successful - message matches response!");
            } else {
                println!("⚠️  Response differs from sent message");
            }
        }
        Err(e) => {
            eprintln!("❌ Error reading response: {}", e);
            return Err(e.into());
        }
    }

    // Demonstrate multiple message exchange
    println!();
    println!("🔄 Testing multiple message exchange...");
    
    for i in 1..=3 {
        let test_message = format!("Test message #{}", i);
        
        println!("📤 Sending: '{}'", test_message);
        stream.write_all(test_message.as_bytes()).await?;
        
        match stream.read(&mut buffer).await {
            Ok(0) => {
                println!("⚠️  Server closed connection");
                break;
            }
            Ok(bytes_read) => {
                let response = String::from_utf8_lossy(&buffer[..bytes_read]);
                println!("📥 Received: '{}'", response.trim());
            }
            Err(e) => {
                eprintln!("❌ Error reading response #{}: {}", i, e);
                break;
            }
        }
    }

    println!();
    println!("🎯 Client completed successfully!");
    Ok(())
}