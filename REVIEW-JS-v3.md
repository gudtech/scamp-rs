# scamp-rs vs scamp-js Review — v3

Reviewer: Claude (claude-sonnet-4-6)
Date: 2026-04-13

## Summary

All Rust source files under `src/` and all JS files under `lib/` were read in their entirety. The comparison is function-by-function across each key area. Status codes: MATCH, PARTIAL, MISSING, DIVERGENT.

---

## 1. Server Read Loop

**File:** `src/service/server_connection.rs` vs `lib/transport/beepish/connection.js`

**Status: MATCH**

The old BufReader-based approach caused an infinite busy-loop on TLS partial-packet reads because BufReader's internal `fill_buf()` would call into the TLS layer repeatedly even when no complete line was available, burning CPU waiting. The rewrite uses a manual `Vec<u8>` accumulation buffer with `reader.read(&mut tmp[..4096])`:

```rust
let n = match reader.read(&mut tmp).await {
    Ok(0) => break,
    Ok(n) => n,
    ...
};
buf.extend_from_slice(&tmp[..n]);
```

This exactly mirrors JS `_ondata()` which appends incoming TCP chunks into `_inBuf` and then calls `_readPackets()` in a loop:

```js
buf.copy(this._inBuf, this._inEnd);
this._inEnd += buf.length;
if (this._started) this._readPackets();
```

The loop-until-parsed structure in both is: read raw bytes → append to buffer → loop over `Packet::parse()` / `_readPackets()` consuming complete packets → drain consumed bytes. This is semantically identical.

**Idle timeout:** Rust implements a `DEFAULT_SERVER_TIMEOUT_SECS = 120` idle timeout that applies only when no requests are in flight (matching JS `setBaseTimeout(120*1000)` and `_setTimeout()` which disables the timeout when busy). The JS idle detection uses `socket.setTimeout()` with a `'timeout'` event; Rust uses `tokio::time::timeout()` wrapping the read. Behaviorally equivalent.

**One minor divergence:** JS does an "overlong header line" check at 40 bytes (`>= 40`), while Rust uses 80 bytes (`scan_len >= 80`). Rust's constant is wider but still sane (the actual limit in Perl is 80). This is harmless.

---

## 2. Client Duplex Proxy

**File:** `src/transport/beepish/client/connection.rs` — `ConnectionHandle::from_stream()`

**Status: PARTIAL — Correct but adds latency tier; see note**

Rust cannot `split()` a `TlsStream<TcpStream>` because the underlying tokio-native-tls type doesn't implement `AsyncRead + AsyncWrite + Split`. Without split, a single task holding a `&mut TlsStream` cannot simultaneously write (from the writer task) and read (from the reader task) without deadlocking on the lock.

The solution is a duplex proxy:

```rust
let (proxy_client, mut proxy_server) = tokio::io::duplex(65536);
let mut real = stream;
tokio::spawn(async move {
    let _ = tokio::io::copy_bidirectional(&mut real, &mut proxy_server).await;
});
let (read_half, write_half) = tokio::io::split(proxy_client);
```

This adds an in-process copying task and a 65536-byte ring buffer. For SCAMP's typical workload (small JSON payloads), this is completely negligible. For large payloads (5000+ bytes), the extra copy adds one allocation and buffer copy per 65536 bytes — still negligible compared to TLS overhead.

JS has no equivalent because Node.js streams are inherently full-duplex — a single socket can be read and written concurrently without an explicit split.

**Behavioral difference with JS:** None. The proxy faithfully relays all bytes in both directions. No framing is changed; no latency is visible at the protocol level.

**Correctness note:** If the TLS stream yields a half-close, `copy_bidirectional` will propagate it correctly. If the proxy task panics, both halves of the duplex will return errors, which will be surfaced to the reader and writer tasks. No silent data loss.

---

## 3. Client Reader

**File:** `src/transport/beepish/client/reader.rs` vs `lib/transport/beepish/connection.js` (`_ondata` / `_onpacket`)

**Status: MATCH**

The rewrite uses the same manual-buffer approach as the server (area 1). Both accumulate raw bytes and loop over packet parsing until incomplete.

Packet routing matches JS `_onpacket` one-for-one:

| Packet | JS behavior | Rust behavior | Match? |
|--------|-------------|---------------|--------|
| HEADER | Stores in `_incoming[msgno]`, emits `'message'` | Inserts into `incoming` map | MATCH |
| DATA | Appends payload, immediately ACKs if not paused | Appends body, immediately sends ACK | MATCH |
| EOF | Calls `info.message.end()`, deletes from `_incoming` | Removes from map, delivers to `pending` oneshot | MATCH |
| TXERR | Sets `msg.error`, calls `end()` | Delivers error to oneshot | MATCH |
| ACK | Updates `info.acked`, calls `msg.resume()` if below watermark | Updates `state.acknowledged`, notifies `ack_notify` | MATCH |
| PING | Calls `_sendPacket('PONG', ...)` | Sends PONG via writer_tx | MATCH |
| PONG | Clears `_heartbeatPending` | Logs comment, does nothing | PARTIAL (see area 7) |

**One semantic difference on DATA:** JS only sends an ACK immediately if `info.message.write()` returns non-false (i.e. the message stream is not paused). Rust always ACKs immediately on DATA. For the client reader, this is correct — the client always has unlimited buffer (data goes into `Vec<u8>`), so there is never a reason to defer ACK. JS's deferred-ACK path is for when the consuming stream is paused (backpressure from a slow consumer). Since Rust accumulates all body bytes before delivering, immediate ACK is correct.

**Empty DATA skip:** Both Rust and JS skip zero-length DATA bodies (`if payload.length: return` / `if packet.body.is_empty() { return }`). MATCH.

---

## 4. Flow Control

**File:** `src/transport/beepish/client/connection.rs` (`send_request`) and `src/transport/beepish/client/reader.rs` (`ACK` handler) vs `lib/transport/beepish/connection.js`

**Status: MATCH**

Both use the same watermark: `65536` bytes (JS line 4: `const maxflow = scamp.config().val('flow.max_inflight', 65536)`; Rust: `const FLOW_CONTROL_WATERMARK: u64 = 65536`).

JS flow control (sender side):
```js
info.sent += d.length;
if (info.sent - info.acked >= maxflow)
    msg.pause();
```
And on ACK:
```js
info.acked = payload;
if (info.sent - info.acked < maxflow)
    info.message.resume();
```

Rust flow control (sender side, in `send_request`):
```rust
let over_watermark = out.get(&msg_no)
    .is_some_and(|s| s.sent.saturating_sub(s.acknowledged) >= FLOW_CONTROL_WATERMARK);
if !over_watermark { break; }
self.ack_notify.notified().await;
```
And on ACK receipt in reader:
```rust
state.acknowledged = ack_val;
ack_notify.notify_one();
```

The mechanism is different (JS uses stream pause/resume events; Rust uses a `tokio::sync::Notify`), but the semantics are identical: sender blocks when inflight bytes >= watermark, resumes when an ACK brings it below.

**DATA chunk size:** JS sends up to 131072 bytes per DATA packet. Rust sends 2048-byte chunks (`DATA_CHUNK_SIZE = 2048`, matching Perl's chunk size). This is harmless — the receiver handles any chunk size up to `MAX_PACKET_SIZE = 131072`.

---

## 5. error_data Field

**Files:** `src/transport/beepish/proto/header.rs`, `src/service/handler.rs`, `src/service/server_reply.rs` vs `lib/transport/beepish/connection.js`, `lib/handle/Message.js`

**Status: MATCH**

`error_data` is defined in `PacketHeader` as `Option<serde_json::Value>`, serialized as `#[serde(skip_serializing_if = "Option::is_none")]`. This matches JS where `error_data` is an optional field set on `rpy.header`.

`ScampReply` has `error_data: Option<serde_json::Value>` and a constructor `error_with_data()` that takes a `Value`.

`send_reply()` passes `reply.error_data` directly into `PacketHeader.error_data`, which is then serialized to JSON in the HEADER packet.

JS wires it the same way: `msg.header.error_data` is set before `sendMessage(msg)`.

The field is correctly propagated end-to-end.

---

## 6. dispatch_failure

**Status: DIVERGENT (Bug)**

**JS behavior (actor/service.js, line 257):**
```js
var handler = me._actions[ String(req.header.action).toLowerCase() + '.v' + req.header.version ];
if(!handler) return onrpy(new Error("action not found"));
```

The JS `service.js` sets a plain error ("action not found"). However the JS `client.js` (transport/beepish/client.js) sets `error_data: { dispatch_failure: true }` on the **connection-lost** path, not on action-not-found:

```js
proto.on('lost', () => {
    for (let id of this._pending.keys()) {
        let msg = new Message(started ?
            { error_code: 'transport', error: 'Connection lost' } :
            { error_code: 'transport', error: 'Connection could not be established',
              error_data: { dispatch_failure: true } });
        ...
    }
});
```

So `dispatch_failure` in JS is a **connection-establishment failure indicator**, not a service-level "action not found" indicator. The `requester.js` checks for it:

```js
if (rpy && service && rpy.header.error_data && rpy.header.error_data.dispatch_failure && !params.retried && !params.ident) {
    service.registration.connectFailed();
    params.retried = true;
    return this.makeRequest(params, body, callback);
}
```

**Rust behavior (`src/service/server_connection.rs` line 256):**
```rust
let reply = if let Some(registered) = actions.get(&action_key) {
    (registered.handler)(request).await
} else {
    ScampReply::error(format!("No such action: {}", action_key), "not_found".to_string())
};
```

Rust returns `error_code: "not_found"` with no `error_data`, which means `dispatch_failure` is **never set** by the Rust server when an action is missing.

**Rust requester (`src/requester.rs` line 64-69):**
```rust
let is_dispatch_failure = resp.header.error_data
    .as_ref()
    .and_then(|d| d.get("dispatch_failure"))
    .and_then(|v| v.as_bool())
    .unwrap_or(false)
    || resp.header.error_code.as_deref() == Some("dispatch_failure");
```

Rust checks both `error_data.dispatch_failure` (wire compat) and `error_code == "dispatch_failure"` (a special code). Neither is set by the Rust server on action-not-found, nor would JS set it there.

**Assessment:** The Rust requester's retry-on-dispatch_failure logic (`request_with_opts`) is correct but will never be triggered by Rust-to-Rust calls because Rust never emits `dispatch_failure`. The retry logic would fire if a Perl or JS server sends `error_data: {"dispatch_failure": true}` (connection-lost path). This is an **interop gap** not an internal Rust bug: a Rust client talking to a Perl/JS server that disconnects during connection setup will correctly retry. A Rust client talking to a Rust server will get `error_code: "not_found"` on action-not-found, which won't trigger retry (correct behavior — it would be wrong to retry a permanently missing action).

**The previous audit finding (A3)** stated Rust should set `dispatch_failure` when dispatching fails. This was likely based on confusion with Perl behavior. Looking at the actual JS code, `dispatch_failure` is a **transport-layer** signal from the connecting client side (not from the server). There is no action needed here from the server side.

---

## 7. PING/PONG Heartbeat

**Status: DIVERGENT (Known gap)**

**JS behavior (`lib/transport/beepish/connection.js`, `setHeartbeat()`):**
JS initiates heartbeats from both sides when `setHeartbeat(msecs)` is called:
```js
this._heartbeatTimer = setInterval(() => {
    if (this._heartbeatPending) this._onerror(true, 'Heartbeat expired');
    this._heartbeatPending = true;
    this._sendPacket('PING', 0, Buffer.alloc(0));
}, msecs);
```

JS client sets heartbeat via `client.setHeartbeat(msecs)`. JS server sets heartbeat via `conn.setHeartbeat(msecs)`. The JS `_onpacket.PONG` clears `_heartbeatPending`; if no PONG arrives before the next PING interval, the connection is torn down with `'Heartbeat expired'`.

**Rust behavior:**
Both the server (`server_connection.rs`) and client reader (`reader.rs`) correctly handle incoming PINGs by sending PONG. Neither initiates PINGs. The PONG handler in the client reader has a comment: `// Heartbeat response — would reset timer (not implemented)`.

There is no timer-based PING initiator in Rust. Rust never sends unsolicited PINGs.

**Impact:** If a Perl or JS peer expects the Rust endpoint to initiate heartbeats, it will time out after `beepish.server_timeout * timeoutMultiplier`. If Rust connects to a JS service that sends PINGs, Rust will correctly respond with PONGs, so that direction works. The gap is: Rust does not actively detect dead connections via PING timeouts — it relies solely on TCP keepalive / read errors.

**Recommendation:** Implement a `tokio::time::interval`-based PING sender in `ConnectionHandle` (client side) and optionally in `handle_connection` (server side). This is the A6 finding from the previous audit and remains unresolved.

---

## 8. Connection Pool Eviction

**File:** `src/transport/beepish/client/connection.rs` (`BeepishClient`) vs `lib/util/connectionMgr.js`

**Status: DIVERGENT (Known gap)**

**JS behavior (`connectionMgr.js`):**
```js
if (client.on) client.on('lost', function () {
    if (me._cache[address] === client) {
        console.log('--closed connection--', address);
        delete me._cache[address];
    }
});
```

When a JS connection emits `'lost'`, it is immediately removed from the cache. New requests get a fresh connection.

**Rust behavior (`BeepishClient::get_connection`):**
```rust
if let Some(conn) = connections.get(&service_info.uri) {
    if !conn.closed.load(Ordering::Relaxed) {
        return Ok(conn.clone());
    }
    connections.remove(&service_info.uri);
}
```

Rust evicts stale entries **lazily**: at the next `get_connection()` call it checks `conn.closed` and removes it if true. If no new request arrives, the dead connection stays in the map (holding an `Arc` reference, keeping the `ConnectionHandle` alive until `Drop`).

The `closed` flag is set when the reader task exits (`reader_closed.store(true, Ordering::Relaxed)`) and when `Drop` is called. So connections that die between requests will be correctly replaced on the next request.

**What's missing:** The pool has no maximum size. If many different `service_info.uri` values are seen (e.g., many service instances rotating through ports), the pool grows without bound. JS similarly has no eviction by count, but JS evicts immediately on `'lost'` so the pool shrinks promptly. Rust's lazy eviction means a burst of failed connections leaves stale `Arc` entries until the next request to that URI.

**For a typical single-service deployment this is harmless.** For a deployment with many short-lived service instances (common in rolling deployments), the Rust pool could accumulate stale entries. The pool never serves stale connections (the `closed` check prevents it), but it does hold memory.

**Recommendation:** Add an eager eviction callback: when the reader task exits, remove the entry from the connections map. This requires passing a `Weak<Mutex<HashMap<...>>>` into the reader task. This is the A7 finding and remains unresolved.

---

## 9. E2E Test Architecture

**File:** `tests/e2e_full_stack.rs`

**Status: MATCH (well-structured)**

The five tests cover:
1. `test_echo_roundtrip` — full TLS → discovery → request → response cycle
2. `test_large_body` — DATA chunking through TLS (5000 bytes → 3 chunks)
3. `test_unknown_action` — error response, not transport error
4. `test_sequential_requests` — connection reuse (5 requests over same `ConnectionHandle`)
5. `test_announcement_signature_verification` — RSA SHA256 signature roundtrip

**Architecture is correct.** Key design decisions:
- Uses `tokio::test(flavor = "multi_thread", worker_threads = 2)` — necessary because the server and client run as separate tasks and the duplex-proxy also needs a thread. Single-threaded executor would deadlock.
- Uses `tempfile::NamedTempFile` for the discovery cache and auth file — they are kept alive by the returned tuple until end of test.
- The 50ms `sleep` after `service.run()` spawn is needed to let the TLS acceptor bind before the client connects. Fragile but adequate for CI.
- The test correctly drops `client` before sending the shutdown signal, ensuring the connection drain completes immediately rather than waiting 30 seconds.

**Issues found:**
1. **The `sleep(50ms)` is a race condition.** In a loaded CI environment, 50ms may not be enough for `TcpListener::bind()` + TLS acceptor setup. A more robust approach would be for `ScampService::run()` to signal readiness on a `tokio::sync::oneshot` channel after the acceptor is ready.
2. **Test 3 (`test_unknown_action`) sends a request for `"NonExistent.action"` to the service registered as `"ScampRsTest.echo"`. The registry lookup uses `find_action("main", "ScampRsTest.echo", 1)` to get the `service_info` (correct — we need a valid address to connect to), then sends a different action name. This is correct and intentional — it tests that the server returns a well-formed error response, not that the registry refuses the call.
3. **No test for flow control under backpressure.** The flow control path (`FLOW_CONTROL_WATERMARK`) is only exercised if the server is slow to ACK. None of the E2E tests verify that the sender actually blocks and resumes correctly.
4. **No test for PING/PONG.** The server unit test (`server_connection_tests.rs::test_ping_pong`) covers the server responding to a PING, but there is no E2E test that verifies a full PING/PONG cycle across TLS.
5. **No concurrent-request test.** Test 4 sends requests sequentially. An important correctness test would be sending multiple requests concurrently on the same `ConnectionHandle` and verifying that responses are matched to the correct `request_id`.

---

## 10. Additional Area-by-Area Findings

### Wire Protocol (packet framing)

**Status: MATCH**

Both use `TYPE MSGNO SIZE\r\n<payload>END\r\n`. Rust enforces `\r\n` (rejects bare `\n`). JS checks for `\r\n` in the regex: `/^(\w+) (\d+) (\d+)\r\n$/`. Both reject overlong headers. Both reject packets > 131072 bytes (`MAX_PACKET_SIZE` = JS `131072`).

### PacketHeader JSON

**Status: MATCH**

All field names, serialization behavior, and null handling match:
- `"type"` field (not `"message_type"`) — confirmed by test `test_packet_header_json_field_names`
- `envelope` as lowercase string
- `error`/`error_code` omitted when None via `skip_serializing_if`
- `ticket: null` and `identifying_token: null` deserialize to empty strings — confirmed by tests

### Announcement packet format

**Status: MATCH**

Rust builds v3+v4 announcements matching the JS format. The JS `_makePacket` produces:
```
[3, ident, sector, weight, interval, address, [envelopes...], classes, Date.now()]
```

Rust `build_announcement_packet` produces the same array with `v4_hash` appended to position 6. Signing uses `sha256` + RSA PKCS1v15 with 76-char base64 wrapping — identical to JS `crypto.createSign('sha256')...sign(me.key, 'base64').replace(/.{1,76}/g, "$&\n")`.

**One difference:** JS signs the JSON blob and writes `blob + '\n\n' + cert + '\n' + sig + '\n'`. Rust does the same. The JS `cert` variable is `chunks[1] + '\n'` where `chunks[1]` is the PEM between the two `\n\n` delimiters. Rust writes the cert PEM directly. This should be equivalent, but PEM trailing newlines are sensitive — if the cert does not end with `\n`, the signature will differ. The Rust code writes `cert_pem_str` (the raw bytes from the key file), which in practice always ends with `\n`. **Low risk.**

**Important gap in v4 action encoding:** The `build_announcement_packet` function computes `v4_acns`, `v4_acname`, etc., but they are all declared as `Vec::new()` and never populated. The v4 extension hash is emitted with empty RLE arrays. Actions are placed only in the v3 section. This means Rust-announced services are visible to JS observers that parse v3 format, but the v4 fields carry no information. JS observers would fall back to v3 parsing, which works. **However,** this means action flags like `noauth` and timeout values are encoded only in the v3 `flags` string, which is less structured than v4. This is a completeness gap but not a correctness bug for the v3 parsing path.

### Discovery / ServiceRegistry

**Status: MATCH**

Both use the same index key format: `sector:namespace.action.vVERSION` (lowercased). Both implement weighted random selection. Both implement CRUD aliases (`_create`, `_read`, `_update`, `_destroy`). Both implement replay protection via timestamps. Both implement exponential backoff failure marking (`min(failure_count, 60) * 60 seconds`).

Rust uses a `BTreeMap` for `actions_by_key` (deterministic iteration) vs JS's plain object. Functionally identical.

**JS has TTL-based active/inactive state** via `Registration.refresh()` with a `setTimeout(interval * 2.1)`. When the timeout fires, the service is marked inactive (weight-0-like). This implements the "service went quiet" detection.

**Rust has no equivalent TTL-based eviction.** Once a service is injected from the cache file, it stays until the cache is reloaded. The Rust observer (`discovery/observer.rs`) injects packets as they arrive but does not prune services that stop announcing. This is the **most significant correctness gap in discovery**: a service that crashes but whose last announcement is still in the cache will continue to receive traffic until the Rust process reloads the cache or is restarted.

### Authorization (ticket / authz)

**Status: PARTIAL**

The Rust `AuthzChecker` correctly fetches `Auth.getAuthzTable~1`, caches for 5 minutes, verifies ticket signature, and checks privilege IDs. This matches JS `ticket.js:checkAccess`.

**One difference:** JS `ticket.js` checks that the action exists in the authz table at all, and returns an error if the action is missing from the table (`'Unconfigured action ' + real_info.action`). Rust's `authz.rs`:
```rust
if let Some(required_privs) = table.get(&key) {
    for &priv_id in required_privs { ... }
}
// If action not in table, no specific privileges required
```

Rust silently allows access when an action is not in the authz table. JS would deny with "Unconfigured action". This is a security difference: in Rust, a new action not yet added to the authz table is accessible to any ticket holder. In JS, it would be locked out until configured.

**Recommendation:** When `table.get(&key)` returns `None` and the action is not `noauth`, Rust should return an authorization error ("action not configured in authz table").

### Authorized Services

**Status: MATCH**

The file format, fingerprint lookup, `_meta.*` bypass, `:ALL` expansion, `main:` default prefix, case-insensitive matching, and comment stripping all match JS `handle/service.js` behavior exactly. Confirmed by tests.

### Requester

**Status: PARTIAL**

Rust `Requester` supports `request()` and `request_with_opts()` with dispatch-failure retry. JS `Requester` additionally supports:
- `forwardRequest()` — allows specifying a target `ident` to route to a specific service instance (Rust lacks this)
- `makeJsonRequest()` — wraps request+response with JSON parsing and structured logging (Rust has no equivalent logging)
- `subscribe` / `notify` / `onEvent` — event subscription protocol (Rust has no event subscription support at all)

The event subscription protocol (`subscribe`/`event`/`subscribe_done` message types) is not implemented in Rust at all. The Rust `MessageType` enum only has `Request` and `Reply`; parsing any other `"type"` value returns a `de::Error`.

### Service (actor/service.js)

**Status: PARTIAL**

Rust `ScampService` provides:
- `register()` / `register_with_flags()`
- `bind_pem()` with port randomization (30100-30399)
- `run()` with graceful shutdown (drain with 30s timeout)
- `build_announcement_packet()`

Missing from Rust vs JS:
- **`queue_depth` built-in action** — JS auto-registers `${tag}.queue_depth` returning `{ queue_depth: _activeRequests - 1 }`. Rust has no equivalent.
- **`stopService()` grace period** — JS waits 5000ms after suspending the announcer before draining. Rust goes directly to drain. This means Rust services can receive requests during the 5s "wind down" window that JS would not.
- **pidfile writing** — minor operational gap.
- **`staticJsonHandler` / `cookedHandler` / `fullHandler`** convenience wrappers — Rust exposes raw `ScampRequest`/`ScampReply` and users write handlers directly. Less ergonomic but functionally equivalent.

---

## Summary Table

| Area | Status | Severity |
|------|--------|----------|
| Server read loop (BufReader → Vec buffer) | MATCH | — |
| Client duplex proxy (TLS split workaround) | PARTIAL | Low — extra copy, no behavior change |
| Client reader (manual buffer) | MATCH | — |
| Flow control (watermark, pause/resume) | MATCH | — |
| error_data field wiring | MATCH | — |
| dispatch_failure on action-not-found | DIVERGENT | Low — no server should set this; requester retry is JS-interop-correct |
| PING/PONG heartbeat initiation | DIVERGENT | Medium — Rust never sends PINGs, can't detect dead connections without TCP error |
| Connection pool eviction | DIVERGENT | Low — lazy eviction is correct; just accumulates stale entries under churn |
| E2E test architecture | MATCH (5 tests, well-structured) | — |
| Wire protocol packet framing | MATCH | — |
| PacketHeader JSON wire format | MATCH | — |
| Announcement packet format | MATCH (v4 fields empty but v3 correct) | Low |
| Discovery / ServiceRegistry | MATCH (no TTL eviction from multicast) | Medium — stale service entries persist |
| Authorization (authz table) | PARTIAL | Medium — missing action not in table → deny |
| Authorized services file | MATCH | — |
| Requester | PARTIAL | Medium — no event subscription, no ident routing |
| Service actor | PARTIAL | Low — missing queue_depth, grace period |

---

## Priority Recommendations

1. **(Medium)** Implement TTL-based service eviction in `ServiceRegistry`. When the multicast observer injects a packet, start/reset a timer per service identity. When the timer fires (at `interval * 2.1`), remove the service. Without this, crashed services stay in the routing table indefinitely.

2. **(Medium)** Fix authz table "unconfigured action" handling. When an action is not in the authz table and is not `noauth`, return an authorization error rather than silently allowing access.

3. **(Medium)** Implement PING initiation in `ConnectionHandle`. Add a configurable heartbeat interval; if no PONG arrives before the next PING, mark the connection closed and remove from pool.

4. **(Low)** Add eager pool eviction: when the reader task exits, proactively remove the dead connection from `BeepishClient::connections`.

5. **(Low)** Fix the `setup_service()` race in E2E tests: signal readiness via oneshot channel after bind, rather than sleeping 50ms.

6. **(Low)** Add E2E test for concurrent in-flight requests on a single `ConnectionHandle` to verify request_id demultiplexing.
