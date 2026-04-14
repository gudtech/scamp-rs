# scamp-rs vs scamp-go Parity Review — v3

**Date:** 2026-04-13  
**Reviewer:** Claude (Sonnet 4.6)  
**Scope:** All Rust source files under `src/` compared against all Go files in `scamp-go/scamp/`. Cross-referenced against Perl and JS where noted. Key changes from the review prompt are evaluated in dedicated sections.

---

## Legend

| Status | Meaning |
|--------|---------|
| MATCH | Behavior is functionally equivalent |
| PARTIAL | Core behavior present but incomplete or missing edge cases |
| MISSING | Feature exists in Go, not implemented in Rust |
| DIVERGENT | Deliberate design difference, not a bug |
| BETTER | Rust exceeds Go in correctness or spec fidelity |

---

## 1. Packet Framing (`transport/beepish/proto/packet.rs` vs `packet.go`)

**Status: MATCH / BETTER**

Both parse the `TYPE MSGNO SIZE\r\n<body>END\r\n` wire format. Key comparison:

| Aspect | Go | Rust |
|--------|-----|------|
| Packet types | HEADER, DATA, EOF, TXERR, ACK (5) | Same + PING, PONG (7) |
| Unknown packet type | Error, fatal | Fatal — logs and drops connection |
| Bare `\n` without `\r` | `ReadLine` silently strips `\r` — silently accepts bare `\n` | **Explicit rejection** with `Fatal` error — more correct per Perl spec |
| Header line length limit | No explicit limit (bufio scanner default) | **80-byte scan limit per Perl Connection.pm:46** |
| Overlong header detection | None | Fatal "Overlong header line" |
| Binary body support | Yes (io.ReadFull) | Yes (raw bytes, no UTF-8 assumption) |
| MAX_PACKET_SIZE guard | None | 131072 bytes — prevents OOM |

The Rust parser is more defensive than Go. Go uses `bufio.ReadLine` which silently normalizes CRLF; Rust explicitly rejects bare `\n` (correct per Perl spec). Ping/Pong are present in Rust, absent in Go — these are used for keepalive by the JS implementation.

---

## 2. Key Change Review: Server Read Loop (Manual Buffer vs BufReader)

**Status: BETTER than Go and old Rust**

**Go approach (`connection.go:100-135`):** Uses `bufio.ReadWriter` wrapping a `*tls.Conn`, calling `ReadLine` which can block indefinitely until a `\n`. This creates a busy-loop risk under TLS partial reads.

**New Rust approach (`server_connection.rs:49-107`):** Manual buffer loop using `reader.read(&mut tmp)` with a 4096-byte chunk, accumulating into a `Vec<u8>`, then repeatedly calling `Packet::parse()` until `TooShort`/`NeedBytes`. Key properties:

- `reader.read()` returns whatever TLS has available — no blocking for a delimiter
- Idle timeout (120s) only applied when no in-progress messages — matches Perl Connection.pm:131-135
- Busy (in-flight requests) path has no timeout — correct behavior
- Buffer draining with `buf.drain(..consumed)` avoids quadratic copies

This is a genuine fix for the TLS busy-loop. The Go code would never exhibit this bug because `net/tls` in Go uses blocking reads with internal buffers, but for Rust's async TLS (tokio-native-tls which wraps a synchronous TLS crate over an async socket), the old `BufReader` approach would spin on partial TLS records.

---

## 3. Key Change Review: Client `copy_bidirectional` Duplex Proxy

**Status: BETTER than Go**

**Go approach (`connection.go:82`):** Wraps the `*tls.Conn` directly in a `bufio.ReadWriter`. Go's `crypto/tls` handles both directions from the same mutex internally, so there is no split deadlock.

**Rust approach (`connection.rs:87-93`):** Creates an in-process `tokio::io::duplex(65536)` and spawns a `copy_bidirectional` task to proxy between the real TLS stream and the duplex. The read half and write half of the duplex are then split cleanly for the reader and writer tasks.

This solves a real problem with `tokio_native_tls`: the underlying `native_tls::TlsStream` wraps a synchronous TLS implementation (`openssl` or `Security.framework`) over an async socket via polling. When the reader task calls `read()` and the writer task calls `write()` concurrently, they can deadlock because TLS needs to perform internal renegotiation handshakes that require reads before writes can proceed. The duplex proxy ensures the real TLS I/O is handled by a single task using `copy_bidirectional`, while the reader/writer tasks operate on the pure-async duplex pipe.

Go avoids this because `crypto/tls` was written natively async-safe with Go's concurrency model.

---

## 4. Key Change Review: Client Reader Manual Buffer

**Status: MATCH to server reader design, CORRECT**

`transport/beepish/client/reader.rs:46-83` uses the same manual buffer loop (`reader.read(&mut tmp)`, accumulate, parse) as the server. This is symmetric and correct. The Go client (`connection.go:100-135`) uses the same `bufio.ReadWriter`/`ReadLine` approach for both directions.

---

## 5. Key Change Review: `error_data` Field in `ScampReply`

**Status: BETTER than Go**

Go's `PacketHeader` (`packetheader.go:26-37`) has `Error string` and `ErrorCode string` but **no `error_data` field**. The `Message` struct (`message.go:10-22`) similarly has only `Error` and `ErrorCode`.

Rust `PacketHeader` (`proto/header.rs:33`) adds:
```rust
pub error_data: Option<serde_json::Value>,
```

And `ScampReply` (`service/handler.rs:26`) exposes it:
```rust
pub error_data: Option<serde_json::Value>,
```

This is sent in the reply HEADER packet via `server_reply.rs:25`. The `Requester` uses it for dispatch_failure detection (`requester.rs:65-69`). This matches the JS implementation (`connection.js error_data`) and C# (`ErrorData`). Go is deficient here.

---

## 6. Key Change Review: `register_with_flags()` in `ScampService`

**Status: BETTER than Go**

Go's `service.go:147-167` `Register()` accepts `*ActionOptions` which carries no concept of per-action flags for dispatch purposes. The options are stored in a `ServiceOptionsFunc` wrapper but not checked at dispatch time for auth bypass.

Rust `listener.rs:89-105` adds `register_with_flags()` which stores flags in `RegisteredAction.flags`. The server connection (`server_connection.rs:226-239`) checks for `"noauth"` flag to bypass `AuthzChecker`. Without this, the AuthzChecker bypass path in `dispatch_and_reply` would be dead code. Go has no equivalent runtime flag check.

---

## 7. Key Change Review: E2E Test with Self-Signed Certs

**Status: BETTER than Go**

`tests/e2e_full_stack.rs` provides 5 end-to-end tests:
1. `test_echo_roundtrip` — full stack: cert gen → bind → announce → registry load → TLS request → response
2. `test_large_body` — 5000-byte body exercises DATA chunking through real TLS
3. `test_unknown_action` — error path through real TLS
4. `test_sequential_requests` — connection reuse verification
5. `test_announcement_signature_verification` — RSA signature round-trip

Go has `connection_test.go`, `service_test.go`, and `requester_test.go` but they require a live SCAMP environment; there are no self-contained TLS E2E tests using self-signed certs.

The Rust tests run without any external dependencies by generating RSA 2048 keypairs at runtime via OpenSSL.

---

## 8. TLS Fingerprint Verification

**Status: MATCH / BETTER**

**Go (`connection.go:55-63`):** Computes SHA1 fingerprint of peer cert on `NewConnection()` but stores it in `conn.Fingerprint` for the caller to check. The caller (`serviceproxy.go:365-393`) checks it via `validateSignature()` — but `DialConnection` sets `InsecureSkipVerify: true` and never actually validates the fingerprint against the announced one at connect time.

**Rust (`connection.rs:126-157`):** Fingerprint verification happens **before any packets are sent** (natural corking after TLS handshake). The expected fingerprint from the discovery announcement is passed in, and if it mismatches, the connection is refused with a clear error. This is the correct behavior per Perl Connection.pm:61-68.

Rust behavior: **correct and enforced**. Go behavior: **fingerprint computed but not enforced at dial time**.

---

## 9. Announcement Building/Parsing

### Building (`service/announce.rs` vs `serviceproxy.go:MarshalJSON`)

**Status: PARTIAL (known gap)**

| Aspect | Go | Rust |
|--------|-----|------|
| Format version | 3 | 3 |
| Array positions | [ver, ident, sector, weight, interval_ms, connspec, protocols, classes, ts] | Same |
| v4 extension hash | Yes (from extension field if present) | Yes — always included, but **v4 action arrays are empty** |
| Action serialization | [className, [actionName, crudTags, version], ...] | Same format |
| RSA signing | PKCS1v15 SHA256 | PKCS1v15 SHA256 — MATCH |
| Base64 wrapping | `base64.StdEncoding` (no line wrap for announcements) | 76-char wrapped (matches Perl MIME::Base64) |

**Known gap in Rust announce.rs:** The v4 extension hash vectors (`v4_acns`, `v4_acname`, etc.) are always empty:
```rust
let v4_acns: Vec<String> = Vec::new();  // never populated
```
The v4 hash is included in the packet but with all empty arrays. This means receivers that only understand v4 (no v3 fallback) will see zero actions. Receivers that parse v3 (including Go, Perl, and Rust's own parser) will correctly parse v3 actions. This is marked as a known gap — Rust services will be discoverable via v3 but not v4.

### Parsing (`discovery/service_info/parse.rs` vs `serviceproxy.go:newServiceProxy`)

**Status: MATCH / BETTER**

Rust parses both v3 and v4 actions with full RLE decoding. Go parses v3 actions and the v4 extension struct but does not decode RLE into individual actions — it stores the raw `ServiceProxyDiscoveryExtension` struct and the action index is built from v3 class records only.

Rust's `parse_v4_actions` fully decodes RLE-encoded v4 vectors using `itertools::izip!`, matching the Perl parser exactly.

---

## 10. Discovery Cache

**Status: MATCH / BETTER**

**Go (`servicecache.go`):** Uses a `%%%`-delimited file, scans line by line, assembles class records + cert + sig, then calls `newServiceProxy`. Action index key format: `sector:class.action~version#envelope` (lowercase).

**Rust (`discovery/cache_file.rs`):** Uses the same `\n%%%\n` delimiter, reads into chunks, parses each as an `AnnouncementPacket`. Iterator-based design is cleaner and works with any `Read` impl.

**Key difference:** The Rust cache iterator splits on `\n%%%\n` as a 5-byte window, while Go scans for a line that **equals** `%%%`. Go's approach is more robust to leading `\n` variations; Rust's approach requires exactly `\n%%%\n` (no prefix whitespace). In practice the Perl writer always emits exactly this format so it's compatible.

**Action index key format:**
- Go: `sector:class.action~version#envelope`  
- Rust: `sector:action.vversion` (in `service_registry.rs:make_index_key`)

These formats differ but are only used internally — Go's format is used by `ServiceCache.SearchByAction`, Rust's by `ServiceRegistry.find_action`. Both are correct for their own consumers.

---

## 11. `ServiceRegistry` vs `ServiceCache`

**Status: DIVERGENT (by design, Rust is better)**

Go's `ServiceCache` is a flat `identIndex` + `actionIndex` with no TTL/expiry, no replay protection, no failure tracking, and no authorized_services integration at lookup time.

Rust's `ServiceRegistry` adds:
- **TTL/expiry check** (`inject_packet:90-94`): skips stale announcements (interval × 2.1)
- **Replay protection** (`inject_packet:98-109`): deduplicates by `fingerprint+identity`, rejects older timestamps
- **Failure tracking** (`mark_failed`, `pick_healthy`): exponential backoff per JS serviceMgr.js
- **authorized_services integration** at inject time — `authorized` flag on each entry
- **CRUD aliases** (`_create`, `_read`, `_update`, `_destroy`) per Perl ServiceInfo.pm:191-192
- **Per-action timeout** (`timeout_secs()` from `t600` flags)

These are all features of the Perl `ServiceManager.pm` that Go never implemented.

---

## 12. Authorized Services

**Status: MATCH / BETTER**

**Go (`authorizedservices.go`):** Parses fingerprint + space-separated class names. Stores raw class names in `AuthorizedServiceSpec.Actions`. No regex matching — the spec is never used to filter actions during dispatch. The `Validate()` call only calls `validateSignature()`, not the authorized_services check.

**Rust (`auth/authorized_services.rs`):** Full implementation per Perl ServiceInfo.pm:111-168:
- Regex-based pattern matching with `:ALL` → `:.*` expansion
- Prefix match with `(?:\.|$)` boundary
- Case-insensitive
- `_meta.*` always authorized
- Rejects `:` in sector or action
- Hot-reload via `reload_if_changed()` with mtime check

Go's authorized_services implementation is effectively a stub. Rust's is production-grade.

---

## 13. Multicast (Discovery Announcer)

**Status: MATCH / BETTER**

**Go (`discoveryannounce.go` + `multicast.go`):** Sends raw (uncompressed) service `MarshalText()` output via `ipv4.PacketConn.WriteTo`. Uses `eth0` interface hardcoded (`multicast.go:44`).

**Rust (`service/multicast.rs`):** 
- zlib-compresses packets (Perl Announcer.pm:203) — Go skips compression entirely
- Configurable interface via `MulticastConfig`
- Shutdown announcing: 10 rounds of weight=0 packets at 1s intervals (Perl Announcer.pm:82-101) — Go has no shutdown announcing
- Uses `socket2` for raw UDP socket configuration

**Rust observer (`discovery/observer.rs`):**
- Handles `R`/`D` prefix bytes (Perl Observer.pm:48) — Go has no observer
- zlib decompresses received packets
- Injects into `ServiceRegistry` with write lock

Go has a `DiscoveryAnnouncer` but no multicast observer. The Go announcer sends uncompressed packets and has no shutdown sequence. Rust is significantly more complete.

---

## 14. Graceful Shutdown

**Status: BETTER than Go**

**Go (`service.go:286-291`):** `Stop()` closes the listener. The `Run()` loop then breaks, closes all clients, sends to `statsCloseChan`, and removes the liveness file. No drain — in-flight requests can be interrupted.

**Rust (`listener.rs:168-220`):** 
1. `tokio::select!` on `listener.accept()` vs `shutdown_rx.changed()`
2. On shutdown: 30-second drain loop polling `active_connections` counter every 100ms
3. Logs warning if connections remain after 30s

The multicast announcer also sends 10 weight=0 shutdown announcements before exiting, allowing requesters to route around the service before it stops accepting connections.

---

## 15. Ticket Verification

**Status: MATCH / BETTER**

**Go (`ticket.go`):** 
- Format: `version,user_id,client_id,timestamp,ttl,privs,signature` (comma-separated)
- Base64URL (raw, no padding) signature
- SHA256 RSA PKCS1v15
- Single global `verifyKey` loaded via `sync.Once` from a fixed path
- Privilege map: `map[int]bool`

**Rust (`auth/ticket.rs`):**
- Same format and crypto
- `Ticket::verify()` takes the PEM public key bytes directly — no global state
- `has_privilege()` / `has_all_privileges()` helper methods
- `validity_start` check (ticket not yet valid) — Go only checks expiry, not not-yet-valid

Rust correctly checks both lower and upper bounds of the ticket validity window. Go only checks the upper bound. Both implementations handle the `privs` empty string case.

---

## 16. PacketHeader Wire Format

**Status: MATCH**

Both implementations use the same JSON field names:
- `"type"` (not `"message_type"`) for `MessageType`
- `"envelope"` as lowercase string (`"json"`, `"jsonstore"`)
- `"request_id"`, `"client_id"`, `"action"`, `"ticket"`, `"identifying_token"`, `"version"`, `"error"`, `"error_code"`

Both have `flexInt` / `FlexInt` for `client_id` to handle both integer and string JSON values. Rust additionally handles `null` for `ticket` and `identifying_token` (Perl sends `null` for these). Go would reject `null` for these fields because `string` JSON unmarshal fails on `null`.

Rust adds `error_data` (missing in Go). Rust uses `skip_serializing_if = "Option::is_none"` for error fields — Go uses `omitempty` — both produce the same wire format.

---

## 17. Config Parsing

**Status: DIVERGENT (both correct)**

| Aspect | Go | Rust |
|--------|-----|------|
| File format | `key = value` per line | Same |
| Comment stripping | Regex `configLine` (no inline comments) | Inline `#` comments stripped |
| Duplicate keys | Last wins (map overwrite) | **First wins** (per Perl Config.pm:30-31) |
| Default paths | `/etc/SCAMP/soa.conf` | `/etc/scamp/scamp.conf`, `/etc/GTSOA/scamp.conf`, `$GTSOA/etc/soa.conf`, `~/GT/backplane/etc/soa.conf` |
| Env var | None | `SCAMP_CONFIG`, `GTSOA` |
| Path rewrites | None | `/backplane/*` → `~/GT/backplane/*` (dev env) |

The first-wins vs last-wins difference is a correctness issue in Go. Perl Config.pm:30-31 is first-wins; Rust matches Perl.

---

## 18. Requester / `MakeJSONRequest`

**Status: DIVERGENT (by design, Rust is better)**

**Go (`requester.go:MakeJSONRequest`):**
- Refreshes the full cache on every request
- Collects all matching service proxies, shuffles, sorts by `openReplies` depth
- Iterates until one `client.Send()` succeeds
- Polls `responseChan` up to 20 times before giving up (can miss responses)
- No dispatch_failure retry

**Rust (`requester.rs`):**
- Registry is a single consistent snapshot (no per-request refresh)
- `find_action_with_envelope` does weighted random selection with failure-avoidance
- Single dispatch, one retry on `dispatch_failure` (JS requester.js:50-58)
- Per-action timeout from `t600` flags

The polling loop in Go (`RetryLoop: for attempts := 0; attempts < MAX_RETRIES`) is a known deficiency — if the channel receives `nil` 20 times, it gives up even if a valid response is pending. Rust uses `oneshot` channels with proper `await`, eliminating this race.

---

## 19. Transport Module Organization

**Status: DIVERGENT (Rust is more modular)**

Go has a flat package: `connection.go` contains everything (connect, read, write, route, ack).

Rust splits this into:
- `transport/beepish/proto/` — wire format only (packet framing, header serde)
- `transport/beepish/client/connection.rs` — connection pool + send logic
- `transport/beepish/client/reader.rs` — reader task (separate for testability)
- `service/server_connection.rs` — server-side connection handling
- `service/server_reply.rs` — reply writing (extracted to stay under 300 lines)

This separation enables unit testing of the server connection without TLS (using `tokio::io::duplex`), which Go cannot easily do.

---

## 20. Server-Side Connection vs Go's `client.go` + `service.Handle()`

**Status: MATCH in behavior, DIVERGENT in design**

Go separates connection reading (`connection.go:packetReader`) from message routing (`client.go:splitReqsAndReps`) from action dispatch (`service.go:Handle`). These communicate via channels.

Rust combines all three in `server_connection.rs:handle_connection` as a single async function. Both correctly implement: HEADER → DATA* → EOF/TXERR assembly, ACK sending on DATA, and action dispatch. The timeout behavior differs:

- Go `service.go:252`: `time.After(msgTimeout)` (120s) resets on each message — timeout is per-message idle
- Rust `server_connection.rs:64-67`: Idle timeout (120s) applies only when no in-flight requests — timeout is per-connection idle

Rust is more correct: if a handler is slow, Go would time out the connection even with work in progress. Rust only times out when the connection has been completely idle.

---

## 21. Connection Pooling

**Status: DIVERGENT (Rust adds pooling)**

Go's `serviceproxy.go:GetClient()` creates one client per `serviceProxy` and caches it; no pool.

Rust's `BeepishClient` maintains a `HashMap<String, Arc<ConnectionHandle>>` keyed by URI. If the `ConnectionHandle.closed` flag is set, the stale connection is removed and a new one is established. Concurrent requests to the same service reuse the same connection with multiple in-flight request IDs (the `pending` map).

---

## 22. ACK / Flow Control

**Status: MATCH / BETTER**

**Go (`connection.go:307-333`):** Sends ACK packets with cumulative byte count. No flow control on the send side — writes all packets immediately.

**Rust (client `connection.rs:208-244`, server `server_connection.rs:138-156`):** Sends ACK packets per DATA packet. Client side additionally enforces a flow control watermark (`FLOW_CONTROL_WATERMARK = 65536`): the sender pauses if `sent - acknowledged >= 65536`. This matches JS `connection.js:298`. Go has no send-side flow control.

---

## 23. Missing from Rust (vs Go)

| Feature | Go has | Rust has |
|---------|--------|----------|
| `service.createRunningServiceFile()` | Yes (liveness file) | No |
| `stats.go` / `service_stats.go` | Service stats counters | No |
| `scampDebugger` / wire tee logging | Yes (binary debug output) | No (only log::debug!) |
| Global `DefaultCache` / `DefaultConfig` | Yes (package globals) | No (explicit config threading) |
| `action_options.go` / `ServiceOptionsFunc` | Yes | Partially (flags only, no CRUD tags on handler) |

The liveness file and stats are operational features; absence doesn't affect protocol correctness. The Rust approach of explicit config threading is architecturally cleaner.

---

## 24. Missing from Go (vs Rust)

| Feature | Rust has | Go has |
|---------|----------|--------|
| Multicast observer | Yes | No |
| Shutdown weight=0 announcements | Yes | No |
| zlib compression/decompression | Yes | No |
| error_data field | Yes | No |
| register_with_flags / noauth bypass | Yes | No (stub only) |
| Per-action timeout from flags | Yes | No |
| CRUD alias lookup | Yes | No |
| Replay protection | Yes | No |
| TTL-based expiry | Yes | No |
| Service failure tracking | Yes | No |
| Authorized services enforcement | Yes (full regex) | No (parsing stub) |
| E2E tests (no external deps) | Yes (5 tests) | No |
| PING/PONG keepalive | Yes | No |

---

## 25. Summary Table

| Area | Status | Notes |
|------|--------|-------|
| Packet framing (parse + write) | BETTER | Adds \r\n enforcement, length limit, PING/PONG |
| Server read loop (manual buffer) | BETTER | Fixes TLS busy-loop, correct idle timeout |
| Client duplex proxy | BETTER | Fixes TLS split deadlock |
| Client reader (manual buffer) | MATCH | Same design as server reader |
| `error_data` field | BETTER | Go missing this field entirely |
| `register_with_flags()` | BETTER | Go has no noauth bypass |
| E2E test with self-signed certs | BETTER | Go has no equivalent |
| TLS fingerprint verification | BETTER | Rust enforces at connect, Go computes but ignores |
| Announcement building | PARTIAL | v4 vectors empty (known gap); v3 correct |
| Announcement parsing | BETTER | Full v3+v4 with RLE decoding |
| Discovery cache | MATCH | Different key format, same semantics |
| Service registry | BETTER | TTL, replay protection, failure tracking, CRUD aliases |
| Authorized services | BETTER | Full regex vs Go stub |
| Multicast announcer | BETTER | zlib, shutdown rounds, Go has neither |
| Multicast observer | BETTER | Go has no observer |
| Graceful shutdown | BETTER | 30s drain + weight=0 announcements |
| Ticket verification | BETTER | Checks validity_start, Go does not |
| PacketHeader wire format | MATCH | Plus error_data, null-ticket handling |
| Config parsing | DIVERGENT | Rust: first-wins (correct); Go: last-wins (wrong per Perl) |
| Requester | BETTER | oneshot channels, dispatch_failure retry, no poll loop |
| Connection pooling | BETTER | Rust pools; Go one client per serviceProxy |
| Flow control (ACK watermark) | BETTER | Rust enforces 64KB watermark; Go has none |
| Ping/Pong keepalive | BETTER | Implemented; Go missing |
| AuthzChecker (ticket authz) | BETTER | Rust implemented; Go has no equivalent |
| Liveness file | MISSING | Go creates one; Rust does not |
| Stats counters | MISSING | Go has service_stats; Rust does not |
| Wire debug logging | MISSING | Go has scampDebugger tee; Rust uses log::debug! |

---

## Critical Findings

### 1. v4 Action Vector Gap in Announcement Builder

`service/announce.rs:39-44`: The v4 extension hash is always sent with empty vectors. This means Rust services will not be discoverable by receivers that require v4 (e.g., future services that drop v3 parsing). Not a current interop blocker since all known implementations parse v3, but should be fixed.

### 2. Announcement Cache Delimiter Sensitivity

`discovery/cache_file.rs:12`: Delimiter is `"\n%%%\n"` (5 bytes). The Perl writer emits `\n%%%\n` between records. If a record happens to end without a trailing newline, or if the file starts with `%%%` on the first line (as Go's `servicecache.go` handles via scanning for `sep` bytes), the Rust iterator would skip that record. In practice Perl always includes the trailing newline, so this is low risk.

### 3. Config First-wins vs Go Last-wins

`config.rs:171-173`: Rust implements first-wins per Perl. Go silently overwrites with last-wins. This means if a soa.conf file has duplicate keys, Rust and Go will disagree. Since soa.conf files are generated and well-formed, this is unlikely to cause issues in practice.

### 4. No Liveness File

Rust does not create/remove the running service file that `service.createRunningServiceFile()` creates in Go. This file is used by monitoring tools to detect running services. A future addition.

### 5. ACK Validation in Server Reply Path

`server_reply.rs:83`: After sending the reply EOF, the outgoing state is removed immediately (`outgoing.remove(&reply_msg_no)`), before waiting for all ACKs from the client. This means if the client sends late ACKs, the server won't validate them (the msgno won't be in `outgoing`). Go has the same behavior (no ACK validation for reply direction at all). This is benign for correctness but means flow control on large replies is advisory only.
