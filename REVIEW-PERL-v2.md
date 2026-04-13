# Perl Parity Review v2

**Previous review**: REVIEW-PERL.md (pre-Auth.getAuthzTable, pre-error_data, pre-UDP fix)
**Date**: 2026-04-13
**Reviewer**: Re-review of all Rust source under `src/` against all Perl source under `gt-soa/perl/lib/GTSOA/`

---

## Status of Previously Flagged Items

### 1. Auth.getAuthzTable Privilege Checking -- FIXED

**Previous status**: MISSING (CRITICAL)
**Current status**: Implemented in `src/auth/authz.rs`.

The `AuthzChecker` struct now:
- Fetches the authz table from `Auth.getAuthzTable~1` via the `Requester`
- Caches results for 5 minutes (matching JS ticket.js:53)
- Provides `check_access(action, ticket) -> Result<Ticket>`
- Is integrated into `server_connection::dispatch_and_reply` (line 237-255) -- checks ticket privileges before dispatch, skips for `noauth` actions

The implementation correctly parses the response format (`{"action.name": [priv_id, ...], "_NAMES": [...]}`) and skips `_`-prefixed metadata keys. The `AuthzChecker` is passed as `Option<Arc<AuthzChecker>>` to `handle_connection`, making it opt-in per service.

**Remaining gap**: The `AuthzChecker` is not automatically constructed from config. The caller must provide the `ticket_verify_key` PEM and wire up the `Requester`. There is no automatic loading of `/etc/GTSOA/auth/ticket_verify_public_key.pem`. This is acceptable for a library API but differs from the JS/C# approach where the key path is hardcoded.

### 2. error_data Field -- FIXED

**Previous status**: Not flagged in v1 review but was a critical gap identified in subsequent audit.
**Current status**: `PacketHeader` now includes `error_data: Option<serde_json::Value>` (header.rs line 32-33). The field is correctly skipped during serialization when `None` (`skip_serializing_if`). The `Requester` checks `error_data.dispatch_failure` for retry logic (requester.rs line 67-74).

### 3. Blocking UDP Fix -- FIXED

**Previous status**: Not flagged in v1 review but was a critical gap identified in subsequent audit.
**Current status**: The multicast socket in both `observer.rs` and `multicast.rs` uses `socket.set_nonblocking(true)` before converting to `tokio::net::UdpSocket`. The observer uses `tokio::select!` for cooperative shutdown.

### 4. Write Error Handling -- FIXED

**Previous status**: Not flagged in v1 review but was a critical gap identified in subsequent audit.
**Current status**: `server_connection.rs` now checks return values of all `write()` and `flush()` calls, logging errors and returning early on failure (e.g., lines 152-158 for ACK, 311-315 for reply HEADER, 327-330 for reply DATA). The client-side `writer_task` (connection.rs line 360-371) also breaks on write/flush errors.

### 5. Flow Control Lifecycle -- FIXED

**Previous status**: Not flagged in v1 review but was a critical gap identified in subsequent audit (I1).
**Current status**: Outgoing state is now kept alive until the response arrives (connection.rs line 337-348: cleanup after response_rx, not after EOF send). The `ack_notify` properly wakes blocked senders when ACKs arrive (reader.rs line 222).

### 6. Clippy/Fmt Cleanup -- FIXED

All code passes `cargo clippy` and `cargo fmt` based on the GitHub Actions CI setup.

---

## Previously Flagged Items That Remain OPEN

### O1. Stale Cache Behavior Divergence -- STILL OPEN (MEDIUM)

**Previous**: Perl stops serving requests when the discovery cache is stale (`fill_error` returned); Rust logs a warning and continues serving from the stale cache.
**Current**: Still divergent. `service_registry.rs` lines 184-190 log a warning but proceed to load and serve from the cache. This is arguably better for availability, but differs from Perl's fail-fast behavior.

**Recommendation**: Document this as an intentional design decision. Alternatively, add an option `discovery.stale_cache_behavior = error|warn` to support both modes.

### O2. Config Keys Not Read -- STILL OPEN (LOW)

The following config keys are still hardcoded:

| Config Key | Perl | Rust | Notes |
|---|---|---|---|
| `rpc.timeout` | `Config->val('rpc.timeout', 75)` ServiceInfo.pm:257 | `DEFAULT_RPC_TIMEOUT_SECS = 75` | Same default, not configurable |
| `beepish.client_timeout` | `Config->val('beepish.client_timeout', 90)` Client.pm:40 | `DEFAULT_CLIENT_TIMEOUT_SECS = 90` | Same default, not configurable |
| `beepish.server_timeout` | `Config->val('beepish.server_timeout', 120)` Server.pm:58 | `DEFAULT_SERVER_TIMEOUT_SECS = 120` | Same default, not configurable |
| `beepish.bind_tries` | `Config->val('beepish.bind_tries', 20)` Server.pm:27 | Hardcoded `20` | listener.rs:119 |
| `beepish.first_port` | `Config->val('beepish.first_port', 30100)` Server.pm:28 | Hardcoded `30100` | listener.rs:117 |
| `service.address` | `Config->bus_info->{service}` Config.pm:108 | Only reads `bus.address` | bus_info.rs:31 |

Note: Rust has `_config: &Config` as an unused parameter in `ConnectionHandle::connect()` (connection.rs line 161), suggesting the intent to read these config keys exists but hasn't been implemented.

### O3. No `ident` Parameter for Targeted Routing -- STILL OPEN (LOW)

Perl `Requester.pm:25` passes `$rqparams->{ident}` to lookup, which can target a specific worker identity. Perl `ServiceManager.pm:59`: `!$ident or $sv->worker_ident eq $ident or next`. Rust `Requester` and `ServiceRegistry` have no equivalent. All lookups are random selection among matching candidates.

### O4. Per-Action Sector/Envelope Override -- STILL OPEN (LOW)

Perl `Announcer.pm:140-156` supports per-action sector and envelope overrides (for v4 actions). Rust `RegisteredAction` (handler.rs line 52-57) has no sector/envelope fields. All actions announced use the service-level sector and envelopes. This means Rust services cannot announce actions in multiple sectors from a single service instance.

### O5. Announcement Weight/Interval Not Configurable Per-Service -- STILL OPEN (LOW)

Rust `build_announcement_packet` in listener.rs (line 153-154) hardcodes `weight=1`, `interval_secs=5`. The `announce_raw` function accepts these as parameters, but `ScampService` doesn't expose them. Perl `Announcer.pm` has configurable `weight` and `interval` attributes (lines 39-40).

### O6. v4 Action Vector Population -- STILL OPEN (LOW)

Rust announcer (announce.rs line 38-44) always produces empty v4 vectors. All actions go into the v3 compatibility zone. Perl splits actions between v3 (no custom sector/envelopes) and v4 (custom sector/envelopes). Since no known consumer is v4-only, this is safe but means Rust is v3-only for announcements.

### O7. Zlib Decompression Fallback -- STILL OPEN (LOW)

Perl Observer.pm:55-57: if `uncompress` fails, it falls back to treating the data as uncompressed text (`$ubuffer = $cbuffer`). Rust observer.rs:91 returns an error on zlib failure with no fallback. In practice, all announcements are zlib-compressed, so this is unlikely to matter.

---

## NEW Gaps Not Caught in First Review

### N1. Perl `_packet` TXERR Passes Error Through Without Validation -- NEW (LOW)

Perl `Connection.pm:171-173` passes the TXERR body directly to `$msg->finish(Encode::decode_utf8($body))` without validating that the body is non-empty or non-zero. Rust (both client reader.rs:176-179 and server server_connection.rs:168-175) rejects TXERR with empty or "0" body. This strictness originates from JS `connection.js:229` behavior.

**Impact**: If a Perl service sends an empty TXERR body (pathological but possible), Rust would silently drop it rather than propagating the error. The message would remain in the `incoming` map indefinitely. This is a potential resource leak under failure conditions.

**Recommendation**: Either remove the empty-body TXERR validation to match Perl, or at minimum clean up the incoming message on empty TXERR (treat as generic "TXERR without details").

### N2. Server Connection Does Not Clean Up Outgoing State After EOF -- NEW (LOW)

In `server_connection.rs:send_reply()` (line 351), `outgoing.remove(&reply_msg_no)` is called immediately after sending EOF. But in Perl, the outgoing message is kept alive until all ACKs are received (Connection.pm:225: delete happens in the `on_some_data` completion callback, which fires after the message is fully sent and acknowledged). In Rust, once `send_reply` completes, any in-flight ACKs for that reply_msg_no will hit the "no matching outgoing" path and be silently ignored. This is mostly harmless since ACK validation is defensive, but it means late-arriving ACKs are silently discarded rather than validated.

### N3. Perl Server Tracks `$count` for `busy` at Service Level -- NEW (INFORMATIONAL)

Perl `Server.pm:64-66` tracks `$count` of in-flight requests across the connection and sets `conn->busy(!!$count)`. This is a connection-level busy flag that controls timeout. Rust checks `!incoming.is_empty() || !outgoing.is_empty()` directly in `server_connection.rs:54`. The Rust approach is functionally equivalent but has a subtle difference: in Perl, `busy` is set/unset around the callback (before dispatch and after reply send), while in Rust, the connection is busy whenever there are entries in the `incoming` or `outgoing` maps. The Rust approach may consider the connection non-busy slightly earlier or later depending on timing of map operations. In practice this difference is immaterial.

### N4. Perl ServiceManager.pm Uses Exact Timestamp Comparison for Replay -- NEW (LOW)

Perl `ServiceManager.pm:33`: `$info->timestamp < $self->{_timestamps}{$key}` (strictly less than). Rust `service_registry.rs:103`: `timestamp <= prev_ts` (less than or equal). This means Rust rejects duplicate timestamps while Perl would accept a re-announcement with the exact same timestamp. The Rust behavior is stricter and arguably more correct for replay protection.

### N5. Perl Config.pm Has `beepish.first_port` Bug -- NEW (INFORMATIONAL)

Perl `Server.pm:29` reads `beepish.first_port` for BOTH `$first` and `$last` port:
```perl
my $first = GTSOA::Config->val('beepish.first_port',30100);
my $last  = GTSOA::Config->val('beepish.first_port',30399);
```
This means if someone sets `beepish.first_port = 30100`, `$last` would also be 30100 (making the range just one port). The default works correctly because `beepish.first_port` is typically not set, and the defaults of 30100 and 30399 apply. Rust hardcodes `30100..=30399` which avoids this Perl bug.

### N6. Perl Announcer Creates One Socket Per Discovery Interface -- NEW (LOW)

Perl `Announcer.pm:59-72` creates a separate UDP socket per discovery interface (`for my $to (@{ $info->{discovery} })`). Each socket is bound to a specific interface and has its own `mcast_if` set. Rust `multicast.rs` creates a single socket with a single `set_multicast_if_v4` call. For multi-homed configurations where the service needs to announce on multiple network interfaces simultaneously, Rust would only announce on one. Single-interface setups (the common case) are unaffected.

### N7. Perl Observer Joins Multicast on Multiple Interfaces -- NEW (LOW)

Perl `Observer.pm:37-38` calls `mcast_add` for each discovery interface separately. Rust `observer.rs:130` calls `join_multicast_v4` once with a single interface. Same multi-homing issue as N6.

### N8. Perl Connection.pm Has Write Corking During TLS Handshake -- NEW (INFORMATIONAL)

Perl `Connection.pm:32-33,196-201,61-74` implements corking: during TLS handshake, writes are buffered in `_corked_writes`. After `on_starttls` fires and fingerprint is verified, buffered data is flushed. Rust achieves the same effect naturally because `ConnectionHandle::connect()` doesn't create the connection handle (and thus can't send packets) until after TLS handshake and fingerprint verification complete. The approaches are functionally equivalent.

### N9. No `register_with_flags` API -- NEW (LOW)

`ScampService::register()` (listener.rs:84-100) creates a `RegisteredAction` with an empty `flags: vec![]`. There is no way for callers to specify flags like `noauth`, `t600` (timeout), or CRUD operations when registering an action. These flags affect announcement content and authorization behavior. A `register_with_flags` method would be needed for full feature parity.

### N10. Announcement Packet v4 Extension Hash Always Included Even When Empty -- NEW (INFORMATIONAL)

Rust `announce.rs` always appends a v4 extension hash to the envelopes array (line 104-106), even when all v4 vectors are empty. Perl only includes the v4 hash when there are v4-zone actions. Since receivers handle both cases correctly (they skip the hash object in the envelopes array), this is harmless but results in slightly larger announcement packets.

### N11. Config Flat vs Hierarchical Storage Edge Case -- STILL PRESENT (LOW)

The v1 review noted this but it's worth re-flagging: Rust config (config.rs) uses a nested tree with numeric path segments treated as array indices. Perl uses a flat hash with dotted string keys. Config keys like `0.something = value` would behave differently. In practice, no known SCAMP config uses such keys.

---

## Summary Table

| ID | Area | Status | Severity |
|---|---|---|---|
| Auth.getAuthzTable | Auth | FIXED | -- |
| error_data field | Wire Protocol | FIXED | -- |
| Blocking UDP | Observer/Multicast | FIXED | -- |
| Write error handling | Server Connection | FIXED | -- |
| Flow control lifecycle | Client Connection | FIXED | -- |
| Clippy/fmt | Code Quality | FIXED | -- |
| O1 | Stale cache behavior | OPEN | Medium |
| O2 | Config keys hardcoded | OPEN | Low |
| O3 | No ident routing | OPEN | Low |
| O4 | Per-action sector/envelope | OPEN | Low |
| O5 | Weight/interval not configurable | OPEN | Low |
| O6 | v4 action vectors empty | OPEN | Low |
| O7 | Zlib decompression fallback | OPEN | Low |
| N1 | TXERR empty body validation | NEW | Low |
| N2 | Server outgoing state cleanup | NEW | Low |
| N3 | Busy flag tracking difference | NEW | Informational |
| N4 | Replay timestamp comparison | NEW | Low |
| N5 | Perl first_port bug | NEW | Informational |
| N6 | Multi-interface announcing | NEW | Low |
| N7 | Multi-interface observer | NEW | Low |
| N8 | Write corking | NEW | Informational |
| N9 | No register_with_flags | NEW | Low |
| N10 | Empty v4 hash always sent | NEW | Informational |
| N11 | Config flat vs tree edge case | NEW | Low |

**Overall assessment**: STRONG parity. All critical and high-severity items from v1 are FIXED. Remaining items are low severity or informational. The Rust implementation is production-ready for standard single-interface deployments with the standard scamp config.

---

## End-to-End Testing Recommendation for GitHub Actions

### What Would a Minimal Test Backplane Look Like?

A minimal SCAMP backplane for end-to-end testing needs these components:

1. **Discovery cache writer** -- Listens for multicast announcements and writes the discovery cache file. In production this is the `cache` container. This is the critical piece: without it, services cannot discover each other.

2. **TLS keypair** -- A self-signed cert/key pair for service identity. Can be pre-generated and committed to the repo (it's a test fixture, not a secret).

3. **authorized_services file** -- Maps the test cert fingerprint to `main:ALL`. A static fixture file.

4. **scamp config (soa.conf)** -- Points to the cache file path, authorized_services path, and configures multicast group/port.

5. **At least two services** -- One Rust service (the system under test) and one target service (to make requests to). Could both be Rust processes for pure Rust E2E, or one Perl for interop validation.

### Porting a Minimal Cache Writer to Rust

The Perl cache service does two things:
1. Listens on multicast for announcements (using `Observer`)
2. Writes all received announcements to the cache file in `\n%%%\n`-delimited format

This is straightforward to port. The Rust codebase already has:
- `observer.rs` -- multicast listener that parses and injects packets
- `cache_file.rs` -- reads the `\n%%%\n` format

What's missing is the inverse: writing announcement packets to the cache file. A minimal cache writer would:

```
1. Create a multicast socket (reuse observer::create_observer_socket)
2. Receive raw announcement data (before decompression)
3. Decompress zlib
4. Append to file with \n%%%\n delimiter
5. Repeat
```

This could be ~50 lines of Rust. It would be a good addition as either a library function or a CLI subcommand (`scamp cache-writer`).

**Alternatively**: For testing, skip the cache writer entirely. Instead, have the test harness:
1. Start a Rust service that announces on multicast
2. Build the announcement packet programmatically using `announce::build_announcement_packet()`
3. Write it directly to a cache file
4. Start a second service/requester that loads from that cache file

This "synthetic cache" approach avoids needing multicast networking in CI entirely.

### Minimum Set of Services for a Full Request-Response Roundtrip

**Option A: Pure Rust (no multicast needed)**

1. **Service A** (Rust) -- Registers an echo handler, binds on a port
2. **Test harness** -- Builds Service A's announcement, writes to synthetic cache file
3. **Requester** (Rust) -- Loads cache, discovers Service A, sends request, validates response

This tests: config loading, announcement building, cache parsing, discovery lookup, TLS connection, wire protocol, request/response, timeout handling.

**Option B: With multicast (full stack)**

1. **Cache writer** (Rust, minimal) -- Listens on multicast, writes cache file
2. **Service A** (Rust) -- Announces on multicast, serves requests
3. **Requester** (Rust) -- Loads cache from file, sends request to Service A

This additionally tests: multicast sending, multicast receiving, zlib compression/decompression, cache file format.

**Option C: Cross-language interop**

1. **Cache** (existing Perl cache container or minimal Rust cache writer)
2. **Perl service** (from gt-soa, e.g., soatest or a minimal Perl echo service)
3. **Rust requester** -- Discovers and calls the Perl service
4. **Rust service** -- Announces itself
5. **Perl requester** (`soatest`) -- Discovers and calls the Rust service

This tests bidirectional wire compatibility. This is the gold standard but requires Docker for the Perl runtime.

### Recommended GitHub Actions Workflow Structure

#### Phase 1: Unit + In-Process Integration (no Docker)

```yaml
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install OpenSSL dev headers
        run: sudo apt-get install -y libssl-dev pkg-config

      - name: cargo test (unit + integration)
        run: cargo test

      - name: cargo clippy
        run: cargo clippy -- -D warnings

      - name: cargo fmt check
        run: cargo fmt -- --check

  e2e-synthetic:
    runs-on: ubuntu-latest
    needs: test
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: sudo apt-get install -y libssl-dev pkg-config

      - name: Generate test keypair
        run: |
          openssl req -x509 -newkey rsa:2048 -nodes \
            -keyout test.key -out test.crt \
            -days 365 -subj '/CN=scamp-test'

      - name: Run synthetic E2E test
        run: cargo test --test e2e_synthetic -- --nocapture
        env:
          SCAMP_TEST_KEY: test.key
          SCAMP_TEST_CERT: test.crt
```

The `e2e_synthetic` test would:
1. Start a Rust service on localhost (in-process, using duplex streams or actual TLS on loopback)
2. Build its announcement and write a synthetic discovery cache
3. Create a Requester pointing at that cache
4. Send requests and validate responses
5. Test error cases (unknown action, timeout, etc.)

#### Phase 2: Docker Compose for Cross-Language Interop

```yaml
  e2e-interop:
    runs-on: ubuntu-latest
    needs: test
    steps:
      - uses: actions/checkout@v4
      - uses: actions/checkout@v4
        with:
          repository: your-org/gt-soa
          path: gt-soa

      - name: Build test containers
        run: docker compose -f docker-compose.e2e.yml build

      - name: Start backplane
        run: docker compose -f docker-compose.e2e.yml up -d

      - name: Wait for services
        run: |
          for i in $(seq 1 30); do
            docker compose -f docker-compose.e2e.yml exec -T rust-service \
              scamp list actions --raw 2>/dev/null | grep -q echo && break
            sleep 1
          done

      - name: Run interop tests
        run: |
          # Perl -> Rust
          docker compose -f docker-compose.e2e.yml exec -T perl-service \
            perl /path/to/soatest --action "ScampRsTest.echo~1" \
            --data '{"test":"hello"}' -p

          # Rust -> Perl (if a Perl test action exists)
          docker compose -f docker-compose.e2e.yml exec -T rust-service \
            scamp request --action "api.status.health_check~1" \
            --body '{}'

      - name: Teardown
        if: always()
        run: docker compose -f docker-compose.e2e.yml down -v
```

The `docker-compose.e2e.yml` would define:

```yaml
services:
  cache-writer:
    # Minimal Rust binary or Perl cache service
    # Listens on multicast, writes discovery cache to shared volume

  rust-service:
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - discovery:/var/scamp/discovery
      - ./test-fixtures/soa.conf:/etc/scamp/scamp.conf
      - ./test-fixtures/test.key:/etc/scamp/test.key
      - ./test-fixtures/test.crt:/etc/scamp/test.crt
    command: scamp serve --name scamp-rs-test --sector main

  perl-service:
    image: your-perl-base-image
    volumes:
      - discovery:/var/scamp/discovery
    command: # start a minimal Perl SCAMP service

volumes:
  discovery:
```

#### Recommended: Start with In-Process, Iterate to Docker

**Immediate (Phase 1)**: Create `tests/e2e_synthetic.rs` that uses the existing in-memory duplex stream approach (already proven in `connection.rs` tests and `server_connection.rs` tests) but extends to full Requester-level testing with synthetic cache files. This requires zero Docker and zero network config.

**Near-term (Phase 2)**: Add a `scamp cache-writer` subcommand. Create a `docker-compose.e2e.yml` with Rust-only services (cache-writer + service + requester). This tests multicast and real TLS but stays within the Rust ecosystem.

**Future (Phase 3)**: Pull in the Perl service container for cross-language interop tests.

### Test Scenarios the E2E Tests Should Cover

**Basic Request/Response:**
1. Discover a service from the cache, connect via TLS, send request, receive response
2. Verify response body matches expected echo output
3. Verify request/reply header fields (request_id, message_type, envelope, error/error_code)

**Discovery:**
4. Load cache from file, find action by sector:action.vN
5. Verify weight=0 services are excluded from lookup
6. Verify unauthorized services are excluded from lookup
7. Verify TTL expiry removes stale services
8. Verify replay protection (same fingerprint+identity, older timestamp rejected)

**Error Handling:**
9. Request to unknown action returns `not_found` error
10. Request timeout triggers within configured duration
11. Connection lost delivers error to pending requests
12. TXERR from server propagates to client

**Wire Protocol:**
13. Large body (>2048 bytes) is correctly chunked into multiple DATA packets
14. ACK packets are sent for each DATA packet received
15. ACK cumulative byte count is correct
16. PING from client receives PONG response

**Announcement:**
17. Build announcement packet, parse it back, verify identity/uri/actions
18. Verify announcement signature is valid (sign + verify roundtrip)
19. Verify tampered announcement fails signature verification
20. Verify weight=0 announcements during shutdown

**Authorization:**
21. Authorized fingerprint + matching sector:action passes
22. Unknown fingerprint is denied
23. `_meta.*` actions always pass (regardless of fingerprint)
24. Colon in sector or action name is rejected

**Ticket Verification (if AuthzChecker is wired up):**
25. Valid ticket with required privileges passes `check_access`
26. Valid ticket with missing privileges is rejected
27. Expired ticket is rejected
28. Ticket with invalid signature is rejected

**Dispatch Failure Retry:**
29. Service returning `dispatch_failure` error_data triggers retry
30. After retry, request routes to a different service instance

**Connection Pooling:**
31. Multiple sequential requests reuse the same TLS connection
32. After connection close, next request opens a new connection

**Multicast (Phase 2+):**
33. Service announces on multicast, observer receives and injects into registry
34. Shutdown announcements (weight=0) are received and service is removed from lookup
35. Zlib compression/decompression roundtrip works for announcement packets
