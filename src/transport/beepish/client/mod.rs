//! SCAMP BEEPish client: connection pooling, TLS, request/response.

mod connection;
#[cfg(test)]
mod connection_tests;
mod reader;

pub use connection::{BeepishClient, ConnectionHandle, ScampResponse};
