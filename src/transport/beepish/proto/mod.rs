//! SCAMP BEEPish wire protocol types: packet framing, header JSON, serde.

#[cfg(test)]
pub(crate) mod fixtures;
mod header;
mod packet;
#[cfg(test)]
mod tests;

pub use header::{EnvelopeFormat, FlexInt, MessageType, PacketHeader};
pub use packet::{Packet, ParseResult};

pub const MAX_PACKET_SIZE: usize = 131072;

/// Maximum DATA chunk size when sending.
/// Perl Connection.pm:218 uses 2048. All receivers handle up to MAX_PACKET_SIZE.
pub const DATA_CHUNK_SIZE: usize = 2048;

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
