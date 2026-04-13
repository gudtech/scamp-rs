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
- `DEFICIENCIES.md` — remaining gaps from the latest 6-agent audit
- `CODING_STANDARDS.md` — 300-line file limit, no split impl blocks, test organization

## Current State (as of 2026-04-13)

### Completed Milestones

**M1: Secure Client Connection** ✓
**M2: Announcement Signature Verification** ✓
**M3: Authorized Services Filtering** ✓
**M4: Multicast Announcing** ✓
**M5: Full Bidirectional Interop** ✓

Key fix: Perl sends `"ticket": null` in JSON headers. Our `String` deserializer
couldn't handle null, causing silent HEADER drops. Fixed with `nullable_string`.

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

### Next Milestone: M6 — Wire Protocol Hardening + Test Infrastructure

**Goal**: Wire protocol matches Perl exactly, backed by captured test vectors.

Priority items:
1. Capture wire packets from Perl as test fixtures
2. Server hot path tests (handle_connection, route_packet, dispatch_and_reply)
3. Shared test helpers (packet builders, default headers)
4. Require `\r\n` in header parsing (D16)
5. Send-side flow control — validate ACKs, pause at 65536 (D5)
6. TXERR body validation (D27)
7. Connection idle timeout + busy flag (D6)

### Remaining Deficiencies (from 6-agent audit)

See DEFICIENCIES.md for full details. Summary:
- **Code quality**: 5 items (Q1-Q5), Q1-Q4 fixed
- **Wire protocol**: 6 items (D5, D6, D12, D16, D27, D30)
- **Discovery**: 6 items (D7-D9, D24-D26)
- **Service lifecycle**: 5 items (D10, D11, D17, D18, D29)
- **Config**: 7 items (D15, D19-D23, D28)
- **Test coverage**: 10 items (T1-T10), T5/T7/T9 fixed

## Dev Environment

- `gud dev status -g` shows running containers (main, auth, cache, soabridge, etc.)
- Docker network: `gtnet`
- Dev keypair: `~/GT/backplane/devkeys/dev.key` and `dev.crt`
  - Fingerprint: `BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0`
- Build for Docker: `docker build --platform linux/amd64 -f Dockerfile.interop-test -t scamp-rs-test .`
- Run on gtnet: `docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro -v ~/GT/backplane/etc:/etc/GT:ro -e SCAMP_CONFIG=/backplane/etc/soa.conf scamp-rs-test [subcommand]`

## Audit Protocol

After each milestone, dispatch verification agents:

```
Agent(name="verify-vs-perl", prompt="...")
Agent(name="verify-vs-js", prompt="...")
Agent(name="verify-vs-go", prompt="...")
Agent(name="verify-vs-csharp", prompt="...")
Agent(name="review-tests", prompt="...")
Agent(name="review-code", prompt="...")
```
