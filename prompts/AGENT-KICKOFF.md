# scamp-rs Implementation Agent — Kickoff Prompt

You are taking ownership of completing scamp-rs — a Rust implementation of the
SCAMP protocol at /Users/daniel/GT/repo/scamp-rs/. Your goal is to bring it to
full production parity with the existing implementations.

## Reference Implementation Priority

**This is critical.** The implementations are NOT equal. In priority order:

1. **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`) — THE
   canonical implementation. The Perl services (gt-main-service, gt-auth-service,
   gt-payment-service, etc.) are the primary consumers of the SCAMP protocol.
   scamp-rs MUST be wire-compatible with the Perl implementation above all else.
   Key files:
   - `Transport/BEEPish/Connection.pm` — wire protocol, packet handling
   - `Transport/BEEPish/Client.pm` — client connections
   - `Transport/BEEPish/Server.pm` — server listener
   - `Transport/ConnectionManager.pm` — connection pooling
   - `Discovery/Announcer.pm` — multicast announcement sending
   - `Discovery/Observer.pm` — announcement receiving/cache
   - `Discovery/ServiceInfo.pm` — service/action metadata
   - `Discovery/ServiceManager.pm` — service registry
   - `Requester.pm` — high-level request API
   - `Config.pm` — configuration

2. **gt-soa/js** (`/Users/daniel/GT/repo/gt-soa/js/lib/`) — the original JS
   implementation within gt-soa. Some services still use this directly.

3. **scamp-js** (`/Users/daniel/GT/repo/scamp-js/lib/`) — a modernized fork/port
   of gt-soa/js. Has the most features (PING/PONG, flow control, graceful
   shutdown, pub/sub message types). Some newer Node.js services use this.

4. **scamp-go** (`/Users/daniel/GT/repo/scamp-go/scamp/`) — the WORST
   implementation. It works but is the least featureful and has known
   deficiencies (no PING/PONG, limited flow control). Do NOT treat it as
   authoritative. Use it as a cross-reference only.

**ALL FOUR must be cross-checked.** Where they disagree, the Perl implementation
wins. Where Perl is ambiguous, prefer scamp-js behavior (as the most modern and
featureful). Go is the tiebreaker of last resort.

Also read the protocol documentation:
- `/Users/daniel/GT/repo/gt-soa/MESSAGE.pod` — canonical message header spec
- `/Users/daniel/GT/repo/gt-soa/CONFIG.pod` — configuration spec
- `/Users/daniel/GT/repo/gt-soa/ERRORS` — error handling spec (if it exists)

## Orientation

Read these files IN THIS ORDER:

1. `PUNCHLIST.md` — the master checklist of all work items
2. `prompts/README.md` — overview of all prompt files and their sequencing
3. `prompts/00-cleanup-and-interop-fix.md` through `prompts/05-hardening.md` —
   the six implementation phases
4. `prompts/06-review-protocol-correctness.md` through
   `prompts/08-review-async-architecture.md` — the three review audits
5. `prompts/09-test-unit.md` through `prompts/11-test-fuzz-and-stress.md` —
   the three test suites
6. `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md` —
   detailed gap analysis (NOTE: this doc incorrectly treats scamp-go as the
   primary reference. Cross-check its claims against the Perl implementation.)

Then read the actual scamp-rs source code (it's small — read every file):
7. All `.rs` files under `src/`

Then read the reference implementations:
8. **gt-soa/perl** — read ALL the `.pm` files listed above. This is your ground truth.
9. **gt-soa/js** — read the key files in `js/lib/`
10. **scamp-js** — read `lib/transport/beepish/connection.js`, `lib/actor/service.js`,
    `lib/actor/requester.js`, `lib/util/ticket.js`
11. **scamp-go** — skim `scamp/` directory for cross-reference only

## Phase 1: Critical Review

Before writing any code, do a thorough critical review:

- **Perl cross-check**: The existing punchlist and prompts were written with
  scamp-go as the primary reference. Go through each prompt and verify the
  described behavior against the Perl implementation. Where they conflict,
  the prompts are WRONG and must be corrected.
- **Are there gaps?** Things the Perl implementation does that neither the
  punchlist nor the parity analysis mentions?
- **Are there incorrect assumptions?** Especially about wire format, header
  fields, packet semantics, discovery format, ticket format.
- **Phase ordering**: Are there hidden dependencies between items?
- **Cross-implementation differences**: The parity analysis documents Go-vs-JS
  differences but largely ignores Perl. Add Perl to every comparison.

Revise `PUNCHLIST.md` and any prompt files as needed. Commit revisions separately
with a clear message about what changed and why.

## Phase 2: Implementation

After review, begin implementing starting with Phase 0 (cleanup + interop fix).
Follow the prompts in order. After each phase:

1. Ensure `cargo build` succeeds
2. Ensure `cargo test` passes
3. Ensure `cargo clippy` is clean (warnings OK initially)
4. Commit with a descriptive message
5. Update `PUNCHLIST.md` — check off completed items

Move at whatever pace lets you be thorough. It's better to get Phase 0 and
Phase 1 right than to rush through all phases with bugs.

**Wire compatibility with Perl is the #1 priority.** Every packet you generate
must be parseable by the Perl `BEEPish::Connection` module. Every packet the
Perl server sends must be parseable by your code. Test this empirically — the
dev environment has running Perl services (check with `gud dev status -g`).

## Key Context

- CRITICAL: The `BEEP\r\n` handshake in `client.rs` is a Rust-only invention
  that breaks interop. Remove it first thing.
- The dev environment is running. Real Perl/Go/JS services are available for
  integration testing. The `main` service (gt-main-service) is the biggest
  Perl service and the most important interop target.
- The discovery cache file can be found via the config at the path specified by
  `discovery.cache_path`. This file contains real announcements from all running
  services — invaluable for testing parsing correctness.
- The `soabridge` service uses scamp-go — useful for Go interop testing but
  remember Go is the least reliable reference.
