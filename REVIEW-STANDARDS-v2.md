# Standards Compliance Review v2

Review date: 2026-04-13
Reviewer: Automated standards audit against CODING_STANDARDS.md
Previous review: REVIEW-STANDARDS.md (same date, pre-fixes)

## Summary

The codebase has improved since the v1 review. 3 of 4 violations were fixed (thiserror removed, PUNCHLIST.md updated, DEFICIENCIES.md duplicates removed). However, the 300-line limit violations have **worsened** (2 files in v1 -> 4 files now), and several v1 violations remain open. Documentation accuracy has improved but is still not fully correct.

All 39 source files reviewed against all 10 documented standards.

---

## Status of v1 Violations

### V1-1: 300-Line Limit — PARTIALLY FIXED, NEW VIOLATIONS

**v1 flagged**: `connection.rs` (362 lines) and `server_connection.rs` (447 lines) for inline tests pushing them over 300.

**Current status**:

| File | Total Lines | Prod Lines | Inline Test Lines | Status |
|------|-------------|------------|-------------------|--------|
| `src/transport/beepish/client/connection.rs` | 497 | ~372 | ~125 | **OPEN** (worsened: was 362) |
| `src/service/server_connection.rs` | 500 | ~353 | ~147 | **OPEN** (worsened: was 447) |
| `src/discovery/service_registry.rs` | 322 | 322 | 0 | **NEW VIOLATION** (no inline tests, pure prod code over 300) |
| `src/bin/scamp/list.rs` | 305 | 305 | 0 | **NEW VIOLATION** (no inline tests, pure prod code over 300) |
| `src/transport/beepish/proto/tests.rs` | 438 | 0 | 438 | PASS (separate test file, exempt per standard) |
| `src/config.rs` | 296 | ~252 | ~44 | PASS (under 300) |
| `src/service/announce.rs` | 299 | ~198 | ~101 | PASS (under 300) |

**Remediation needed**:
- `connection.rs` and `server_connection.rs`: Extract inline tests to separate `tests.rs` files (as recommended in v1).
- `service_registry.rs`: 322 lines of production code with no tests. Split out helper functions (e.g., `pick_healthy`, `mark_failed`, failure state tracking) into a separate file, or split into `registry.rs` + `failure_tracking.rs`.
- `list.rs`: 305 lines. The three `list_*` methods could be split into individual files or a helper module.

### V1-2: Logic in `service_info/mod.rs` — OPEN

**v1 flagged**: `discovery/service_info/mod.rs` has 191 lines including `AnnouncementBody::parse()` (37 lines), `Flag::parse_str()` (15 lines), `CrudOp::parse_str()` (10 lines).

**Current status**: The file is now 243 lines (up from 191). It still contains:
- `AnnouncementBody::parse()` — lines 138-212 (74 lines of parsing logic)
- `CrudOp::parse_str()` — lines 215-224
- `Flag::parse_str()` — lines 227-242
- `ServiceInfoParseError` enum + Display impl — lines 95-136
- Type definitions — lines 13-93

The standard says "No logic in mod.rs." While the file is under 300 lines, it contains substantial parsing and matching logic beyond pure re-exports and type definitions.

**Remediation**: Move `AnnouncementBody::parse()`, `CrudOp::parse_str()`, `Flag::parse_str()`, and `ServiceInfoParseError` into `parse.rs` (which already exists). Keep only struct/enum definitions in `mod.rs`.

### V1-3: Remove `thiserror` from Cargo.toml — FIXED

`thiserror` is no longer in `Cargo.toml`. Confirmed: only `anyhow` is used for error handling.

### V1-4: Update PUNCHLIST.md — FIXED

All M4-M9 items are now checked off as `[x]`. The header still says "32 remaining items after M1-M3 + audit fixes" which is stale context, but the actual checklist items are correct.

### V1-5: Clean up DEFICIENCIES.md — FIXED

The resolved table no longer has duplicate rows. The 46 unique resolved items are listed cleanly. The "All 46 Deficiencies Resolved" claim matches the table.

Note: Counting the table rows, there are actually 40 unique resolved items (D1-D32 partial, plus Q1-Q5, T1-T10 partial, plus BUG). The "46" number appears to be approximate. This is a minor documentation inaccuracy, not a code issue.

### V1-6: Update HANDOFF.md stale reference — OPEN

**v1 flagged**: HANDOFF.md says "16 remaining gaps (30 resolved)" which contradicts "All 46 deficiency items resolved."

**Current status**: Line 36 of HANDOFF.md still reads:
```
- `DEFICIENCIES.md` -- 16 remaining gaps (30 resolved) from 6-agent audit
```
This is stale. All deficiencies are resolved.

Additionally, HANDOFF.md line 48 says:
```
**M7: Config & Behavioral Parity** -- (mostly -- bus_info/interface resolution remains)
```
But bus_info is implemented (M7 is complete in PUNCHLIST.md). This is also stale.

HANDOFF.md lines 91-94 list "Potential future work" items that include things already done:
- "T2: Client request sending tests" — done (connection.rs has 4 client tests)
- "T4: authorized_services tests through load()" — done (T4 in DEFICIENCIES.md resolved)
- "T10: RLE decode edge cases" — done (service_info/tests.rs has 6 unrle tests)

### V1-7: Move `announce_raw()` out of `service/mod.rs` — OPEN (nice-to-have)

`service/mod.rs` still contains the `announce_raw()` function (14 lines of wrapper logic, lines 17-30). The standard says mod.rs should be "just `pub mod` declarations and `pub use` re-exports. No logic in mod.rs."

This is a minor violation — the function is a thin wrapper. But it does contain argument forwarding logic.

---

## New Violations Found

### N1: `service_registry.rs` exceeds 300 lines

322 lines of pure production code with no inline tests. This file has grown since v1 (previously not flagged because it was under 300).

Contains: `ServiceRegistry` struct, 10 methods, `ServiceFailureState` struct, `ActionEntry` struct + impl, plus 3 free functions. The failure-tracking logic (`mark_failed`, `pick_healthy`, `is_failed`, `ServiceFailureState`) could be extracted.

### N2: `list.rs` exceeds 300 lines

305 lines. Contains three large methods (`list_actions`: 80 lines, `list_services`: 97 lines, `list_sectors`: 53 lines). Being a CLI binary file makes this less critical than library code, but the standard applies to all files.

### N3: `connection.rs` inline tests grew significantly

Was 362 lines in v1, now 497 lines. The production code grew (flow control logic, additional error handling) and tests were added. This needs immediate attention.

### N4: `server_connection.rs` inline tests grew significantly

Was 447 lines in v1, now 500 lines. Additional tests were added (7 test functions). This needs immediate attention.

---

## Passing Standards (unchanged from v1)

### No Split Impl Blocks: PASS

No struct has its `impl` block split across files.

### Test Organization: PASS (with notes)

Separate test files exist for `proto/tests.rs`, `service_info/tests.rs`, `test_helpers.rs`. Many files still use inline `#[cfg(test)]` tests (the standard prefers separate files but this is advisory for small test blocks).

All 5 `#[ignore]` tests have explanatory comments.

### Naming: PASS

Types PascalCase, functions snake_case, constants SCREAMING_SNAKE, modules snake_case, SCAMP terminology used correctly.

### Error Handling: PASS (minor notes unchanged)

Public APIs return `Result`. The same minor `unwrap()` calls on infallible operations exist as noted in v1 (SHA1 hash, constant parsing, SystemTime). No new panics introduced.

### Dependencies: PASS

18 dependencies (was 19 — thiserror removed). All justified. `homedir` and `dotenv` remain as minor concerns (each used in one place) but are not violations.

### Wire Compatibility: PASS

Perl file:line references throughout protocol-critical code. No changes since v1.

### Comments: PASS

Why-not-what comments. Perl references. No orphaned TODOs.

### Commits: NOTED

Recent commits visible from git status: "Add completion punchlist and agent prompts for full scamp-rs implementation", "WIP" (x2), "request subcommand works with mock client", "stub out subcommand". The standard asks for punchlist references and explanatory bodies. The "WIP" commits remain from before. Not actionable retroactively, but future commits should follow the standard.

---

## Documentation Accuracy

### PUNCHLIST.md — MOSTLY ACCURATE

**Fixed since v1**: M6-M9 items all checked off.

**Remaining issues**:
- Header still says "32 remaining items after M1-M3 + audit fixes" — stale context but not harmful since all items are visibly checked.
- M1-M3 milestones still show `[ ]` on individual sub-items (lines 51-103), though the "Completed Work" section at top correctly marks them `[x]`. The unchecked sub-items within each milestone section are cosmetic — the milestone-level checkboxes in "Completed Work" are correct.
- Two items in M6 and M9 remain `[ ]` as intentionally incomplete future work:
  - M6: `W-12` Busy flag (not implemented, correctly unchecked)
  - M9: "Typed error enum" and "Connection reconnection with backoff" (not implemented, correctly unchecked)

### DEFICIENCIES.md — ACCURATE

No duplicate rows. Resolved table is clean. "All 46 Deficiencies Resolved" stated (actual unique count is ~40 but the numbering includes sub-items like D5b, T1-T10, Q1-Q5, BUG which could reasonably total 46 when counting the original audit items).

### HANDOFF.md — STALE in 3 places

1. Line 36: "16 remaining gaps (30 resolved)" should say "All deficiency items resolved"
2. Line 48: M7 "(mostly -- bus_info/interface resolution remains)" should say M7 is complete
3. Lines 91-94: Lists T2, T4, T10 as "potential future work" but all three are implemented

---

## Summary of Action Items

### Must Fix (Standard Violations)

| # | Issue | File(s) | Lines | Severity |
|---|-------|---------|-------|----------|
| 1 | Extract inline tests to separate files | `connection.rs` (497 lines), `server_connection.rs` (500 lines) | n/a | High — 166% and 167% of limit |
| 2 | Split production code | `service_registry.rs` (322 lines) | n/a | Medium — 7% over limit |
| 3 | Split production code | `list.rs` (305 lines) | n/a | Low — 2% over limit |
| 4 | Move logic out of mod.rs | `service_info/mod.rs` (243 lines, substantial parsing logic) | n/a | Medium |

### Should Fix (Documentation)

| # | Issue | File |
|---|-------|------|
| 5 | Fix stale "16 remaining gaps" reference | HANDOFF.md:36 |
| 6 | Fix stale M7 "bus_info remains" note | HANDOFF.md:48 |
| 7 | Remove completed items from "future work" | HANDOFF.md:91-94 |

### Nice to Have

| # | Issue |
|---|-------|
| 8 | Move `announce_raw()` out of `service/mod.rs` |
| 9 | Update PUNCHLIST.md header to remove stale "32 remaining" text |
| 10 | Future commits should reference punchlist/deficiency IDs |

---

## Comparison: v1 vs v2

| Item | v1 Status | v2 Status |
|------|-----------|-----------|
| `thiserror` unused dep | VIOLATION | **FIXED** |
| PUNCHLIST.md M6-M9 unchecked | VIOLATION | **FIXED** |
| DEFICIENCIES.md duplicates | VIOLATION | **FIXED** |
| `connection.rs` over 300 lines | VIOLATION (362) | **OPEN** (497, worsened) |
| `server_connection.rs` over 300 lines | VIOLATION (447) | **OPEN** (500, worsened) |
| `service_info/mod.rs` logic | VIOLATION (191 lines) | **OPEN** (243 lines, worsened) |
| `service/mod.rs` announce_raw() | Nice-to-have | **OPEN** (unchanged) |
| HANDOFF.md stale references | Should-fix | **OPEN** (partially stale) |
| `service_registry.rs` over 300 lines | Not flagged | **NEW VIOLATION** (322) |
| `list.rs` over 300 lines | Not flagged | **NEW VIOLATION** (305) |
| CI: CircleCI | n/a | **Replaced with GitHub Actions** |
