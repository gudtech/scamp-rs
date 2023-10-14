pub mod action;
pub mod agent;
pub(crate) mod common;
pub mod error;
pub mod message;
pub mod transport;

pub use crate::action::Action;
pub use crate::agent::Agent;
pub use crate::error::Error;
pub use crate::message::Message;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
