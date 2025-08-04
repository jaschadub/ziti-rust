//! Simple Ziti echo server example
//!
//! This example demonstrates how to use the Ziti Rust SDK to create a
//! server that listens for incoming connections on a Ziti service and
//! echoes back any data it receives.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example server -- <identity-file> <service-name>
//! ```
//!
//! ## Example
//!
//! ```bash
//! cargo run --example server -- server-identity.json echo-service
//! ```

use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ziti_sdk::{Context, ZitiResult};

#[tokio::main]
async fn main() -> ZitiResult<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <identity-file> <service-name>", args[0]);
        eprintln!("Example: {} server-identity.json echo-service", args[0]);
        std::process::exit(1);
    }

    let identity_file = &args[1];
    let service_name = &args[2];

    println!("Starting Ziti echo server...");
    println!("Identity file: {}", identity_file);
    println!("Service name: {}", service_name);

    // Load Ziti identity from file
    println!("Loading Ziti identity...");
    let context = Context::from_file(identity_file).await?;
    println!("Identity loaded successfully");

    // Create a listener for the specified service
    println!("Creating listener for service '{}'...", service_name);
    let mut listener = context.listen(service_name).await?;
    println!("✓ Listening on service: {}", listener.service_name());
    println!("✓ Terminator ID: {}", listener.terminator_id());
    println!();
    println!("Echo server is ready! Waiting for connections...");

    // Accept incoming connections in a loop
    let mut connection_count = 0;
    loop {
        match listener.accept().await {
            Ok(stream) => {
                connection_count += 1;
                println!("📥 Accepted connection #{}", connection_count);

                // Handle each connection in a separate task
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, connection_count).await {
                        eprintln!("❌ Error handling connection #{}: {}", connection_count, e);
                    }
                });
            }
            Err(e) => {
                eprintln!("❌ Error accepting connection: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Handle a single client connection
///
/// This function reads data from the client and echoes it back.
/// It continues until the client closes the connection or an error occurs.
async fn handle_connection(mut stream: ziti_sdk::ZitiStream, connection_id: u32) -> ZitiResult<()> {
    println!("🔄 Handling connection #{}", connection_id);
    
    let mut buffer = [0u8; 4096];
    let mut total_bytes = 0;

    loop {
        // Read data from the client
        match stream.read(&mut buffer).await {
            Ok(0) => {
                // Client closed the connection
                println!("📤 Connection #{} closed by client (total bytes: {})", connection_id, total_bytes);
                break;
            }
            Ok(bytes_read) => {
                total_bytes += bytes_read;
                
                // Convert bytes to string for logging (best effort)
                let data_str = String::from_utf8_lossy(&buffer[..bytes_read]);
                println!(
                    "📨 Connection #{}: received {} bytes: '{}'", 
                    connection_id, 
                    bytes_read,
                    data_str.trim()
                );

                // Echo the data back to the client
                if let Err(e) = stream.write_all(&buffer[..bytes_read]).await {
                    eprintln!("❌ Error writing to connection #{}: {}", connection_id, e);
                    break;
                }

                println!("📡 Connection #{}: echoed {} bytes back", connection_id, bytes_read);
            }
            Err(e) => {
                eprintln!("❌ Error reading from connection #{}: {}", connection_id, e);
                break;
            }
        }
    }

    println!("✅ Connection #{} handler finished", connection_id);
    Ok(())
}