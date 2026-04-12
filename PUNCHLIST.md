# scamp-rs Completion Punchlist

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done, `[!]` blocked

## Phase 0: Critical Interop Fix

- [ ] **P0-1** Remove `BEEP\r\n` handshake from `src/transport/beepish/client.rs` (lines ~138-148). Neither scamp-go nor scamp-js implements this. Breaks all interop.

## Phase 1: Transport Core

- [ ] **P1-1** Fix PacketHeader serde: custom serialize/deserialize for `EnvelopeFormat` (→ `"json"`, `"jsonstore"`) and `MessageType` (→ `"request"`, `"reply"`) as lowercase strings
- [ ] **P1-2** Implement `FlexInt` type for `client_id` (deserializes from both JSON string and integer)
- [ ] **P1-3** Inbound message assembly: HEADER → DATA* → EOF/TXERR packet routing to complete messages. Track incoming message number counter; validate sequential ordering.
- [ ] **P1-4** Outbound message serialization: Message → HEADER + DATA chunks (131072 bytes) + EOF packets. Track outgoing message number counter.
- [ ] **P1-5** Request-response correlation: generate request_id, map incoming replies to waiting `oneshot::Sender<Response>` by request_id + msg_no
- [ ] **P1-6** Timeout per request via `tokio::time::timeout`
- [ ] **P1-7** Flow control: send ACK after receiving DATA; pause sending when `sent - acked >= 65536`; resume on ACK receipt
- [ ] **P1-8** PING/PONG heartbeat: send PING on interval, respond to PING with PONG, close on missed heartbeat. Must be optional (Go doesn't support PING/PONG).
- [ ] **P1-9** Connection architecture: mpsc channel for serialized writes, reader task for packet dispatch, `ConnectionHandle` with pending requests map

## Phase 2: Service Infrastructure

- [ ] **P2-1** TLS server listener: accept connections on random port in configured range (30100-30399), load service cert/key
- [ ] **P2-2** Action registration: `service.register("Name.action", version, handler_fn)` with sector, flags, envelope types
- [ ] **P2-3** Request dispatch: route incoming requests by action name + version to registered handler, send reply with correct request_id correlation
- [ ] **P2-4** Handler trait: `async fn handle(request: ScampRequest) -> Result<ScampResponse, ScampError>`
- [ ] **P2-5** Service identity generation: `name:base64(random 18 bytes)`
- [ ] **P2-6** Announcement packet generation: serialize action list as v3 JSON array, sign with RSA SHA256, format as `json\n\ncert\n\nsig`

## Phase 3: Security

- [ ] **P3-1** RSA SHA256 announcement signature verification (replace stub that returns `true`)
- [ ] **P3-2** SHA1 certificate fingerprinting
- [ ] **P3-3** `authorized_services` file parsing: fingerprint → action pattern (glob/regex) mapping
- [ ] **P3-4** Action authorization filtering in ServiceRegistry
- [ ] **P3-5** Ticket format parsing: `version,userId,clientId,timestamp,ttl,privs,signature`
- [ ] **P3-6** Ticket RSA PKCS1v15 SHA256 signature verification
- [ ] **P3-7** Ticket expiry + privilege checking

## Phase 4: Discovery

- [ ] **P4-1** UDP multicast announcement sending at configured interval (default 5s)
- [ ] **P4-2** Discovery cache file watching (using `notify` crate) with re-parse on modification
- [ ] **P4-3** Announcement expiry/TTL: `sendInterval * 2.1` (match JS)
- [ ] **P4-4** Include sector prefix in action index key (match Go/JS)
- [ ] **P4-5** Envelope-based action filtering at lookup time
- [ ] **P4-6** CRUD tag aliases for action lookup
- [ ] **P4-7** Suspend (weight=0) for graceful shutdown announcements

## Phase 5: Hardening

- [ ] **P5-1** Typed error enum: `ScampError { Transport, Auth, Timeout, ActionNotFound, Remote, ConnectionLost, Io, Tls }`
- [ ] **P5-2** TXERR handling: non-empty body validation, propagate as error
- [ ] **P5-3** Connection reconnection on failure with backoff
- [ ] **P5-4** Graceful shutdown: stop announcing / send weight=0, drain active requests, close connections
- [ ] **P5-5** Running service file (liveness indicator)

## Phase 6: Cleanup

- [ ] **P6-1** Remove dead code: `src/message/`, `src/common/`, `src/error.rs`, `src/agent/`, `src/transport/beepish/tcp.rs`
- [ ] **P6-2** Remove unused deps: `bytes`, `http`, `net2`, `atty`, `pnet`
- [ ] **P6-3** Add deps: `rustls` + `tokio-rustls` (replace `tokio-native-tls`), `ring`, `base64`, `notify`
- [ ] **P6-4** New module structure: `src/service.rs`, `src/client.rs`, `src/auth/ticket.rs`, `src/auth/authorized_services.rs`, `src/crypto.rs`

## Testing

- [ ] **T1** Unit: packet parse/write roundtrip for all packet types
- [ ] **T2** Unit: PacketHeader serde roundtrip (JSON string representation matches Go/JS)
- [ ] **T3** Unit: FlexInt deserialization from string and integer
- [ ] **T4** Unit: config parsing with real soa.conf files
- [ ] **T5** Unit: announcement parsing with real discovery cache data
- [ ] **T6** Unit: ticket parsing and verification with known-good tickets
- [ ] **T7** Integration: Rust client → Go service (request/response)
- [ ] **T8** Integration: Rust client → JS service (request/response)
- [ ] **T9** Integration: Go client → Rust service (request/response)
- [ ] **T10** Integration: JS client → Rust service (request/response)
- [ ] **T11** Integration: connection multiplexing (concurrent requests on one connection)
- [ ] **T12** Integration: flow control under load (large message bodies)
- [ ] **T13** Integration: heartbeat (PING/PONG with JS, verify no PING sent to Go)
- [ ] **T14** Integration: announcement send/receive cycle
- [ ] **T15** Integration: graceful shutdown (drain in-flight requests)
- [ ] **T16** Compatibility: parse production discovery cache file
- [ ] **T17** Compatibility: wire capture comparison (Rust vs Go packet bytes for same message)

## Cross-Implementation Differences (document decisions)

- [ ] **D1** Timestamp format: Go=seconds.microseconds, JS=milliseconds. Pick one.
- [ ] **D2** PING/PONG: Go doesn't support. Must be negotiable or auto-detected.
- [ ] **D3** Multicast compression: JS uses zlib, Go doesn't. Need to handle both.
- [ ] **D4** Action index key format: Go=`sector:class.action~version#envelope`, JS=`sector:class.action.vVERSION`. Pick one.
- [ ] **D5** `error_data` field: JS-only. Support for forward compat or skip?
- [ ] **D6** Pub/sub message types (event/subscribe/notify): JS-only. Needed?
