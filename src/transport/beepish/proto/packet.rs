//! Packet framing: parse and write SCAMP BEEPish packets.
//!
//! Wire format: `TYPE MSGNO SIZE\r\n<body>END\r\n`
//! Matches Perl Connection.pm:46,192-201.

use anyhow::anyhow;
use tokio::io::AsyncWriteExt;

use super::{PacketHeader, PacketType, MAX_PACKET_SIZE};

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
        let type_str: &[u8] = match self.packet_type {
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

        let header_line = format!(
            "{} {} {}\r\n",
            std::str::from_utf8(type_str).unwrap(),
            self.msg_no,
            body_buf.len()
        );

        writer.write_all(header_line.as_bytes()).await?;
        writer.write_all(&body_buf).await?;
        writer.write_all(b"END\r\n").await?;

        Ok(header_line.len() + body_buf.len() + 5)
    }

    pub fn parse(buf: &[u8]) -> ParseResult {
        if buf.is_empty() {
            return ParseResult::TooShort;
        }

        // Perl Connection.pm:46 limits header line to 80 bytes
        let hdr_len = buf.len().min(80);
        let hdr = match std::str::from_utf8(&buf[..hdr_len]) {
            Ok(hdr) => hdr.trim_end_matches('\0'),
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid UTF-8 in header")),
        };

        let Some(cut) = hdr.find('\n') else {
            if hdr_len >= 80 {
                return ParseResult::Fatal(anyhow!("Overlong header line"));
            }
            return ParseResult::TooShort;
        };

        let header_line = &hdr[..cut + 1];
        let parts: Vec<&str> = header_line.split_whitespace().collect();
        if parts.len() != 3 {
            return ParseResult::Fatal(anyhow!("Malformed header line"));
        }

        let cmd = parts[0];
        let msg_no: u64 = match parts[1].parse() {
            Ok(n) => n,
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid message number")),
        };
        let siz: usize = match parts[2].parse() {
            Ok(n) => n,
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid packet size")),
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
            _ => {
                return ParseResult::Drop {
                    bytes_used: payload_end + 5,
                }
            }
        };

        let packet = Packet {
            packet_header: if packet_type == PacketType::Header {
                match serde_json::from_slice(payload) {
                    Ok(header) => Some(header),
                    Err(_) => {
                        return ParseResult::Drop {
                            bytes_used: payload_end + 5,
                        };
                    }
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
    }
}
