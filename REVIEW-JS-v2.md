# JavaScript Parity Review v2

Previous review: `REVIEW-JS.md` (pre-fix baseline).
This review re-evaluates all items after targeted fixes and identifies new gaps.

Reviewed against: `scamp-js/lib/` and `gt-soa/js/lib/` (functionally identical minus debug logging and BigQuery destination).

---

## Status of Previously Flagged Issues

### 1. `error_data` header field -- FIXED

**Previous**: Rust `PacketHeader` had no `error_data` field. Could not send or receive structured error metadata.

**Now**: `PacketHeader` has `error_data: Option<serde_json::Value>` (header.rs:31-33), with proper serde annotations (`skip_serializing_if = "Option::is_none"`). The field is populated in `send_request` construction (connection.rs:237) and accessible on received headers. Wire-compatible with JS `rpy.header.error_data`.

### 2. `dispatch_failure` detection -- FIXED

**Previous**: Rust checked `error_code == "dispatch_failure"` (a string field). JS checks `error_data.dispatch_failure` (a nested boolean in an object).

**Now**: `requester.rs:67-74` correctly checks `error_data.dispatch_failure` first (via `serde_json::Value` traversal), with a fallback to `error_code == "dispatch_failure"` for backward compat with Perl. This matches JS `requester.js:50` behavior and additionally handles legacy error_code-based signaling.

### 3. Write error handling -- FIXED

**Previous**: Writer task did not handle errors; packets could be silently dropped.

**Now**: `connection.rs:361-371` writer_task breaks on write error and logs it. `server_connection.rs:152-158` handles ACK write failures, `server_connection.rs:311-348` handles reply HEADER/DATA/EOF write failures with early return. All write paths now check for errors.

### 4. Flow control lifecycle -- FIXED

**Previous**: Flow control outgoing state could leak if the connection closed mid-request.

**Now**: `connection.rs:265-268, 278-281, 306-309, 331-334` -- all error paths (header send failure, flow control wait during close, data send failure, EOF send failure) clean up both `pending` and `outgoing` maps. The flow control wait loop at line 277 checks the `closed` flag to break out of a blocked flow control wait. Outgoing state is removed after response receipt at line 347.

### 5. Blocking UDP -- FIXED (by design)

Rust uses `tokio::net::UdpSocket::from_std` with `set_nonblocking(true)` on the socket2 socket (multicast.rs:87, observer.rs:131). All send/recv operations are async. The observer and announcer both use `tokio::select!` with shutdown receivers for clean termination.

---

## Remaining OPEN Issues from Previous Review

### 6. Heartbeat initiation -- OPEN

**JS**: `connection.js:152-167 setHeartbeat(msecs)` starts an interval timer that sends PING and expects PONG before the next interval. If PONG not received, connection is errored. `client.js:146` exposes `setHeartbeat()` to callers (used by subscriptions at 15000ms).

**Rust**: PING handler responds with PONG correctly (reader.rs:225-232 client, server_connection.rs:202-217 server). PONG handler is a no-op comment: `"would reset timer (not implemented)"` (reader.rs:234-236).

**Impact**: Rust cannot detect hung connections via heartbeat. Only affects long-lived connections (subscriptions, idle pooled connections).

### 7. Connection eviction on failure -- OPEN

**JS**: `connectionMgr.js:42-47` listens for `lost` event and proactively deletes dead connections from cache. `client.js:69-78` emits `lost` on proto `lost` and notifies all pending requests with `dispatch_failure`.

**Rust**: `connection.rs:77-81` checks `closed` flag lazily on next `get_connection()` call and removes stale entries. No proactive eviction mechanism. Combined with no heartbeat, a dead connection lingers until the next request to that service.

**Impact**: Slightly delayed detection of dead connections. In practice, next request will detect and reconnect.

### 8. Client-side `setBusy` flag -- OPEN

**JS**: `connection.js:311-320` tracks busy state (pending incoming/outgoing messages) and disables idle timeout when busy. `client.js:141-143 adjTimeout()` called on every request start/complete.

**Rust**: Server side has busy tracking (server_connection.rs:54 `is_busy` check before idle timeout). Client side (`connection.rs`) has no busy tracking or idle timeout at all -- the connection lives until the reader exits or `Drop` is called.

**Impact**: Low. Rust client connections don't have idle timeout currently, so busy tracking would be a no-op.

### 9. Stream backpressure (pause/resume on ACK) -- OPEN

**JS**: `connection.js:198-211` DATA handler calls `msg.write(payload)` which returns false when the consumer is paused. ACK is deferred until the `drain` event fires. This provides true end-to-end backpressure.

**Rust**: `reader.rs:146-152` sends ACK immediately on every DATA packet receipt, regardless of consumer state. The body is accumulated into a `Vec<u8>` in memory. No pause/resume semantics.

**Impact**: For typical RPC (small bodies, fast handlers), this is irrelevant. Would matter for very large streaming responses with slow consumers, which SCAMP services rarely produce.

### 10. Request timeout triggers connection eviction -- OPEN

**JS**: `client.js:127-130` on timeout, emits `lost` which causes connectionMgr to evict the connection from the pool and open a new one for subsequent requests.

**Rust**: `connection.rs:342-344` returns timeout error without any pool eviction. The timed-out connection stays in the pool.

**Impact**: After a timeout, subsequent requests to the same service reuse the potentially-stuck connection rather than opening a fresh one.

### 11. Shutdown coordination (sequential vs concurrent) -- OPEN

**JS** (`service.js:78-91`): Sequential: (1) `announcer.suspend()` sends weight=0 at 200ms intervals x3, (2) wait 5 seconds, (3) wait for `_activeRequests === 0`, (4) wait 1 more second for replies to drain.

**Rust** (`serve.rs:110-163`, `multicast.rs:141-157`, `listener.rs:169-228`): Announcer and listener receive the same `watch::Receiver<bool>` shutdown signal simultaneously. Announcer sends weight=0 for 10 rounds at 1s intervals (Perl convention). Listener stops accepting and drains connections for 30s. These run concurrently, not sequentially.

**Impact**: Peers may continue routing to this service while it's already shutting down. In practice, the 10x1s announcer rounds provide a long enough window, but the sequencing differs from JS.

### 12. Subscription/Event system -- OPEN (by design)

JS has `subscribe`/`subscribe_done`/`event`/`notify` message types, `ServerConnection` subscription management, `Subscription` class with reconnection and heartbeat.

Rust has none of this. This is a deliberate scope limitation -- subscriptions are not needed for basic RPC services.

### 13. `forwardRequest` action aliasing -- OPEN

**JS** `requester.js:63-72`: `forwardRequest()` resolves the action via `findAction()`, then overwrites `request.header.action` and `request.header.version` with the resolved info's values before sending. This handles CRUD alias resolution at the wire level.

**Rust** `requester.rs:91-133`: `dispatch_once()` looks up the action entry but sends `opts.action` (the original caller-supplied name) directly as the header action. The resolved entry's action path is not propagated to the wire header.

**Impact**: If a caller requests `Namespace._create.v1`, Rust will send `_create` as the action name on the wire, while JS would resolve it to the actual action name (e.g., `Namespace.create`). This could cause dispatch failures on the receiving service.

### 14. `circularClient` -- OPEN (by design)

Not implemented. This is a P2P discovery mechanism used for multi-datacenter announcement distribution. Not needed for single-cluster deployments.

---

## NEW Gaps Found in This Review

### N1. JS announcement timestamp is milliseconds; Rust TTL check uses seconds

**JS** `announce.js:206`: `Date.now()` produces milliseconds-since-epoch in the timestamp field (position [8] of the JSON array).

**Perl** Announcer uses `Time::HiRes::time()` which is seconds with fractional part.

**Rust** `announce.rs:82-85`: Uses `SystemTime::now().duration_since(UNIX_EPOCH).as_secs_f64()` -- fractional seconds, matching Perl.

**Rust TTL check** `service_registry.rs:95`: `now_f > body.params.timestamp + interval_secs * 2.1` -- `now_f` is in seconds. If a JS service sends `Date.now()` (milliseconds), the timestamp will be ~1000x larger than expected, and the TTL check will always fail (the announcement would appear to be from far in the future and never expire, but also never be considered stale).

**Impact**: Announcements from JS services will not be correctly TTL-checked. Since JS sends milliseconds and Rust expects seconds, `timestamp + interval_secs * 2.1` will be enormous, so announcements will be accepted (they won't be considered expired), but dedup timestamp comparison (`timestamp <= prev_ts`) will also behave differently.

**Recommendation**: The Rust TTL check should detect millisecond timestamps (e.g., value > 1e12) and convert to seconds, or simply match whichever convention the announcing implementation uses. In practice, most deployments use Perl services, so this may not matter.

### N2. JS `checkStale()` returns an error string; Rust has no equivalent

**JS** `serviceMgr.js:231`: `checkStale()` returns the stale error string (or `false`). `requester.js:66-67`: If the cache is stale, returns a specific error before attempting dispatch.

**Rust** `service_registry.rs:177-191`: Logs a warning when the cache is stale but still loads it. No mechanism to surface staleness to the Requester at request time.

**Impact**: Rust will silently use a stale cache. JS returns an explicit error. In production, the cache daemon keeps the cache fresh, so this rarely matters.

### N3. Server-side `error_data` not populated for dispatch failures

**JS** `client.js:72-74`: When a connection cannot be established (before TLS completes), the `lost` handler creates a reply with `{ error_data: { dispatch_failure: true } }`. This is how JS signals dispatch failure.

**Rust** `server_connection.rs:268-275`: When an action is not found, sends `error: "No such action"`, `error_code: "not_found"`, but `error_data: None`. There is no code path in Rust that sets `error_data: { dispatch_failure: true }`.

**Impact**: If a JS requester talks to a Rust service that can't handle an action, the JS requester won't see `dispatch_failure` and won't retry. The Rust requester handles this correctly for both patterns (checks error_data first, falls back to error_code). For full interop, Rust server should set `error_data: {"dispatch_failure": true}` when appropriate.

### N4. JS sends DATA in 131072-byte chunks; Rust sends 2048-byte chunks

**JS** `connection.js:291-295`: `sendMessage` DATA handler slices at `131072` (MAX_PACKET_SIZE).

**Rust**: `DATA_CHUNK_SIZE = 2048` (proto/mod.rs:17), matching Perl.

This was noted in the previous review as a "minor behavioral difference." It remains wire-compatible since receivers accept up to MAX_PACKET_SIZE. However, it means Rust services generate ~64x more packets for the same data volume, increasing ACK traffic and protocol overhead.

### N5. Announcement v4 actions not populated during announce building

**Rust** `announce.rs:39-45`: The v4 vectors (`v4_acns`, `v4_acname`, `v4_acver`, etc.) are initialized as empty Vecs and never populated. Only v3 actions are built (lines 46-73). The v4 extension hash is included but always contains empty arrays.

**JS** `announce.js:196-206`: Only builds v3-format announcements (classes array). The v4 extension hash is passed through from `params.extend` (if any).

**Impact**: Both JS and Rust only produce v3 action entries in announcements. The v4 extension hash is included but empty in both cases. The Perl implementation is the one that produces v4 actions. This is consistent -- both JS and Rust implementations only need to *parse* v4 (which they do correctly), not *produce* it. No gap here.

### N6. Rust `ScampReply` lacks `error_data` field

**Rust** `handler.rs:20-24`: `ScampReply` has `error`, `error_code`, but no `error_data`.

**Rust** `server_connection.rs:293`: Reply header always sets `error_data: None`.

If a handler needs to signal dispatch_failure or other structured error metadata, there's no way to do so through the handler API. The `error_data` field on `PacketHeader` exists for *receiving* but the handler reply path cannot *send* it.

### N7. JS `Date.now()` timestamp units in announcements

**JS** `announce.js:206`: Uses `Date.now()` (milliseconds) for the timestamp field in the announcement JSON array.

**Perl**: Uses `Time::HiRes::time()` (fractional seconds).

**Rust** `announce.rs:82-85` and `service_registry.rs:95`: Uses fractional seconds.

This means JS-originated announcements have timestamps ~1000x larger than Perl/Rust-originated ones. The Rust parser and registry handle this because the timestamp field is parsed as `f64` and the TTL check (`now_f > timestamp + interval * 2.1`) would consider JS timestamps as being in the far future (not expired). But the replay protection comparison (`timestamp <= prev_ts`) would be confused if a service alternates between JS and Rust announcement sources.

Note: This is the same underlying issue as N1, listed here for completeness of the announcement format analysis.

### N8. `_activeRequests` counting granularity

**JS** `service.js:237-254`: Counts individual requests (`_activeRequests++`/`--`). Shutdown waits for `_activeRequests === 0`.

**Rust** `listener.rs:177,188,200`: Counts TCP connections (`active_connections`). A connection with completed requests but still open TCP session still counts as "active."

**Impact**: During shutdown, Rust may wait for idle connections to close (up to 120s server timeout or 30s drain timeout), while JS exits as soon as all in-flight requests complete.

---

## Summary Table

| # | Area | Status | Severity |
|---|------|--------|----------|
| 1 | `error_data` header field | FIXED | -- |
| 2 | `dispatch_failure` detection | FIXED | -- |
| 3 | Write error handling | FIXED | -- |
| 4 | Flow control lifecycle | FIXED | -- |
| 5 | Blocking UDP | FIXED | -- |
| 6 | Heartbeat initiation | OPEN | Low |
| 7 | Connection eviction on failure | OPEN | Low |
| 8 | Client-side `setBusy` | OPEN | Negligible |
| 9 | Stream backpressure | OPEN | Low |
| 10 | Timeout eviction | OPEN | Medium |
| 11 | Shutdown coordination | OPEN | Low |
| 12 | Subscription system | OPEN (by design) | N/A |
| 13 | `forwardRequest` aliasing | OPEN | Medium |
| 14 | `circularClient` | OPEN (by design) | N/A |
| N1/N7 | JS millisecond timestamps | NEW | Low |
| N2 | Stale cache reporting | NEW | Low |
| N3 | Server `error_data` for dispatch failure | NEW | Medium |
| N6 | `ScampReply` missing `error_data` | NEW | Medium |
| N8 | Request vs connection counting | NEW | Low |

---

## E2E Testing Recommendations for GitHub Actions

### Can a Rust-only test harness replace Perl/JS services?

**Yes, for the core protocol.** The existing test suite already demonstrates this:
- `server_connection.rs` tests run full HEADER+DATA+EOF roundtrips over in-memory duplex streams
- `connection.rs` tests run client/server pairs (ConnectionHandle + server_connection::handle_connection) over tokio duplex streams
- These validate: packet framing, header JSON serde, ACK handling, flow control, PING/PONG, unknown-action errors, multi-chunk bodies, timeouts

For wire-format compliance, the Perl fixture tests (`proto/tests.rs` using `fixtures.rs`) already verify that Rust can parse packets produced by canonical Perl framing. Adding JS fixture packets (captured from a JS service) would strengthen this.

### Minimum Viable Discovery Pipeline for E2E Tests

The full pipeline: **announce -> cache -> lookup -> connect -> request -> response**.

This can be done entirely in-process without multicast or file I/O:

1. **Announce**: Build an announcement packet using `announce::build_announcement_packet()` (already tested in `announce.rs` tests)
2. **Cache/Inject**: Parse the announcement with `AnnouncementPacket::parse()`, inject into a `ServiceRegistry` via `inject_packet()`
3. **Lookup**: Call `registry.find_action_with_envelope()` to resolve action -> ServiceInfo
4. **Connect + Request**: Use `ConnectionHandle::from_stream()` over a tokio duplex stream connected to `server_connection::handle_connection()`
5. **Response**: Verify response header and body

### Recommended In-Process E2E Test

```rust
#[tokio::test]
async fn test_full_discovery_pipeline() {
    // 1. Create a service with registered actions
    // 2. Build an announcement packet (requires dev keypair or test keypair)
    // 3. Parse the announcement and inject into a ServiceRegistry
    // 4. Look up the action via the registry
    // 5. Open an in-memory duplex stream
    // 6. Spawn server_connection::handle_connection on one end
    // 7. Create a ConnectionHandle::from_stream on the other end
    // 8. send_request through the ConnectionHandle
    // 9. Assert response matches expected
}
```

**Blocker**: This test requires an RSA keypair for announcement signing and signature verification. Options:
- **Generate a test keypair at build time** (via openssl CLI in a build script or a test fixture)
- **Embed a test keypair in the repo** (a self-signed cert+key used only for testing, not for production)
- **Skip announcement signing in tests** by adding a `ServiceRegistry::inject_unsigned()` method for testing

The simplest approach is to embed a dedicated test keypair in `samples/` and use it for E2E tests. The announcement roundtrip test (`test_roundtrip_announcement`) already does this pattern but is `#[ignore]` because it requires the dev keypair.

### What interop tests specifically need Perl/JS?

1. **Wire-format edge cases from real implementations**: Verify that Rust correctly handles packets produced by actual Perl/JS services (field ordering, null handling, Unicode, ticket formats). The fixture-based tests partially cover this, but live interop catches encoding quirks.

2. **TLS interop**: Verify that Rust's TLS implementation (tokio-native-tls) successfully negotiates with Perl's OpenSSL and JS's Node.js TLS. Certificate fingerprint matching across implementations.

3. **Multicast announcement interop**: Verify that Rust can receive and parse announcements from live Perl/Go services, and that Perl/JS services can receive and parse Rust announcements. Particularly: zlib compression compatibility, announcement format edge cases.

4. **Dispatch failure retry interop**: Verify that when a JS service returns `error_data: { dispatch_failure: true }`, a Rust requester correctly retries. And vice versa (N3 blocks this direction currently).

5. **Ticket verification**: Verify that Rust correctly verifies tickets generated by the actual Auth service (Base64URL encoding nuances, key format compatibility).

6. **Authorized services file format**: Verify that Rust's authorized_services parser handles the exact format written by the deployment tooling.

### Proposed CI Test Tiers

**Tier 1 -- Rust-only (run on every PR, no containers)**:
- All existing unit tests (80 tests)
- New in-process E2E test with embedded test keypair
- Wire fixture tests for Perl/JS/Go packet formats

**Tier 2 -- Container-based interop (run on merge to main or nightly)**:
- Spin up Perl service in Docker
- Rust requester calls Perl service actions
- Perl requester calls Rust service actions
- Verify announcement exchange via multicast
- Verify ticket verification with real Auth service

Tier 1 provides >95% of the value and can run in any CI environment. Tier 2 catches the remaining interop edge cases but requires the Docker dev environment.
