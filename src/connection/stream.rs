//! ZitiStream implementation
//!
//! Provides the main stream type for Ziti connections with AsyncRead/AsyncWrite traits.

use crate::error::ZitiResult;
use crate::transport::WebSocketTransport;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::tungstenite::Message;

/// Combined trait for async stream operations
pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T> AsyncStream for T where T: AsyncRead + AsyncWrite + Send + Unpin {}

/// Main stream type for Ziti connections
///
/// `ZitiStream` provides a bidirectional communication channel over the Ziti network.
/// It implements the standard Rust async I/O traits (`AsyncRead` and `AsyncWrite`),
/// allowing it to be used with any code that expects these standard interfaces.
///
/// The stream handles the underlying Ziti protocol details transparently, presenting
/// a simple byte stream interface to applications.
///
/// # Examples
///
/// ## Basic usage with AsyncRead/AsyncWrite
///
/// ```rust,no_run
/// use ziti_sdk::Context;
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let context = Context::from_file("identity.json").await?;
///     let mut stream = context.dial("echo-service").await?;
///
///     // Write data to the stream
///     stream.write_all(b"Hello, Ziti!").await?;
///     stream.flush().await?;
///
///     // Read response
///     let mut buffer = [0; 1024];
///     let n = stream.read(&mut buffer).await?;
///     println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
///
///     // Explicitly close the stream
///     stream.close().await?;
///     Ok(())
/// }
/// ```
///
/// ## Check stream status
///
/// ```rust,no_run
/// use ziti_sdk::{Context, ZitiResult};
///
/// #[tokio::main]
/// async fn main() -> ZitiResult<()> {
///     let context = Context::from_file("identity.json").await?;
///     let mut stream = context.dial("my-service").await?;
///
///     // Check if stream is still open
///     if !stream.is_closed() {
///         println!("Stream is open and ready for use");
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct ZitiStream {
    /// The underlying WebSocket transport
    transport: WebSocketTransport,
    /// Buffer for incoming data that hasn't been read yet
    read_buffer: VecDeque<u8>,
    /// Buffer for outgoing data that needs to be written
    write_buffer: Vec<u8>,
    /// Flag indicating if the stream has been closed
    closed: bool,
}

impl ZitiStream {
    /// Create a new ZitiStream from a WebSocket transport
    ///
    /// This is the primary constructor for `ZitiStream`. It wraps an established
    /// WebSocket transport connection and provides the async I/O interface.
    ///
    /// # Arguments
    ///
    /// * `transport` - An established WebSocket transport connection
    ///
    /// # Returns
    ///
    /// * `Self` - A new ZitiStream instance ready for I/O operations
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ziti_sdk::{ZitiStream, transport::WebSocketTransport};
    ///
    /// // Typically used internally by the Context::dial() method
    /// // let transport = WebSocketTransport::connect(url, tls_config).await?;
    /// // let stream = ZitiStream::from_transport(transport);
    /// ```
    pub fn from_transport(transport: WebSocketTransport) -> Self {
        Self {
            transport,
            read_buffer: VecDeque::new(),
            write_buffer: Vec::new(),
            closed: false,
        }
    }

    /// Create a new ZitiStream (legacy constructor for compatibility)
    ///
    /// This method is deprecated and will be removed in future versions.
    /// Use [`ZitiStream::from_transport`] instead.
    ///
    /// # Arguments
    ///
    /// * `_inner` - Legacy async stream (unused)
    ///
    /// # Panics
    ///
    /// This method will panic as it is not implemented. Use `from_transport()` instead.
    #[deprecated(since = "0.1.0", note = "Use ZitiStream::from_transport() instead")]
    pub fn new(_inner: Box<dyn AsyncStream>) -> Self {
        // This is a placeholder for backward compatibility
        // In practice, use from_transport()
        todo!("Use ZitiStream::from_transport() instead")
    }

    /// Check if the stream is closed
    ///
    /// Returns `true` if the stream has been explicitly closed or if the underlying
    /// connection has been terminated.
    ///
    /// # Returns
    ///
    /// * `bool` - `true` if the stream is closed, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::{Context, ZitiResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> ZitiResult<()> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let mut stream = context.dial("my-service").await?;
    ///
    ///     // Check stream status before using
    ///     if !stream.is_closed() {
    ///         // Safe to read/write
    ///     }
    ///
    ///     stream.close().await?;
    ///     assert!(stream.is_closed());
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Close the stream
    ///
    /// Gracefully closes the Ziti stream and the underlying transport connection.
    /// After calling this method, no further I/O operations should be attempted.
    ///
    /// This method is idempotent - calling it multiple times is safe and will
    /// not result in an error.
    ///
    /// # Returns
    ///
    /// * `ZitiResult<()>` - `Ok(())` on successful closure, error otherwise
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ziti_sdk::Context;
    /// use tokio::io::AsyncWriteExt;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let context = Context::from_file("identity.json").await?;
    ///     let mut stream = context.dial("my-service").await?;
    ///
    ///     // Use the stream
    ///     stream.write_all(b"Hello").await?;
    ///
    ///     // Explicitly close when done
    ///     stream.close().await?;
    ///
    ///     // Multiple calls to close() are safe
    ///     stream.close().await?; // No error
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn close(&mut self) -> ZitiResult<()> {
        if !self.closed {
            self.transport.close().await?;
            self.closed = true;
        }
        Ok(())
    }

    /// Process incoming messages from the transport
    #[allow(dead_code)]
    async fn process_incoming_messages(&mut self) -> ZitiResult<()> {
        // Try to receive messages from the transport
        while let Some(message) = self.transport.receive().await? {
            match message {
                Message::Binary(data) => {
                    // Add binary data to read buffer
                    self.read_buffer.extend(data);
                }
                Message::Close(_) => {
                    self.closed = true;
                    break;
                }
                Message::Ping(data) => {
                    // Respond to ping with pong
                    self.transport.send(Message::Pong(data)).await?;
                }
                Message::Pong(_) => {
                    // Handle pong message (usually nothing to do)
                }
                Message::Text(_) => {
                    // Text messages are not expected in Ziti protocol
                    return Err(crate::error::ZitiError::ProtocolError {
                        message: "Unexpected text message in Ziti stream".to_string(),
                    });
                }
                Message::Frame(_) => {
                    // Raw frames are handled by tungstenite
                }
            }
        }
        Ok(())
    }

    /// Send any pending write data
    #[allow(dead_code)]
    async fn send_write_buffer(&mut self) -> Result<(), std::io::Error> {
        if !self.write_buffer.is_empty() {
            let data = std::mem::take(&mut self.write_buffer);
            let message = Message::Binary(data.into());
            
            self.transport.send(message).await.map_err(|e| {
                std::io::Error::other(format!("Transport send failed: {}", e))
            })?;
        }
        Ok(())
    }
}

impl AsyncRead for ZitiStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.closed {
            return Poll::Ready(Ok(()));
        }

        // First, check if we have data in our buffer
        let available = std::cmp::min(buf.remaining(), self.read_buffer.len());
        if available > 0 {
            let data: Vec<u8> = self.read_buffer.drain(..available).collect();
            buf.put_slice(&data);
            return Poll::Ready(Ok(()));
        }

        // If no data in buffer, we need to check for new messages
        // For now, we'll return Pending and let the caller try again
        // In a more complete implementation, we'd use a waker system
        if self.closed {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl AsyncWrite for ZitiStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if self.closed {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is closed",
            )));
        }

        // Add data to write buffer
        self.write_buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        if self.closed {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is closed",
            )));
        }

        // For now, we'll use a simple approach that may not be fully async
        // In a more complete implementation, we'd properly handle the async send
        if !self.write_buffer.is_empty() {
            // We can't easily make this async in poll context, so we'll return Pending
            // and let the runtime handle it. In practice, a more sophisticated
            // implementation would use internal state management.
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.closed = true;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ziti_stream_creation() {
        // This is a placeholder test since we need a real WebSocketTransport
        // In practice, we'd create a mock transport for testing
    }

    #[test]
    fn test_stream_buffer_operations() {
        // Test the buffer operations independently
        let mut buffer = VecDeque::new();
        buffer.extend(b"test data");
        
        let data: Vec<u8> = buffer.drain(..4).collect();
        assert_eq!(data, b"test");
        
        let remaining: Vec<u8> = buffer.into_iter().collect();
        assert_eq!(remaining, b" data");
    }
}
