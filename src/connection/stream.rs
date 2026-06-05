//! ZitiStream implementation
//!
//! Provides the main stream type for Ziti connections with AsyncRead/AsyncWrite traits.

use crate::error::ZitiResult;
use crate::transport::WebSocketTransport;
use bytes::Bytes;
use futures_util::{Sink, Stream};
use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// Outbound frame produced by a channel-backed [`ZitiStream`].
///
/// The listener's demux task is the only writer to the underlying
/// WebSocket, so streams hand it ready-to-send `ZitiMessage` bytes plus
/// the source `conn_id` and a hint of whether the message is a `Close`
/// (so the demuxer can drop bookkeeping for that conn).
#[derive(Debug, Clone)]
pub(crate) struct OutboundFrame {
    pub conn_id: u32,
    pub bytes: Bytes,
    pub is_close: bool,
}

/// Backend driving a [`ZitiStream`].
///
/// Dial connections own their WebSocket and read/write raw binary
/// frames directly. Listener accept()s share a single WebSocket via a
/// demux task, so they exchange already-extracted payload bytes through
/// channels instead.
#[derive(Debug)]
enum StreamBackend {
    Direct(Box<WebSocketTransport>),
    Channel(ChannelBackend),
}

#[derive(Debug)]
struct ChannelBackend {
    conn_id: u32,
    inbound_rx: mpsc::UnboundedReceiver<Bytes>,
    outbound_tx: mpsc::UnboundedSender<OutboundFrame>,
    next_seq: u32,
}

impl ChannelBackend {
    fn send_close(&mut self) {
        let frame = match build_close_frame(self.conn_id, self.next_seq()) {
            Ok(f) => f,
            Err(_) => return,
        };
        let _ = self.outbound_tx.send(frame);
    }

    fn next_seq(&mut self) -> u32 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1).max(1);
        seq
    }
}

/// Build the `OutboundFrame` for a `Data` message carrying `payload`.
pub(crate) fn build_data_frame(
    conn_id: u32,
    sequence: u32,
    payload: Bytes,
) -> ZitiResult<OutboundFrame> {
    use crate::transport::protocol::{ContentType, ZitiMessage};

    let mut msg = ZitiMessage::new(ContentType::Data, sequence, payload);
    msg.header
        .add_header("conn_id".to_string(), conn_id.to_string());
    Ok(OutboundFrame {
        conn_id,
        bytes: msg.serialize()?,
        is_close: false,
    })
}

/// Build the `OutboundFrame` for a `Close` message for `conn_id`.
pub(crate) fn build_close_frame(conn_id: u32, sequence: u32) -> ZitiResult<OutboundFrame> {
    use crate::transport::protocol::{ContentType, ZitiMessage};

    let mut msg = ZitiMessage::new(ContentType::Close, sequence, Bytes::new());
    msg.header
        .add_header("conn_id".to_string(), conn_id.to_string());
    Ok(OutboundFrame {
        conn_id,
        bytes: msg.serialize()?,
        is_close: true,
    })
}

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
///     stream.write_all(b"Hello, Ziti!").await?;
///     stream.flush().await?;
///
///     let mut buffer = [0; 1024];
///     let n = stream.read(&mut buffer).await?;
///     println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
///
///     stream.close().await?;
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct ZitiStream {
    backend: StreamBackend,
    read_buffer: VecDeque<u8>,
    closed: bool,
}

impl ZitiStream {
    /// Create a new ZitiStream from a WebSocket transport.
    ///
    /// Used by [`crate::connection::dial`] when each Ziti connection
    /// owns its own WebSocket to an edge router.
    pub fn from_transport(transport: WebSocketTransport) -> Self {
        Self {
            backend: StreamBackend::Direct(Box::new(transport)),
            read_buffer: VecDeque::new(),
            closed: false,
        }
    }

    /// Create a new channel-backed ZitiStream.
    ///
    /// Used by [`crate::connection::ZitiListener::accept`] for inbound
    /// connections multiplexed over a single edge-router WebSocket.
    pub(crate) fn from_channels(
        conn_id: u32,
        inbound_rx: mpsc::UnboundedReceiver<Bytes>,
        outbound_tx: mpsc::UnboundedSender<OutboundFrame>,
    ) -> Self {
        Self {
            backend: StreamBackend::Channel(ChannelBackend {
                conn_id,
                inbound_rx,
                outbound_tx,
                next_seq: 1,
            }),
            read_buffer: VecDeque::new(),
            closed: false,
        }
    }

    /// Check if the stream is closed.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Gracefully close the stream and the underlying transport/conn.
    pub async fn close(&mut self) -> ZitiResult<()> {
        if self.closed {
            return Ok(());
        }
        match &mut self.backend {
            StreamBackend::Direct(transport) => transport.close().await?,
            StreamBackend::Channel(ch) => ch.send_close(),
        }
        self.closed = true;
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
            if !this.read_buffer.is_empty() {
                let n = std::cmp::min(buf.remaining(), this.read_buffer.len());
                if n > 0 {
                    let data: Vec<u8> = this.read_buffer.drain(..n).collect();
                    buf.put_slice(&data);
                }
                return Poll::Ready(Ok(()));
            }

            if this.closed {
                return Poll::Ready(Ok(()));
            }

            match &mut this.backend {
                StreamBackend::Direct(transport) => {
                    match Pin::new(transport.stream_mut()).poll_next(cx) {
                        Poll::Ready(Some(Ok(message))) => match message {
                            Message::Binary(data) => {
                                this.read_buffer.extend(data);
                            }
                            Message::Close(_) => {
                                this.closed = true;
                                return Poll::Ready(Ok(()));
                            }
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
                StreamBackend::Channel(ch) => match ch.inbound_rx.poll_recv(cx) {
                    Poll::Ready(Some(data)) => {
                        this.read_buffer.extend(data);
                    }
                    Poll::Ready(None) => {
                        this.closed = true;
                        return Poll::Ready(Ok(()));
                    }
                    Poll::Pending => return Poll::Pending,
                },
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

        match &mut this.backend {
            StreamBackend::Direct(transport) => {
                match Pin::new(transport.stream_mut()).poll_ready(cx) {
                    Poll::Ready(Ok(())) => {
                        Pin::new(transport.stream_mut())
                            .start_send(Message::Binary(buf.to_vec().into()))
                            .map_err(|e| {
                                io::Error::other(format!("WebSocket send failed: {}", e))
                            })?;
                        Poll::Ready(Ok(buf.len()))
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                        "WebSocket sink error: {}",
                        e
                    )))),
                    Poll::Pending => Poll::Pending,
                }
            }
            StreamBackend::Channel(ch) => {
                let payload = Bytes::copy_from_slice(buf);
                let seq = ch.next_seq();
                let frame = build_data_frame(ch.conn_id, seq, payload).map_err(|e| {
                    io::Error::other(format!("Failed to frame outbound bytes: {}", e))
                })?;
                ch.outbound_tx
                    .send(frame)
                    .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "listener closed"))?;
                Poll::Ready(Ok(buf.len()))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match &mut this.backend {
            StreamBackend::Direct(transport) => {
                match Pin::new(transport.stream_mut()).poll_flush(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                        "WebSocket flush failed: {}",
                        e
                    )))),
                    Poll::Pending => Poll::Pending,
                }
            }
            // The unbounded mpsc has no flush concept; the demux task
            // drains it. There's nothing to wait on here.
            StreamBackend::Channel(_) => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match &mut this.backend {
            StreamBackend::Direct(transport) => {
                match Pin::new(transport.stream_mut()).poll_close(cx) {
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
            StreamBackend::Channel(ch) => {
                if !this.closed {
                    ch.send_close();
                    this.closed = true;
                }
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl Drop for ZitiStream {
    fn drop(&mut self) {
        // Best-effort: tell the demuxer to drop bookkeeping for this conn
        // so the peer learns the listener side hung up. The Direct backend
        // is owned end-to-end and needs no signaling.
        if !self.closed
            && let StreamBackend::Channel(ch) = &mut self.backend
        {
            ch.send_close();
            self.closed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::protocol::{ContentType, ZitiMessage};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_stream_buffer_operations() {
        let mut buffer = VecDeque::new();
        buffer.extend(b"test data");

        let data: Vec<u8> = buffer.drain(..4).collect();
        assert_eq!(data, b"test");

        let remaining: Vec<u8> = buffer.into_iter().collect();
        assert_eq!(remaining, b" data");
    }

    #[test]
    fn test_build_data_frame_includes_conn_id_header() {
        let frame = build_data_frame(42, 7, Bytes::from_static(b"hello")).unwrap();
        assert!(!frame.is_close);
        assert_eq!(frame.conn_id, 42);

        let parsed = ZitiMessage::deserialize(frame.bytes).unwrap();
        assert_eq!(parsed.content_type(), ContentType::Data);
        assert_eq!(parsed.sequence(), 7);
        assert_eq!(parsed.header.get_header("conn_id").unwrap(), "42");
        assert_eq!(parsed.payload().as_ref(), b"hello");
    }

    #[test]
    fn test_build_close_frame() {
        let frame = build_close_frame(13, 99).unwrap();
        assert!(frame.is_close);

        let parsed = ZitiMessage::deserialize(frame.bytes).unwrap();
        assert_eq!(parsed.content_type(), ContentType::Close);
        assert_eq!(parsed.header.get_header("conn_id").unwrap(), "13");
    }

    #[tokio::test]
    async fn channel_stream_reads_inbound_bytes() {
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, _out_rx) = mpsc::unbounded_channel();

        in_tx.send(Bytes::from_static(b"hello ")).unwrap();
        in_tx.send(Bytes::from_static(b"world")).unwrap();
        drop(in_tx);

        let mut stream = ZitiStream::from_channels(1, in_rx, out_tx);
        let mut got = Vec::new();
        stream.read_to_end(&mut got).await.unwrap();
        assert_eq!(got, b"hello world");
        assert!(stream.is_closed());
    }

    #[tokio::test]
    async fn channel_stream_writes_frame_data() {
        let (_in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();

        let mut stream = ZitiStream::from_channels(5, in_rx, out_tx);
        stream.write_all(b"payload").await.unwrap();

        let frame = out_rx.recv().await.unwrap();
        assert_eq!(frame.conn_id, 5);
        let parsed = ZitiMessage::deserialize(frame.bytes).unwrap();
        assert_eq!(parsed.content_type(), ContentType::Data);
        assert_eq!(parsed.header.get_header("conn_id").unwrap(), "5");
        assert_eq!(parsed.payload().as_ref(), b"payload");
    }

    #[tokio::test]
    async fn channel_stream_close_emits_close_frame() {
        let (_in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();

        let mut stream = ZitiStream::from_channels(9, in_rx, out_tx);
        stream.close().await.unwrap();
        assert!(stream.is_closed());

        let frame = out_rx.recv().await.unwrap();
        assert!(frame.is_close);
        let parsed = ZitiMessage::deserialize(frame.bytes).unwrap();
        assert_eq!(parsed.content_type(), ContentType::Close);
        assert_eq!(parsed.header.get_header("conn_id").unwrap(), "9");
    }
}
