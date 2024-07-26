use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use tokio::io::AsyncWriteExt;

const MAX_PACKET_SIZE: usize = 131072;

#[derive(Debug, PartialEq)]
pub enum PacketType {
    Header,
    Data,
    Eof,
    Txerr,
    Ack,
    Ping,
    Pong,
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

pub enum ParseResult {
    TooShort,
    NeedBytes { bytes: usize },
    Success { packet: Packet, bytes_used: usize },
    Drop { bytes_used: usize },
    Fatal(anyhow::Error),
}

impl Packet {
    pub async fn write<W>(&self, writer: &mut W) -> std::io::Result<usize>
    where
        W: AsyncWriteExt + Unpin,
    {
        let packet_type_bytes: &[u8] = match self.packet_type {
            PacketType::Header => b"HEADER",
            PacketType::Data => b"DATA",
            PacketType::Eof => b"EOF",
            PacketType::Txerr => b"TXERR",
            PacketType::Ack => b"ACK",
            PacketType::Ping => b"PING",
            PacketType::Pong => b"PONG",
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

        writer.write_all(header_bytes.as_bytes()).await?;
        writer.write_all(&body_buf).await?;
        writer.write_all(b"END\r\n").await?;

        Ok(header_bytes.len() + body_buf.len() + 5)
    }

    pub fn parse(buf: &[u8]) -> ParseResult {
        let hdr = if let Ok(hdr) = std::str::from_utf8(&buf[..buf.len().min(40)]) {
            hdr.trim_end_matches('\0')
        } else {
            return ParseResult::Fatal(anyhow!("Invalid UTF-8 in header"));
        };

        if let Some(cut) = hdr.find('\n') {
            let header_line = &hdr[..cut + 1];
            let parts: Vec<&str> = header_line.split_whitespace().collect();
            if parts.len() != 3 {
                return ParseResult::Fatal(anyhow!("Malformed header line"));
            }

            let cmd = parts[0];
            let msg_no: u64 = if let Ok(msg_no) = parts[1].parse() {
                msg_no
            } else {
                return ParseResult::Fatal(anyhow!("Invalid message number"));
            };

            let siz: usize = if let Ok(siz) = parts[2].parse() {
                siz
            } else {
                return ParseResult::Fatal(anyhow!("Invalid packet size"));
            };

            if siz > MAX_PACKET_SIZE {
                return ParseResult::Fatal(anyhow!("Unreasonably large packet"));
            }

            let payload_start = header_line.len();
            let payload_end = payload_start + siz;

            if payload_end + 5 > buf.len() {
                return ParseResult::NeedBytes {
                    bytes: payload_end + 5 - buf.len(),
                };
            }

            let payload = &buf[payload_start..payload_end];
            let trailer = &buf[payload_end..payload_end + 5];

            if trailer != b"END\r\n" {
                return ParseResult::Fatal(anyhow!("Malformed trailer"));
            }

            let packet_type = match cmd {
                "HEADER" => PacketType::Header,
                "DATA" => PacketType::Data,
                "EOF" => PacketType::Eof,
                "TXERR" => PacketType::Txerr,
                "ACK" => PacketType::Ack,
                "PING" => PacketType::Ping,
                "PONG" => PacketType::Pong,
                // Drop otherwise-valid but unknown packet types
                _ => {
                    return ParseResult::Drop {
                        bytes_used: payload_end + 5,
                    }
                }
            };

            let packet = Packet {
                packet_header: if packet_type == PacketType::Header {
                    if let Ok(header) = serde_json::from_slice(payload) {
                        Some(header)
                    } else {
                        return ParseResult::Drop {
                            bytes_used: payload_end + 5,
                        };
                    }
                } else {
                    None
                },
                body: if packet_type != PacketType::Header {
                    payload.to_vec()
                } else {
                    vec![]
                },
                packet_type,
                msg_no,
            };

            ParseResult::Success {
                packet,
                bytes_used: payload_end + 5,
            }
        } else {
            ParseResult::TooShort
        }
    }
}
