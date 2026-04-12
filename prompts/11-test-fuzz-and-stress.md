# Prompt: Write Fuzz and Stress Tests for scamp-rs

## Context

You are writing adversarial tests for scamp-rs at `/Users/daniel/GT/repo/scamp-rs/` to find bugs that normal unit and integration tests miss. These tests target robustness under malformed input and high load.

## Fuzz Tests

Use `cargo-fuzz` (or `proptest` if fuzz harnesses are impractical). Target the parsing code that handles untrusted input from the network.

### Fuzz 1: Packet parsing

Feed arbitrary bytes to the packet parser (`Packet::parse` or equivalent). It should never panic — only return parse errors.

```rust
// fuzz_targets/packet_parse.rs
fuzz_target!(|data: &[u8]| {
    let _ = Packet::parse(data);
});
```

### Fuzz 2: PacketHeader JSON deserialization

Feed arbitrary JSON to `PacketHeader` deserialization. Should never panic.

```rust
fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<PacketHeader>(data);
});
```

### Fuzz 3: Announcement body parsing

Feed arbitrary bytes/JSON to the announcement body parser. Should never panic.

### Fuzz 4: Config file parsing

Feed arbitrary text to the config parser. Should never panic.

### Fuzz 5: Ticket parsing

Feed arbitrary strings to the ticket parser. Should never panic, never accept invalid tickets.

## Stress Tests

These require a running scamp-rs service (or mock). Use `#[ignore]` and document how to run them.

### Stress 1: Connection storm

Open 100 concurrent connections to a Rust service. Each sends 10 requests. Verify all 1000 responses arrive correctly. No leaks (check fd count before and after).

### Stress 2: Large message throughput

Send 100 messages of 10MB each through a single connection. Verify all arrive intact (checksum). Measure throughput.

### Stress 3: Rapid connect/disconnect

Open a connection, send one request, close immediately. Repeat 1000 times. Verify the service doesn't crash, leak memory, or leak file descriptors.

### Stress 4: Slow client

Connect to a Rust service. Send a HEADER packet, then wait 30 seconds before sending DATA and EOF. Verify the service handles this gracefully (doesn't crash, respects timeout, frees resources).

### Stress 5: Concurrent action dispatch

Register 50 actions on a Rust service. Send requests to all 50 concurrently. Verify correct dispatch (no action gets another's request).

### Stress 6: Discovery cache churn

While a Rust client is making requests, rewrite the discovery cache file rapidly (simulating services coming and going). Verify the client handles registry updates without crashing or routing to stale services.

## Property-Based Tests (proptest)

If `proptest` is preferred over `cargo-fuzz`:

### Property 1: Packet roundtrip

For any valid `Packet`, `Packet::parse(packet.write())` should return the original packet.

```rust
proptest! {
    #[test]
    fn packet_roundtrip(
        packet_type in prop::sample::select(vec![
            PacketType::Header, PacketType::Data, PacketType::Eof,
            PacketType::Txerr, PacketType::Ack, PacketType::Ping, PacketType::Pong,
        ]),
        msg_no in 1u64..1000,
        body in prop::collection::vec(any::<u8>(), 0..1000),
    ) {
        // construct, write, parse, compare
    }
}
```

### Property 2: PacketHeader JSON roundtrip

For any valid `PacketHeader`, `serde_json::from_str(serde_json::to_string(header))` returns the original header.

### Property 3: Message chunking

For any body of length N, splitting into chunks of 131072 and reassembling produces the original body.

## Output

- Write fuzz targets in `fuzz/fuzz_targets/` (if using cargo-fuzz) or as proptest tests in `tests/`
- Write stress tests in `tests/stress/` with `#[ignore]`
- Document how to run them in `tests/README.md`
- Report any bugs found
