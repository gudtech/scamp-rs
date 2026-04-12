# scamp-rs Coding Standards

## File Size

**No file should exceed 300 lines** (including tests). This is a forcing function
for functional decomposition. If a file is approaching 300 lines, split it.

Tests do NOT count toward the 300-line limit if they are in a separate file
(see Module Structure below).

## Module Structure

Prefer directory-based modules with `mod.rs` for re-exports:

```
src/transport/beepish/
  mod.rs          # pub mod + pub use re-exports only
  proto.rs        # types, parsing, writing (<300 lines)
  client.rs       # client connection, pooling (<300 lines)
  connection.rs   # shared connection logic if needed
  tests.rs        # unit tests (or tests/ dir if large)
```

### Re-exports

`mod.rs` should be thin — just `pub mod` declarations and `pub use` re-exports.
No logic in `mod.rs`.

### Tests

- **Short tests**: put in `submodule/tests.rs` as `#[cfg(test)]` module
- **Many tests**: use `submodule/tests/` directory with one file per test area
- **Integration tests**: use top-level `tests/` directory
- Tests that require the dev environment should be `#[ignore]` with a comment
  explaining how to run them

```rust
// In proto.rs — no inline tests, just the implementation
// In tests.rs:
#[cfg(test)]
mod tests {
    use super::*;
    // ...
}
```

## Naming

- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`
- Match the SCAMP protocol terminology where possible (e.g., `PacketHeader` not
  `MessageHeader`, `msg_no` not `message_number`)

## Error Handling

- Public APIs return `Result<T, ScampError>` (once the typed error enum exists)
  or `anyhow::Result<T>` for now
- Internal helpers can use `anyhow` freely
- Log errors at the point of detection with `log::error!` / `log::warn!`
- Don't panic in library code — return errors

## Dependencies

- Prefer the `openssl` crate for crypto (already linked via `native-tls`)
- Prefer `tokio` primitives for async (channels, timers, tasks)
- Don't add a dependency for something achievable in <20 lines of code
- Keep `Cargo.toml` tidy — remove unused deps promptly

## Wire Compatibility

This is the #1 priority. Every decision should be evaluated against:
"Will this produce/parse bytes identically to the Perl implementation?"

- When in doubt, match Perl behavior exactly
- Document deviations from Perl with a comment citing the Perl file:line
- Cross-reference at least one other implementation (JS or Go) for confirmation

## Comments

- Comment **why**, not **what**
- Reference Perl implementation file:line for protocol-critical code:
  ```rust
  // Perl Connection.pm:162 — EOF body must be empty
  ```
- Don't leave TODO comments without a DEFICIENCIES.md or PUNCHLIST.md reference

## Commits

- One logical change per commit
- Commit message: first line is a short summary, body explains **why**
- Reference punchlist items (e.g., "M4:", "D5:") in commit messages
- Run `cargo test` before every commit
