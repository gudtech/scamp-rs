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

## Workspace Layout

```
scamp-rs/
  Cargo.toml              ← workspace root
  rustfmt.toml            ← max_width = 140
  scamp/                   ← core library (protocol, transport, discovery, auth)
  scamp-macros/            ← proc-macro crate (#[rpc] attribute)
  scamp-cli/               ← CLI binary (list, request, serve)
  sample-service/          ← example service demonstrating #[rpc] usage
```

## Reference Implementations (Priority Order)

1. **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`) — THE canonical implementation
2. **scamp-js** (`/Users/daniel/GT/repo/scamp-js/lib/`) — most featureful
3. **gt-soa/csharp** (`/Users/daniel/GT/repo/gt-soa/csharp/`) — C# client
4. **scamp-go** (`/Users/daniel/GT/repo/scamp-go/scamp/`) — LEAST reliable

Service manager reference:
- **gt-service-manager-perl** (`/Users/daniel/GT/repo/gt-service-manager-perl/`) — action registration, `:RPC` attributes, UnRPC dispatcher
- **gt-main-service** (`/Users/daniel/GT/repo/gt-main-service/`) — real service using the manager

## Key Files

- `DEFICIENCIES.md` — comprehensive tracking of all findings across 3 audit rounds
- `E2E_TEST_PLAN.md` — plan for end-to-end integration testing
- `CODING_STANDARDS.md` — 300-line file limit (tests NOT exempt), no split impl blocks
- `REVIEW-*-v3.md` — latest 6-agent audit reports

## Current State (as of 2026-04-13)

### Test Suite

92 tests total (85 lib + 5 E2E + 1 macro + 1 scamp-macros), 5 ignored (dev env only).

### Core Protocol (complete)

Wire framing, header JSON, ACK/EOF/TXERR, PING/PONG, flow control watermark
(65536), DATA chunk 2048, `\r\n` strict, sequential msgno.

### Security (complete)

TLS fingerprint verification, RSA PKCS1v15 SHA256 signatures,
authorized_services filtering, ticket verification (parse/verify/expiry/privileges),
Auth.getAuthzTable privilege checking with 5-minute cache.

AuthzChecker supports three modes:
- `from_requester()` — production (fetches from Auth.getAuthzTable~1 via SCAMP)
- `from_table()` — tests/standalone (static in-memory table)
- `from_file()` — standalone (loads JSON from disk)

Deny by default: empty tickets rejected for non-noauth actions, unconfigured
actions denied.

### Discovery

Cache file loading, multicast announcing (zlib, v4 extension hash),
multicast observer (live in-memory registry updates), cache reload,
announcement TTL/expiry, replay protection, service deduplication, failure
tracking with exponential backoff.

**Gap**: No cache file writer. The observer updates the in-memory registry from
multicast but doesn't persist to disk. Standalone scamp-rs setups can't serve
as cache providers for other services on the backplane. The pieces exist
(observer receives announcements, cache file format is known) — needs a writer
that periodically flushes accumulated announcements to disk.

### Service Lifecycle

Random port binding (30100-30399), graceful shutdown (drain 30s + weight=0
announcing 10 rounds), bus_info interface resolution (`if:ethN`, private IP
auto-detect), announceable flag filtering.

### #[rpc] Macro + Auto-Discovery

Ergonomic action registration via `#[scamp::rpc]` proc macro + `inventory` crate:

```rust
#[scamp::rpc(noauth)]
async fn health_check(_ctx: RequestContext, _state: &AppState) -> &'static str {
    "ok"
}

#[scamp::rpc(read)]
async fn fetch(ctx: RequestContext, state: &AppState) -> Result<Json<Vec<Item>>> {
    let items = state.db.query(&ctx.body).await?;
    Ok(Json(items))
}
```

Features:
- Module path auto-derives SCAMP namespace (`actions::api::status` → `Api.Status`)
- snake_case fn names → camelCase wire names (`health_check` → `healthCheck`)
- Flags: `noauth`, `read`, `public`, `create`, `update`, `destroy`
- `version = N`, `timeout = N`, `sector = "..."`, `namespace = "..."` overrides
- `IntoScampReply` trait: handlers can return `ScampReply`, `String`, `Vec<u8>`,
  `Json<T>`, `Result<T>` — errors auto-convert to error replies
- Fixed state type per service, passed as `&S` to every handler
- `auto_discover_into()` wires all `#[rpc]` registrations at startup

See `sample-service/` for a complete working example.

### Critical Bugs Fixed This Session

1. **BufReader busy-loop on partial TLS packets**: `fill_buf()` returns same
   data when buffer isn't empty, causing infinite loop. Fixed with manual
   `Vec<u8>` buffer in both server and client reader.

2. **tokio::io::split deadlock on TLS streams**: TLS read may need to write
   internally, but split coordinates via a lock. Fixed with `copy_bidirectional`
   proxy through `tokio::io::duplex`.

3. **Empty ticket auth bypass**: Empty ticket on non-noauth actions was allowed.
   Now denied with "Authentication required".

### v3 Audit Status

3 full 6-agent audits completed. All high and medium findings resolved.

**Remaining low-priority items** (see DEFICIENCIES.md):
- A5: V4 action vectors empty in announcements
- A6: No heartbeat initiation (responds to PING, never sends)
- A7: Connection pool grows without bound
- A4: Config keys hardcoded (rpc.timeout, beepish.* timeouts, port range)
- A8: Stale cache behavior divergence
- M4: AtomicBool Relaxed ordering (should be Acquire/Release)
- M8: RSA signature verification blocks ServiceRegistry write lock

### Verified Interop (Docker on gtnet)

| Test | Result |
|------|--------|
| Rust client → Perl mainapi (health_check) | pass |
| Rust client → Perl mainapi (_meta.documentation, 400KB+) | pass |
| Discovery cache parsing (all announcements) | pass |
| Perl BEEPish::Client → Rust service (direct) | pass |
| Perl soatest → Rust service (via discovery) | pass |
| Perl Requester->simple_request → Rust (via discovery) | pass |
| lssoa shows Rust service | pass |

### Next Work

1. **Cache file writer** — flush observer announcements to disk periodically
2. **Sample service E2E test** — start sample-service, connect, call each action
3. **Interop load/stress testing** — scamp-rs ↔ Perl services on gtnet at volume
4. **Phase 2 E2E**: JS interop test via scamp-js Docker
5. **Phase 3 E2E**: Perl interop test

## Dev Environment

- `gud dev status -g` shows running containers
- Docker network: `gtnet`
- Dev keypair: `~/GT/backplane/devkeys/dev.key` and `dev.crt`
  (fingerprint: `BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0`)
- Build for Docker: `docker build --platform linux/amd64 -f Dockerfile.interop-test -t scamp-rs-test .`
- Test with soatest: `docker exec main perl /service/main/gt-soa/perl/script/soatest --action "ScampRsTest.echo~1" --data '{"test":"hello"}' -p`

## CI

GitHub Actions (`.github/workflows/ci.yml`): build, test, `cargo fmt --all --check`,
`cargo clippy --workspace -D warnings`. Runs on push to main and PRs.

## Audit Protocol

After each milestone, dispatch 6 verification agents:

```
Agent(name="verify-vs-perl", prompt="Compare function-by-function. Report MATCH/PARTIAL/MISSING/DIVERGENT.")
Agent(name="verify-vs-js", prompt="Compare. Report.")
Agent(name="verify-vs-go", prompt="Compare. Report.")
Agent(name="verify-vs-csharp", prompt="Compare. Report.")
Agent(name="review-elegance", prompt="Review correctness, deadlocks, resource leaks, performance.")
Agent(name="review-standards", prompt="Check every file against CODING_STANDARDS.md.")
```
