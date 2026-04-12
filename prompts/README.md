# scamp-rs Agent Prompts

These prompt files are designed to be fed to AI coding agents (one at a time, in order) to implement, review, and test scamp-rs.

## Implementation (sequential — each depends on the previous)

| File | Phase | Description |
|------|-------|-------------|
| [00-cleanup-and-interop-fix.md](00-cleanup-and-interop-fix.md) | Phase 0 | Remove BEEP handshake, delete dead code, clean deps |
| [01-transport-core.md](01-transport-core.md) | Phase 1 | Connection multiplexing, flow control, heartbeat, message assembly |
| [02-service-infrastructure.md](02-service-infrastructure.md) | Phase 2 | TLS listener, action registration, request dispatch |
| [03-security.md](03-security.md) | Phase 3 | Signatures, tickets, authorized_services, TLS migration |
| [04-discovery.md](04-discovery.md) | Phase 4 | Multicast announcing, cache watching, sector routing |
| [05-hardening.md](05-hardening.md) | Phase 5 | Error types, reconnection, graceful shutdown, client API |

## Review (can run after each implementation phase, or all at once after Phase 5)

| File | Focus |
|------|-------|
| [06-review-protocol-correctness.md](06-review-protocol-correctness.md) | Wire-level protocol fidelity vs Go and JS |
| [07-review-security.md](07-review-security.md) | Signature verification, tickets, authorization |
| [08-review-async-architecture.md](08-review-async-architecture.md) | Concurrency, deadlocks, leaks, cancellation safety |

## Testing (can start after Phase 1, expand after each phase)

| File | Scope |
|------|-------|
| [09-test-unit.md](09-test-unit.md) | Unit tests for all modules |
| [10-test-integration.md](10-test-integration.md) | Cross-language interop tests (requires dev environment) |
| [11-test-fuzz-and-stress.md](11-test-fuzz-and-stress.md) | Fuzz parsing, stress connections, property-based tests |

## Getting Started

**Use [AGENT-KICKOFF.md](AGENT-KICKOFF.md) as the entry point.** It contains the
full orientation, review instructions, and implementation plan for an agent taking
ownership of the scamp-rs completion work.

## Reference Implementation Priority

1. **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`) — THE canonical implementation. Perl services are the primary SCAMP consumers. Wire compat with Perl is the #1 priority.
2. **gt-soa/js** (`/Users/daniel/GT/repo/gt-soa/js/lib/`) — original JS impl within gt-soa
3. **scamp-js** (`/Users/daniel/GT/repo/scamp-js/lib/`) — modernized JS fork, most featureful
4. **scamp-go** (`/Users/daniel/GT/repo/scamp-go/scamp/`) — least reliable reference, cross-check only

NOTE: The individual prompt files (00-11) were initially written with scamp-go as the
primary reference. The AGENT-KICKOFF.md corrects this. The agent's first task is to
review and revise the prompts against the Perl implementation.

The punchlist at `../PUNCHLIST.md` tracks overall progress.
