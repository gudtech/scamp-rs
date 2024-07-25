use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

const MAX_PACKET_SIZE: usize = 131072;

#[derive(Debug, PartialEq)]
pub enum PacketType {
    Header,
    Data,
    Eof,
    Txerr,
    Ack,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PacketHeader {
    action: String,
    envelope: EnvelopeFormat,
    error: Option<String>,
    error_code: Option<String>,
    request_id: i64,
    client_id: i64,
    ticket: String,
    identifying_token: String,
    message_type: MessageType,
    version: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum EnvelopeFormat {
    Json,
    JsonStore,
    Other(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    Request,
    Reply,
}

pub struct Packet {
    pub packet_type: PacketType,
    pub msg_no: u64,
    pub packet_header: Option<PacketHeader>,
    pub body: Vec<u8>,
}

impl Packet {
    fn write(&self, writer: &mut dyn Write) -> std::io::Result<usize> {
        let packet_type_bytes: &[u8] = match self.packet_type {
            PacketType::Header => b"HEADER",
            PacketType::Data => b"DATA",
            PacketType::Eof => b"EOF",
            PacketType::Txerr => b"TXERR",
            PacketType::Ack => b"ACK",
        };

        let mut body_buf = Vec::new();
        if let Some(header) = &self.packet_header {
            serde_json::to_writer(&mut body_buf, header)?;
        } else {
            body_buf.extend_from_slice(&self.body);
        }

        let header_bytes = format!(
            "{} {} {}\r\n",
            std::str::from_utf8(packet_type_bytes).unwrap(),
            self.msg_no,
            body_buf.len()
        );
        writer.write_all(header_bytes.as_bytes())?;
        writer.write_all(&body_buf)?;
        writer.write_all(b"END\r\n")?;

        Ok(header_bytes.len() + body_buf.len() + 5)
    }
}
