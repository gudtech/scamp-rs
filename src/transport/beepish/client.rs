use anyhow::{anyhow, Context, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{self, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf,
    WriteHalf,
};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{interval, timeout};
use tokio_native_tls::{native_tls, TlsConnector, TlsStream};

use super::proto::{Packet, PacketHeader, PacketType, ParseResult};
use crate::config::Config;
use crate::discovery::{ActionEntry, ServiceInfo};
use crate::transport::{Client, Response};

const MAX_FLOW: usize = 65536;

fn assert_send_sync<T: Send + Sync>() {}

fn main() {
    assert_send_sync::<TlsStream<TcpStream>>();
}

pub struct BeepishClient {
    config: Config,
    connections: Arc<Mutex<HashMap<String, Arc<ClientConnection>>>>,
}

// Lets try to simplify the connection struct
// The big question is: How do we dispatch received messages to the correct response stream
struct ClientConnection {
    config: Config,
    writer: Arc<WriteHalf<TlsStream<TcpStream>>>,
    reader_handle: Option<tokio::task::JoinHandle<()>>,
    // incoming: HashMap<u64, IncomingMessage>,
    // outgoing: HashMap<u64, OutgoingMessage>,
    // next_incoming_id: u64,
    // next_outgoing_id: u64,
    // heartbeat_interval: Option<Duration>,
    // last_heartbeat: Instant,
}

struct Message {
    header: PacketHeader,
    body: Box<dyn AsyncRead + Unpin + Send>,
}

struct IncomingMessage {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
    acked: usize,
}

struct OutgoingMessage {
    message: Message,
    response_tx: Option<oneshot::Sender<Response>>,
    sent: usize,
    acked: usize,
}

impl BeepishClient {
    pub fn new(config: &Config) -> Self {
        BeepishClient {
            config: config.clone(),
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn connect(&self, service_info: &ServiceInfo) -> Result<Arc<ClientConnection>> {
        let mut connections = self.connections.lock().await;

        use std::collections::hash_map::Entry;
        match connections.entry(service_info.uri.clone()) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let connection = Arc::new(ClientConnection::new(&self.config, service_info).await?);
                Ok(entry.insert(connection).clone())
            }
        }
    }
}

impl Client for BeepishClient {
    async fn request<'a>(
        &self,
        action: &'a ActionEntry,
        headers: BTreeMap<String, String>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<Response> {
        //         let connection = self.connect(&action.service_info).await?;

        //         // let (response_tx, response_rx) = oneshot::channel();

        //         // let request = Message::new(PacketHeader::Request(headers), body);
        //         // connection.send_message(request, response_tx).await?;

        //         // match tokio::time::timeout(Duration::from_secs(30), response_rx).await {
        //         //     Ok(Ok(response)) => Ok(response),
        //         //     Ok(Err(_)) => Err(anyhow!("Failed to receive response")),
        //         //     Err(_) => Err(anyhow!("Request timed out")),
        //         // }
        todo!()
    }
}

impl ClientConnection {
    async fn new(config: &Config, service_info: &ServiceInfo) -> Result<Self> {
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true) // Note: Only use this for testing!
            .build()?;
        let connector = TlsConnector::from(tls);

        let addr = service_info.socket_addr();

        // Add timeout to TCP connection
        let stream = timeout(Duration::from_secs(30), TcpStream::connect(addr))
            .await
            .context("TCP connection timed out")?
            .context("Failed to connect to TCP stream")?;

        //         // Add timeout to TLS connection
        let mut tls_stream = timeout(
            Duration::from_secs(30),
            connector.connect("foo.com", stream),
        )
        .await
        .context("TLS connection timed out")?
        .context("Failed to establish TLS connection")?;

        // Add timeout to BEEPish handshake
        timeout(Duration::from_secs(10), async {
            tls_stream.write_all(b"BEEP\r\n").await?;

            let mut response = [0u8; 6];
            tls_stream.read_exact(&mut response).await?;
            if &response != b"BEEP\r\n" {
                return Err(anyhow!("Invalid BEEPish handshake response"));
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("BEEPish handshake timed out")??;

        let (reader, writer) = tokio::io::split(tls_stream);

        // spawn a task to handle inbound messages
        // the task should be able to directly own the reader without an Arc

        let mut connection = ClientConnection {
            config: config.clone(),
            writer: Arc::new(writer),
            reader_handle: None,
            //             incoming: HashMap::new(),
            //             outgoing: HashMap::new(),
            //             next_incoming_id: 0,
            //             next_outgoing_id: 0,
            //             heartbeat_interval: None,
            //             last_heartbeat: Instant::now(),
        };

        connection.setup_reader(reader)?;

        let heartbeat_interval = config
            .get::<u64>("beepish.heartbeat_interval")
            .unwrap_or(Ok(10))?;

        let mut interval_timer = tokio::time::interval(Duration::from_secs(heartbeat_interval));
        // connection.set_heartbeat(Duration::from_millis(heartbeat_interval))?;
        tokio::spawn(async move {
            loop {
                interval_timer.tick().await;

                let packet = Packet {
                    packet_type: PacketType::Ping,
                    msg_no: 0,
                    body: vec![],
                    packet_header: None,
                };
                // self.send_packet(packet).await?;
                todo!()
            }
        });

        Ok(connection)
    }

    fn setup_reader(&mut self, reader: ReadHalf<TlsStream<TcpStream>>) -> Result<()> {
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(reader);

            loop {
                // Fill the buffer with data from the reader
                let buf = reader.fill_buf().await.unwrap_or(&[]);
                if buf.is_empty() {
                    break;
                }

                let mut bytes_used = 0;

                while bytes_used < buf.len() {
                    match Packet::parse(&buf[bytes_used..]) {
                        ParseResult::TooShort => {
                            // Not enough data to determine packet length, wait for more data
                            break;
                        }
                        ParseResult::NeedBytes { bytes } => {
                            // Not enough data for the full packet, wait for more data
                            if bytes_used + bytes > buf.len() {
                                // This shouldn't happen with BufReader, but handle it just in case
                                break;
                            }
                            break;
                        }
                        ParseResult::Drop { bytes_used: used } => {
                            // Drop the packet and consume the used bytes
                            bytes_used += used;
                        }
                        ParseResult::Success {
                            packet,
                            bytes_used: used,
                        } => {
                            // Successfully parsed a packet, consume the used bytes
                            bytes_used += used;
                            Self::receive_packet(packet);
                        }
                        ParseResult::Fatal(err) => {
                            // Handle fatal error, close the connection
                            eprintln!("Fatal error: {}", err);
                            return;
                        }
                    }
                }

                // Consume the processed data from the buffer
                reader.consume(bytes_used);
            }
        });

        self.reader_handle = Some(reader_handle);

        Ok(())
    }
    fn receive_packet(packet: Packet) {
        match packet.packet_type {
            PacketType::Header => {
                // Handle HEADER packet
            }
            PacketType::Data => {
                // Handle DATA packet
            }
            PacketType::Eof => {
                // Handle EOF packet
            }
            PacketType::Txerr => {
                // Handle TXERR packet
            }
            PacketType::Ack => {
                // Handle ACK packet
            }
            PacketType::Ping => {
                // Handle PING packet
            }
            PacketType::Pong => {
                // Handle PONG packet
            }
        }
    }
    async fn send_packet(&mut self, packet: Packet) -> Result<()> {
        // let mut stream = self.stream.lock().await;
        // packet.write(&*self.writer).await?;
        todo!();
        Ok(())
    }
    //     // Removed beepish_handshake method

    //     fn set_heartbeat(&mut self, interval: Duration) -> Result<()> {
    //         self.heartbeat_interval = Some(interval);

    //         let stream = Arc::clone(&self.stream);
    //         l

    //         Ok(())
    //     }

    //     async fn handle_heartbeat(&mut self) -> Result<()> {
    //         if let Some(interval) = self.heartbeat_interval {
    //             if self.last_heartbeat.elapsed() >= interval {
    //                 self.send_ping().await?;
    //                 self.last_heartbeat = Instant::now();
    //             }
    //         }
    //         Ok(())
    //     }

    //     async fn handle_pong(&mut self) -> Result<()> {
    //         self.last_heartbeat = Instant::now();
    //         Ok(())
    //     }

    //     async fn send_message(
    //         &mut self,
    //         msg: Message,
    //         response_tx: oneshot::Sender<Response>,
    //     ) -> Result<()> {
    //         let id = {
    //             let mut outgoing = self.outgoing.lock().await;
    //             let id = self.next_outgoing_id;
    //             self.next_outgoing_id += 1;

    //             outgoing.insert(
    //                 id,
    //                 OutgoingMessage {
    //                     message: msg,
    //                     response_tx: Some(response_tx),
    //                     sent: 0,
    //                     acked: 0,
    //                 },
    //             );

    //             id
    //         };

    //         self.send_packet(PacketType::Header, id, &serde_json::to_vec(&msg.header)?)
    //             .await?;

    //         // Start sending data packets
    //         self.send_data_packets(id).await?;

    //         Ok(())
    //     }

    //     async fn send_data_packets(&mut self, id: u64) -> Result<()> {
    //         let outgoing = self
    //             .outgoing
    //             .get_mut(&id)
    //             .ok_or_else(|| anyhow!("Message not found"))?;
    //         let mut buffer = [0u8; 1024];

    //         loop {
    //             match outgoing.message.body.read(&mut buffer).await {
    //                 Ok(0) => break, // EOF
    //                 Ok(n) => {
    //                     self.send_packet(
    //                         PacketType::Data,
    //                         id,
    //                         &buffer[..n],
    //                     )
    //                     .await?;
    //                     outgoing.sent += n;
    //                     if outgoing.sent - outgoing.acked >= MAX_FLOW {
    //                         break; // Flow control: pause sending
    //                     }
    //                 }
    //                 Err(e) => return Err(e.into()),
    //             }
    //         }

    //         if outgoing.message.body.read(&mut buffer).await? == 0 {
    //             // All data sent, send EOF
    //             self.send_packet(
    //                 PacketType::Eof,
    //                 id,
    //                 &[] as &[u8],
    //             )
    //             .await?;
    //         }

    //         Ok(())
    //     }

    //     async fn handle_incoming_packet(&mut self, packet: Packet) -> Result<()> {
    //         match packet.packet_type {
    //             PacketType::Header => self.handle_header_packet(packet).await?,
    //             PacketType::Data => self.handle_data_packet(packet).await?,
    //             PacketType::Eof => self.handle_eof_packet(packet).await?,
    //             PacketType::Txerr => self.handle_txerr_packet(packet).await?,
    //             PacketType::Ack => self.handle_ack_packet(packet).await?,
    //             PacketType::Ping => {
    //                 self.send_packet(
    //                     PacketType::Pong,
    //                     packet.msg_no,
    //                     &[] as &[u8],
    //                 )
    //                 .await?;
    //             }
    //             PacketType::Pong => self.handle_pong().await?,
    //         }
    //         Ok(())
    //     }

    //     async fn handle_header_packet(&mut self, packet: Packet) -> Result<()> {
    //         let header: PacketHeader = serde_json::from_slice(&packet.payload)?;
    //         let incoming = IncomingMessage {
    //             header,
    //             body: Vec::new(),
    //             received: 0,
    //             acked: 0,
    //         };
    //         self.incoming.insert(packet.msg_no, incoming);
    //         Ok(())
    //     }

    //     async fn handle_data_packet(&mut self, packet: Packet) -> Result<()> {
    //         let incoming = self
    //             .incoming
    //             .get_mut(&packet.msg_no)
    //             .ok_or_else(|| anyhow!("Received DATA for unknown message"))?;
    //         incoming.body.extend_from_slice(&packet.payload);
    //         incoming.received += packet.payload.len();

    //         // Send ACK if we've received a significant amount of data
    //         if incoming.received - incoming.acked >= MAX_FLOW / 2 {
    //             self.send_packet(
    //                 PacketType::Ack,
    //                 packet.msg_no,
    //                 &incoming.received.to_le_bytes(),
    //             )
    //             .await?;
    //             incoming.acked = incoming.received;
    //         }
    //         Ok(())
    //     }

    //     async fn handle_eof_packet(&mut self, packet: Packet) -> Result<()> {
    //         let incoming = self
    //             .incoming
    //             .remove(&packet.msg_no)
    //             .ok_or_else(|| anyhow!("Received EOF for unknown message"))?;

    //         let response = Response {
    //             headers: incoming.header.into_headers(),
    //             body: incoming.body,
    //         };

    //         if let Some(outgoing) = self.outgoing.remove(&packet.msg_no) {
    //             if let Some(response_tx) = outgoing.response_tx {
    //                 response_tx
    //                     .send(response)
    //                     .map_err(|_| anyhow!("Failed to send response"))?;
    //             }
    //         }

    //         Ok(())
    //     }

    //     async fn handle_txerr_packet(&mut self, packet: Packet) -> Result<()> {
    //         let error_message = String::from_utf8(packet.payload)?;
    //         log::error!(
    //             "Received TXERR for message {}: {}",
    //             packet.msg_no,
    //             error_message
    //         );

    //         if let Some(outgoing) = self.outgoing.remove(&packet.msg_no) {
    //             if let Some(response_tx) = outgoing.response_tx {
    //                 let error_response = Response {
    //                     headers: BTreeMap::new(),
    //                     body: error_message.into_bytes(),
    //                 };
    //                 response_tx
    //                     .send(error_response)
    //                     .map_err(|_| anyhow!("Failed to send error response"))?;
    //             }
    //         }

    //         self.incoming.remove(&packet.msg_no);
    //         Ok(())
    //     }

    //     async fn handle_ack_packet(&mut self, packet: Packet) -> Result<()> {
    //         let acked = u64::from_le_bytes(
    //             packet
    //                 .payload
    //                 .try_into()
    //                 .map_err(|_| anyhow!("Invalid ACK payload"))?,
    //         );
    //         if let Some(outgoing) = self.outgoing.get_mut(&packet.msg_no) {
    //             outgoing.acked = acked;
    //             if outgoing.sent - outgoing.acked < MAX_FLOW {
    //                 // Resume sending if we were paused due to flow control
    //                 self.send_data_packets(packet.msg_no).await?;
    //             }
    //         }
    //         Ok(())
    //     }
}
