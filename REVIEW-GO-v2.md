# Go Parity Review v2

Updated 2026-04-13. Re-reviewed after Auth.getAuthzTable, error_data, and clippy fixes.

## Summary

Previous findings confirmed: scamp-rs covers every feature present in scamp-go,
plus 17+ features Go lacks entirely. No new Go features have appeared.

## Changes Since v1

Three additions to Rust since the last review:

1. **`error_data` field on PacketHeader** -- `Option<serde_json::Value>` with
   `skip_serializing_if`. Used for `dispatch_failure` detection in the Requester
   retry path. Go has no equivalent field and no dispatch retry logic.

2. **`Auth.getAuthzTable` integration** (`auth/authz.rs`) -- `AuthzChecker`
   fetches the authz table from the Auth service, caches for 5 minutes, and
   verifies ticket privileges per-action before dispatch. Go's `ActionOptions`
   only does local ticket verification; it never fetches the authz table.

3. **Clippy compliance** -- various `#[allow(dead_code)]`, `clippy::inherent_to_string`
   annotations. No semantic changes.

All three widen the gap: Rust now has runtime authz table checking that Go lacks.

## New Divergences Found

None. The original 17-item list of "features Rust has that Go lacks" remains
accurate and unchanged. No new Go functionality has been added.

### Minor observation (not a divergence)

The Rust `Requester::dispatch_once` checks for `dispatch_failure` in both
`error_data.dispatch_failure` (JS style) and `error_code == "dispatch_failure"`
(belt-and-suspenders). Go has no dispatch retry at all -- its `MakeJSONRequest`
retries by polling the response channel, not by re-dispatching to a different
service instance.

## Previous Findings Status

All confirmed to still hold:

| Finding | Status |
|---|---|
| Rust exceeds Go on every axis | Confirmed |
| No critical gaps where Go has features Rust lacks | Confirmed |
| Go's 128KB DATA chunk size is non-standard | Confirmed (Go still uses `msgChunkSize = 128 * 1024`) |
| Go's `json.NewEncoder` trailing newline in HEADER | Confirmed (Go `packetheader.go:138-143` still uses `Encode`) |
| Go's ACK handling is a stub (TODO comment) | Confirmed (Go `connection.go:230-233` unchanged) |
| Go's config duplicate-key behavior diverges from Perl | Confirmed (Go `config.go:112-114` still overwrites) |
| Go's authorized_services parsing never used for filtering | Confirmed (Go `serviceproxy.go:352-363` has TODO, no integration) |

## Go Test Patterns: E2E Harness Recommendations

The Go test suite is weak -- most tests are commented out, stubbed, or require
external infrastructure. However, two patterns are worth noting:

1. **Serialization round-trip with known fixtures** (`serviceproxy_test.go`):
   Go tests `ServiceProxySerialize` by building a `serviceProxy` struct with
   known values, marshaling to JSON, and comparing byte-for-byte against a
   captured fixture. Rust already does this better (Perl wire fixtures in
   `proto/fixtures.rs`), but the Go test includes the *full* announce format
   (9-element array with timestamp). If the Rust E2E harness needs to test
   announce packet interop with Go services, capturing a Go-produced announce
   packet as a fixture would be useful.

2. **Fingerprint verification against known certs** (`cert_test.go`): Go
   hardcodes a certificate and its expected fingerprint. Rust already has this
   (`crypto.rs::test_fingerprint_of_dev_cert`), but the Go test uses a
   self-contained fixture cert rather than requiring the dev environment. Adding
   a non-ignored Rust fingerprint test with an embedded cert would strengthen CI.

**Overall recommendation**: The Go tests offer nothing that the Rust test suite
doesn't already cover more thoroughly. The Rust suite has 80 tests including
wire fixture parsing, RLE decode edge cases, authorized services pattern
matching, flow control, multi-chunk roundtrips, and PING/PONG -- none of which
Go tests at all. No Go test patterns need to be adopted.

## Conclusion

No action needed. The Rust implementation continues to exceed Go on every axis.
The three additions since v1 (error_data, AuthzChecker, clippy) all widen the
gap further.
