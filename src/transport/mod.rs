pub mod beepish;


pub use self::beepish::Beepish as Transport;
// For now we're not going to worry about actual transport modularity
// Just factoring to make it easier to do that later.

// Later we should support stuff like websocket, https, amqp, etc.