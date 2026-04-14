# scamp-rs Code Review — Elegance & Correctness (v3)

**Reviewer:** Claude Sonnet 4.6  
**Date:** 2026-04-13  
**Scope:** All Rust source under `src/` + `tests/e2e_full_stack.rs`

---

## Summary

The codebase is in good shape overall. The protocol implementation is solid, test coverage is meaningful, and the decomposition is coherent. Most findings are Medium or lower; there are two High-severity issues and no Criticals assuming the protocol is only used in a trusted internal network (the TLS fingerprint verification gap is the closest thing to a Critical in an adversarial context).

---

## Critical

*(None)*

---

## High

### H1 — `from_stream` proxy task is a silent orphan that leaks on drop

**File:** `src/transport/beepish/client/connection.rs:90-92`

```rust
tokio::spawn(async move {
    let _ = tokio::io::copy_bidirectional(&mut real, &mut proxy_server).await;
});
```

The spawned proxy task holds `real` (the TLS stream) and `proxy_server` (one half of the duplex). When `ConnectionHandle` is dropped, `Drop` aborts `reader_handle` and `writer_handle`, but **never aborts the proxy task**. The proxy task will then block until the remote side closes the TCP connection (up to the idle timeout, or forever for long-lived idle connections). Until then, the TLS stream and the OS file descriptor are not released.

On connection errors or test teardown, this means:
- The TLS stream's `Drop` (and its underlying `TcpStream`) is deferred by an unknown amount of time.
- In the tests `test_client_echo`, etc., `_server` is never awaited; with many test runs the proxy tasks accumulate.
- In production, failed connections leave OS FDs open until remote close.

**Fix:** Store the proxy `JoinHandle` in `ConnectionHandle` and abort it in `Drop`:

```rust
pub struct ConnectionHandle {
    // ...existing fields...
    proxy_handle: tokio::task::JoinHandle<()>,
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        self.proxy_handle.abort();
        self.reader_handle.abort();
        self.writer_handle.abort();
    }
}
```

**Secondary concern (design):** The proxy is added to avoid "split-lock contention on TLS streams." `tokio_native_tls::TlsStream` is not `Split` natively, which is the real constraint. The proxy is a legitimate workaround, but it adds two extra kernel pipe copies per byte on the hot path. The comment should name the actual constraint (TLS stream is not `AsyncRead + AsyncWrite` splittable without `Arc<Mutex<>>`) so future maintainers don't remove it thinking it's needless.

**Also:** There is a subtle ordering risk: the `copy_bidirectional` proxy could observe the `proxy_server` side closed (because `reader_handle` and `writer_handle` completed) and return `Ok(...)` before their spawned closures even run. In that case the proxy task ends *before* `Drop` is called, so the abort is a no-op — fine. But if the proxy task exits due to a real I/O error (network drop), neither `reader_handle` nor `writer_handle` gets an explicit error notification; they will naturally observe EOF/broken pipe on their next read/write, so this is not a correctness issue, just worth knowing.

---

### H2 — Server-side ACK tracking is unused (flow control silently missing)

**File:** `src/service/server_connection.rs` and `src/service/server_reply.rs`

`outgoing: HashMap<u64, OutgoingReplyState>` is maintained in `route_packet` (ACK updates `state.acknowledged`) and in `send_reply` (updates `state.sent`), but **the server never waits for ACKs**. `send_reply` writes all DATA packets in a tight loop with no flow-control check. This means:

- The server will transmit an unbounded amount of data into the TLS write buffer without back-pressure.
- For large replies to a slow client, this can exhaust memory (the write buffer grows without bound).
- There is no `ack_notify`/`Notify` on the server side, so there is nowhere to add the wait even if desired.

The client side (`connection.rs:207-244`) has proper flow control with `FLOW_CONTROL_WATERMARK`. The server does not.

This is a High rather than Critical because: (a) in practice TLS + TCP flow control applies back-pressure to the write syscall when the client's TCP receive window is full, so memory growth is bounded by OS socket buffers; (b) `DATA_CHUNK_SIZE` is small (2048 bytes). But for large replies to a genuinely slow consumer it can still queue significant data in userspace.

**Fix:** Either add a `Notify` + watermark check to `send_reply` (matching client-side logic), or document that the server relies on OS-level back-pressure and does not implement D5 on the send side.

---

## Medium

### M1 — Buffer growth is unbounded on malformed streams

**Files:** `src/service/server_connection.rs:43, 53-77` and `src/transport/beepish/client/reader.rs:42-56`

Both the server and client reader loop do:
```rust
let mut buf = Vec::with_capacity(8192);
// ...
buf.extend_from_slice(&tmp[..n]);
// parse, drain consumed...
buf.drain(..consumed);
```

`MAX_PACKET_SIZE` (131072) caps any single packet, and `ParseResult::Fatal` causes an early return. However, consider the malformed stream that never produces a valid packet boundary: it sends data byte-by-byte (or in chunks) with valid-looking header lines but always claims the packet is slightly larger than what's available. The parser will repeatedly return `NeedBytes` and the buffer will grow toward `MAX_PACKET_SIZE + header_overhead` per incomplete packet before eventually getting the terminal bytes.

More dangerously: a stream that sends many partial header lines (each valid UTF-8, each under 80 bytes, each without `\r\n`) would cause repeated `TooShort` returns while `buf` grows without any bound, since `TooShort` means we read more data and try again. There is no cap on `buf.len()`.

**Fix:** Add a `buf.len() > MAX_PACKET_SIZE * 2` guard (or similar) that returns `Fatal` after accumulating more data than any valid packet could require.

---

### M2 — `next_incoming_msg_no` check rejects out-of-order only on HEADER, not DATA/EOF

**Files:** `src/service/server_connection.rs:122-128` and `src/transport/beepish/client/reader.rs:113-128`

```rust
PacketType::Header => {
    if packet.msg_no != *next_incoming_msg_no { ... }
```

The sequence counter is only enforced on HEADER packets. If the remote sends DATA or EOF with an out-of-sequence or spoofed `msg_no`, the code silently logs "DATA with no active message" and returns. This is the correct _resilience_ behavior (don't crash), but it means a confused sender that interleaves two requests does not get a diagnostic indicating a sequencing violation — it just gets dropped packets. This is consistent with how Perl/JS handle it; just worth knowing.

---

### M3 — Race condition in `send_request` between timeout and cleanup

**File:** `src/transport/beepish/client/connection.rs:263-272`

```rust
let result = match timeout(timeout_duration, response_rx).await {
    Ok(Ok(response)) => Ok(response),
    Ok(Err(_)) => Err(anyhow!("Connection lost while waiting for response")),
    Err(_) => {
        self.pending.lock().await.remove(&request_id);   // (A)
        Err(anyhow!("Request timed out after {:?}", timeout_duration))
    }
};
self.outgoing.lock().await.remove(&msg_no);              // (B)
result
```

On timeout (the `Err(_)` arm), `pending` is cleaned up at (A), but `outgoing` is not. Line (B) does clean up `outgoing` after the match, so in the timeout case the path is: remove from `pending` → return `Err` → fall through to (B). **This is actually correct** — `outgoing` is always cleaned up at (B). But there is a subtle window: after (A) removes `pending[request_id]`, a stale ACK for `msg_no` could still arrive from the reader task and update `outgoing[msg_no]`. The `outgoing.remove` at (B) will then remove the entry, but in the window between timeout and line (B) the reader is doing unnecessary work. This is benign but leaves a minor inconsistency.

More importantly, in the normal success path, if the response arrives simultaneously with the timeout, `Err` is returned (timeout wins the `select`) but the response body is dropped on the floor — this is correct behavior for a timeout.

---

### M4 — `closed` flag checked with `Ordering::Relaxed` across multiple threads

**File:** `src/transport/beepish/client/connection.rs`

The `closed` flag is set with `Relaxed` in `Drop` and checked with `Relaxed` in `send_request`. On x86/ARM this is safe in practice because of TSO, but the MFST (memory model) does not guarantee that a `Relaxed` store in `Drop` (on thread A) is visible before a `Relaxed` load in `send_request` (on thread B). The correct ordering for this pattern is `Release` on store and `Acquire` on load, which provides the happens-before guarantee. Currently there is no memory ordering guarantee that the `closed.store(true)` in `Drop` is visible to `send_request` without a full synchronization point. In practice the tokio runtime and async executor do provide ordering, but relying on that rather than explicit memory ordering is fragile.

**Fix:**
```rust
// In Drop:
self.closed.store(true, Ordering::Release);

// In send_request:
if self.closed.load(Ordering::Acquire) { ... }
```

---

### M5 — `dispatch_and_reply` authorization logic has a gap: empty ticket is not checked

**File:** `src/service/server_connection.rs:231-239`

```rust
if let Some(checker) = authz {
    if !noauth && !msg.header.ticket.is_empty() {
        if let Err(e) = checker.check_access(...).await { ... }
    }
}
```

The condition `!msg.header.ticket.is_empty()` means: if the ticket is empty (unauthenticated caller), authorization is **skipped entirely**. Only if a ticket is present is it checked. This is the correct behavior for SCAMP (unauthenticated callers without tickets are handled by the noauth flag or caller policy), but it means that deploying an `AuthzChecker` does not enforce "all calls must have a ticket." If an `AuthzChecker` is present and the action is not `noauth`, a call with an empty ticket is still allowed through. This may be intentional but is not documented.

---

### M6 — `CacheFileAnnouncementIterator` can silently lose the last record

**File:** `src/discovery/cache_file.rs:33`

```rust
Ok(buffer) if buffer.is_empty() => return None, // EOF
```

If the cache file ends with `\n%%%\n` (which the E2E test writes), the last announcement is correctly returned. But if a file ends without the delimiter (e.g., a truncated cache), the data accumulated in `announcement_data` is silently discarded when `fill_buf()` returns empty. The caller gets `None` without any indication that bytes were left unconsumed.

**Fix:** After `fill_buf()` returns empty, check if `announcement_data` is non-empty and return it as a final record (or return an error).

---

### M7 — `announce.rs`: v4 extension vectors are always empty

**File:** `src/service/announce.rs:39-44`

```rust
let v4_acns: Vec<String> = Vec::new();
let v4_acname: Vec<String> = Vec::new();
// ...
if active {
    for action in actions { /* populates v3_class_map only */ }
}
```

All v4 vectors (`v4_acns`, `v4_acname`, `v4_acver`, etc.) are initialized as empty and never populated. The v3 class map is populated. This means the announcement only uses v3 format — v4-only receivers will see no actions. This is likely a known stub, but it is undocumented. If the Perl/Go/JS receivers rely on v4 for routing, services announced by scamp-rs will be invisible to them.

---

### M8 — `AuthzChecker::get_table` has a TOCTOU race on cache update

**File:** `src/auth/authz.rs:68-93`

```rust
{
    let cached = self.cached.read().await;  // (1) read lock
    if let Some(ct) = &*cached {
        if now_secs() < ct.expires_at {
            return Ok(ct.table.clone());
        }
    }
}
// lock released here
let resp = self.requester.request(...).await?;  // (2) fetch
let mut cached = self.cached.write().await;     // (3) write lock
*cached = Some(CachedAuthzTable { ... });
```

Multiple concurrent requests can all observe a stale cache at (1), all call `requester.request` at (2) in parallel, and all write to the cache at (3). The last write wins, but the Auth service gets `N` redundant calls instead of 1. This is a thundering-herd problem, not a correctness problem, but it can cause a burst of Auth service calls on cache expiry.

**Fix:** Use a `tokio::sync::OnceCell` or a `Mutex<Option<...>>` + `notify` pattern to serialize the refresh.

---

## Low

### L1 — `send_reply` does not track `sent` correctly while holding mutex

**File:** `src/service/server_reply.rs:49-67`

`send_reply` holds the writer lock (`let mut w = writer.lock().await`) across the entire send loop. It reads and writes `outgoing.get_mut(&reply_msg_no)` inside the loop while also holding the writer lock. Since `outgoing` is passed as `&mut HashMap` (not behind a Mutex), this is fine for correctness. But the flow-control state in `outgoing` (sent/acknowledged) is updated even though (per H2 above) the server never acts on it. The `outgoing.remove(&reply_msg_no)` at the end of `send_reply` means the ACK handler in `route_packet` could receive an ACK for a `msg_no` that was already removed — it is silently ignored (`if let Some(state) = outgoing.get_mut(...)` returns `None`). This is benign but means server-side ACK processing is dead code.

---

### L2 — `get_action` / `pick_healthy` calls `rand::random::<usize>()` on each call

**File:** `src/discovery/service_registry.rs:251`

```rust
Some(pool[rand::random::<usize>() % pool.len()])
```

`rand::random` with `%` introduces modulo bias for non-power-of-two pool sizes, and creates a new `ThreadRng` on each call. For the typical case of a small pool (1-5 services) the bias is negligible, but `rand::thread_rng().gen_range(0..pool.len())` or `pool.choose(&mut rng)` would be more idiomatic and bias-free.

---

### L3 — `Ticket::parse` signature field assumes `parts[6]` but does not handle trailing commas

**File:** `src/auth/ticket.rs:51`

```rust
let sig_str = if parts.len() > 6 { parts[6] } else { "" };
```

If the ticket string has exactly 6 fields with an empty signature (`"1,42,100,1700000000,3600,,"`), `parts.len()` is 7 and `parts[6]` is `""`. `base64url_decode("")` will return an empty `Vec<u8>` (base64 decodes successfully to nothing). The subsequent RSA verify call will then fail with a cryptic OpenSSL error rather than a clear "missing signature" message. The test `test_parse_ticket_no_privileges` passes because it only calls `parse`, not `verify`.

---

### L4 — `buf.drain(..consumed)` causes O(n) shift on every read iteration

**Files:** `src/service/server_connection.rs:106` and `src/transport/beepish/client/reader.rs:83`

`Vec::drain(..n)` shifts all remaining elements left, which is O(remaining). For small packets this is fine, but for a high-throughput connection with many small packets per read, this accumulates cost. A `VecDeque` or a sliding-window index (`let mut start = 0;` + `buf = buf.split_off(consumed);`) would be more efficient. `buf.split_off(consumed)` is O(remaining) in allocation cost but avoids the copy if consumed is close to `buf.len()`. The truly zero-copy approach is a ring buffer or bytes::BytesMut, which are common in Tokio ecosystems (`bytes` crate).

This is a minor concern at DATA_CHUNK_SIZE=2048 and typical SCAMP workloads, but worth noting for high-throughput paths.

---

### L5 — `writer_task` flushes after every packet; could batch

**File:** `src/transport/beepish/client/connection.rs:289-300`

```rust
async fn writer_task(mut writer: impl AsyncWrite + Unpin, mut rx: mpsc::Receiver<Packet>) {
    while let Some(packet) = rx.recv().await {
        packet.write(&mut writer).await?;
        writer.flush().await?;
    }
}
```

Each packet triggers a flush, which may trigger a TLS record and a TCP segment. For a 5000-byte body chunked into 3 × 2048-byte DATA packets, this produces 3 TLS records + 1 for HEADER + 1 for EOF = 5 TLS records. Batching (flush only when the channel is empty) would reduce this to 1-2 TLS records. Use `rx.recv_many` or `try_recv` in a drain loop:

```rust
// After writing one packet, drain queued packets before flushing
while let Ok(next) = rx.try_recv() {
    next.write(&mut writer).await?;
}
writer.flush().await?;
```

---

### L6 — `ScampReply::error_with_data` vs. builder pattern

**File:** `src/service/handler.rs:49-56`

The three constructors (`ok`, `error`, `error_with_data`) are sufficient for current use. As the API grows, callers wanting `ok` with `error_data` (e.g., structured metadata in a success reply) would need a fourth variant. A builder is more future-proof:

```rust
ScampReply::ok(body).with_error_data(data)
ScampReply::error("msg", "code").with_error_data(data)
```

This is a style note, not a bug. The current API is ergonomic for the existing callers.

---

### L7 — E2E test uses `tokio::time::sleep(50ms)` for service readiness

**File:** `tests/e2e_full_stack.rs:93, 135, 174, etc.`

```rust
tokio::time::sleep(std::time::Duration::from_millis(50)).await;
```

Sleep-for-readiness is a common anti-pattern in async tests. If the service takes longer than 50ms to bind (e.g., on a slow CI machine under load), the test will race. The correct pattern is to use the fact that `bind_pem` completes synchronously before `service.run` is called, and that `TcpListener::bind` succeeds before `run` starts accepting. Since `service.run` starts accepting immediately, the client just needs to retry on `connection refused`. Alternatively, pass the bound address (which is already known via `service.address()`) and attempt the connection with retries.

For a test-only concern, the 50ms sleep is "good enough," but it introduces flakiness risk on slow hosts.

---

### L8 — `observer.rs` holds `RwLock<ServiceRegistry>` write lock while doing CPU-bound signature verification

**File:** `src/discovery/observer.rs:87-88`

```rust
let mut reg = registry.write().await;
reg.inject_packet(packet, auth);
```

`inject_packet` calls `packet.signature_is_valid()` internally. `signature_is_valid` does RSA public key verification — CPU-bound work — while holding the write lock on the registry. This blocks all readers (including request routing) for the duration of the RSA verify. Signature verification should be done before acquiring the write lock:

```rust
if !packet.signature_is_valid() { return Ok(()); }
let mut reg = registry.write().await;
reg.inject_packet_presigned(packet, auth); // assumes valid sig
```

This requires a refactor of `inject_packet` to either take `verified: bool` or split the signature check out, but the benefit (shorter critical section) is significant in a high-multicast environment.

---

### L9 — `announce.rs` timestamp uses `SystemTime::now()` without monotonic protection

**File:** `src/service/announce.rs:82-85`

```rust
let timestamp = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs_f64();
```

`SystemTime` can go backward (NTP adjustment, leap second). A backward-moving timestamp in the announcement will cause `inject_packet` to reject the announcement as a replay (`timestamp <= prev_ts`). On a host with NTP corrections this could silently stop service announcements from being accepted. This is unlikely but worth noting; it matches the behavior of Perl/JS implementations which have the same issue.

---

### L10 — `authorized_services.rs` regex construction silently drops failed patterns

**File:** `src/auth/authorized_services.rs:107-121`

```rust
.filter_map(|token| {
    // ...
    regex::Regex::new(&rx).ok()
})
```

If a regex fails to compile (e.g., a malformed pattern in the auth file), it is silently dropped with no warning. A service could appear authorized (or not) for the wrong reasons. At minimum a `log::warn!` should appear on failed regex compilation.

---

### L11 — `send_request` acquires `outgoing` lock twice per chunk

**File:** `src/transport/beepish/client/connection.rs:215-244`

Inside the DATA loop:
1. `self.outgoing.lock().await` to check watermark (line 216)
2. `self.outgoing.lock().await.get_mut(...)` to update `sent` (line 241)

These are two separate lock acquisitions per chunk. Since this is async (not blocking), contention is low, but the pattern could be collapsed into one acquisition per chunk by restructuring:

```rust
let chunk = &body[offset..end];
let over_watermark = {
    let mut out = self.outgoing.lock().await;
    let state = out.get_mut(&msg_no); // check + update in one lock
    // ...
};
```

---

## Style / Cosmetic

### S1 — `ScampResponse` vs `ScampReply` naming asymmetry

`ScampResponse` (client-side, `connection.rs`) and `ScampReply` (server-side, `handler.rs`) represent structurally similar things (the received/sent body + header). The naming asymmetry (`Response` vs `Reply`) is intentional (client receives a response; server sends a reply) but can be confusing. The comment in `handler.rs` says "A response to send back" which blurs the distinction.

---

### S2 — `#[cfg(test)]` on `test_helpers.rs` prevents use from integration tests

**File:** `src/lib.rs:8-9`

```rust
#[cfg(test)]
pub(crate) mod test_helpers;
```

Because `test_helpers` is `#[cfg(test)]`, it is unavailable from `tests/e2e_full_stack.rs` (which is a separate crate boundary). The e2e test does not use test_helpers — it reimplements its own helpers. If e2e tests ever need `echo_actions()` or `write_request()`, they cannot import them. Making `test_helpers` a `#[cfg(any(test, feature = "test-helpers"))]` pub module would solve this without shipping test code in production builds.

---

### S3 — `server_connection_tests` and `connection_tests` declared with `#[cfg(test)] mod`

**Files:** `src/service/mod.rs:12-13` and `src/transport/beepish/client/mod.rs:5-6`

The pattern:
```rust
#[cfg(test)]
mod server_connection_tests;
```
where the test file lives in the same directory is idiomatic Rust. The alternative (inline `#[cfg(test)] mod tests { ... }`) avoids the extra file. Both are valid; the file-per-module approach aids discoverability but can confuse `cargo test --list` output since module names appear as `service::server_connection_tests::test_*`. This is a style preference, not a bug.

---

### S4 — `announce.rs` variables declared but never mutated

**File:** `src/service/announce.rs:39-44`

```rust
let v4_acns: Vec<String> = Vec::new();
let v4_acname: Vec<String> = Vec::new();
// ...
```

These should all be `#[allow(unused_variables)]` or simply removed if v4 population is not yet implemented, to keep `cargo clippy` clean.

---

### S5 — `unwrap()` usage inventory

All `unwrap()` calls found:

| Location | Call | Risk |
|---|---|---|
| `crypto.rs:16` | `hash(...).expect("SHA1 hash failed")` | Panics if OpenSSL SHA1 is broken (catastrophic system failure) — acceptable |
| `announce.rs:85` | `.unwrap()` on `duration_since(UNIX_EPOCH)` | Panics if system time is before Unix epoch — acceptable |
| `bus_info.rs:128` | `to_string_lossy().into_owned()` | No unwrap — fine |
| `multicast.rs:39` | `DEFAULT_MULTICAST_GROUP.parse().unwrap()` | Panics if compile-time constant is wrong — acceptable |
| `service_info/mod.rs:209` | `caps[1].parse().unwrap()` | The regex `(\d+)` guarantees `\d+` is parseable — acceptable |
| `test_helpers.rs` | Multiple `.unwrap()` | Test code — fine |
| `connection_tests.rs`, `server_connection_tests.rs` | Multiple `.unwrap()` | Test code — fine |
| `serve.rs:90` | `announce_ip.parse().unwrap_or(bind_ip)` | Has fallback — fine |
| `config.rs:113` | `Regex::new(...).unwrap()` | Compile-time constant — acceptable |

No panicking `unwrap()` was found in production hot paths (request/response handling). The codebase uses `?` and `anyhow` appropriately throughout.

---

### S6 — `MockClient` is misnamed and inconsistently used

**File:** `src/transport/mock.rs`

`MockClient` uses `BTreeMap<String, String>` headers and custom `MockResponse` rather than the `BeepishClient` interface. It appears to be a leftover from an HTTP-style transport design and is not integrated with the current SCAMP/BEEPish client. It has exactly one test. If it is not used by any production code path, it should either be integrated, adapted to the BEEPish protocol, or removed to reduce maintenance surface.

---

## Focus Area Responses

### 1. BufReader → manual buffer migration

The migration was done correctly. Both server and client use the same `tmp: [u8; 4096]` + `Vec<u8>` pattern. The `buf.drain(..consumed)` drain is correct. The one concern (M1) is that there is no cap on buffer growth for adversarial streams.

The original BufReader approach would have had the same problem (BufReader's internal buffer also grows). The manual approach is slightly more control but does not add a safety bound.

### 2. `copy_bidirectional` proxy

The proxy pattern is the right answer for making a `!Split` TLS stream usable with separate reader/writer tasks. The `copy_bidirectional` call is correct. The issues are:
- **H1**: The proxy task JoinHandle is not stored and cannot be aborted on drop.
- Performance: Double-copy overhead (acceptable for SCAMP workloads).
- The pattern works and has no correctness bugs beyond H1.

### 3. `server_reply.rs` extraction

Clean decomposition. The coupling between `server_connection.rs` and `server_reply.rs` is minimal: `server_reply` uses `OutgoingReplyState` and `ServerWriter` from `server_connection`, which is appropriate. The `pub(crate)` visibility scoping is correct. No issues.

### 4. Test extraction pattern

The `#[cfg(test)] mod server_connection_tests;` pattern is idiomatic and correct. The test files have access to what they need via `pub(crate)` visibility. The `test_helpers` module gating on `#[cfg(test)]` is correct for unit tests but creates the minor S2 issue for future integration test reuse.

### 5. E2E test architecture

**Strengths:**
- RSA 2048 key generation per-test is correct but slow (~200ms per test on a modern machine). Consider pre-generating once with `once_cell::sync::Lazy`.
- `NamedTempFile` lifetimes are correctly managed by returning them as part of the tuple — the files live as long as the test.
- `multi_thread` flavor with `worker_threads = 2` is appropriate for testing TLS + async I/O.
- The `tokio::time::sleep(50ms)` for service readiness is the main fragility point (L7).
- No race conditions in the test structure itself.
- The `drop(client)` before `shutdown_tx.send(true)` pattern correctly ensures the connection is closed before shutdown drain completes.

### 6. `error_data` on `ScampReply`

The three-method API (`ok`, `error`, `error_with_data`) is pragmatic. A builder pattern would be more flexible but is over-engineering for the current use cases. The `Option<serde_json::Value>` type is appropriate — it handles both structured and unstructured metadata without needing a dedicated type. The `skip_serializing_if = "Option::is_none"` on the wire format is correct. No issues.

### 7. General

- No unsafe code except `bus_info.rs` `getifaddrs` (correctly wrapped, freeing `ifaddrs` in all paths).
- No deadlocks identified: all lock acquisitions are brief and never nested.
- No obvious resource leaks beyond H1 (proxy task).
- Async patterns are correct: no blocking operations in async context, no `std::thread::sleep`, no `Mutex<T>` from `std` in async context (all uses are `tokio::sync::Mutex`).
- Error propagation via `anyhow::Result<T>` is consistent and appropriate.
