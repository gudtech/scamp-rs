# Code Elegance Review v2

**Reviewed:** 2026-04-13
**Previous review:** REVIEW-ELEGANCE.md (v1)
**Scope:** All source files under `src/`

## Status of Previously Flagged Issues

### Critical (production risk)

1. **Blocking `std::net::UdpSocket::send_to` in async context** -- **FIXED**
   `multicast.rs:114` now converts the std socket to `tokio::net::UdpSocket::from_std(std_socket)`, and lines 122/150 use `.await` on `socket.send_to(...)`. The socket is created with `set_nonblocking(true)` at line 87 (was `false`). Correct fix.

2. **Silent swallowing of write errors throughout server_connection.rs** -- **FIXED**
   All write sites now log the error and return early. Specifically:
   - Lines 152-155: ACK write checks `Err(e)`, logs, returns.
   - Lines 209-214: PONG write checks `Err(e)`, logs, returns.
   - Lines 311-315: Reply HEADER write checks, logs, cleans up outgoing state, returns.
   - Lines 327-331: Reply DATA write checks, logs, cleans up, returns.
   - Lines 344-346: Reply EOF write checks and logs.
   - Lines 347-349: Flush errors logged.
   Good fix. One minor note: on DATA write failure at line 329, the code removes outgoing state and returns, which is correct (abandons the reply).

3. **Race condition removing outgoing state too early** -- **FIXED**
   `client/connection.rs:347` now removes the outgoing state *after* the response is received (line 339-346 does the timeout/response wait, then line 347 removes). The comment at line 336-338 explicitly documents this: "Outgoing state kept alive until response arrives so ACK validation works for the full request lifecycle (I1 from audit)." Correct fix.

4. **`std::sync::Mutex` in async code** (`service_registry.rs:60`) -- **OPEN (acknowledged)**
   `failures` field still uses `std::sync::Mutex`. The current usage is safe because the lock is never held across an await point (both `mark_failed` and `pick_healthy` acquire, mutate, and drop within synchronous code). This is actually the correct pattern per tokio documentation -- `std::sync::Mutex` is preferred when the lock is not held across await points. **Downgrading from Critical to Informational.** No change needed.

### Important (should fix)

5. **`cert_sha1_fingerprint` panics in library code** (`crypto.rs:16`) -- **OPEN**
   Still uses `.expect("SHA1 hash failed")`. Low risk in practice (OpenSSL SHA1 will not fail on valid input), but library code should prefer `Result`.

6. **`now_secs()` panics** -- **PARTIALLY FIXED**
   - `authz.rs:129-132` now uses `.unwrap_or_default()` -- FIXED.
   - `service_registry.rs:309-313` still uses `.unwrap()` -- OPEN.
   - `announce.rs:82-85` still uses `.unwrap()` -- OPEN.
   - `ticket.rs:100-103` still uses `.unwrap()` -- OPEN.

7. **`ConfElement::to_string()` shadows Display** (`config.rs:246`) -- **OPEN**
   Still present with `#[allow(dead_code, clippy::inherent_to_string)]` and `.unwrap()` on line 248. The `clippy::inherent_to_string` allow was added (acknowledging the issue), and the method is `#[allow(dead_code)]`, but the `.unwrap()` inside remains.

8. **V4 actions never populated in announce.rs** -- **OPEN**
   Lines 39-44: `v4_acns`, `v4_acname`, `v4_acver`, `v4_acflag`, `v4_acsec`, `v4_acenv` are still declared as empty `Vec::new()` and never populated. Actions only go into the v3 class map. The v4 extension hash in announcements will always contain empty RLE arrays. This is functionally harmless since consumers fall back to v3, but it is dead/incomplete code.

9. **`use log;` import is non-idiomatic** -- **OPEN**
   Still present in 6 files: `config.rs:2`, `multicast.rs:9`, `listener.rs:6`, `packet.rs:1`, `reader.rs:6`, `connection.rs:4`. Should be `use log::{info, warn, error, debug};` or simply removed (the `log::info!()` macro syntax works without any import).

10. **Blocking `std::fs::read` in async `serve.rs:72-73`** -- **OPEN**
    `std::fs::read(&key_path)` and `std::fs::read(&cert_path)` are synchronous reads inside an async function. These are one-time startup reads of small files, so impact is minimal, but should use `tokio::fs::read` for consistency. Note: `request.rs:65` correctly uses `tokio::fs::read(file).await?`.

11. **Blocking `std::fs::File::open` and `BufReader` in `reload_from_cache`** -- **OPEN**
    `service_registry.rs:173-197` uses synchronous `File::open` and the synchronous `CacheFileAnnouncementIterator`. When called at startup this is fine; if called from the observer's async context it would block.

12. **`CacheFileAnnouncementIterator` delimiter-spanning-buffer issue** -- **OPEN**
    The `windows()` search at `cache_file.rs:36` can miss a delimiter that spans two `fill_buf()` returns. In practice, BufReader's default 8KB buffer makes this unlikely for the 5-byte delimiter `\n%%%\n`, but the code is still not theoretically correct.

13. **Connection pool never evicts idle or failed connections** -- **OPEN**
    `client/connection.rs:71-91`: `get_connection` only evicts on access when `closed` is true. No background cleanup, no pool size limit.

14. **`eprintln!` mixed with `log::error!` in library code** -- **OPEN**
    `discovery/packet.rs:104-107` still has `eprintln!` alongside `log::error!` in `signature_is_valid()`. The `mock.rs:56` `eprintln!` is acceptable (test/mock code).

### Minor (nice to have)

15. **Unnecessary `clone()` in request paths** -- **OPEN**
    `requester.rs:124`: `opts.body.clone()` still copies the entire request body. Could take `&[u8]`.

16. **`list_sectors` prints table even when `--raw`** -- **OPEN**
    `list.rs:294`: `print!("{}", table.render())` is outside any `if !raw` block in `list_sectors`. The `list_actions` (line 147) and `list_services` (line 246) methods correctly gate on `!raw`, but `list_sectors` does not.

17. **Duplicate `DEFAULT_SERVER_TIMEOUT_SECS` constant** -- **OPEN**
    Still in both `server_connection.rs:18` and `client/connection.rs:30`.

18. **`_headers` unused in request.rs:51** -- **OPEN**
    Headers are parsed into `_headers` BTreeMap but never sent with the request. The underscore prefix suppresses the warning but the functionality is incomplete.

19. **`RegexSet` imported but unused** -- **FIXED**
    No longer present in `authorized_services.rs`. The import was removed.

20. **`MockClient` methods take `&mut self` unnecessarily** -- **OPEN**
    `expectations_met`, `expectation_count`, `clear`, `expect` all still take `&mut self` but only need `&self` since they use interior mutability via Mutex.

21. **Missing explicit `Send` bound on `ScampReply`** -- **OPEN** (informational)
    `ScampReply` is `Send` by default (all fields are `Send`), but no explicit derive documents this intent.

22. **`as` casts for integer conversions** -- **OPEN**
    `header.rs:180`: `FlexInt(v as i64)` in `visit_u64` still silently truncates u64 values above i64::MAX. `service_info/mod.rs:165`: `as u32` casts still present for `weight` and `interval`.

23. **`Config::get<T>` double-wrapped return type** -- **OPEN**
    Still returns `Option<Result<T, T::Err>>`. Every call site chains `.and_then(|r| r.ok())`.

24. **Empty `cli/` directory** -- **FIXED** (no longer present in the file listing).

## New Issues Introduced by Recent Changes

### N1. Writer task flushes after every packet (client/connection.rs:366-369)

The client `writer_task` calls `writer.flush().await` after every single packet. For a multi-chunk request (HEADER + N x DATA + EOF), this means N+2 flush calls, each potentially triggering a TCP send. This defeats Nagle's algorithm and TCP corking. The server side in `server_connection.rs` has the same pattern but only flushes after ACK/PONG/reply-complete, which is more efficient.

**Recommendation:** Batch writes and flush once after the full message, or use a brief coalescing window. Alternatively, since TCP_NODELAY is set, each write already goes out immediately and flush is a no-op for most async writers.

### N2. `list_services` always shows "Authorized" column (list.rs:237)

In `list_services`, the `Authorized` column header is only added when `*all` is true (line 175-176), but line 237 unconditionally pushes `auth.to_string()` into every row. When `--all` is not set, this creates a data column with no header. Contrast with `list_actions` which correctly gates the column push behind `if *all` at line 136.

### N3. `authz.rs` now_secs() uses `unwrap_or_default` but `ticket.rs` does not

The fix for issue #6 was applied inconsistently. `authz.rs:131` uses `unwrap_or_default()` (safe), but `ticket.rs:101` and `service_registry.rs:311` still use `.unwrap()`. This creates an inconsistent panic surface.

### N4. Observer creates `tokio::net::UdpSocket` correctly but inconsistently with announcer

The observer (`observer.rs:50`) and announcer (`multicast.rs:114`) both correctly use `tokio::net::UdpSocket::from_std()`, which is good. However, the observer creates its std socket with `set_nonblocking(true)` (line 131) while the announcer also sets `set_nonblocking(true)` (line 87). These are consistent now, but the observer socket creation is in a different module (`observer.rs:115-133`) -- consider sharing socket construction to avoid drift.

### N5. `serve.rs` build_packet closure borrows service then service is moved

In `serve.rs:114-119`, a closure `build_packet` borrows `&service`, but the service is later consumed by `service.run(...)` at line 163. The code works because the closure is only used for a test build at line 124 and is never moved into the spawned task (separate copies of the data are made at lines 129-135). However, this is fragile -- any attempt to move `build_packet` into the spawned task would fail. The pattern is correct but confusing.

## Summary Scorecard

| # | Issue | Severity | Status |
|---|-------|----------|--------|
| 1 | Blocking UDP send_to | Critical | **FIXED** |
| 2 | Silent write error swallowing | Critical | **FIXED** |
| 3 | Outgoing state removed too early | Critical | **FIXED** |
| 4 | std::sync::Mutex in async | Critical->Info | **OK** (correct usage) |
| 5 | cert_sha1_fingerprint panics | Important | OPEN |
| 6 | now_secs() panics | Important | PARTIAL (1/4 fixed) |
| 7 | ConfElement::to_string shadows | Important | OPEN |
| 8 | V4 actions never populated | Important | OPEN |
| 9 | `use log;` non-idiomatic | Important | OPEN |
| 10 | Blocking fs::read in serve.rs | Important | OPEN |
| 11 | Blocking I/O in reload_from_cache | Important | OPEN |
| 12 | CacheFile delimiter boundary | Important | OPEN |
| 13 | Connection pool no eviction | Important | OPEN |
| 14 | eprintln in library code | Important | OPEN |
| 15 | Unnecessary body clone | Minor | OPEN |
| 16 | list_sectors raw mode | Minor | OPEN |
| 17 | Duplicate timeout constant | Minor | OPEN |
| 18 | _headers unused in request.rs | Minor | OPEN |
| 19 | RegexSet unused import | Minor | **FIXED** |
| 20 | MockClient &mut self | Minor | OPEN |
| 21 | Missing Send on ScampReply | Minor | OPEN |
| 22 | as casts for integers | Minor | OPEN |
| 23 | Config::get double-wrapped | Minor | OPEN |
| 24 | Empty cli/ directory | Minor | **FIXED** |
| N1 | Writer flushes per packet | Minor | NEW |
| N2 | list_services auth column | Minor | NEW |
| N3 | Inconsistent unwrap_or_default | Minor | NEW |
| N4 | Socket construction drift risk | Info | NEW |
| N5 | Confusing borrow/move in serve.rs | Info | NEW |

**Fixed: 5 | Partially Fixed: 1 | Open: 18 | New: 5**

All 3 critical production-risk issues are fixed. The remaining items are important-to-minor correctness and ergonomic improvements.

---

## E2E Integration Test Architecture Recommendation

### Goal

Exercise the complete SCAMP stack in a single `cargo test` invocation, with no external dependencies (no running Perl/Go services, no pre-existing certs, no multicast network):

```
Self-signed cert generation
       |
   ScampService (TLS listener, echo handler)
       |
   Multicast announcer --> in-memory or loopback UDP
       |
   Cache file write (serialize announcements)
       |
   ServiceRegistry (load from cache file)
       |
   BeepishClient (TLS connect, fingerprint verify)
       |
   Requester.request("TestAction.echo", ...) --> response
       |
   Assert body == request body
```

### 1. TLS Certificates in CI

Generate self-signed certs at test time using the `openssl` crate (already a dependency):

```rust
/// Generate a self-signed RSA 2048 cert + key pair for testing.
fn generate_test_keypair() -> (Vec<u8>, Vec<u8>) {
    use openssl::rsa::Rsa;
    use openssl::x509::{X509Builder, X509NameBuilder};
    use openssl::pkey::PKey;
    use openssl::hash::MessageDigest;
    use openssl::asn1::Asn1Time;

    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();

    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "scamp-test").unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(1).unwrap()).unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();

    let cert_pem = builder.build().to_pem().unwrap();
    let key_pem = pkey.private_key_to_pem_pkcs8().unwrap();
    (key_pem, cert_pem)
}
```

This avoids checked-in test certs, filesystem dependencies, and CI secrets. The cert lives for 1 day, which is plenty for a test run. Use `rcgen` as an alternative if you want a lighter dependency, but `openssl` is already in the dep tree.

### 2. Test Structure: Cargo Integration Test

Create `tests/e2e_full_stack.rs` as a cargo integration test:

```rust
//! E2E integration test: TLS service -> announce -> cache -> registry -> request -> response
//!
//! Exercises the full SCAMP stack with no external dependencies.
//! Uses loopback networking and self-signed certificates generated at test time.

use std::io::Write;
use tempfile::NamedTempFile;

use scamp::config::Config;
use scamp::crypto::cert_sha1_fingerprint;
use scamp::discovery::packet::AnnouncementPacket;
use scamp::discovery::service_registry::ServiceRegistry;
use scamp::auth::authorized_services::AuthorizedServices;
use scamp::service::{ScampService, ScampReply};
use scamp::transport::beepish::{BeepishClient, ScampResponse};
use scamp::transport::beepish::proto::EnvelopeFormat;

#[tokio::test]
async fn test_full_stack_e2e() {
    // Phase 1: Generate test TLS keypair
    let (key_pem, cert_pem) = generate_test_keypair();
    let fingerprint = cert_fingerprint_from_pem(&cert_pem);

    // Phase 2: Start a ScampService with an echo handler
    let mut service = ScampService::new("e2e-test", "main");
    service.register("E2ETest.echo", 1, |req| async move {
        ScampReply::ok(req.body)
    });
    service.bind_pem(&key_pem, &cert_pem, "127.0.0.1".parse().unwrap()).await.unwrap();
    let service_uri = service.uri().unwrap();
    let service_addr = service.address().unwrap();

    // Phase 3: Build an announcement packet (skip multicast, build directly)
    let announcement_bytes = service.build_announcement_packet(true).unwrap();
    let announcement_str = String::from_utf8(announcement_bytes.clone()).unwrap();

    // Phase 4: Verify the announcement parses and has a valid signature
    let parsed = AnnouncementPacket::parse(&announcement_str).unwrap();
    assert!(parsed.signature_is_valid());
    assert_eq!(parsed.body.info.uri, service_uri);

    // Phase 5: Write a synthetic cache file
    let mut cache_file = NamedTempFile::new().unwrap();
    write!(cache_file, "{}\n%%%\n", announcement_str).unwrap();
    let cache_path = cache_file.path().to_string_lossy().to_string();

    // Phase 6: Write a synthetic authorized_services file
    let mut auth_file = NamedTempFile::new().unwrap();
    writeln!(auth_file, "{} main:ALL", fingerprint).unwrap();
    let auth_path = auth_file.path().to_string_lossy().to_string();

    // Phase 7: Build a minimal Config and load the registry
    let config = build_test_config(&cache_path, &auth_path);
    let registry = ServiceRegistry::new_from_cache(&config).unwrap();

    // Phase 8: Verify the action is discoverable
    let entry = registry.find_action("main", "e2etest.echo", 1)
        .expect("Action not found in registry");
    assert_eq!(entry.service_info.uri, service_uri);
    assert!(entry.authorized);

    // Phase 9: Start the service listener (in a background task)
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let service_handle = tokio::spawn(service.run(shutdown_rx));

    // Phase 10: Send a request through BeepishClient
    let client = BeepishClient::new(&config);
    let response = client.request(
        &entry.service_info,
        "e2etest.echo",
        1,
        EnvelopeFormat::Json,
        "",
        0,
        b"hello e2e".to_vec(),
        Some(5),
    ).await.unwrap();

    // Phase 11: Verify the response
    assert!(response.error.is_none(), "Unexpected error: {:?}", response.error);
    assert_eq!(response.body, b"hello e2e");

    // Phase 12: Shutdown
    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service_handle,
    ).await;
}
```

### 3. Key Design Decisions

**Why a cargo integration test (not a separate binary)?**
- Runs with `cargo test` -- no custom harness, no special CI step.
- Automatically parallelized with other tests.
- Has access to the full public API of the `scamp` crate, which is what you want to test.
- Can use `#[tokio::test]` for async test support.

**Why skip actual multicast?**
- Multicast requires specific network configuration (loopback multicast may not work in all CI environments, Docker containers often lack multicast support).
- The multicast send/receive path is already unit-tested in `multicast.rs` and `observer.rs`.
- The E2E test focuses on the integration of: cert generation -> announcement building/parsing/signing -> cache file I/O -> registry loading -> TLS connection -> request/response. Multicast is an orthogonal transport concern.

**Why write a synthetic cache file?**
- This tests the `CacheFileAnnouncementIterator` and `ServiceRegistry::reload_from_cache` paths with real data.
- The cache file format (`json\n\ncert\n\nsig\n%%%\n`) is a critical interop surface. Building a real announcement and writing it to a file exercises the full chain.
- Use `tempfile::NamedTempFile` for automatic cleanup.

**Why not use `Requester` directly?**
- `Requester::from_config` requires a valid config with `discovery.cache_path`. You can use it if you wire up the config correctly, but `BeepishClient` is more direct for the test and avoids coupling to config-loading details.
- A second test variant could exercise `Requester::request()` for completeness.

### 4. Do We Need a Minimal "Cache Service" in Rust?

**No, not for the E2E test.** The cache file can be synthesized by building an announcement packet (which the service already knows how to do) and writing it to a temp file. This is simpler and more reliable than running a separate cache writer process.

A cache service *would* be needed for a production deployment to replace the Perl/Go `discovery_cache_writer` daemon that listens on multicast and writes the cache file. But that is a runtime component, not a test dependency. For the E2E test, the synthetic cache file approach is sufficient and avoids needing actual multicast.

### 5. Test Dependencies to Add

```toml
[dev-dependencies]
tempfile = "3"
```

The `openssl` crate (for cert generation) is already a regular dependency.

### 6. Variations to Consider

- **Multi-request test:** Send multiple requests on the same connection to verify connection reuse and sequential message numbering.
- **Large body test:** Send a body larger than `DATA_CHUNK_SIZE` (2048 bytes) to exercise chunking and flow control through TLS.
- **Error path test:** Request a non-existent action and verify the error response propagates correctly.
- **Concurrent requests test:** Spawn multiple requesters hitting the same service to verify the server handles concurrent connections.
- **Fingerprint mismatch test:** Connect with a client that expects a different fingerprint and verify the connection is rejected.
- **Announcement round-trip via observer:** If multicast loopback works in the test environment, send an announcement via the announcer and receive it via the observer, then verify registry injection. Gate this behind `#[ignore]` for CI environments without multicast.

### 7. File Layout

```
tests/
  e2e_full_stack.rs          # Main E2E integration test
  helpers/
    mod.rs                   # Shared test utilities
    certs.rs                 # generate_test_keypair(), cert_fingerprint_from_pem()
    config.rs                # build_test_config() -- creates minimal in-memory config
```

Alternatively, put the cert generation helper in `src/test_helpers.rs` (which already exists) and keep the integration test as a single file.

## Commendations (unchanged from v1, plus new)

All commendations from the v1 review remain valid. Additionally:

- **Proper error handling in server_connection.rs.** The write error fix is thorough -- every write site now logs and returns early, and the outgoing state is cleaned up on failure. The pattern is consistent across ACK, PONG, and reply sending.

- **Correct flow control lifecycle.** The fix to keep outgoing state alive until the response arrives is well-documented with a comment referencing the audit item (I1). The cleanup path handles all error cases (timeout, connection loss, send failure).

- **Clean socket migration.** The multicast announcer migration from blocking `std::net::UdpSocket` to `tokio::net::UdpSocket::from_std()` is the correct approach -- it reuses the `socket2` configuration (multicast interface, reuse_address) while getting async I/O.
