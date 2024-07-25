use std::collections::{BTreeMap, HashMap};

use crate::discovery::ActionEntry;
use crate::transport::{Client, Request, Response};
use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::packet;

pub struct BeepishClient {
    connection: ClientConnection,
}

impl BeepishClient {
    pub fn new() -> Self {
        BeepishClient {
            connection: ClientConnection::new(),
        }
    }
    // pub async fn connect(address: &str) -> Result<Self, std::io::Error> {
    //     // Establish TLS connection and initialize ClientConnection
    //     let stream = TcpStream::connect(address).await?;
    //     let connection = ClientConnection::new(stream);
    //     Ok(BeepishClient { connection })
    // }
}
impl Client for BeepishClient {
    async fn request<'a>(
        &self,
        action: &'a ActionEntry,
        headers: BTreeMap<String, String>,
        body: Box<dyn AsyncRead + Unpin>,
    ) -> Result<Response> {
        // // Send request and wait for response
        // let request_packet = Message::new(PacketHeader::Request, request.body);
        // self.connection.send_message(request_packet).await?;

        // // Stream the body to the service
        // let mut buffer = [0; 1024];
        // let mut body = Box::new(tokio::io::BufReader::new(&request.body[..])) as Box<dyn AsyncRead + Unpin>;
        // loop {
        //     match body.read(&mut buffer).await {
        //         Ok(0) => break,
        //         Ok(bytes_read) => {
        //             let data_packet = Message::new(PacketHeader::Data, buffer[..bytes_read].to_vec());
        //             self.connection.send_message(data_packet).await?;
        //         }
        //         Err(e) => {
        //             eprintln!("Error reading body: {}", e);
        //             return Err(e);
        //         }
        //     }
        // }

        // // Receive the response packet
        // let response_packet = self.connection.on_packet().await?;
        // let response = Response::from_packet(response_packet);

        // // Print the response headers
        // let mut headers = BTreeMap::new();
        // headers.insert("content-type".to_string(), "application/json".to_string());
        // println!("    * Response headers: {:?}", headers);

        // Ok(response)
        unimplemented!()
    }
}

struct ClientConnection {
    incoming: HashMap<u64, Message>,
    outgoing: HashMap<u64, Message>,
    next_incoming_id: u64,
    next_outgoing_id: u64,
}

impl ClientConnection {
    fn new() -> Self {
        ClientConnection {
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

        unimplemented!()
        // let header_packet = packet::Packet {
        //     packet_type: packet::PacketType::Header,
        //     msg_no: id,
        //     packet_header: Some(msg.header.clone()),
        //     body: Vec::new(),
        // };
        // TODO: Send header packet

        // TODO: Send data packets as message is consumed

        // TODO: Send EOF or TXERR packet when message ends
    }

    fn on_packet(&mut self, packet_type: packet::PacketType, msg_no: u64, payload: &[u8]) {
        use packet::PacketType;
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
    header: packet::PacketHeader,
    // TODO: Add fields to track message state
}
