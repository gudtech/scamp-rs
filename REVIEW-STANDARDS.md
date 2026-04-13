# Standards Compliance Review

Review date: 2026-04-13
Reviewer: Automated standards audit against CODING_STANDARDS.md

## Summary

The codebase is in **good overall compliance** with CODING_STANDARDS.md. All 38 source files were reviewed against all 10 documented standards. There are 4 violations (2 moderate, 2 minor) and several areas where documentation has drifted from code reality.

## Violations

### 1. 300-Line Limit: VIOLATION

**Standard**: "No file should exceed 300 lines (including tests). Tests do NOT count toward the 300-line limit if they are in a separate file."

The standard says tests do not count if they are in a **separate file**. Files with inline `#[cfg(test)] mod tests` that remain in the same .rs file have their tests counted toward the limit.

Violations:
- `src/transport/beepish/client/connection.rs`: **362 lines** (271 prod + 91 inline test). VIOLATION.
  Production code is under 300, but the standard says tests only exempt in separate files.
- `src/service/server_connection.rs`: **447 lines** (299 prod + 148 inline test). VIOLATION.
  Production code is just under 300, but total file with inline tests is 447 lines.
- `src/transport/beepish/proto/tests.rs`: **380 lines** (all test). This is a separate test file, which the standard implies is exempt. PASS.
- `src/config.rs`: **285 lines** total with inline tests. PASS (under 300).

**Remediation**: Extract inline tests from `connection.rs` and `server_connection.rs` into separate `tests.rs` files within their respective module directories.

### 2. Module Structure (mod.rs thin): PASS

All `mod.rs` files are thin re-export modules with no logic:
- `src/transport/beepish/proto/mod.rs`: 29 lines (mod declarations, pub use, constants, one simple enum). The `PacketType` enum (8 lines) and two constants are borderline but acceptable as protocol type definitions closely tied to re-exports.
- `src/transport/beepish/client/mod.rs`: 7 lines, pure re-exports.
- `src/discovery/service_info/mod.rs`: 191 lines. **VIOLATION**. This file contains substantial logic: the `AnnouncementBody::parse()` method (37 lines), `Flag::parse_str()` (15 lines), `CrudOp::parse_str()` (10 lines), type definitions, error types, and Display impls. The standard says "No logic in mod.rs" -- but these types and their parse methods are the core of the module. Moving just the parsing logic to `parse.rs` (which already exists for v3/v4) and keeping types here would be more compliant.
- `src/service/mod.rs`: 32 lines. Contains the `announce_raw()` convenience function (8 lines of logic). Minor -- this is a thin wrapper, but strictly it is logic in mod.rs.
- `src/auth/mod.rs`: 2 lines, pure re-exports. PASS.
- `src/discovery.rs`: 8 lines. Has `pub use service_info::*` which re-exports everything. PASS but glob re-exports can be surprising.
- `src/transport.rs`: 2 lines, PASS.
- `src/lib.rs`: 11 lines, PASS.

### 3. No Split Impl Blocks: PASS

No struct has its `impl` block split across files. Checked all `impl` declarations:
- `ConnectionHandle` has impl in `connection.rs` only (96, 252)
- `BeepishClient` has impl in `connection.rs` only (62)
- `ServiceRegistry` has impl in `service_registry.rs` only (63)
- `Config` has impl in `config.rs` only (36)
- `ScampService` has impl in `listener.rs` only (34)
- All other structs have single-file impls.

The multiple `impl` blocks for the same struct within a single file (e.g., `FlexInt` in header.rs with trait impls) is standard Rust and not a violation.

### 4. Test Organization: PASS (with notes)

**Separate test files**: `proto/tests.rs`, `service_info/tests.rs`, `test_helpers.rs` -- all correct.

**Inline tests**: Many files use `#[cfg(test)] mod tests` inline. The standard says "Short tests: put in `submodule/tests.rs`" and "In proto.rs -- no inline tests, just the implementation." This is advisory but several files use inline tests (config.rs, mock.rs, packet.rs, cache_file.rs, observer.rs, multicast.rs, announce.rs, authorized_services.rs, ticket.rs, crypto.rs, bus_info.rs, connection.rs, server_connection.rs). The standard prefers separate test files.

**#[ignore] with comments**: All 5 `#[ignore]` tests have explanatory comments:
- `src/crypto.rs:115` -- no inline comment, but doc comment above says "Run with: cargo test -- --ignored"
- `src/service/announce.rs:236` -- `#[ignore] // requires dev keypair`
- `src/auth/authorized_services.rs:235` -- `#[ignore] // requires live dev environment`
- `src/discovery/packet.rs:138` -- `#[ignore] // requires live dev environment`
- `src/discovery/packet.rs:175` -- `#[ignore] // requires live dev environment`

PASS.

### 5. Naming: PASS

- Types: All PascalCase (`PacketHeader`, `EnvelopeFormat`, `FlexInt`, `ScampService`, etc.)
- Functions/methods: All snake_case (`parse_config`, `send_request`, `inject_packet`, etc.)
- Constants: All SCREAMING_SNAKE (`MAX_PACKET_SIZE`, `DATA_CHUNK_SIZE`, `DEFAULT_RPC_TIMEOUT_SECS`, etc.)
- Modules: All snake_case
- Protocol terminology: `PacketHeader` (not `MessageHeader`), `msg_no` (not `message_number`), `PacketType` -- all match SCAMP terminology.

### 6. Error Handling: VIOLATION (minor)

**Public APIs return Result**: Most public APIs correctly return `Result<T>`:
- `Config::new()` returns `Result<Self>` -- PASS
- `Requester::request()` returns `Result<ScampResponse>` -- PASS
- `ServiceRegistry::new_from_cache()` returns `Result<Self>` -- PASS
- `BeepishClient::request()` returns `Result<ScampResponse>` -- PASS
- `ScampService::bind_pem()` returns `Result<()>` -- PASS
- `ScampService::run()` returns `Result<()>` -- PASS

**No panics in library code**: Mostly PASS with these exceptions:
- `src/crypto.rs:16`: `hash(...).expect("SHA1 hash failed")` -- in library code. OpenSSL SHA1 should never fail but technically this panics. Minor.
- `src/service/multicast.rs:38`: `DEFAULT_MULTICAST_GROUP.parse().unwrap()` -- parsing a compile-time constant, safe but technically a panic path. Appears in `MulticastConfig::new()` (public API) and `from_config()` fallback. Minor.
- `src/service/announce.rs:87`: `.unwrap()` on `SystemTime::now().duration_since(UNIX_EPOCH)` -- safe (system clock won't be before epoch) but technically a panic. Minor.
- `src/discovery/service_info/mod.rs:176,181`: `Regex::new(...).unwrap()` inside `Lazy` (safe, compile-time constant) and `caps[1].parse().unwrap()` (safe, regex guarantees digits).
- `src/discovery/service_registry.rs:218,253,275`: `failures.lock().unwrap()` -- standard Mutex pattern; panics on poisoned mutex. This is idiomatic Rust.
- `src/transport/mock.rs:32,35,38,41,58,63,82`: Multiple `.unwrap()` calls in MockClient methods. This is a test helper type, not production library code. PASS.
- `src/transport/beepish/proto/packet.rs:50`: `std::str::from_utf8(type_str).unwrap()` -- safe because `type_str` is always a valid ASCII byte literal. Minor.
- `src/auth/ticket.rs:94`: `.unwrap()` on `SystemTime` -- same safe pattern.
- `src/config.rs:129,246`: `Regex::new().unwrap()` (safe constant) and `to_string()` method unwrap (dead-code helper). Minor.

**Summary**: No serious panic-in-library violations. The `unwrap()` calls are on infallible operations (compile-time constants, system time). The `expect` in crypto.rs is the closest to a real concern.

### 7. Dependencies: PASS

Cargo.toml has 18 dependencies. Reviewing each:
- `tokio`, `serde`, `serde_json`: essential
- `openssl`: required per standard ("Prefer the openssl crate")
- `tokio-native-tls`: TLS support
- `clap`: CLI
- `regex`, `once_cell`: used for pattern matching and lazy statics
- `itertools`: used for `izip!` in parse.rs -- could be replaced with manual zip chains, but the macro is cleaner
- `flate2`: zlib compression/decompression (not achievable in 20 lines)
- `socket2`: multicast socket setup (not achievable in 20 lines)
- `base64`: base64 encoding/decoding (not achievable in 20 lines)
- `rand`: random number generation
- `log`, `env_logger`: logging
- `term-table`: CLI table formatting
- `dotenv`: env file loading
- `homedir`: home directory detection
- `anyhow`, `thiserror`: error handling
- `libc`: getifaddrs in bus_info.rs

**Potential concern**: `thiserror = "1.0.63"` is listed in dependencies but no `#[derive(Error)]` appears in the codebase. `thiserror` is unused.

**Potential concern**: `homedir = "0.3.3"` -- getting home directory could be done with `std::env::var("HOME")` which is already used elsewhere in the codebase (e.g., serve.rs:50, announce.rs:238). The `homedir` crate adds cross-platform support but it's only used in one place (config.rs:121).

**Potential concern**: `dotenv = "0.15.0"` -- only called once (`dotenv::dotenv().ok()` in config.rs:63). Whether this is needed depends on deployment.

### 8. Wire Compatibility (Perl file:line references): PASS

Protocol-critical code consistently references Perl implementation:
- `src/transport/beepish/proto/packet.rs:4` -- "Perl Connection.pm:46,192-201"
- `src/transport/beepish/proto/packet.rs:69` -- "Perl Connection.pm:46 limits header line to 80 bytes"
- `src/transport/beepish/proto/packet.rs:134` -- "Perl Connection.pm:187 -- unknown packet type is fatal"
- `src/transport/beepish/proto/packet.rs:146` -- "Perl Connection.pm:148-149 -- malformed header JSON is fatal"
- `src/transport/beepish/proto/mod.rs:17` -- "Perl Connection.pm:218 uses 2048"
- `src/transport/beepish/client/reader.rs:24-25` -- "Perl Connection.pm:177-183"
- `src/transport/beepish/client/reader.rs:126` -- "Perl Connection.pm:153"
- `src/transport/beepish/client/reader.rs:134` -- "Perl Connection.pm:162 -- EOF body must be empty"
- `src/transport/beepish/client/connection.rs:23-30` -- timeout constants with Perl refs
- `src/service/server_connection.rs:16` -- "Perl Server.pm:58, Connection.pm:131-135"
- `src/config.rs:95,150,196-197` -- Perl Config.pm references
- `src/discovery/observer.rs:78,84` -- "Perl Observer.pm:48,50"
- `src/crypto.rs:9-13` -- "Perl ServiceInfo.pm:82-87" and "Go cert.go:14-31"
- `src/auth/authorized_services.rs:6-17` -- "Perl ServiceInfo.pm:111-168" and "JS handle/service.js:168-219"

### 9. Comments: PASS (with one note)

**Why not what**: Comments consistently explain rationale, not mechanics. Good examples:
- "Perl sends `ticket => undef` which JSON-encodes as `"ticket": null`" (header.rs:68)
- "D5b: Wake sender if it's blocked on flow control watermark" (reader.rs:189)
- "Perl Config.pm:30-31 -- first-wins for duplicate keys" (config.rs:197)

**Perl file:line references**: Extensively present (see Wire Compatibility above).

**No TODO without DEFICIENCIES ref**: No TODO comments found in any source file. PASS.

### 10. Commits: PASS (with note)

Recent commit messages from git log:
```
31ca5bf Add completion punchlist and agent prompts for full scamp-rs implementation
75c46ec WIP
473f636 WIP
ff0a865 request subcommand works with mock client
cf6a187 stub out subcommand
```

The "WIP" commits are technically violations of "One logical change per commit" and "commit message: first line is a short summary, body explains why." However, these appear to be development-in-progress commits that were likely squashed or represent active work. The standard says to "Reference punchlist items (e.g., 'M4:', 'D5:') in commit messages" -- this is absent from all visible commits.

## Documentation Accuracy

### HANDOFF.md

- **"80 tests (5 ignored for dev environment)"**: Accurate per code review -- 5 `#[ignore]` tests found.
- **"All 46 deficiency items resolved"**: Matches DEFICIENCIES.md.
- **Completed milestones M1-M9**: All listed features are present in the code.
- **"Key fixes this session"**: All mentioned fixes are verifiable in code (server_connection.rs extraction, flow control, binary body fix, etc.).
- **Interop verification table**: Claims are plausible given the code, though interop results aren't verifiable from source alone.
- **"16 remaining gaps (30 resolved)"** in "Key Files" section: Inconsistent with "All 46 deficiency items resolved" stated later. The line "16 remaining gaps" appears stale.
- **Reference to `scamp-js` paths**: Correct paths listed.

### DEFICIENCIES.md

- **"All 46 Deficiencies Resolved"**: The resolved table has duplicate entries (D1-D4, D12-D16, D19-D21, D27, D30 appear twice). The total unique items are approximately 38, not 46. The duplicate rows should be cleaned up.
- All resolved items can be verified in code (e.g., D5b flow control in connection.rs:200-218, D10 graceful shutdown in listener.rs:214-229, D22 bus_info in bus_info.rs, etc.).
- **No remaining items**: Accurate -- all listed items have corresponding code.

### PUNCHLIST.md

- **"32 remaining items after M1-M3 + audit fixes"** in header: Stale -- all milestones are complete.
- **M4 item 7**: "[~] Fix flags: filter to announceable set -- constant defined, not yet applied (actions don't have flags yet)". Code at announce.rs:54-62 shows the filter IS applied. This item should be `[x]`.
- **M6**: Still shows `[ ]` for "T-2 Server hot path tests" and "W-5 Send-side flow control" -- but both are implemented (server_connection.rs tests, connection.rs:200-218). These should be `[x]`.
- **M7**: Still shows `[ ]` for "C-6 bus_info()" and "S-23 Bind to service.address" -- both implemented (bus_info.rs, listener.rs:108). Should be `[x]`.
- **M8**: All items shown as `[ ]` but all are implemented: D7 (service_registry.rs:162-174), D8 (service_registry.rs:92-93), D9 (service_registry.rs:97-107), D26 (service_registry.rs:97-107), D24 (observer.rs), D25 (service_registry.rs:144).
- **M9**: All items shown as `[ ]` but all are implemented: D10 (listener.rs:214-229), D11 (ticket.rs), D28 (requester.rs), D31 (requester.rs:66-78), D32 (service_registry.rs:214-226).
- **Overall**: PUNCHLIST.md is significantly stale. M6-M9 items are not checked off despite being implemented.

## Recommendations

### Must Fix (Violations)

1. **Extract inline tests from connection.rs and server_connection.rs** into separate test files. Both exceed 300 lines total. Move `#[cfg(test)] mod tests` to `tests.rs` files within their module directories.

2. **Move logic out of `discovery/service_info/mod.rs`**. Move `AnnouncementBody::parse()`, `Flag::parse_str()`, `CrudOp::parse_str()` into `parse.rs` (which already handles v3/v4 parsing). Keep type definitions and error types in mod.rs.

3. **Remove `thiserror` from Cargo.toml** -- it is listed as a dependency but never used anywhere in the codebase.

### Should Fix (Documentation Drift)

4. **Update PUNCHLIST.md**: Check off all completed items in M4 (item 7), M6, M7, M8, and M9. The punchlist significantly understates project completion.

5. **Clean up DEFICIENCIES.md**: Remove duplicate rows in the resolved table (D1-D4, D12-D16, D19-D21, D27, D30 each appear twice).

6. **Update HANDOFF.md**: Fix the stale "16 remaining gaps" reference to match "all 46 resolved."

### Nice to Have

7. **Move `announce_raw()` out of `service/mod.rs`** to keep mod.rs purely re-exports per the standard.

8. **Consider extracting `MulticastConfig::new()` unwraps** to return `Result` for stricter no-panic compliance, though the current constant-parsing unwraps are practically safe.

9. **Commit message hygiene**: Future commits should reference punchlist/deficiency IDs and avoid bare "WIP" messages per the standard.
