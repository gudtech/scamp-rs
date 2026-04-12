//! SCAMP BEEPish client: connection pooling, TLS, request/response.

mod connection;
mod reader;

pub use connection::{BeepishClient, ConnectionHandle, ScampResponse};
