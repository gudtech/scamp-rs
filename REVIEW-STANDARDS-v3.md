# Standards Compliance Review v3

**Review date**: 2026-04-13
**Reviewer**: Automated standards audit against CODING_STANDARDS.md
**Previous review**: REVIEW-STANDARDS-v2.md (same date)
**Files reviewed**: 43 `.rs` files (src/ + tests/)

---

## Executive Summary

The codebase is in significantly better shape than v2. The most severe v2 violations (inline tests in `connection.rs` and `server_connection.rs`) have been fully resolved — those tests are now in separate files and both production files are at or under 300 lines. `service_registry.rs` has been trimmed. The E2E test file now exists with 5 real test scenarios. Cargo.toml is clean.

Five findings remain open. Three are real violations against the coding standard (F1, F2, F5). Two are quality concerns worth tracking (F3, F4).

---

## Finding F1 — 300-Line Limit: `tests.rs` Exceeds 300 Lines

**Severity**: VIOLATION (coding standard explicit)
**File**: `src/transport/beepish/proto/tests.rs`
**Line count**: 396 lines

The coding standard states "No file should exceed 300 lines (including tests)" and "Tests do NOT count toward the 300-line limit if they are in a separate file." The exemption from the production file's 300-line budget is granted when tests are extracted to a separate file, but the separate test file is not itself exempt from the 300-line limit. The standard says it applies to _all_ files.

`tests.rs` contains 24 test functions across two areas: header/serde tests (lines 1–228) and wire fixture tests (lines 229–396). These are readily splittable.

**Remediation**: Split into `proto/tests/header.rs` and `proto/tests/wire_fixtures.rs` under a `tests/` subdirectory, with `proto/tests/mod.rs` containing `mod header; mod wire_fixtures;`.

---

## Finding F2 — Logic in `mod.rs`: `service_info/mod.rs`

**Severity**: VIOLATION (coding standard: "No logic in mod.rs")
**File**: `src/discovery/service_info/mod.rs`
**Line count**: 218 lines (all production)

The standard says `mod.rs` should be "thin — just `pub mod` declarations and `pub use` re-exports. No logic in `mod.rs`."

`service_info/mod.rs` contains:
- `AnnouncementBody::parse()` (lines 134–187) — 54 lines of parsing logic calling `parse::parse_v4_actions` and `parse::parse_v3_actions`
- `CrudOp::parse_str()` (lines 191–199) — string matching logic
- `Flag::parse_str()` (lines 202–218) — regex matching with `TIMEOUT_RE` lazy static
- `ServiceInfoParseError` enum (lines 91–126) — this is a type definition, tolerable in mod.rs

`parse.rs` exists but does not contain these entry points. `AnnouncementBody::parse()` belongs in `parse.rs` since it is the public entry point that orchestrates both v3 and v4 parsing. `CrudOp::parse_str()` and `Flag::parse_str()` are already used by `parse.rs` and should live there or be in a `types.rs` alongside the struct definitions.

**Remediation**: Move `AnnouncementBody::parse()`, `CrudOp::parse_str()`, and `Flag::parse_str()` into `parse.rs`. Leave only type definitions and `pub mod`/`pub use` in `mod.rs`.

---

## Finding F3 — Duplicate `eprintln!` in Library Code

**Severity**: STANDARDS CONCERN (not a hard violation, but contradicts Q3 fix)
**Files and lines**:
- `src/discovery/packet.rs:86` — `eprintln!("Signature verification error for {}: {}", ...)` inside `signature_is_valid()`. The preceding line already calls `log::error!` with the same message. The `eprintln!` is redundant and violates the "use log macros, not println!" principle that was fixed as Q3.
- `src/transport/mock.rs:56` — `eprintln!("  * Mock Call to {} at {}", ...)` — `mock.rs` is used only in tests (no `#[cfg(test)]` annotation on the module itself, but it's only called from tests). Still, the pattern is inconsistent.

The bin code (`serve.rs`, `request.rs`) legitimately uses `println!` for CLI UX output, which is appropriate for a binary. The `list.rs` use of `println!` is also CLI output, appropriate.

**Remediation**: Remove the duplicate `eprintln!` at `packet.rs:86` (the `log::error!` on line 85 is sufficient). For `mock.rs`, either add `#[cfg(test)]` at the module level or change to `log::debug!`.

---

## Finding F4 — `unwrap()` on `SystemTime` in Production Code

**Severity**: STANDARDS CONCERN (standard says "Don't panic in library code")
**Instances**:
- `src/service/announce.rs:84` — `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` in `build_announcement_packet()`. This panics if system clock is before 1970 (impossible in practice, but still).
- `src/auth/ticket.rs:88` — `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` in `Ticket::verify()`.
- `src/discovery/service_registry.rs:268` — `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` in the `now_secs()` helper.

HANDOFF.md acknowledged this: "Several `unwrap()` on SystemTime that should use `unwrap_or_default()`." It's been tracked. The fix is trivial.

Note: `src/auth/authz.rs:121` uses `.unwrap_or_default()` correctly — that's the pattern to follow.

**Remediation**: Change all three instances to `.unwrap_or_default().as_secs()`.

---

## Finding F5 — Inline `#[cfg(test)]` Modules Remaining in Production Files

**Severity**: VIOLATION per coding standard ("Short tests: put in `submodule/tests.rs`")
**Files with remaining inline test modules**:

| File | Test module start line | Test count (approx) | File total lines |
|------|----------------------|---------------------|-----------------|
| `src/service/announce.rs` | 199 | 4 (3 unit + 1 ignored) | 288 |
| `src/auth/authorized_services.rs` | 152 | 9 | 248 |
| `src/auth/authz.rs` | 125 | 2 | 150 |
| `src/auth/ticket.rs` | 120 | 5 | 171 |
| `src/transport/mock.rs` | 99 | 1 | 159 |
| `src/discovery/packet.rs` | 99 | 4 (2 unit + 2 ignored) | 199 |
| `src/discovery/cache_file.rs` | 59 | 1 | 75 |
| `src/config.rs` | 226 | 3 | 259 |
| `src/service/multicast.rs` | 148 | 2 | 175 |
| `src/bus_info.rs` | 140 | 4 | 178 |
| `src/crypto.rs` | 78 | 3 (2 unit + 1 ignored) | 116 |

The v2 review noted that `connection.rs` (497 lines) and `server_connection.rs` (500 lines) had been flagged because of inline tests. Those were fixed by extracting to `connection_tests.rs` and `server_connection_tests.rs`. The same treatment is due for the files above.

**However**, the files above are all under 300 lines (including their inline tests), so the 300-line limit is not violated. The coding standard allows "Short tests: put in `submodule/tests.rs`" — for files well under 300 lines, this is a judgment call. The standard does not _require_ extraction below 300 lines; it says to _prefer_ it.

**Priority ranking for extraction**:
1. `authorized_services.rs` (9 tests, 248 lines total) — highest priority; test module alone is ~97 lines and the file is close to the limit
2. `announce.rs` (4 tests at line 199, 288 lines) — file is right at the limit with tests; worth extracting given proximity to 300
3. Others are small enough that inline tests do not cause a problem today

**Remediation**: Not a blocking violation for files under 300 lines. Treat as low-priority improvement. For `authorized_services.rs` and `announce.rs`, consider extracting to `auth/authorized_services/tests.rs` and `service/announce/tests.rs` if those files grow.

---

## Check: 300-Line Limit (All Files)

| File | Lines | Status |
|------|-------|--------|
| `src/transport/beepish/proto/tests.rs` | **396** | **OVER — F1** |
| `src/transport/beepish/client/connection.rs` | 300 | AT LIMIT (exactly) |
| `src/service/announce.rs` | 288 | PASS |
| `src/discovery/service_registry.rs` | 275 | PASS (was 322 in v2, fixed) |
| `src/bin/scamp/list.rs` | 273 | PASS (was 305 in v2, fixed) |
| `src/service/server_connection.rs` | 260 | PASS (was 500 in v2, fixed) |
| `src/config.rs` | 259 | PASS |
| `tests/e2e_full_stack.rs` | 254 | PASS |
| `src/auth/authorized_services.rs` | 248 | PASS |
| `src/transport/beepish/client/reader.rs` | 230 | PASS |
| `src/service/listener.rs` | 225 | PASS |
| `src/discovery/service_info/mod.rs` | 218 | PASS (line count) |
| All remaining files | ≤199 | PASS |

`connection.rs` at exactly 300 lines is borderline. The standard says "no file should exceed 300 lines" — 300 exactly is compliant. One more line added by any future change will violate it. Consider extracting the `writer_task` free function (lines 289–300) into a separate module preemptively.

---

## Check: Module Structure

**`mod.rs` files reviewed**:

| File | Content | Status |
|------|---------|--------|
| `src/auth/mod.rs` | 3 lines: `pub mod` only | PASS |
| `src/service/mod.rs` | 31 lines: `pub mod`, `pub use`, one thin `announce_raw` wrapper function | BORDERLINE |
| `src/transport.rs` | 2 lines: `pub mod` only | PASS |
| `src/transport/beepish.rs` | 7 lines: `mod client`, `pub mod proto`, `pub use` | PASS |
| `src/transport/beepish/proto/mod.rs` | 28 lines: `pub mod`, `pub use`, 2 constants, `PacketType` enum | BORDERLINE |
| `src/transport/beepish/client/mod.rs` | 8 lines: `mod`, `pub use` | PASS |
| `src/discovery.rs` | 8 lines: `pub mod`, `pub use` | PASS |
| `src/discovery/service_info/mod.rs` | 218 lines with logic | **VIOLATION — F2** |

**`service/mod.rs` borderline**: The `announce_raw` function (lines 19–31) is a thin wrapper that delegates to `announce::build_announcement_packet`. It exists for public API convenience. This is arguably "too much" for a `mod.rs` but pragmatically it's one function that serves as a stable public API boundary. Acceptable if it stays this small.

**`proto/mod.rs` borderline**: Contains `PacketType` enum (lines 19–28). Types belong in submodules, but `PacketType` is tightly coupled to parsing and is used everywhere. Moving it to `packet.rs` would be cleaner; `mod.rs` would then be pure re-exports + constants.

---

## Check: No Split `impl` Blocks

All `impl` blocks for each struct are in a single file. No violations found.

Notable: `ConnectionHandle` has three `impl` blocks in `connection.rs`:
- `impl BeepishClient` (line 47)
- `impl ConnectionHandle` (line 84)
- `impl Drop for ConnectionHandle` (line 281)

These are all in one file — conformant. The `Drop` impl is a separate trait impl, not a split `impl ConnectionHandle`.

---

## Check: Naming Conventions

**PascalCase types**: All types follow PascalCase. ✓
- `ScampService`, `BeepishClient`, `ConnectionHandle`, `PacketHeader`, `EnvelopeFormat`, `FlexInt`, `MessageType`, `AnnouncementPacket`, `ServiceRegistry`, `AuthorizedServices`, `AuthzChecker` — all correct.

**snake_case functions**: All functions and methods use snake_case. ✓

**SCREAMING_SNAKE constants**: All constants follow the convention. ✓
- `DEFAULT_RPC_TIMEOUT_SECS`, `FLOW_CONTROL_WATERMARK`, `DEFAULT_SERVER_TIMEOUT_SECS`, `ANNOUNCEABLE`, `DEFAULT_MULTICAST_GROUP`, `DEFAULT_MULTICAST_PORT`, `SHUTDOWN_ROUNDS`, `AUTHZ_CACHE_TTL_SECS` — all correct.

**SCAMP terminology**: Protocol terminology is well-matched.
- `PacketHeader` (not `MessageHeader`) ✓
- `msg_no` (not `message_number`) ✓
- `packet_type` ✓
- `FlexInt` — matches Go's `flexInt` ✓

One terminology question: `ScampReply` in `handler.rs` vs. `ScampResponse` in `client/connection.rs`. These represent different things (server-side reply vs. client-side received response), so different names are correct. No issue.

---

## Check: Error Handling

**Public APIs returning Result**: All public library functions that can fail return `Result`. ✓
- `Config::new()`, `ScampService::bind_pem()`, `ScampService::run()`, `BeepishClient::request()`, `ConnectionHandle::send_request()`, `ServiceRegistry::new_from_cache()`, `AuthorizedServices::load()`, `Ticket::verify()`, `AnnouncementPacket::parse()` — all return `Result`.

**Internal helpers using anyhow freely**: Correct. ✓

**Log errors at detection**: Most errors are logged at detection. The `signature_is_valid()` double-log (`log::error!` + `eprintln!` on same error) is the exception (F3 above).

**No panics in library code**: Three `unwrap()` on `SystemTime` in production code (F4 above). These cannot realistically panic but technically violate the standard.

One instance in `MulticastConfig::new()` at `src/service/multicast.rs:39`:
```rust
group: DEFAULT_MULTICAST_GROUP.parse().unwrap(),
```
This panics if `DEFAULT_MULTICAST_GROUP` is not a valid IP. Since it's a compile-time constant string `"239.63.248.106"`, the parse cannot fail. The `unwrap()` is safe but could be replaced with a `const` assertion or by storing the `Ipv4Addr` directly. Low priority.

---

## Check: Wire Compatibility Comments

Protocol-critical code is well-commented with Perl file:line references. Survey of key sites:

- `packet.rs`: `// Perl Connection.pm:46 limits header line to 80 bytes` (line 65), `// Perl Connection.pm:148-149` (line 137), `// Perl Connection.pm:187` (line 127) ✓
- `connection.rs`: `// Perl Client.pm:33` (line 117), `// JS connection.js:298` (line 209), `// Perl Connection.pm:162` (line 246) ✓
- `reader.rs`: `// Perl Connection.pm:140`, `// Perl Connection.pm:153`, `// Perl Connection.pm:162`, `// JS connection.js:229` ✓
- `server_connection.rs`: `// Perl Connection.pm:177-183`, `// JS connection.js:229`, `// Perl Connection.pm:131-135` ✓
- `announce.rs`: `// Perl Announcer.pm:103`, `// Perl Announcer.pm:127-157`, `// Perl Announcer.pm:159-175`, `// Perl Announcer.pm:187`, `// Perl Announcer.pm:196-197`, `// Perl Announcer.pm:199-201` ✓
- `service_registry.rs`: `// Perl ServiceInfo.pm:257-258`, `// Perl ServiceManager.pm:29`, `// Perl ServiceManager.pm:66-98` ✓
- `authorized_services.rs`: `// Perl ServiceInfo.pm:111-168`, `// Perl ServiceInfo.pm:117-118`, `// Perl ServiceInfo.pm:125-126`, `// Perl ServiceInfo.pm:130`, `// Perl ServiceInfo.pm:131-132`, `// Perl ServiceInfo.pm:135`, `// Perl ServiceInfo.pm:141-167`, `// Perl ServiceInfo.pm:147`, `// Perl ServiceInfo.pm:149` ✓
- `authz.rs`: `// JS ticket.js:51-95`, `// JS ticket.js:53`, `// JS ticket.js:55-68`, `// JS ticket.js:60-67` ✓

Wire compatibility comments are thorough and consistent. No deficiencies found.

---

## Check: Test Organization

**Fully extracted test files** (correct pattern):
- `src/transport/beepish/proto/tests.rs` — extracted from `proto/mod.rs` ✓
- `src/transport/beepish/client/connection_tests.rs` — extracted (v2 fix) ✓
- `src/service/server_connection_tests.rs` — extracted (v2 fix) ✓
- `src/discovery/service_info/tests.rs` — extracted ✓

**`#[ignore]` with explanation**: All dev-environment tests have `#[ignore]` with a comment explaining the requirement. ✓
- `announce.rs:237`: `#[ignore] // requires dev keypair` ✓
- `authorized_services.rs:227`: `#[ignore] // requires live dev environment` ✓
- `crypto.rs:106`: `#[ignore]` with panic message if cert missing ✓
- `packet.rs:125,158`: `#[ignore] // requires live dev environment` ✓

**E2E test**: `tests/e2e_full_stack.rs` exists with 5 test scenarios (echo, large body, unknown action, sequential requests, announcement signature). Matches E2E_TEST_PLAN.md Phase 1 requirements. Only scenario 7 (authorized services filtering at lookup) from the plan is absent — the test file covers the other 6 implicitly through the setup flow. Overall: PASS.

**`test_helpers.rs`**: Correctly annotated with `#![cfg(test)]` (line 4) and gated as `pub(crate)` in `lib.rs`. ✓

---

## Check: Comments Quality

**"Why" not "what"**: Comments explain motivations and reference implementations rather than narrating code. ✓

**No bare TODOs**: No `TODO`, `FIXME`, or `HACK` comments found in any source file. ✓

**No stale comments**: No obviously outdated comments detected. The comment at `listener.rs:224` (`// Re-export for use by register() callers`) references a `use` statement that is already used by the `register()` method above — accurate. ✓

---

## Check: Dependencies (Cargo.toml)

```toml
[dependencies]
tokio, serde, serde_json, regex, once_cell, itertools,
clap, dotenv, homedir, anyhow, log, env_logger,
term-table, rand, base64, openssl, tokio-native-tls,
flate2, socket2, libc

[dev-dependencies]
tempfile
```

All dependencies are actively used:
- `tokio`: async runtime, everywhere
- `serde` + `serde_json`: packet headers, authz parsing
- `regex`: config comment stripping, authorized_services patterns, flag parsing
- `once_cell`: `TIMEOUT_RE` lazy static in `service_info/mod.rs`
- `itertools`: `izip!` in `parse.rs` for v4 action parsing
- `clap`: CLI arg parsing in `src/bin/scamp/`
- `dotenv`: `.env` file loading in `config.rs`
- `homedir`: home directory resolution in `config.rs`
- `anyhow`: error handling throughout
- `log` + `env_logger`: logging
- `term-table`: table rendering in `list.rs`
- `rand`: random port selection, random identity bytes
- `base64`: announcement signing, ticket signature decoding
- `openssl`: crypto (fingerprint, RSA signing, TLS cert generation in tests)
- `tokio-native-tls`: TLS client/server
- `flate2`: zlib compression/decompression for multicast
- `socket2`: multicast socket configuration
- `libc`: `getifaddrs` in `bus_info.rs`
- `tempfile` (dev): E2E test temp cache files

No unused dependencies found. `thiserror` was correctly removed (L3 fix from v1). ✓

---

## Check: HANDOFF.md Accuracy

HANDOFF.md states:
- "82 tests (5 ignored for dev environment)" — needs verification count
- "All Milestones Complete (M1-M9)" — consistent with code
- "Inline tests make connection.rs (415) and server_connection.rs (475) exceed 300 lines → Extract to separate test files" — this is listed under "Known Remaining Gaps" but has been **fixed**. The HANDOFF.md still lists it as a known gap under "Code quality." This is **stale**.
- "service_info/mod.rs has parsing logic (should be in parse.rs)" — still accurate (F2 above)
- "service_registry.rs at ~322 lines" — **stale**, now 275 lines (fixed)
- "Several `unwrap()` on SystemTime that should use `unwrap_or_default()`" — still accurate (F4 above)

**HANDOFF.md inaccuracies**:
1. Line 87: "Inline tests make connection.rs (415) and server_connection.rs (475) exceed 300 lines → Extract to separate test files" — this is listed as remaining work but is already done. Should be moved to "Resolved in audit response" in DEFICIENCIES.md and removed from HANDOFF.md.
2. Line 89: "service_registry.rs at ~322 lines" — should read "~275 lines (reduced)".

---

## Check: DEFICIENCIES.md Accuracy

DEFICIENCIES.md correctly tracks:
- All 46 original deficiencies as resolved ✓
- Post-audit findings with correct status (A1, A2, S1 marked as fixed) ✓
- S1 (inline tests) — marked `~~S1~~ | ~~Standards~~ | ~~Inline tests~~ Fixed: extracted to separate files, all under 300` — BUT `proto/tests.rs` is now 396 lines (F1 above). S1 was closed prematurely for `tests.rs`. The two connection test files were correctly extracted and are compliant (87 and 143 lines).
- S3 (service_registry.rs at ~322 lines) — marked open, correct — though the file is now at 275 lines and should be marked resolved.

**DEFICIENCIES.md inaccuracies**:
1. S3 `service_registry.rs at ~322 lines` — should be marked resolved (now 275 lines).
2. S1 should note the remaining issue: `proto/tests.rs` at 396 lines exceeds 300 (F1).

---

## Check: E2E_TEST_PLAN.md vs. Implementation

E2E_TEST_PLAN.md Phase 1 plan called for 7 test scenarios. `tests/e2e_full_stack.rs` implements:

| Plan # | Test | Implemented |
|--------|------|-------------|
| 1 | Echo roundtrip | ✓ `test_echo_roundtrip` |
| 2 | Large body (> 2048 bytes) | ✓ `test_large_body` |
| 3 | Unknown action | ✓ `test_unknown_action` |
| 4 | Multiple sequential requests | ✓ `test_sequential_requests` |
| 5 | Concurrent connections | ✗ Not implemented |
| 6 | Announcement signature verification | ✓ `test_announcement_signature_verification` |
| 7 | Authorized services filtering | ✗ Not implemented |

Two scenarios (concurrent connections, authorized services filtering at lookup) are not implemented. The plan lists them as part of Phase 1. This is not a coding standards violation but a gap in coverage vs. the plan.

E2E_TEST_PLAN.md also specifies `Config::from_str()` or `Config::builder()` as an "API gap to fix." This was implemented as `Config::from_content()` in `config.rs:51–54` — good, though the name differs from the plan. Functionally equivalent.

---

## Summary of Findings

| ID | Severity | File(s) | Description |
|----|----------|---------|-------------|
| F1 | **VIOLATION** | `src/transport/beepish/proto/tests.rs:1` | 396-line test file exceeds 300-line limit |
| F2 | **VIOLATION** | `src/discovery/service_info/mod.rs:134` | Parsing logic in mod.rs (`AnnouncementBody::parse`, `Flag::parse_str`, `CrudOp::parse_str`) |
| F3 | CONCERN | `src/discovery/packet.rs:86` | Redundant `eprintln!` alongside `log::error!` in `signature_is_valid()` |
| F4 | CONCERN | `src/service/announce.rs:84`, `src/auth/ticket.rs:88`, `src/discovery/service_registry.rs:268` | `unwrap()` on `SystemTime::now().duration_since(UNIX_EPOCH)` in production code |
| F5 | LOW | Multiple files | Inline `#[cfg(test)]` modules remain in files under 300 lines (not a current limit violation, low priority) |

**Documentation accuracy**:
- HANDOFF.md: Two stale statements (lines 87, 89) — connection test extraction already done, registry size already reduced
- DEFICIENCIES.md: S3 should be marked resolved; S1 should note `proto/tests.rs` at 396 lines

**E2E tests**: 5 of 7 Phase 1 scenarios implemented. Concurrent connections and authorized services filtering tests are missing.

---

## Prioritized Remediation List

1. **(F1, high)** Split `proto/tests.rs` (396 lines) into `proto/tests/header.rs` + `proto/tests/wire_fixtures.rs`
2. **(F2, medium)** Move `AnnouncementBody::parse()`, `Flag::parse_str()`, `CrudOp::parse_str()` from `service_info/mod.rs` into `parse.rs`
3. **(F3, low)** Remove `eprintln!` at `packet.rs:86` (duplicate of `log::error!` on line 85)
4. **(F4, low)** Replace `.unwrap()` with `.unwrap_or_default()` on three `SystemTime::now()` calls
5. **(docs)** Update HANDOFF.md lines 87 and 89; update DEFICIENCIES.md S1 and S3 status
6. **(tests)** Implement E2E test scenarios 5 (concurrent connections) and 7 (authorized services filtering)
