use std::collections::HashMap;

enum Packet {
    HEADER{ id: u32, values: HashMap<String,String>)
    DATA{ id: u32, buffer: Vec<u8> },
    EOF,
    TXERR,
    ACK
}
struct Packet {

}