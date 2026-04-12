# Prompt: Phase 1 — Transport Core

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`, implementing the core transport layer for the SCAMP protocol. Phase 0 cleanup should already be done.

Reference implementations (in priority order):
- **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`): THE canonical implementation
  - `Transport/BEEPish/Connection.pm` — wire protocol, packet handling, flow control
  - `Transport/BEEPish/Client.pm` — client connections, request correlation
  - `Transport/BEEPish/Server.pm` — server listener, reply dispatch
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/transport/beepish/connection.js` — most featureful
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` — cross-reference only

Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Architecture

Implement the following connection architecture (using tokio):

```
ScampClient
  connections: HashMap<String, Arc<ConnectionHandle>>

ConnectionHandle
  writer_tx: mpsc::Sender<Packet>       // serialized writes through channel
  pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>  // keyed by request_id
  next_request_id: AtomicI64            // sequential, starting from 1
  closed: AtomicBool

// Spawned tasks per connection:
// 1. Reader task: reads packets from TLS stream, assembles messages, delivers via pending map
// 2. Writer task: receives packets from mpsc channel, writes to TLS stream
```

## Critical Perl Cross-Check Notes

These corrections were identified by reading the canonical Perl implementation:

1. **JSON field name is `"type"`, NOT `"message_type"`**. Perl `Client.pm:87`: `$request->header->{type} = 'request'`. Go json tag: `json:"type"`. The current scamp-rs struct has `message_type` which is WRONG.

2. **Message numbers start at 0**. Perl `Connection.pm:97-98`: `_next_message_in = 0; _next_message_out = 0`.

3. **request_id is sequential integer starting from 1**. Perl `Client.pm:23,85`: `_nextcorr = 1`, then `$self->{_nextcorr}++`.

4. **Reply type is `"reply"`**, not `"response"`. Perl `Server.pm:68`: `$reply->header->{type} = 'reply'`.

5. **ACK body format**: decimal string matching `/^[1-9][0-9]*$/`. Represents cumulative bytes received. Must strictly advance. Perl `Connection.pm:179-181`.

6. **EOF body must be empty**. Perl `Connection.pm:162`: `return $self->_error(error => 'EOF packet must be empty') if $body ne ''`.

7. **PING/PONG: NOT supported by Perl or Go**. Perl `Connection.pm:186`: unknown packet types cause connection error. Only scamp-js supports PING/PONG. **Default must be disabled.**

8. **DATA chunk size**: Perl uses 2048 bytes, Go uses 128KB, JS uses 131072. Use 131072 for sending (all receivers handle it).

## Tasks

### 1. Fix PacketHeader serialization (`src/transport/beepish/proto.rs`)

The existing `PacketHeader` has two critical problems:
- The struct field `message_type` serializes as `"message_type"` in JSON. It MUST be `"type"`.
- `EnvelopeFormat` and `MessageType` use default serde which produces `"Json"`, `"Request"` etc. They MUST be lowercase strings.

Fix:
- Rename the field and add `#[serde(rename = "type")]` 
- Custom serde for `EnvelopeFormat`: `Json` ↔ `"json"`, `JsonStore` ↔ `"jsonstore"`, `Other(s)` ↔ `s`
- Custom serde for `MessageType`: `Request` ↔ `"request"`, `Reply` ↔ `"reply"`
- Add `FlexInt` type for `client_id` that deserializes from both `42` and `"42"` in JSON
- Add `#[serde(skip_serializing_if = "Option::is_none")]` on optional fields (`error`, `error_code`) to match Perl behavior (omit when None)

Write unit tests: serialize a PacketHeader to JSON, verify it matches what Perl/Go/JS would produce.

### 2. Define Message and Response types

Create `src/message.rs` (fresh, not the old dead code):

```rust
pub struct ScampRequest {
    pub action: String,
    pub version: i32,
    pub envelope: EnvelopeFormat,
    pub request_id: i64,
    pub client_id: FlexInt,
    pub ticket: String,
    pub identifying_token: String,
    pub body: Vec<u8>,
}

pub struct ScampResponse {
    pub request_id: i64,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub body: Vec<u8>,
}
```

Implement `ScampRequest::to_packets()` — serialize into HEADER packet + DATA chunk packets (max 131072 bytes each) + EOF packet. Message number is assigned by the connection.

Implement `ScampResponse::from_packets()` — assemble from HEADER + DATA* + EOF/TXERR.

### 3. Inbound message assembly

In the connection reader task, implement packet routing:

- Track `next_incoming_msg_no: u64`, **starting at 0**
- On HEADER: validate msgno == next_incoming_msg_no, increment. Start a new in-progress message (store header, begin body accumulation)
- On DATA: append body bytes to in-progress message for this msg_no. Send ACK with cumulative bytes received (as decimal string)
- On EOF: validate body is empty. Complete the message, deliver to the pending requests map as a response
- On TXERR: body is the error message (UTF-8 string). Complete as error, deliver to pending map
- On ACK: update flow control state (acked bytes)

Reference: Perl `Connection.pm` `_packet()` method (lines 136-188)

### 4. Outbound message serialization

Implement `send_request()` on ConnectionHandle:

1. Allocate next `request_id` (sequential integer, atomic increment, starting from 1)
2. Allocate next outgoing msg_no (atomic increment, starting from 0)
3. Create a `oneshot::channel` for the response
4. Insert sender into `pending` map keyed by `request_id`
5. Set header fields: `type = "request"`, `request_id`, etc.
6. Serialize request to packets (HEADER + DATA chunks at 131072 + EOF)
7. Send all packets through `writer_tx` channel
8. Return the oneshot receiver (caller awaits it)

### 5. Request-response correlation

Incoming replies are matched to waiting requests by `request_id` from the reply header. This is how all implementations work:
- Perl `Client.pm:58-65`: `my $id = $reply->header->{request_id}; my $rec = delete $self->{_pending}{$id}`
- Go uses `openReplies` map
- JS uses `_pending` Map

### 6. Timeout per request

Wrap the response future with `tokio::time::timeout(duration, receiver)`. Default timeout: **75 seconds** (from `rpc.timeout` in soa.conf — Perl `ServiceInfo.pm:256`). Per-action timeouts from `tN` flags: add 5 seconds (Perl `ServiceInfo.pm:257`). On timeout, remove the pending entry and return `ScampError::Timeout`.

### 7. Flow control

- After receiving each DATA packet, send an ACK packet with cumulative bytes received as a **decimal string** (e.g., `"131072"`)
- Track `bytes_sent` and `bytes_acked` per outgoing message
- If `bytes_sent - bytes_acked >= 65536`, pause sending
- On ACK receipt: validate the ACK value strictly advances and doesn't exceed bytes sent (Perl `Connection.pm:179-181`), update `bytes_acked`, resume if paused

### 8. PING/PONG heartbeat

**Default: DISABLED.** Perl and Go do not support PING/PONG. Unknown packet types cause a connection error in Perl (`Connection.pm:186`).

Only enable when explicitly configured and connecting to a known scamp-js service. Implementation:
- Optionally spawn a heartbeat task that sends PING every N seconds
- On incoming PING, immediately respond with PONG
- If a PING is sent and no PONG received within timeout, close the connection

## Interop Validation

After completing this phase, validate against the running dev environment:

1. Parse the live discovery cache file — all records must parse without error
2. Connect to gt-main-service (Perl) and send `API.Status.health_check~1` request
3. Verify a valid response is received with no protocol errors

Use `gud dev status -g` to verify the dev environment is running. The `main` container runs gt-main-service.

## Success Criteria

- [ ] PacketHeader JSON uses `"type"` (not `"message_type"`), `"json"` (not `"Json"`), `"request"`/`"reply"`
- [ ] FlexInt handles both `42` and `"42"`
- [ ] Message numbers start at 0
- [ ] Can send a request and receive a response through the connection
- [ ] Message assembly handles multi-packet bodies correctly
- [ ] Large messages are chunked at 131072 bytes
- [ ] Request timeout fires after configured duration
- [ ] ACK packets sent as decimal string of cumulative bytes
- [ ] Flow control pauses/resumes correctly
- [ ] PING/PONG disabled by default, works when enabled
- [ ] EOF body validated as empty
- [ ] Successful request/response to gt-main-service in dev environment
- [ ] All new code has unit tests
- [ ] `cargo test` passes
