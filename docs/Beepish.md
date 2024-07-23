# Beepish Packet Protocol

The Beepish packet protocol is used for reliable, bidirectional communication between clients and servers. It supports sending messages split across multiple packets, with acknowledgements and flow control.

## Packet Structure

```
+--------+--------+--------+----------+
| Header | MsgNo  | Length |  Payload |
+--------+--------+--------+----------+
|  ASCII | UINT64 | UINT32 |   Bytes  |
+--------+--------+--------+----------+
```

- **Header**: ASCII string identifying the packet type (`HEADER`, `DATA`, `EOF`, `TXERR`, `ACK`)
- **MsgNo**: 64-bit unsigned integer uniquely identifying the message
- **Length**: 32-bit unsigned integer specifying the length of the payload in bytes
- **Payload**: Variable-length payload (depends on packet type)

## Packet Types

### `HEADER`

- Initiates a new message
- Payload contains a JSON-serialized `PacketHeader` struct

### `DATA`

- Carries a chunk of message data
- Payload contains raw bytes of message data

### `EOF`

- Indicates the end of a message
- Payload is empty

### `TXERR`

- Indicates an error occurred while sending the message
- Payload contains an error message string

### `ACK`

- Acknowledges receipt of message data
- Payload contains an ASCII-encoded 64-bit unsigned integer indicating the number of bytes received

## Connection Flow

1. Sender initiates a new message by sending a `HEADER` packet
2. Sender sends one or more `DATA` packets with chunks of the message
3. Receiver sends `ACK` packets to acknowledge receipt of data
4. Sender throttles sending data to stay within the receiver's flow control window
5. When the message is complete, sender sends an `EOF` packet
6. If an error occurs during sending, sender sends a `TXERR` packet instead of `EOF`
7. Receiver processes the complete message once `EOF` is received

## PacketHeader Structure

The `PacketHeader` struct is serialized as JSON and sent as the payload of `HEADER` packets.

```rust
struct PacketHeader {
    action: String,
    envelope: EnvelopeFormat,
    error: Option<String>,
    error_code: Option<String>,
    request_id: i64,
    client_id: i64,
    ticket: String,
    identifying_token: String,
    message_type: MessageType,
    version: i32,
}
```

- **action**: String specifying the action being performed
- **envelope**: `EnvelopeFormat` enum (`Json` or `JsonStore`)
- **error**: Optional string with an error message
- **error_code**: Optional string with an error code
- **request_id**: 64-bit signed integer uniquely identifying the request
- **client_id**: 64-bit signed integer identifying the client
- **ticket**: String containing an authorization ticket
- **identifying_token**: String token identifying the client/message
- **message_type**: `MessageType` enum (`Request` or `Reply`)
- **version**: 32-bit signed integer specifying the protocol version
