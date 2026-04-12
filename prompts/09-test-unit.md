# Prompt: Write Unit Tests for scamp-rs

## Context

You are writing comprehensive unit tests for scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`. The tests should verify correctness of individual modules without requiring network connections or external services.

Reference implementations for expected behavior:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/`

## Test Suites to Write

### 1. Packet framing (`tests/packet_test.rs` or in-module tests)

- **Roundtrip**: Create each packet type (HEADER, DATA, EOF, TXERR, ACK, PING, PONG), write to bytes, parse back, assert equal
- **HEADER with full header**: All PacketHeader fields populated, serialize to bytes, parse back
- **DATA with varying sizes**: Empty body, small body, exactly 131072 bytes, larger than 131072 bytes
- **ACK body format**: Verify the byte count is formatted correctly (compare with Go/JS)
- **Malformed packets**: Missing `END\r\n`, truncated header line, negative body size, unknown packet type
- **Zero-length packets**: EOF, ACK with body="0", PING, PONG

### 2. PacketHeader JSON serialization

- **Roundtrip**: Create PacketHeader, serialize to JSON, parse back, assert equal
- **Wire compatibility**: Serialize to JSON, compare with expected JSON string matching Go/JS format
  - Verify field names: `action`, `envelope`, `version`, `request_id`, `client_id`, `ticket`, `identifying_token`, `type`, `error`, `error_code`
  - Verify `envelope` → `"json"` (lowercase string)
  - Verify `type` → `"request"` / `"reply"` (lowercase string)
  - Verify optional fields are omitted when None (or null — match Go behavior)
- **Go compatibility**: Take a JSON string that Go would produce (construct by reading Go source), deserialize in Rust, verify all fields
- **JS compatibility**: Same for a JS-produced JSON string
- **FlexInt**: Deserialize `{"client_id": 42}` and `{"client_id": "42"}` — both should produce 42

### 3. Config parsing

- **Full soa.conf**: Parse a realistic soa.conf (use `/Users/daniel/GT/repo/scamp-rs/samples/soa.conf` if it exists, or write a test fixture)
- **Nested keys**: `discovery.cache_path`, `beepish.first_port`, etc.
- **Missing keys**: Verify graceful behavior when optional keys are absent
- **Path rewriting**: Verify `~` expansion and home directory fallback
- **Multiple config sources**: Verify precedence when env var and file both exist

### 4. Discovery: announcement parsing

- **v3 body**: Parse a v3 announcement JSON array, verify all fields extracted correctly
- **v4 body with RLE**: Parse a v4 announcement with RLE-encoded actions, verify expansion
- **Cache file iteration**: Parse a multi-record cache file with `%%%` delimiters, verify correct number of records
- **Real cache file**: If `discovery.cache_path` is configured and the file exists, parse it and verify no errors. This is the most valuable test.
- **Malformed announcements**: Invalid JSON, missing fields, wrong version number

### 5. Service registry

- **Action lookup**: Register multiple services with overlapping actions, verify correct lookup by name+version
- **Random selection**: Look up an action with multiple providers, verify random distribution (statistical test over many calls)
- **Empty registry**: Lookup returns None
- **Sector filtering** (once implemented): Verify actions in different sectors are isolated

### 6. Ticket parsing (once implemented)

- **Valid ticket**: Parse a known-good ticket string, verify all fields
- **Signature verification**: Verify against known public key
- **Expired ticket**: Verify `is_expired()` returns true for old timestamps
- **Privilege checking**: Verify `has_privilege()` for present and absent privileges
- **Malformed tickets**: Missing fields, extra commas, invalid base64 signature

### 7. Crypto (once implemented)

- **RSA SHA256 verify**: Test with known (message, signature, public_key) triple
- **SHA1 fingerprint**: Compute fingerprint of a known cert, compare with expected value
- **Negative verification**: Tampered message should fail verification

## Guidelines

- Use `#[test]` for sync tests, `#[tokio::test]` for async tests
- Use `assert_eq!` with descriptive messages
- Create test fixtures as constants or lazy_static for real-world data
- Group related tests in modules (`#[cfg(test)] mod tests { ... }`)
- If you need test data from the running environment (cache files, certs), use `#[ignore]` and document how to run them

## Output

Write tests as described above. Ensure `cargo test` passes for all non-ignored tests. Report the count of tests written and any issues found during test development (bugs in the implementation that the tests revealed).
