# Prompt: Phase 1 — Transport Core

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`, implementing the core transport layer for the SCAMP protocol. Phase 0 cleanup should already be done.

Reference implementations:
- **scamp-go** (primary): `/Users/daniel/GT/repo/scamp-go/scamp/` — especially `connection.go`, `message.go`, `packet.go`, `packetheader.go`, `client.go`
- **scamp-js** (secondary): `/Users/daniel/GT/repo/scamp-js/lib/transport/beepish/` — especially `connection.js`, `client.js`
- Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Architecture

Implement the following connection architecture (using tokio):

```
ScampClient
  connections: HashMap<String, Arc<ConnectionHandle>>

ConnectionHandle
  writer_tx: mpsc::Sender<Packet>       // serialized writes through channel
  pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ScampResponse>>>>
  next_outgoing_msg_no: AtomicU64
  closed: AtomicBool

// Spawned tasks per connection:
// 1. Reader task: reads packets from TLS stream, assembles messages, delivers via pending map
// 2. Writer task: receives packets from mpsc channel, writes to TLS stream
// 3. Heartbeat task (optional): sends PING periodically, tracks PONG responses
```

## Tasks

### 1. Fix PacketHeader serialization (`src/transport/beepish/proto.rs`)

The existing `PacketHeader` uses serde derive, but `EnvelopeFormat` and `MessageType` need custom serde to match the wire format:

- `EnvelopeFormat::Json` ↔ `"json"`, `EnvelopeFormat::JsonStore` ↔ `"jsonstore"`, `EnvelopeFormat::Other(s)` ↔ `s`
- `MessageType::Request` ↔ `"request"`, `MessageType::Reply` ↔ `"reply"`
- Add `FlexInt` type for `client_id` that deserializes from both `42` and `"42"` in JSON

Reference: Go `packetheader.go` marshals these as lowercase strings. Verify by reading the Go source.

Write unit tests: serialize a PacketHeader to JSON, verify it matches the format Go/JS would produce. Deserialize a JSON header that Go/JS would send, verify it parses correctly.

### 2. Define Message and Response types

Create `src/message.rs` (fresh, not the old dead code):

```rust
pub struct ScampRequest {
    pub action: String,
    pub version: i32,
    pub envelope: EnvelopeFormat,
    pub request_id: u64,   // or i64 to match header
    pub client_id: FlexInt,
    pub ticket: String,
    pub identifying_token: String,
    pub body: Vec<u8>,
}

pub struct ScampResponse {
    pub request_id: u64,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub body: Vec<u8>,
}
```

Implement `ScampRequest::to_packets()` — serialize into HEADER packet + DATA chunk packets (max 131072 bytes each) + EOF packet. Reference: Go `message.go` `toPackets()`.

Implement `ScampResponse::from_packets()` — assemble from HEADER + DATA* + EOF/TXERR.

### 3. Inbound message assembly

In the connection reader task, implement packet routing:

- Track `next_incoming_msg_no: u64`, validate each incoming packet's msg_no matches
- On HEADER: start a new in-progress message (store header, begin body accumulation)
- On DATA: append body bytes to in-progress message for this msg_no
- On EOF: complete the message, deliver to the pending requests map as a response
- On TXERR: complete the message as an error, deliver to pending map
- On ACK: update flow control state (acked bytes)
- On PONG: mark heartbeat as received

Reference: Go `connection.go` `routePacket()`; JS `connection.js` `_onpacket` handlers (onHeaderPacket, onDataPacket, onEofPacket, etc.)

### 4. Outbound message serialization

Implement `send_request()` on ConnectionHandle:

1. Allocate next outgoing msg_no (atomic increment)
2. Create a `oneshot::channel` for the response
3. Insert sender into `pending` map keyed by msg_no (or request_id)
4. Serialize request to packets via `to_packets()`
5. Send all packets through `writer_tx` channel
6. Return the oneshot receiver (caller awaits it)

### 5. Request-response correlation

Incoming replies are matched to waiting requests. The key question: Go uses `request_id` (from the header). JS uses an internal correlation counter mapped to `request_id`.

Read both implementations to understand the exact correlation mechanism, then implement for Rust. The `pending` map should be keyed on whatever field the reply carries that lets us find the original requester.

### 6. Timeout per request

Wrap the response future with `tokio::time::timeout(duration, receiver)`. Default timeout: 75 seconds (from `rpc.timeout` in soa.conf). On timeout, remove the pending entry and return `ScampError::Timeout`.

### 7. Flow control

- After receiving each DATA packet, send an ACK packet with the number of bytes received
- Track `bytes_sent` and `bytes_acked` per outgoing message
- If `bytes_sent - bytes_acked >= 65536`, pause sending (don't send more DATA packets until ACK arrives)
- On ACK receipt, update `bytes_acked` and resume if paused

Reference: JS `connection.js` ACK handling is the most complete. Go sends ACKs but doesn't pause on receive.

### 8. PING/PONG heartbeat

- Optionally spawn a heartbeat task that sends PING every N seconds (configurable, default 10s)
- On incoming PING, immediately respond with PONG
- If a PING is sent and no PONG received within timeout, close the connection
- **IMPORTANT**: Go does NOT support PING/PONG. Heartbeat must be disabled when connecting to Go services. For now, make it configurable (default off). We'll auto-detect later.

## Success Criteria

- [ ] PacketHeader roundtrips through JSON matching Go/JS wire format
- [ ] FlexInt handles both `42` and `"42"`
- [ ] Can send a request and receive a response through the connection
- [ ] Message assembly handles multi-packet bodies correctly
- [ ] Large messages are chunked at 131072 bytes
- [ ] Request timeout fires after configured duration
- [ ] ACK packets are sent on DATA receipt
- [ ] Flow control pauses/resumes correctly
- [ ] PING/PONG works when enabled
- [ ] All new code has unit tests
- [ ] `cargo test` passes
