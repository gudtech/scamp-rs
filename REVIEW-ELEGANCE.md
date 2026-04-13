# Code Elegance Review

## Summary

scamp-rs is a ~5,800-line Rust implementation of the SCAMP service bus protocol, covering discovery, service hosting, client connections, authentication, and a CLI. The codebase demonstrates strong protocol knowledge with extensive cross-referencing to Perl/Go/JS implementations. Wire-format tests are thorough. The main risks are blocking I/O in async contexts, a few silent error paths, and some unsafe code that could be replaced.

## Issues by Severity

### Critical (production risk)

1. **Blocking `std::net::UdpSocket::send_to` in async context** (`multicast.rs:121,147`). The multicast announcer uses a `std::net::UdpSocket` (with `set_nonblocking(false)` at line 87) inside an `async fn`. The `send_to` call will block the tokio worker thread. Under network pressure or a blocked multicast route, this freezes the entire tokio task scheduler. Should use `tokio::net::UdpSocket` or at minimum call from `tokio::task::spawn_blocking`.

2. **Silent swallowing of write errors throughout server_connection.rs**. Lines 148-149 (ACK write), 203-204 (PONG write), 269 (header write), 281 (data write), 294 (EOF write) all use `let _ =` to discard I/O errors. If a write fails, the connection is silently broken but the server continues processing, potentially accumulating work for a dead connection.

3. **Race condition removing outgoing state too early** (`client/connection.rs:239`). After sending the EOF, `outgoing.lock().await.remove(&msg_no)` is called immediately, before the response is received. If the server sends an ACK for the last DATA chunk after the outgoing entry is removed, the reader task will log a spurious error. More importantly, if the message is large and ACKs are delayed, the outgoing state could be removed while flow control is still relevant. The removal should happen after the response is received or via a timeout cleanup.

4. **Mutex (std::sync) inside async code** (`service_registry.rs:60`). `ServiceRegistry.failures` uses `std::sync::Mutex`, but `pick_healthy()` is called from async contexts via `Requester::request()`. If the Mutex is contended (e.g., concurrent requests hitting `mark_failed`), this blocks the tokio worker thread. Should use `tokio::sync::Mutex` or restructure to avoid holding across await points (the current code does not await while holding, but a `std::sync::Mutex` in async code is fragile and risks deadlock if the code evolves).

### Important (should fix)

5. **`cert_sha1_fingerprint` panics in library code** (`crypto.rs:16`). `.expect("SHA1 hash failed")` will panic on hash failure. While unlikely, library code should propagate errors via `Result`, not panic.

6. **`now_secs()` panics** (`service_registry.rs:273-276`). `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` will panic if the system clock is before epoch. This also applies to `announce.rs:85-88` and `ticket.rs:92-95`. Use `.unwrap_or_default()` or return `Result`.

7. **`ConfElement::to_string()` shadows Display** (`config.rs:244`). This method has the same name as `ToString::to_string()` (which is auto-derived from `Display`), creating confusion. It also calls `.unwrap()` internally, making it panic on write errors. Consider renaming to `to_conf_string()` or implementing `Display`.

8. **V4 actions never populated in announce.rs** (`announce.rs:39-44`). The v4 action vectors (`v4_acns`, `v4_acname`, etc.) are declared as empty `Vec::new()` and never populated. Actions are only put into the v3 class map. This means the v4 extension hash in the announcement packet is always empty. If downstream consumers rely on v4 action data, they will see no actions.

9. **`use log;` import is incorrect** (multiple files). `use log;` brings the crate into scope but is unnecessary and non-idiomatic. The `log::info!()` etc. macros work without this import because `log` is a dependency. Should be removed to avoid confusion.

10. **Blocking `std::fs::read` in async `serve.rs:68-69`**. Reading key/cert files with `std::fs::read` inside an `async fn` blocks the tokio worker thread. Should use `tokio::fs::read`.

11. **Blocking `std::fs::File::open` and `BufReader` in `reload_from_cache`** (`service_registry.rs:158,179`). The cache file is read with synchronous I/O. When called from an async context (e.g., after observer triggers a reload), this blocks the worker thread. The file can be large (megabytes of announcements).

12. **`CacheFileAnnouncementIterator` can loop forever if delimiter spans buffer boundary incorrectly** (`cache_file.rs:31-57`). If `fill_buf` returns a buffer smaller than the delimiter, the `windows()` search will miss the delimiter and the iterator will buffer data indefinitely. A more robust approach would handle delimiter-spanning-buffer boundaries.

13. **Connection pool never evicts idle or failed connections** (`client/connection.rs:70-83`). `get_connection` checks `closed` but only removes on access. The `connections` HashMap grows without bound. There is no background task to evict stale entries or limit pool size.

14. **`eprintln!` mixed with `log::error!` in library code** (`discovery/packet.rs:94`). `signature_is_valid()` uses both `log::error!` and `eprintln!`. Library code should use logging only; `eprintln!` bypasses the logging framework and goes straight to stderr.

### Minor (nice to have)

15. **Unnecessary `clone()` in request paths**. `requester.rs:115` clones `opts.body` (a `Vec<u8>`) for every dispatch. Since `dispatch_once` is called once or twice, this copies the entire request body. Could take `&[u8]` and have `BeepishClient::request` accept a reference.

16. **`list_sectors` prints table even when `--raw`** (`list.rs:202`). `print!("{}", table.render())` is outside the `if !raw` block, so raw mode still prints the table header.

17. **Duplicate `DEFAULT_SERVER_TIMEOUT_SECS` constant**. Defined in both `server_connection.rs:17` (as `DEFAULT_SERVER_TIMEOUT_SECS = 120`) and `client/connection.rs:30` (marked `#[allow(dead_code)]`). Should be in a shared location.

18. **`_headers` unused in request.rs:51**. The parsed headers variable is prefixed with `_` indicating it is intentionally unused. The headers from `--header` flags are parsed but never sent with the request. This is either incomplete functionality or dead code.

19. **`RegexSet` imported but unused** (`authorized_services.rs:20`). `use regex::RegexSet;` is imported but `AuthEntry` uses `Vec<regex::Regex>` instead. Could be an incomplete optimization.

20. **`MockClient` methods take `&mut self` unnecessarily**. `expectations_met`, `expectation_count`, `clear`, `expect` all take `&mut self` but internally lock a Mutex. They could take `&self` since the Mutex provides interior mutability. This limits composability.

21. **Missing `Send` bound on `ScampReply` future**. The `ActionHandlerFn` type (`handler.rs:45-49`) correctly requires `Send`, but `ScampReply` itself has no `Send`/`Sync` derive. It is `Send` by default (all fields are `Send`), but an explicit derive documents intent.

22. **`as` casts for integer conversions**. `FlexInt` deserialize uses `v as i64` for `visit_u64` (`header.rs:185`), which silently truncates values > i64::MAX. Similarly, `service_info/mod.rs:134` casts `u64 as u32`. These should use `try_from()`.

23. **`config.rs:52` generic return type `Config::get<T>` returns `Option<Result<T, T::Err>>`**. The double-wrapped return type is awkward. Every call site chains `.and_then(|r| r.ok())` to flatten it. A method like `get_or<T>(&self, key: &str, default: T)` or returning `Option<T>` with parse errors logged would improve ergonomics.

24. **Empty `cli/` directory** present in the source tree but contains no files.

## Specific Findings

- `src/service/multicast.rs:87` — `set_nonblocking(false)` explicitly makes the socket blocking, then uses it in an async function at line 121. This is the core of Critical issue #1.

- `src/service/multicast.rs:121` — `socket.send_to(&compressed, dest)` is a blocking syscall on a non-async `std::net::UdpSocket` inside `async fn run_announcer`.

- `src/service/server_connection.rs:269,281,294` — `let _ = header_pkt.write(...)`, `let _ = data_pkt.write(...)`, `let _ = eof_pkt.write(...)` silently discard errors writing reply packets. If the first write fails, the loop continues writing data chunks to a broken pipe.

- `src/transport/beepish/client/connection.rs:239` — `self.outgoing.lock().await.remove(&msg_no)` removes flow control tracking before the response is received.

- `src/service/announce.rs:39-44` — v4 vectors declared empty and never populated: `let v4_acns: Vec<String> = Vec::new();` through `let v4_acenv: Vec<String> = Vec::new();`.

- `src/bus_info.rs:132-155` — The `unsafe` block for `getifaddrs` is functionally correct: it checks for null, properly interprets `sockaddr_in`, and calls `freeifaddrs`. However, consider using the `nix` crate for a safe abstraction, or at minimum adding `SAFETY` comments documenting the invariants.

- `src/config.rs:244-248` — `ConfElement::to_string()` calls `.unwrap()` on `write_to_file`, will panic on any write error.

- `src/crypto.rs:16` — `.expect("SHA1 hash failed")` in non-test library code.

- `src/discovery/service_registry.rs:273-276` — `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` panics if system clock is before epoch.

- `src/bin/scamp/list.rs:202` — `print!("{}", table.render())` runs unconditionally, ignoring `--raw` flag.

- `src/bin/scamp/request.rs:51` — `let mut _headers: BTreeMap<String, String>` is populated but never used; headers are not sent with the actual request.

- `src/discovery/cache_file.rs:52` — `if matched && announcement_data.len() > 0` should use `!announcement_data.is_empty()` (Clippy lint).

- `src/transport/beepish/client/connection.rs:185` — `FlexInt(v as i64)` in `visit_u64`: silent truncation for u64 values above i64::MAX.

## Commendations (what's done well)

- **Excellent protocol documentation**. Nearly every function references the specific Perl/Go/JS source it was ported from (e.g., "Perl Connection.pm:46", "JS connection.js:298"). This makes cross-implementation debugging straightforward.

- **Strong wire compatibility testing**. The `fixtures.rs` + `tests.rs` in `proto/` verify exact byte-level compatibility with Perl-generated packets, including edge cases like null fields, flex integers, and CRLF requirements.

- **Clean packet framing implementation**. `Packet::parse` and `Packet::write` in `packet.rs` are well-structured, handle all edge cases (overlong lines, bare newlines, unknown types), and return meaningful error variants via `ParseResult`.

- **Sound `unsafe` in `bus_info.rs`**. The `getifaddrs` usage follows correct patterns: null checks, proper type casts, guaranteed cleanup via `freeifaddrs`. The only risk is that it could be replaced with a safe crate.

- **Good separation of concerns**. The split between `proto/` (wire format), `client/` (connection management), `server_connection` (request dispatch), and `service/` (listener setup) keeps each module focused.

- **Comprehensive discovery parsing**. V3 and V4 announcement format parsing with RLE decoding, signature verification, and round-trip testing shows thorough protocol understanding.

- **Test coverage for the critical path**. Server connection round-trip tests (`test_echo_roundtrip`, `test_multi_chunk_roundtrip`, `test_ping_pong`) and client tests (`test_client_echo`, `test_client_large_body`, `test_client_request_timeout`) cover the main happy and error paths.

- **Flow control implementation**. The D5 flow control watermark logic in `client/connection.rs` with `ack_notify` wake-up is a correct and efficient implementation of backpressure.

- **Proper graceful shutdown**. The service shutdown sequence (drain active connections, send weight=0 announcements for 10 rounds) correctly follows the protocol specification.

- **Ticket verification**. The `auth/ticket.rs` implementation properly handles Base64URL decoding, field parsing, signature verification, and expiry checking.
