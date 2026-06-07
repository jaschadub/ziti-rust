//! Ziti protocol implementation
//!
//! Handles Ziti protocol messages, headers, and serialization for communication
//! with Edge Routers.

use crate::error::{ZitiError, ZitiResult};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::collections::HashMap;

/// Content type constants for Ziti messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ContentType {
    /// Hello message
    Hello = 0,
    /// Session request
    SessionRequest = 1,
    /// Session response
    SessionResponse = 2,
    /// Data transmission
    Data = 3,
    /// Connection close
    Close = 4,
    /// Keepalive/ping
    Ping = 5,
    /// Pong response
    Pong = 6,
    /// Error message
    Error = 7,
    /// Listener bind handshake (host -> edge router)
    Bind = 8,
    /// Inbound dial notification (edge router -> host)
    Dial = 9,
    /// Response to an inbound dial (host -> edge router)
    DialResponse = 10,
    /// Custom message type
    Custom(u16),
}

impl From<u16> for ContentType {
    fn from(value: u16) -> Self {
        match value {
            0 => ContentType::Hello,
            1 => ContentType::SessionRequest,
            2 => ContentType::SessionResponse,
            3 => ContentType::Data,
            4 => ContentType::Close,
            5 => ContentType::Ping,
            6 => ContentType::Pong,
            7 => ContentType::Error,
            8 => ContentType::Bind,
            9 => ContentType::Dial,
            10 => ContentType::DialResponse,
            other => ContentType::Custom(other),
        }
    }
}

impl From<ContentType> for u16 {
    fn from(content_type: ContentType) -> Self {
        match content_type {
            ContentType::Hello => 0,
            ContentType::SessionRequest => 1,
            ContentType::SessionResponse => 2,
            ContentType::Data => 3,
            ContentType::Close => 4,
            ContentType::Ping => 5,
            ContentType::Pong => 6,
            ContentType::Error => 7,
            ContentType::Bind => 8,
            ContentType::Dial => 9,
            ContentType::DialResponse => 10,
            ContentType::Custom(value) => value,
        }
    }
}

/// Ziti message header structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    /// Content type of the message
    pub content_type: ContentType,
    /// Sequence number for message ordering
    pub sequence: u32,
    /// Length of the payload in bytes
    pub payload_length: u32,
    /// Additional headers as key-value pairs
    pub headers: HashMap<String, String>,
}

impl MessageHeader {
    /// Create a new message header
    pub fn new(content_type: ContentType, sequence: u32, payload_length: u32) -> Self {
        Self {
            content_type,
            sequence,
            payload_length,
            headers: HashMap::new(),
        }
    }

    /// Add a header field
    pub fn add_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    /// Get a header field
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    /// Serialize the header to bytes
    pub fn serialize(&self) -> ZitiResult<Bytes> {
        let mut buf = BytesMut::new();

        // Write content type (2 bytes)
        buf.put_u16(self.content_type.into());

        // Write sequence number (4 bytes)
        buf.put_u32(self.sequence);

        // Write payload length (4 bytes)
        buf.put_u32(self.payload_length);

        // Write headers count (2 bytes)
        buf.put_u16(self.headers.len() as u16);

        // Write headers
        for (key, value) in &self.headers {
            // Write key length and key
            buf.put_u16(key.len() as u16);
            buf.put_slice(key.as_bytes());

            // Write value length and value
            buf.put_u16(value.len() as u16);
            buf.put_slice(value.as_bytes());
        }

        Ok(buf.freeze())
    }

    /// Deserialize header from bytes
    pub fn deserialize(mut data: Bytes) -> ZitiResult<Self> {
        if data.len() < 12 {
            return Err(ZitiError::ProtocolError {
                message: "Header too short".to_string(),
            });
        }

        // Read content type
        let content_type = ContentType::from(data.get_u16());

        // Read sequence number
        let sequence = data.get_u32();

        // Read payload length
        let payload_length = data.get_u32();

        // Read headers count
        let headers_count = data.get_u16();

        let mut headers = HashMap::new();

        // Read headers
        for _ in 0..headers_count {
            if data.remaining() < 4 {
                return Err(ZitiError::ProtocolError {
                    message: "Invalid header format".to_string(),
                });
            }

            // Read key
            let key_len = data.get_u16() as usize;
            if data.remaining() < key_len {
                return Err(ZitiError::ProtocolError {
                    message: "Invalid key length".to_string(),
                });
            }
            let key = String::from_utf8(data.copy_to_bytes(key_len).to_vec()).map_err(|_| {
                ZitiError::ProtocolError {
                    message: "Invalid UTF-8 in header key".to_string(),
                }
            })?;

            // Read value
            if data.remaining() < 2 {
                return Err(ZitiError::ProtocolError {
                    message: "Missing value length".to_string(),
                });
            }
            let value_len = data.get_u16() as usize;
            if data.remaining() < value_len {
                return Err(ZitiError::ProtocolError {
                    message: "Invalid value length".to_string(),
                });
            }
            let value = String::from_utf8(data.copy_to_bytes(value_len).to_vec()).map_err(|_| {
                ZitiError::ProtocolError {
                    message: "Invalid UTF-8 in header value".to_string(),
                }
            })?;

            headers.insert(key, value);
        }

        Ok(Self {
            content_type,
            sequence,
            payload_length,
            headers,
        })
    }
}

/// Complete Ziti message with header and payload
#[derive(Debug, Clone)]
pub struct ZitiMessage {
    /// Message header
    pub header: MessageHeader,
    /// Message payload
    pub payload: Bytes,
}

impl ZitiMessage {
    /// Create a new Ziti message
    pub fn new(content_type: ContentType, sequence: u32, payload: Bytes) -> Self {
        let header = MessageHeader::new(content_type, sequence, payload.len() as u32);
        Self { header, payload }
    }

    /// Create a message with headers
    pub fn with_headers(
        content_type: ContentType,
        sequence: u32,
        payload: Bytes,
        headers: HashMap<String, String>,
    ) -> Self {
        let mut header = MessageHeader::new(content_type, sequence, payload.len() as u32);
        header.headers = headers;
        Self { header, payload }
    }

    /// Serialize the complete message to bytes
    pub fn serialize(&self) -> ZitiResult<Bytes> {
        let header_bytes = self.header.serialize()?;
        let mut buf = BytesMut::with_capacity(header_bytes.len() + self.payload.len());
        buf.put(header_bytes);
        buf.put(self.payload.clone());
        Ok(buf.freeze())
    }

    /// Deserialize a complete message from bytes
    pub fn deserialize(data: Bytes) -> ZitiResult<Self> {
        // First, deserialize the header to get payload length
        let header = MessageHeader::deserialize(data.clone())?;

        // Calculate header size by serializing it again
        let header_bytes = header.serialize()?;
        let header_size = header_bytes.len();

        // Extract payload
        if data.len() < header_size + header.payload_length as usize {
            return Err(ZitiError::ProtocolError {
                message: "Message shorter than expected payload length".to_string(),
            });
        }

        let payload = data.slice(header_size..header_size + header.payload_length as usize);

        Ok(Self { header, payload })
    }

    /// Get the content type
    pub fn content_type(&self) -> ContentType {
        self.header.content_type
    }

    /// Get the sequence number
    pub fn sequence(&self) -> u32 {
        self.header.sequence
    }

    /// Get the payload
    pub fn payload(&self) -> &Bytes {
        &self.payload
    }
}

/// Ziti protocol handler for message processing
#[derive(Debug)]
pub struct ZitiProtocol {
    /// Next sequence number to use
    next_sequence: u32,
}

impl ZitiProtocol {
    /// Create a new protocol handler
    pub fn new() -> Self {
        Self { next_sequence: 1 }
    }

    /// Get the next sequence number
    pub fn next_sequence(&mut self) -> u32 {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        seq
    }

    /// Create a new message with auto-incrementing sequence
    pub fn create_message(&mut self, content_type: ContentType, payload: Bytes) -> ZitiMessage {
        ZitiMessage::new(content_type, self.next_sequence(), payload)
    }

    /// Reset sequence counter
    pub fn reset_sequence(&mut self) {
        self.next_sequence = 1;
    }
}

impl Default for ZitiProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_conversion() {
        assert_eq!(ContentType::from(0), ContentType::Hello);
        assert_eq!(ContentType::from(100), ContentType::Custom(100));
        assert_eq!(u16::from(ContentType::Hello), 0);
        assert_eq!(u16::from(ContentType::Custom(100)), 100);
    }

    #[test]
    fn test_header_serialization() {
        let mut header = MessageHeader::new(ContentType::Hello, 42, 1024);
        header.add_header("test".to_string(), "value".to_string());

        let serialized = header.serialize().unwrap();
        let deserialized = MessageHeader::deserialize(serialized).unwrap();

        assert_eq!(header.content_type, deserialized.content_type);
        assert_eq!(header.sequence, deserialized.sequence);
        assert_eq!(header.payload_length, deserialized.payload_length);
        assert_eq!(header.headers, deserialized.headers);
    }

    #[test]
    fn test_message_serialization() {
        let payload = Bytes::from("test payload");
        let message = ZitiMessage::new(ContentType::Data, 123, payload.clone());

        let serialized = message.serialize().unwrap();
        let deserialized = ZitiMessage::deserialize(serialized).unwrap();

        assert_eq!(message.content_type(), deserialized.content_type());
        assert_eq!(message.sequence(), deserialized.sequence());
        assert_eq!(message.payload(), deserialized.payload());
    }

    #[test]
    fn test_protocol_sequence() {
        let mut protocol = ZitiProtocol::new();
        assert_eq!(protocol.next_sequence(), 1);
        assert_eq!(protocol.next_sequence(), 2);
        assert_eq!(protocol.next_sequence(), 3);

        protocol.reset_sequence();
        assert_eq!(protocol.next_sequence(), 1);
    }
}
