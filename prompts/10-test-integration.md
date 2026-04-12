# Prompt: Write Integration Tests for scamp-rs

## Context

You are writing integration tests for scamp-rs at `/Users/daniel/GT/repo/scamp-rs/` that verify interoperability with the production Go and JS implementations. These tests require the RetailOps dev environment to be running (check with `gud dev status -g`).

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/`

The dev environment runs in Docker. Key services:
- `main` — gt-main-service (Perl, via scamp)
- `api` — gt-dispatcher (edge node)
- `soabridge` — scamp-go bridge
- `cache` — cache manager + SOA discovery coordinator

The discovery cache file location is in the soa.conf config. You can find soa.conf at the paths checked by scamp-rs's config module (try `~/GT/backplane/etc/soa.conf` or check the `SCAMP_CONFIG` env var).

## Test Categories

### T7: Rust client → Go service

**Setup**: The `soabridge` service uses scamp-go. Find an action it exposes by reading the discovery cache.

**Test**:
1. Parse the discovery cache to find a scamp-go service and one of its actions
2. Create a Rust `ScampClient`
3. Send a request to the Go service
4. Verify a valid response is received
5. Verify the response body is valid JSON

If no Go service is available, docker exec into the soabridge container and check what actions it announces. Alternatively, write a small Go test service using scamp-go's API.

### T8: Rust client → JS service

**Setup**: If a scamp-js service is running (check for `gt-media` or `channel-modules-node`), use it. Otherwise, start a minimal JS service:

```javascript
var scamp = require('/path/to/scamp-js');
var svc = scamp.service({ tag: 'test-js' });
svc.registerAction('Test.echo', svc.cookedHandler(function(header, data) {
    return { echo: data };
}));
svc.run();
```

**Test**:
1. Send a request to the JS service from Rust
2. Verify response matches expected output
3. Test with various payload sizes (empty, small, large)

### T9: Go client → Rust service

**Setup**: Start a Rust `ScampService` with a test action.

**Test**:
1. Start the Rust service on a local port
2. Verify it appears in the discovery cache (after announcement cycle)
3. From Go (or via a script that uses scamp-go), send a request to the Rust service
4. Verify the Rust handler receives the request and the Go client receives the response

If scripting a Go client is impractical, use the `scamp` CLI tool (if scamp-go has one) or use `scamp-rs`'s own CLI to make the request after verifying the Rust service is discoverable.

### T10: JS client → Rust service

Same as T9 but using scamp-js as the client. Can use a simple Node.js script:

```javascript
var scamp = require('/path/to/scamp-js');
var requester = scamp.requester();
requester.makeJsonRequest('main', 'TestRust.echo', 1, { hello: 'world' }, function(err, data) {
    console.log(err, data);
});
```

### T11: Connection multiplexing

**Test**: Send 10 concurrent requests on a single connection and verify all get correct responses. Use `tokio::join!` or `futures::join_all`.

Verify:
- All 10 responses arrive
- Each response matches its request (no cross-contamination)
- Message numbers are sequential and non-overlapping

### T12: Flow control under load

**Test**: Send a request with a very large body (>1MB) and verify:
- Body is correctly chunked into 131072-byte packets
- ACK packets are received
- The complete response is received without corruption

### T13: Heartbeat

**Test**: Connect to a JS service (which supports PING/PONG) with heartbeat enabled. Verify:
- PING packets are sent on the configured interval
- PONG responses are received
- Connection stays alive over a >30 second period

Also verify: Connect to a Go service WITHOUT sending PING (since Go doesn't support it). Verify the connection works normally.

### T14: Announcement cycle

**Test**:
1. Start a Rust service
2. Wait for at least one announcement cycle (>5 seconds)
3. Read the discovery cache file
4. Verify the Rust service appears with correct identity, address, and action list
5. Stop the Rust service
6. Wait for expiry period
7. Verify the Rust service is no longer in the cache (or marked stale)

### T15: Graceful shutdown

**Test**:
1. Start a Rust service
2. Send a long-running request (handler sleeps for 5 seconds)
3. While the request is in-flight, trigger shutdown
4. Verify the in-flight request completes (response received)
5. Verify the service stops accepting new connections after shutdown begins
6. Verify a weight=0 announcement was sent

### T16: Real discovery cache

**Test**: Parse the actual production discovery cache file from the running dev environment.
- Verify all records parse without errors
- Print summary: number of services, number of actions, sectors found
- Verify at least the expected services are present (main, auth, cache, soabridge, etc.)

### T17: Wire capture comparison

**Test**: For the same request (same action, same payload), capture the bytes sent by:
1. scamp-rs
2. scamp-go (if possible, via test harness)

Compare the packet structures. They don't need to be byte-identical (JSON field ordering may differ, request_id will differ), but the packet framing (type, msgno, bodysize, trailer) should match exactly.

This can be done by writing packets to a `Vec<u8>` in both implementations and comparing structurally.

## Guidelines

- Place integration tests in `tests/integration/` directory
- Use `#[ignore]` for tests that require the dev environment (run with `cargo test -- --ignored`)
- Add a README in `tests/integration/` explaining how to run them
- Each test should be independent (no shared state between tests)
- Use `tokio::time::timeout` on every test to prevent hangs (30 second max per test)
- Print diagnostics on failure: what was sent, what was received, what was expected

## Output

Write integration tests as described. Document which tests pass, which fail (with root cause), and any interoperability issues discovered. Write findings to `/Users/daniel/GT/repo/scamp-rs/INTEGRATION-TEST-RESULTS.md`.
