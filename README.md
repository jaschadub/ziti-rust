<img src="logo.png" alt="Ziti-Rust" style="max-width: 400px; max-height: 400px;">

# Ziti Rust SDK

[![Crates.io](https://img.shields.io/crates/v/ziti-sdk.svg)](https://crates.io/crates/ziti-sdk)
[![Documentation](https://docs.rs/ziti-sdk/badge.svg)](https://docs.rs/ziti-sdk)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, async-first Rust implementation that provides secure, zero-trust networking capabilities through the [OpenZiti](https://github.com/openziti/ziti) platform. This is an unofficial SDK.

## Features

- **Zero Trust Architecture**: End-to-end encrypted connections with identity-based access control
- **Async/Await Support**: Built on Tokio for high-performance asynchronous networking
- **Service Discovery**: Automatic discovery and connection to Ziti services
- **Edge Router Integration**: Smart routing through Ziti edge routers
- **Session Management**: Automatic API and network session management
- **Identity Management**: Support for certificate-based identity authentication
- **Error Recovery**: Comprehensive error handling with automatic retry capabilities

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
ziti-sdk = "0.2.0"
tokio = { version = "1", features = ["full"] }
```

## Getting Started

### Prerequisites

Before using the Ziti Rust SDK, you'll need:

1. **A Ziti Network**: Access to a running Ziti controller and edge routers
2. **Identity File**: A valid Ziti identity file (`.json`) with appropriate service policies
3. **Service Configuration**: Services configured in your Ziti network with proper access policies

### Basic Example

```rust
use ziti_sdk::{Context, ZitiResult};

#[tokio::main]
async fn main() -> ZitiResult<()> {
    // Load identity from file
    let context = Context::from_file("identity.json").await?;
    
    // Connect to a service
    let mut stream = context.dial("echo-service").await?;
    
    // Use the stream for communication
    // (implements standard Rust AsyncRead + AsyncWrite traits)
    
    Ok(())
}
```

## Usage Examples

### Dialing a Service (Client)

```rust
use ziti_sdk::{Context, ZitiResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> ZitiResult<()> {
    // Create context from identity file
    let context = Context::from_file("client-identity.json").await?;
    
    // Connect to the service
    let mut stream = context.dial("my-service").await?;
    
    // Send data
    stream.write_all(b"Hello, Ziti!").await?;
    
    // Read response
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
    
    Ok(())
}
```

### Listening on a Service (Server)

```rust
use ziti_sdk::{Context, ZitiResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> ZitiResult<()> {
    // Create context from identity file
    let context = Context::from_file("server-identity.json").await?;
    
    // Start listening on a service
    let mut listener = context.listen("my-service").await?;
    println!("Listening on service: {}", listener.service_name());
    
    // Accept incoming connections
    loop {
        match listener.accept().await {
            Ok(mut stream) => {
                // Handle connection in a separate task
                tokio::spawn(async move {
                    let mut buffer = [0; 1024];
                    match stream.read(&mut buffer).await {
                        Ok(n) => {
                            println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
                            // Echo back the data
                            let _ = stream.write_all(&buffer[..n]).await;
                        }
                        Err(e) => eprintln!("Error reading from stream: {}", e),
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}
```

### Using Connection Options

```rust
use ziti_sdk::{Context, ZitiResult, DialOptions, ListenOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> ZitiResult<()> {
    let context = Context::from_file("identity.json").await?;
    
    // Dial with custom options
    let dial_options = DialOptions {
        timeout: Some(Duration::from_secs(30)),
        identity: Some("specific-identity".to_string()),
    };
    // Note: Context-level dialing doesn't currently use DialOptions,
    // but this shows the intended API pattern
    
    // Listen with custom options
    let listen_options = ListenOptions {
        identity: Some("server-identity".to_string()),
        cost: Some(100),
        precedence: Some("high".to_string()),
    };
    
    let listener = context.listen_with_options("my-service", &listen_options).await?;
    
    Ok(())
}
```

## Configuration

### Identity Files

Ziti identity files are JSON documents containing certificates and configuration:

```json
{
  "ztAPI": "https://controller.example.com:1280",
  "id": {
    "cert": "-----BEGIN CERTIFICATE-----\n...",
    "key": "-----BEGIN PRIVATE KEY-----\n...",
    "ca": "-----BEGIN CERTIFICATE-----\n..."
  }
}
```

### Loading Configuration

```rust
use ziti_sdk::{Context, ContextBuilder, ZitiConfig};

// Method 1: Load from file (recommended)
let context = Context::from_file("identity.json").await?;

// Method 2: Build programmatically (future feature)
let context = ContextBuilder::new()
    .with_config(config)
    .build()
    .await?;
```

### Environment Variables

The SDK respects these environment variables:

- `ZITI_IDENTITY_FILE`: Default path to identity file
- `ZITI_LOG_LEVEL`: Logging level (error, warn, info, debug, trace)

## Error Handling

The SDK provides comprehensive error types for different scenarios:

```rust
use ziti_sdk::{ZitiError, ZitiResult};

async fn handle_connection() -> ZitiResult<()> {
    let context = Context::from_file("identity.json").await?;
    
    match context.dial("service-name").await {
        Ok(stream) => {
            // Handle successful connection
            Ok(())
        }
        Err(ZitiError::ServiceNotFound { service_name }) => {
            eprintln!("Service '{}' not found or not accessible", service_name);
            Err(ZitiError::ServiceNotFound { service_name })
        }
        Err(ZitiError::ConnectionFailed(reason)) => {
            eprintln!("Connection failed: {}", reason);
            // Potentially retry with backoff
            Err(ZitiError::ConnectionFailed(reason))
        }
        Err(ZitiError::ApiSessionExpired) => {
            eprintln!("Session expired, will be renewed automatically");
            // The SDK handles session renewal internally
            Err(ZitiError::ApiSessionExpired)
        }
        Err(e) => {
            eprintln!("Unexpected error: {}", e);
            Err(e)
        }
    }
}
```

### Error Recovery

The SDK provides helper methods for error categorization:

```rust
use ziti_sdk::ZitiError;

fn handle_error(error: &ZitiError) {
    if error.is_recoverable() {
        println!("This error can be retried");
    }
    
    if error.requires_session_renewal() {
        println!("Session needs renewal");
    }
    
    if error.is_configuration_error() {
        println!("Check your configuration");
    }
}
```

## Advanced Usage

### Context Management

```rust
use ziti_sdk::{Context, IdentityManager, SessionManager};

// Create context with existing managers
let identity_manager = IdentityManager::load_from_file("identity.json").await?;
let session_manager = SessionManager::new(identity_manager.clone());
let context = Context::from_managers(identity_manager, session_manager);

// Access underlying managers
let identity = context.identity_manager();
let sessions = context.session_manager();
```

### Connection Lifecycle

```rust
use ziti_sdk::{Context, ZitiStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn connection_lifecycle() -> ZitiResult<()> {
    let context = Context::from_file("identity.json").await?;
    
    // Establish connection
    let mut stream = context.dial("my-service").await?;
    
    // Use connection
    stream.write_all(b"Hello").await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    
    // Connection automatically closed when stream is dropped
    
    Ok(())
}
```

## API Reference

### Core Types

- [`Context`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.Context.html) - Main SDK entry point
- [`ZitiStream`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.ZitiStream.html) - Bidirectional communication stream
- [`ZitiListener`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.ZitiListener.html) - Server listener for incoming connections

### Configuration Types

- [`DialOptions`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.DialOptions.html) - Options for outbound connections
- [`ListenOptions`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.ListenOptions.html) - Options for service hosting
- [`ZitiConfig`](https://docs.rs/ziti-sdk/latest/ziti_sdk/struct.ZitiConfig.html) - SDK configuration

### Error Types

- [`ZitiError`](https://docs.rs/ziti-sdk/latest/ziti_sdk/enum.ZitiError.html) - Comprehensive error enumeration
- [`ZitiResult<T>`](https://docs.rs/ziti-sdk/latest/ziti_sdk/type.ZitiResult.html) - Result type alias

## Examples

See the [`examples/`](examples/) directory for complete working examples:

- `simple_client.rs` - Basic client connection
- `simple_server.rs` - Basic server implementation
- `echo_server.rs` - Echo server with error handling
- `http_proxy.rs` - HTTP proxy over Ziti

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

- **Documentation**: [https://docs.rs/ziti-sdk](https://docs.rs/ziti-sdk)
- **Issues**: [GitHub Issues](https://github.com/jaschadub/ziti-rust/issues)


## Related Projects

- [OpenZiti](https://github.com/openziti/ziti) - The main OpenZiti project
- [Ziti SDK for C](https://github.com/openziti/ziti-sdk-c) - C SDK implementation
- [Ziti SDK for Go](https://github.com/openziti/sdk-golang) - Go SDK implementation

<a href="https://symbiont.dev" target="_blank" rel="noopener">
  <img src="made-with-symbiont.png" alt="Made with Symbiont" style="float: right; margin: 0 0 10px 10px; max-width: 320px;">
</a>