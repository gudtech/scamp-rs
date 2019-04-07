// The nightly features that are commonly needed with async / await
#![feature(await_macro, async_await, futures_api)]

// This pulls in the `tokio-async-await` crate. While Rust 2018 doesn't require
// `extern crate`, we need to pull in the macros.
#[macro_use]
extern crate tokio;

mod packet;
pub mod message;
pub mod agent;
pub mod transport;
pub mod error;

pub use crate::message::Message;
pub use crate::agent::Agent;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
