# scamp-rs Agent Handoff

## What This Project Is

scamp-rs is a Rust implementation of the SCAMP protocol (Single Connection
Asynchronous Multiplexing Protocol) used by RetailOps/GudTech. It must be
**wire-compatible with the canonical Perl implementation** at
`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`.

## How We Work

**Systematically.** This is not a "write code and hope" project. The workflow is:

1. **Implement** a milestone (small, focused, with clear dependency chain)
2. **Verify** with a concrete test (unit test, Docker interop test, or both)
3. **Audit** by dispatching verification agents (one per reference implementation)
   that read both our code and the reference code function-by-function
4. **Fix** any deficiencies the audit identifies
5. **Repeat**

Every change must be evaluated against: "Will this produce/parse bytes
identically to the Perl implementation?" When in doubt, match Perl exactly.

## Reference Implementations (Priority Order)

1. **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`) — THE canonical implementation
2. **scamp-js** (`/Users/daniel/GT/repo/scamp-js/lib/`) — most featureful (PING/PONG, flow control, graceful shutdown)
3. **gt-soa/js** (`/Users/daniel/GT/repo/gt-soa/js/lib/`) — original JS impl
4. **gt-soa/csharp** (`/Users/daniel/GT/repo/gt-soa/csharp/`) — C# client implementation
5. **scamp-go** (`/Users/daniel/GT/repo/scamp-go/scamp/`) — LEAST reliable, cross-reference only

## Key Files to Read First

- `E2E_TEST_PLAN.md` — plan for end-to-end integration testing in GH Actions
- `PUNCHLIST.md` — milestone-structured todo list with verification tests
- `DEFICIENCIES.md` — all 46 original items resolved; audit findings tracked separately
- `CODING_STANDARDS.md` — 300-line file limit (tests NOT exempt), no split impl blocks
- `REVIEW-*-v2.md` — latest 6-agent audit reports (Perl, JS, Go, C#, elegance, standards)

## Current State (as of 2026-04-13)

### All Milestones Complete (M1-M9)

82 tests (5 ignored for dev environment). CI green (GitHub Actions).

**Core protocol**: Wire framing, header JSON, ACK/EOF/TXERR, PING/PONG, flow
control watermark (65536), DATA chunk 2048, `\r\n` strict, sequential msgno.

**Security**: TLS fingerprint verification, RSA PKCS1v15 SHA256 signatures,
authorized_services filtering, ticket verification (parse/verify/expiry/privileges),
Auth.getAuthzTable privilege checking with 5-minute cache.

**Discovery**: Cache loading, multicast announcing (zlib, v4 extension hash),
multicast observer, cache reload, announcement TTL/expiry, replay protection,
service deduplication, failure tracking with exponential backoff.

**Service lifecycle**: Random port binding (30100-30399), graceful shutdown
(drain 30s + weight=0 announcing 10 rounds), bus_info interface resolution
(`if:ethN`, private IP auto-detect), announceable flag filtering.

**APIs**: High-level Requester (lookup+connect+send+retry), BeepishClient
(connection pooling), `error_data` header field for structured error metadata.

### Audit Status (2x 6-agent audits completed)

Two full audits have been run. All critical items are resolved:
- C1: Auth.getAuthzTable privilege checking ✓
- C2: error_data header field + dispatch_failure detection ✓
- C3: Blocking UDP in async → tokio::net::UdpSocket ✓
- C4: Silent write error swallowing → proper error handling ✓
- I1: Outgoing flow-control lifecycle ✓

### Known Remaining Gaps (from v2 audit, non-blocking)

**Functional gaps (medium/low priority):**
- ScampReply has no `error_data` field (handlers can't send structured errors)
- `register()` always sets empty flags (can't register `noauth` actions)
- Rust server never sets `error_data: {dispatch_failure: true}` on replies
- Config keys hardcoded (rpc.timeout, beepish.* timeouts, bind port range)
- V4 action vectors always empty in announcements (v3 compat zone used)
- No heartbeat initiation (responds to PING but never sends)
- No connection pool eviction (grows without bound)
- Stale cache: Rust continues serving (Perl fails fast)

**Code quality (from standards + elegance reviews):**
- Inline tests make connection.rs (415) and server_connection.rs (475) exceed 300 lines
  → Extract to separate test files (tests are NOT exempt per coding standards)
- service_info/mod.rs has parsing logic (should be in parse.rs)
- service_registry.rs at ~322 lines (slightly over)
- Several `unwrap()` on SystemTime that should use `unwrap_or_default()`

### Verified Interop (Docker on gtnet)

| Test | Result |
|------|--------|
| Rust client → Perl mainapi (health_check) | ✓ with fingerprint verification |
| Rust client → Perl mainapi (_meta.documentation, 400KB+) | ✓ multi-packet |
| Discovery cache parsing (all announcements) | ✓ signatures verified |
| authorized_services filtering | ✓ matches Perl lssoa output |
| Perl BEEPish::Client → Rust service (direct) | ✓ echo works |
| **Perl soatest → Rust service (via discovery)** | **✓ full pipeline** |
| **Perl Requester->simple_request → Rust (via discovery)** | **✓ full pipeline** |
| lssoa shows Rust service | ✓ correct identity, sector, weight, fingerprint, actions |

### Next Work: E2E Testing

See `E2E_TEST_PLAN.md` for the full plan. Summary:
1. **Phase 1 (now)**: Rust-to-Rust E2E test in GH Actions — self-signed certs,
   synthetic cache file, full TLS request/response through discovery pipeline
2. **Phase 2**: JS interop test via scamp-js Docker container (lightweight, 1 dep)
3. **Phase 3**: Perl interop test (heavier, needs backplane deps)

## Dev Environment

- `gud dev status -g` shows running containers (main, auth, cache, soabridge, etc.)
- Docker network: `gtnet`
- Dev keypair: `~/GT/backplane/devkeys/dev.key` and `dev.crt`
  - Fingerprint: `BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0`
- Build for Docker: `docker build --platform linux/amd64 -f Dockerfile.interop-test -t scamp-rs-test .`
- Run on gtnet: `docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro -v ~/GT/backplane/etc:/etc/GT:ro -e SCAMP_CONFIG=/backplane/etc/soa.conf scamp-rs-test [subcommand]`
- Test with soatest: `docker exec main perl /service/main/gt-soa/perl/script/soatest --action "ScampRsTest.echo~1" --data '{"test":"hello"}' -p`

## CI

GitHub Actions (`.github/workflows/ci.yml`): build, test, `cargo fmt --check`,
`cargo clippy -D warnings`. Runs on push to main and PRs.
`rustfmt.toml`: `max_width = 140`.

## Audit Protocol

After each milestone, dispatch 6 verification agents:

```
Agent(name="verify-vs-perl", prompt="Read ALL Rust src and ALL Perl GTSOA files. Compare function-by-function. Report MATCH/PARTIAL/MISSING/DIVERGENT.")
Agent(name="verify-vs-js", prompt="Read ALL Rust src and ALL scamp-js files. Compare. Report.")
Agent(name="verify-vs-go", prompt="Read ALL Rust src and ALL scamp-go files. Compare. Report.")
Agent(name="verify-vs-csharp", prompt="Read ALL Rust src and ALL gt-soa/csharp files. Compare. Report.")
Agent(name="review-tests", prompt="Evaluate test coverage, quality, fixtures, structure. Top 10 tests to add.")
Agent(name="review-code", prompt="Review correctness, elegance, coding standards, wire safety.")
```
