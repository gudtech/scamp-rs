//! SCAMP BEEPish wire protocol types: packet framing, header JSON, serde.

mod header;
mod packet;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod tests;

pub use header::{EnvelopeFormat, FlexInt, MessageType, PacketHeader};
pub use packet::{Packet, ParseResult};

pub const MAX_PACKET_SIZE: usize = 131072;

/// Maximum DATA chunk size when sending. All receivers handle up to 131072.
/// Perl uses 2048, Go uses 128KB, JS uses 131072.
pub const DATA_CHUNK_SIZE: usize = 131072;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PacketType {
    Header,
    Data,
    Eof,
    Txerr,
    Ack,
    Ping,
    Pong,
}
