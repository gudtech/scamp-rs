
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

// enable the await! macro, async support, and the new std::Futures api.
#![feature(await_macro, async_await, futures_api)]

// only needed to manually implement a std future:
#![feature(arbitrary_self_types)]


// This pulls in the `tokio-async-await` crate. While Rust 2018 doesn't require
// `extern crate`, we need to pull in the macros.
#[macro_use]
extern crate tokio;

pub mod message;
pub mod agent;
pub mod transport;
pub mod action;
pub mod error;
pub(crate) mod common;

pub use crate::message::Message;
pub use crate::agent::Agent;
pub use crate::error::Error;
pub use crate::action::Action;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
