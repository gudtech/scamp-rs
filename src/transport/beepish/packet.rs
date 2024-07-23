use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

const MAX_PACKET_SIZE: usize = 131072;

#[derive(Debug, PartialEq)]
enum PacketType {
    Header,
    Data,
    Eof,
    Txerr,
    Ack,
}

#[derive(Debug, Serialize, Deserialize)]
struct PacketHeader {
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
enum EnvelopeFormat {
    Json,
    JsonStore,
    Other(String),
}

#[derive(Debug, Serialize, Deserialize)]
enum MessageType {
    Request,
    Reply,
}

struct Packet {
    packet_type: PacketType,
    msg_no: u64,
    packet_header: Option<PacketHeader>,
    body: Vec<u8>,
}

impl Packet {
    fn write(&self, writer: &mut dyn Write) -> std::io::Result<usize> {
        let packet_type_bytes = match self.packet_type {
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

struct Connection {
    incoming: HashMap<u64, Message>,
    outgoing: HashMap<u64, Message>,
    next_incoming_id: u64,
    next_outgoing_id: u64,
}

impl Connection {
    fn new() -> Self {
        Connection {
            incoming: HashMap::new(),
            outgoing: HashMap::new(),
            next_incoming_id: 0,
            next_outgoing_id: 0,
        }
    }

    fn send_message(&mut self, msg: Message) {
        let id = self.next_outgoing_id;
        self.next_outgoing_id += 1;

        self.outgoing.insert(id, msg);

        let header_packet = Packet {
            packet_type: PacketType::Header,
            msg_no: id,
            packet_header: Some(msg.header),
            body: Vec::new(),
        };
        // TODO: Send header packet

        // TODO: Send data packets as message is consumed

        // TODO: Send EOF or TXERR packet when message ends
    }

    fn on_packet(&mut self, packet_type: PacketType, msg_no: u64, payload: &[u8]) {
        match packet_type {
            PacketType::Header => {
                // TODO: Handle incoming header packet
            }
            PacketType::Data => {
                // TODO: Handle incoming data packet
            }
            PacketType::Eof => {
                // TODO: Handle incoming EOF packet
            }
            PacketType::Txerr => {
                // TODO: Handle incoming TXERR packet
            }
            PacketType::Ack => {
                // TODO: Handle incoming ACK packet
            }
        }
    }
}

struct Message {
    header: PacketHeader,
    // TODO: Add fields to track message state
}
