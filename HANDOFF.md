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
- `DEFICIENCIES.md` — 32 remaining gaps from the last 4-agent audit
- `CODING_STANDARDS.md` — 300-line file limit, no split impl blocks, test organization
- `DEFICIENCIES.md` Tier 1 items — what blocks the next milestone

## Current State (as of 2026-04-12)

### Completed Milestones

**M1: Secure Client Connection** ✓
- SHA1 cert fingerprinting (`src/crypto.rs`)
- TLS peer cert fingerprint verification after handshake (`client/connection.rs`)
- Natural corking (verification before stream split = no packets to unverified peers)
- Verified: Rust client → Perl mainapi with fingerprint `BC:6E:86:...` confirmed

**M2: Announcement Signature Verification** ✓
- RSA PKCS1v15 SHA256 verification (`crypto.rs`, `discovery/packet.rs`)
- Verified against all 3 live Perl-generated announcements in dev cache
- Confirmed: Perl's `use_pkcs1_oaep_padding` is a no-op for sign/verify

**M3: Authorized Services Filtering** ✓
- File parsing with regex matching (`auth/authorized_services.rs`)
- Integrated into ServiceRegistry — unauthorized actions filtered from lookups
- `_meta.*` always authorized, `:` in sector/action rejected, hot-reload on mtime
- Verified against real dev `authorized_services` file

**Also completed:**
- Phase 0: BEEP handshake removed, dead code deleted
- Phase 1: Full transport core (packet framing, header JSON, message assembly, ACKs)
- Phase 2: Service infrastructure (TLS listener, action dispatch, announcement generation)
- Action index key: `sector:action.vVERSION` matching Perl/JS
- CRUD aliases fixed: `_destroy` not `_delete`
- V4 accompat filter enabled
- Code refactored: all files under 300 lines per coding standards

### Verified Interop (Docker on gtnet)

| Test | Result |
|------|--------|
| Rust client → Perl mainapi (health_check) | ✓ with fingerprint verification |
| Rust client → Perl mainapi (_meta.documentation, 400KB+) | ✓ multi-packet |
| Discovery cache parsing (all announcements) | ✓ signatures verified |
| authorized_services filtering | ✓ matches Perl lssoa output |
| Perl BEEPish::Client → Rust service (direct) | ✓ echo works |
| Perl Requester → Rust service (via discovery) | ✗ needs multicast announcing |

### Next Milestone: M4 — Multicast Announcing

**Goal**: Rust service sends real UDP multicast announcements → cache service
picks them up → `lssoa` shows the Rust service → Perl Requester discovers
and calls it through the normal pipeline.

**Dependency chain** (from PUNCHLIST.md):
1. Read config: `discovery.multicast_address` (default 239.63.248.106), `discovery.port` (default 5555), `bus.address`
2. Create UDP socket, join multicast group
3. Zlib compress announcement packet (Perl `Announcer.pm:203`)
4. Send on interval (default 5s)
5. Fix: V4 extension hash in envelopes array, RLE encoding, base64 line-wrapping, announceable flag filtering
6. Shutdown: weight=0, 10 rounds at 1s interval

**Verification**:
```bash
docker run -d --name scamp-rs-service --network gtnet \
  -v ~/GT/backplane:/backplane:ro -v ~/GT/backplane/etc:/etc/GT:ro \
  -e SCAMP_CONFIG=/backplane/etc/soa.conf \
  scamp-rs-test serve --key /backplane/devkeys/dev.key --cert /backplane/devkeys/dev.crt

# Wait ~10s for cache service to pick up multicast, then:
docker exec main perl /service/main/gt-soa/perl/script/lssoa | grep scamp-rs

# Full bidirectional test:
docker exec main perl -e '
  use GTSOA::Requester; use JSON;
  my @r = GTSOA::Requester->simple_request(
    action => "ScampRsTest.echo", version => 1,
    envelope => "json", data => {test => "full interop"},
  );
  die "FAILED" unless $r[0];
  print "SUCCESS: " . encode_json($r[1]) . "\n";
'
```

### Remaining Deficiencies (32 items in DEFICIENCIES.md)

**Tier 1 (blocks M4)**: D1-D4 — multicast, zlib, config keys, v4 extension hash
**Tier 2 (production)**: D5-D15 — flow control, timeouts, cache staleness, TTL, shutdown, tickets
**Tier 3 (behavioral)**: D16-D32 — header parsing, config parity, interface resolution

### Known Issue: Perl Requester Timeout

Direct `BEEPish::Client` → Rust works. But `Requester->simple_request` times out.
The Perl client connects, TLS handshake succeeds (cert presented correctly, fingerprint
matches), but packets stay buffered in `_corked_writes` and never flush. The
`on_starttls` callback appears to not fire in the Requester→ConnectionManager path.
This may resolve itself once we use real multicast announcing instead of the previous
cache injection hack (which was removed — only the cache service should write to the
discovery cache file).

## Dev Environment

- `gud dev status -g` shows running containers (main, auth, cache, soabridge, etc.)
- Docker network: `gtnet`
- Dev keypair: `~/GT/backplane/devkeys/dev.key` and `dev.crt`
  - Fingerprint: `BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0`
  - Authorized for all sectors in `authorized_services`
- Build for Docker: `docker build --platform linux/amd64 -f Dockerfile.interop-test -t scamp-rs-test .`
- Run on gtnet: `docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro -v ~/GT/backplane/etc:/etc/GT:ro -e SCAMP_CONFIG=/backplane/etc/soa.conf scamp-rs-test [subcommand]`

## Audit Protocol

After each milestone, dispatch verification agents:

```
Agent(name="verify-vs-perl", prompt="Read ALL Rust src files and ALL Perl GTSOA files. Compare function-by-function. Report MATCH/PARTIAL/MISSING/DIVERGENT for each.")
Agent(name="verify-vs-js", prompt="Read ALL Rust src files and ALL scamp-js files. Compare. Report.")
Agent(name="verify-vs-go", prompt="Read ALL Rust src files and ALL scamp-go files. Compare. Report.")
Agent(name="verify-vs-csharp", prompt="Read ALL Rust src files and ALL gt-soa/csharp files. Compare. Report.")
```

Each agent reads both codebases and produces a structured report. Findings go into
DEFICIENCIES.md. The punchlist is updated to reflect the verified state.
