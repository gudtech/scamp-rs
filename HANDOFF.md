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

- `PUNCHLIST.md` — milestone-structured todo list with verification tests
- `DEFICIENCIES.md` — 16 remaining gaps (30 resolved) from 6-agent audit
- `CODING_STANDARDS.md` — 300-line file limit, no split impl blocks, test organization

## Current State (as of 2026-04-13)

### Completed Milestones

**M1: Secure Client Connection** ✓
**M2: Announcement Signature Verification** ✓
**M3: Authorized Services Filtering** ✓
**M4: Multicast Announcing** ✓
**M5: Full Bidirectional Interop** ✓
**M6: Wire Protocol Hardening** ✓ (mostly — watermark pause/resume remains)
**M7: Config & Behavioral Parity** ✓ (mostly — bus_info/interface resolution remains)

Key fixes this session:
- `ticket: null` → `nullable_string` deserializer (was causing silent HEADER drops)
- Config first-wins, inline `#` comments, GTSOA env var
- `\r\n` required in header parsing (was accepting bare `\n`)
- ACK validation (format, monotonic, not-past-end)
- Server idle timeout 120s
- DATA chunk 2048 (was 131072)
- Always serialize action/ticket/identifying_token
- Timestamp replay protection + service deduplication
- Announcement TTL/expiry + cache staleness check
- Wire protocol test fixtures from Perl (12 new tests, 55 total)

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

### Next Work: M8/M9 — Discovery + Production Hardening

**16 remaining items** (see DEFICIENCIES.md). Mostly larger features:

**M8 — Discovery Hardening:**
- D24: Multicast receiver/observer
- D25: Cache refresh/reload mechanism

**M9 — Production Hardening:**
- D10: Graceful shutdown (drain active requests)
- D11: Ticket verification
- D22: `bus_info()` interface resolution (`if:ethN`)
- D23: Bind to `service.address` interface
- D28: High-level Requester API
- D29: Announceable flag filtering
- D31/D32: Failure tracking, retry

**Remaining wire items:**
- D5b: Send-side flow control watermark pause/resume (ACK validation done)
- Q5: listener.rs (390 lines) exceeds 300-line limit

**Remaining test items:**
- T1: Server hot path tests
- T2: Client request sending tests
- T4: authorized_services tests through load()
- T10: RLE decode edge cases

## Dev Environment

- `gud dev status -g` shows running containers (main, auth, cache, soabridge, etc.)
- Docker network: `gtnet`
- Dev keypair: `~/GT/backplane/devkeys/dev.key` and `dev.crt`
  - Fingerprint: `BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0`
- Build for Docker: `docker build --platform linux/amd64 -f Dockerfile.interop-test -t scamp-rs-test .`
- Run on gtnet: `docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro -v ~/GT/backplane/etc:/etc/GT:ro -e SCAMP_CONFIG=/backplane/etc/soa.conf scamp-rs-test [subcommand]`
- Test with soatest: `docker exec main perl /service/main/gt-soa/perl/script/soatest --action "ScampRsTest.echo~1" --data '{"test":"hello"}' -p`

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
