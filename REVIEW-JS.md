# JavaScript Parity Review

## Summary

scamp-rs implements the core SCAMP protocol with high fidelity to the wire format and discovery subsystem, but has significant gaps in connection management, heartbeat lifecycle, streaming/backpressure, and several JS-specific features. The wire protocol is the strongest area; connection pooling and graceful shutdown have structural differences.

Reviewed against: `scamp-js/lib/` and `gt-soa/js/lib/` (functionally identical minus debug logging).

## Detailed Findings

### 1. Wire Protocol (packet framing, header JSON, ACK handling, flow control watermark 65536): MATCH

**Packet framing**: Rust `Packet::parse()` and `Packet::write()` match JS `connection.js:_readPackets` and `_sendPacket` exactly:
- Format: `TYPE MSGNO SIZE\r\n<body>END\r\n`
- Max packet size: 131072 (both)
- Overlong header line detection (Rust: 80 bytes, JS: 40 bytes -- minor divergence, Rust follows Perl, JS is stricter)
- Unknown packet type is fatal (both)
- Malformed trailer detection (both)
- CRLF required (both)

**Header JSON**: Rust `PacketHeader` field names match JS Message.header exactly:
- `type` (not `message_type`) via `#[serde(rename = "type")]`
- `envelope`, `request_id`, `client_id`, `ticket`, `identifying_token`, `action`, `version`
- `FlexInt` handles both int and string deserialization (matches Go behavior)
- `nullable_string` handles Perl's `ticket:null` correctly

**ACK handling**: Both implementations:
- Send cumulative byte count as decimal string
- Validate ACK not moving backward
- Validate ACK not past sent bytes
- Treat malformed ACK as error

**Flow control watermark**: Both use 65536:
- Rust: `FLOW_CONTROL_WATERMARK = 65536`, pauses when `sent - acknowledged >= 65536`
- JS: `maxflow = config().val('flow.max_inflight', 65536)`, pauses with `msg.pause()` when `sent - acked >= maxflow`

**DATA chunk size divergence**: Rust uses 2048 (`DATA_CHUNK_SIZE`, matching Perl). JS uses 131072 (full packet size) for outgoing DATA chunks. JS DripSource uses 1024/16384 to feed the Message stream. This is a minor behavioral difference but wire-compatible since receivers accept up to `MAX_PACKET_SIZE`.

### 2. Connection Management (pooling, reconnection, closed detection): PARTIAL

**Connection pooling**:
- Rust `BeepishClient` uses `HashMap<String, Arc<ConnectionHandle>>` keyed by URI. Checks `closed` flag before reuse, removes stale entries.
- JS `connectionMgr.js` uses `_cache` keyed by address. Deletes on `lost` event.
- Both implement single-connection-per-address pooling.

**Closed detection**:
- Rust: `AtomicBool closed` flag set by reader task exit and in `Drop`
- JS: `_closed` flag set in `_onerror`, emits `lost` event

**Missing in Rust**:
- **Reconnection on `lost`**: JS `connectionMgr.js:42-48` deletes the cached client on `lost` event, causing the next request to open a fresh connection. Rust removes stale connections on `get_connection()` but does not proactively evict on reader death. The `lost` event pattern is absent.
- **Connection timeout on establish**: JS `client.js:39` sets 10s initial timeout (`clear.setTimeout(10000)`), switches to configured idle timeout after TLS completes. Rust uses 30s TCP + 30s TLS hardcoded timeouts.
- **`setBusy` flag**: JS `connection.js:312` tracks whether pending requests exist; when busy, idle timeout is disabled. Rust `server_connection.rs:49-73` implements this for server side (no timeout when `incoming` or `outgoing` non-empty) but client side has no equivalent busy tracking for idle timeout.

### 3. Service Manager (registration, failure tracking, exponential backoff, healthy vs failing partition): MATCH

**Failure tracking**: Both implementations match:
- Rust `ServiceFailureState`: 24-hour sliding window of failure timestamps, `reactivate_at` computed as `now + min(failure_count, 60) * 60s`
- JS `Registration.connectFailed()`: identical 24-hour pruning, `min(60, failures.length)` minute backoff, `_reactivateTime` set accordingly

**Healthy vs failing partition**:
- Rust `pick_healthy()`: partitions candidates into healthy/failing, prefers healthy, falls back to failing
- JS `serviceMgr.findAction()`: identical partition logic with `filtered`/`failing` arrays

**Registration expiry**:
- JS uses `setTimeout(() => active(false), interval * 2.1)` for TTL-based expiry of registrations from multicast
- Rust `inject_packet()` checks `now > timestamp + interval * 2.1` at injection time (equivalent but checks at lookup time rather than using a timer)

**Index key format**: Both use `sector:action.vVERSION` (lowercased).

**CRUD aliases**: Both create `_create`, `_read`, `_update`, `_destroy` alias entries.

### 4. Requester (lookup, dispatch, retry on dispatch_failure, timeout handling): PARTIAL

**Lookup + dispatch**: Both:
- Find action by sector/action/version/envelope
- Get or create connection
- Send request with timeout

**Retry on `dispatch_failure`**:
- JS `requester.js:50-58`: checks `rpy.header.error_data.dispatch_failure`, calls `service.registration.connectFailed()`, retries once with `params.retried` guard
- Rust `requester.rs:67-78`: checks `resp.header.error_code == "dispatch_failure"`, calls `mark_failed()`, retries once
- **Divergence**: JS checks `error_data.dispatch_failure` (a nested object field), Rust checks `error_code == "dispatch_failure"` (a string field). The JS implementation treats dispatch_failure as a data flag, not an error code. This could cause mismatched retry behavior when interoperating with services that set one but not the other.

**Timeout handling**:
- JS: `timeout + 5000` (adds 5s padding to action timeout). Per-action timeout from `t\d+` flags, default 75s from config.
- Rust: `timeout_secs + 5` (ActionEntry.timeout_secs adds 5). Default 75s. Matches.

**Missing in Rust**:
- **`forwardRequest` action aliasing**: JS requester overwrites `request.header.action` and `request.header.version` from the resolved `info` before sending. Rust passes the original `opts.action` directly to `client.request()`.
- **Stale cache error reporting**: JS checks `checkStale()` and returns a specific error when the discovery cache is invalid. Rust does not have an equivalent stale-check at request time.

### 5. Discovery (announcement parsing, multicast observer, cache loading): MATCH

**Announcement parsing**: Both implement v3+v4 format:
- 9-element JSON array: `[3, ident, sector, weight, interval, uri, envelopes+v4hash, v3actions, timestamp]`
- v4 extension hash with RLE-encoded vectors (`acname`, `acns`, `acver`, `acflag`, `acsec`, `acenv`, `accompat`)
- RLE decode: `[count, value]` for repeats, bare values for singles

**Signature verification**: Both use RSA PKCS1v15 SHA256:
- Rust: `openssl::sign::Verifier` with explicit PKCS1 padding
- JS: `crypto.createVerify('sha256')` (defaults to PKCS1v15)
- SHA1 fingerprint: both compute from DER, uppercase hex with colons

**Cache file loading**: Both:
- Split on `\n%%%\n` delimiter
- Check cache staleness (default 120s max age)
- Parse each announcement, verify signature, inject into registry

**Multicast observer**:
- Rust: `observer.rs` joins multicast group, decompresses zlib, parses, injects into registry
- JS: `observe.js` uses `fs.watch` on cache file directory (not multicast). This is a design difference -- JS relies on a cache-writing daemon, while Rust listens to multicast directly.

**Announcement building**: Both produce the same wire format:
- JSON blob + `\n\n` + cert PEM + `\n` + base64 signature (76-char wrapped)
- zlib compressed for multicast

### 6. Graceful Shutdown (suspend announcer, drain requests, timing): PARTIAL

**JS implementation** (`service.js:78-91`):
1. `announcer.suspend()` -- sends weight=0 announcements (3 extra rounds at 200ms)
2. Wait 5 seconds
3. Wait for `_activeRequests === 0` (or timeout)
4. Wait 1 more second for replies to drain
5. Exit

**Rust implementation** (`listener.rs:169-232` + `multicast.rs:141-157`):
1. Announcer sends weight=0 for 10 rounds at 1s intervals (Perl behavior)
2. Service listener stops accepting
3. Drain up to 30s for active connections to finish

**Divergences**:
- JS suspend sends 3 fast rounds (200ms), Rust sends 10 slow rounds (1s) -- Rust follows Perl convention
- JS coordinates the sequencing (suspend -> wait 5s -> drain). Rust announcer and listener are independent tasks controlled by the same `watch::Receiver<bool>` shutdown signal but run concurrently rather than sequentially
- JS tracks `_activeRequests` (incremented/decremented per request). Rust tracks `active_connections` (per TCP connection), which is coarser -- a connection with 0 in-flight requests still counts as active

### 7. PING/PONG Heartbeat: PARTIAL

**JS has full heartbeat lifecycle**:
- `connection.js:152-167 setHeartbeat(msecs)`: starts interval timer, sends PING, expects PONG before next interval
- `PONG` handler clears `_heartbeatPending` flag
- If `_heartbeatPending` when next interval fires: connection error (heartbeat expired)
- Client can call `setHeartbeat()` to enable (e.g., subscriptions use 15000ms)

**Rust implementation**:
- PING handler (both client and server): responds with PONG -- correct
- PONG handler: no-op comment `"would reset timer (not implemented)"`
- **Missing**: No `setHeartbeat()` equivalent, no PING send timer, no heartbeat expiry detection
- Rust can respond to PINGs but cannot initiate heartbeat checking

### 8. Ticket Verification: PARTIAL

**JS** (`ticket.js`):
- Parses CSV ticket: `version,user_id,client_id,validity_start,validity_length,privs,signature`
- Base64 standard decode for signature (with URL-safe char replacement: `-` to `+`, `_` to `/`)
- RSA SHA256 PKCS1v15 verify against hardcoded `/etc/scamp/auth/ticket_verify_public_key.pem`
- `checkAccess()`: fetches authz table from `Auth.getAuthzTable` action, caches 5 minutes, checks privileges
- Privilege checking: hash lookup for each required privilege

**Rust** (`ticket.rs`):
- Parses same CSV format correctly
- Base64URL decode (proper URL-safe no-pad)
- RSA SHA256 PKCS1v15 verify against provided public key
- Expiry checking (validity_start + ttl)
- `has_privilege()` / `has_all_privileges()` methods

**Missing in Rust**:
- **No `checkAccess()` equivalent**: JS fetches authorization tables from `Auth.getAuthzTable` action dynamically, caching for 5 minutes. Rust has no equivalent runtime authorization table fetching.
- **Privilege storage format**: JS stores as hash `{priv_id: true}`, Rust stores as `Vec<u64>`. Both functionally equivalent.

### 9. JS Features NOT in Rust: MISSING

**Connection reconnection**: JS `connectionMgr.js:42-48` automatically evicts dead connections from cache on `lost` event. JS `subscription.js` implements full reconnection with truncated exponential backoff (50ms min, 10s max). Rust has no reconnection logic.

**`setBusy` flag on client connections**: JS `connection.js:312-319` sets `_busy` flag when requests are pending, which disables idle timeout. The server-side equivalent exists in Rust but the client-side does not.

**Stream pause/resume with backpressure**: JS `Message` is a full Node.js Stream with `pause()`/`resume()` and `drain` event. The ACK handler checks if `write()` returns false (backpressure), and defers the ACK until `drain`. Rust ACKs immediately on DATA receipt without backpressure awareness.

**Subscription / Event system**: JS has a full pub-sub mechanism:
- `subscribe` / `subscribe_done` / `event` / `notify` message types
- `ServerConnection` manages subscribed connections with filters
- `Server.putSubdata()` / `notify()` for pushing events
- `Subscription` class with reconnection and heartbeat
- Rust has none of this.

**DripSource streaming**: JS `DripSource` breaks buffers into chunks and feeds them as stream events, respecting pause/resume. Rust sends all DATA chunks synchronously (with flow control watermark pausing).

**`circularClient`**: JS has a circular discovery client for peer-to-peer announcement distribution. Not in Rust.

**Request timeout triggers connection eviction**: JS `client.js:127-130` emits `lost` on timeout, causing the connection to be evicted from the pool and a new one opened for subsequent requests. Rust just returns a timeout error without pool eviction.

**`error_data` field in headers**: JS Message headers can carry `error_data` (an object with structured error info like `{dispatch_failure: true}`). Rust `PacketHeader` does not have an `error_data` field.

**Event-driven service manager changes**: JS `serviceMgr` emits `changed` events when registrations change. Rust `ServiceRegistry` is static after construction (no notification mechanism).

**Config `val()` with fatal on missing**: JS `config.val()` calls `scamp.fatal()` when a required config key is missing and no default is provided. Rust `config.get()` returns `Option`, callers handle absence.

## Critical Gaps

1. **No heartbeat initiation**: Rust cannot detect dead connections via heartbeat. Only responds to PINGs, never sends them. This means hung connections will not be detected until a request times out.

2. **No connection eviction on failure**: When a connection dies (reader exits), the closed flag is set but the connection remains in the pool until the next `get_connection()` call checks it. JS proactively evicts via `lost` event. Combined with no heartbeat, a dead connection could persist indefinitely if no new requests target that service.

3. **`dispatch_failure` field mismatch**: Rust checks `error_code == "dispatch_failure"` but JS checks `error_data.dispatch_failure`. These are different fields. The Rust retry logic may not trigger on actual dispatch failures from JS/Perl services, and vice versa.

4. **No subscription/event system**: This is a significant feature gap for services that rely on real-time data push (e.g., announce packet subscriptions, live data feeds).

5. **Shutdown coordination**: The announcer and listener shut down concurrently rather than sequentially. In JS, the announcer suspends first (notifying peers), then after 5s the service drains requests. In Rust, both happen at the same time, so peers may continue sending requests while the service is already shutting down.

6. **Missing `error_data` header field**: Rust cannot send or receive `error_data` in packet headers. This breaks dispatch_failure detection from JS services and prevents sending structured error metadata.

## Recommendations

1. **Add `error_data` to `PacketHeader`**: Add `error_data: Option<serde_json::Value>` to enable structured error metadata. Fix `dispatch_failure` detection to check `error_data.dispatch_failure` like JS does.

2. **Implement heartbeat initiation**: Add a `set_heartbeat(interval)` method to `ConnectionHandle` that periodically sends PING and closes the connection if PONG is not received before the next interval.

3. **Add `lost` event / proactive connection eviction**: When the reader task exits, actively remove the connection from the pool rather than waiting for the next lookup. Consider a background cleanup task or a callback mechanism.

4. **Sequence the shutdown**: Make the announcer suspend first, wait for the configured drain delay, then stop accepting new connections and drain existing ones.

5. **Add client-side busy tracking**: Disable idle timeout when requests are pending, matching JS `setBusy` behavior.

6. **Consider adding the subscription system**: If real-time data push is needed, implement `subscribe`/`notify`/`event` message types.

7. **Align DATA chunk size**: Consider making the outgoing DATA chunk size configurable or matching JS's 131072 behavior, though the current 2048 (matching Perl) is wire-compatible.
