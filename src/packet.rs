use std::collections::HashMap;

pub enum Packet {
    HEADER{ id: u32, values: HashMap<String,String>},
    DATA{ id: u32, buffer: Vec<u8> },
    EOF,
    TXERR,
    ACK,
    PING
}

//struct Packet {
//
//}