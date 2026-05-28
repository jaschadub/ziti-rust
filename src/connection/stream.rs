//! ZitiStream implementation
//!
//! Provides the main stream type for Ziti connections with AsyncRead/AsyncWrite traits.

use crate::error::ZitiResult;
use crate::transport::WebSocketTransport;
use futures_util::{Sink, Stream};
use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::tungstenite::Message;

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
            closed: false,
        }
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
}

impl AsyncRead for ZitiStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        loop {
            // Serve any buffered bytes first.
            if !this.read_buffer.is_empty() {
                let n = std::cmp::min(buf.remaining(), this.read_buffer.len());
                if n > 0 {
                    let data: Vec<u8> = this.read_buffer.drain(..n).collect();
                    buf.put_slice(&data);
                }
                return Poll::Ready(Ok(()));
            }

            // EOF once the connection is closed and the buffer is drained.
            if this.closed {
                return Poll::Ready(Ok(()));
            }

            // Otherwise pull the next WebSocket message from the transport.
            match Pin::new(this.transport.stream_mut()).poll_next(cx) {
                Poll::Ready(Some(Ok(message))) => match message {
                    Message::Binary(data) => {
                        this.read_buffer.extend(data);
                        // Loop back to serve the freshly buffered bytes.
                    }
                    Message::Close(_) => {
                        this.closed = true;
                        return Poll::Ready(Ok(()));
                    }
                    // Tungstenite handles ping/pong and raw frames internally;
                    // keep polling for application data.
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                    Message::Text(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unexpected text message in Ziti stream",
                        )));
                    }
                },
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(io::Error::other(format!(
                        "WebSocket receive failed: {}",
                        e
                    ))));
                }
                Poll::Ready(None) => {
                    this.closed = true;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for ZitiStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        if this.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "stream is closed",
            )));
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        // Wait for the sink to accept a new message, then enqueue the bytes as a
        // single binary frame. Actual transmission happens on poll_flush.
        match Pin::new(this.transport.stream_mut()).poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                Pin::new(this.transport.stream_mut())
                    .start_send(Message::Binary(buf.to_vec().into()))
                    .map_err(|e| io::Error::other(format!("WebSocket send failed: {}", e)))?;
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                "WebSocket sink error: {}",
                e
            )))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        match Pin::new(this.transport.stream_mut()).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                "WebSocket flush failed: {}",
                e
            )))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        match Pin::new(this.transport.stream_mut()).poll_close(cx) {
            Poll::Ready(Ok(())) => {
                this.closed = true;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                "WebSocket close failed: {}",
                e
            )))),
            Poll::Pending => Poll::Pending,
        }
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
