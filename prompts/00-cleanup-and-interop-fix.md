# Prompt: Phase 0 — Dead Code Cleanup + Critical Interop Fix

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`, a Rust implementation of the SCAMP (Single Connection Asynchronous Multiplexing Protocol) used by RetailOps. The codebase has accumulated dead code from earlier iterations and has a critical interoperability bug.

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/` (primary, production)
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/` (secondary, production)

Detailed gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Tasks

### 1. Remove the BEEP handshake (CRITICAL)

In `src/transport/beepish/client.rs`, there is code (~lines 138-148) that sends `BEEP\r\n` on connection open and expects a `BEEP\r\n` response. **Neither scamp-go nor scamp-js implements this handshake.** It was invented in scamp-rs and breaks all interoperability. Remove it completely — the TLS connection should begin directly with SCAMP packet exchange after the TLS handshake completes.

### 2. Remove dead modules

The following modules are commented out in `src/lib.rs` and contain obsolete code (pre-tokio-1.0 APIs, hyper-inspired abstractions that don't match SCAMP):

- `src/message/` (entire directory) — old hyper-style Message/Payload abstractions. All methods are `unimplemented!()`.
- `src/message/message.rs`, `src/message/mod.rs`, `src/message/packet.rs`, `src/message/payload.rs`
- `src/common/` (entire directory) — contains only a `Never` type. Use `std::convert::Infallible` instead.
- `src/common/mod.rs`, `src/common/never.rs`
- `src/error.rs` — old error type, not used by active code
- `src/agent/mod.rs` — old Agent concept, not the right abstraction
- `src/action.rs` — old Action struct with `unimplemented!()` methods
- `src/transport/beepish/tcp.rs` — old hyper-style TCP listener, pre-tokio-1.0

Delete these files entirely. Update `src/lib.rs` to remove the commented-out module declarations.

### 3. Clean up Cargo.toml dependencies

Remove unused dependencies:
- `pnet` — heavy network library, not needed for SCAMP (was probably used for multicast experimentation)
- `net2` — unused optional dependency
- `atty` — deprecated, use `std::io::IsTerminal` (stable since Rust 1.70)

Keep everything else for now. Dependency additions (rustls, ring, base64, notify) happen in later phases.

### 4. Verify the remaining code compiles and tests pass

After cleanup, run `cargo build` and `cargo test` to ensure nothing broke. The active modules should be:
- `src/lib.rs` (exporting config, discovery, transport)
- `src/config.rs`
- `src/discovery/` (mod.rs, packet.rs, cache_file.rs, service_info.rs, service_registry.rs)
- `src/transport.rs`
- `src/transport/beepish.rs`, `src/transport/beepish/proto.rs`, `src/transport/beepish/client.rs`
- `src/transport/mock.rs`
- `src/bin/scamp/` (main.rs, list.rs, request.rs)

## Success Criteria

- [ ] `BEEP\r\n` handshake code removed
- [ ] All dead code files deleted
- [ ] `src/lib.rs` has no commented-out module declarations
- [ ] Unused deps removed from Cargo.toml
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes
- [ ] `cargo clippy` has no errors (warnings OK for now)
