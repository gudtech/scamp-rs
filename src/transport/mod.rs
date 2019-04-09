//pub mod beepish;


//pub use self::beepish::Beepish as Transport;

pub struct Transport{

}

use crate::{Agent, Error};
impl Transport{
    pub fn new (agent: &mut Agent) -> Result<Self, Error> {
        unimplemented!()
    }
}

// For now we're not going to worry about actual transport modularity
// Just factoring to make it easier to do that later.

// Later we should support stuff like websocket, https, amqp, etc.