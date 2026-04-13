# E2E Test Plan for scamp-rs

## Goals

1. **Rust-to-Rust E2E**: Prove the full stack works — TLS, discovery, announce,
   request/response — with zero external dependencies. Runs in `cargo test` and GH Actions.
2. **JS interop**: Verify wire compatibility against scamp-js (lightweight, 1 npm dep).
3. **Perl interop**: Verify against the canonical Perl implementation.

## Phase 1: Rust-to-Rust E2E (priority — do first)

### What It Proves

A Rust service can announce itself, be discovered via a cache file, and respond
to requests from a Rust client — the complete SCAMP lifecycle without any
external services.

### Architecture

```
generate_test_keypair()       ← self-signed cert at test time (openssl crate)
       │
ScampService::new()           ← register echo handler
       │
service.bind_pem()            ← TLS listener on 127.0.0.1, random port
       │
service.build_announcement_packet()  ← signed announcement blob
       │
write cache file (tempfile)   ← announcement + %%%\n delimiter
write auth file (tempfile)    ← fingerprint + main:ALL
       │
ServiceRegistry::new_from_cache()  ← load from synthetic cache
       │
registry.find_action()        ← verify action is discoverable
       │
tokio::spawn(service.run())   ← start accepting connections
       │
BeepishClient::request()      ← TLS connect, fingerprint verify, send, receive
       │
assert body == request body   ← prove the full round trip works
       │
shutdown_tx.send(true)        ← graceful shutdown
```

### Dependencies to Add

```toml
[dev-dependencies]
tempfile = "3"
```

The `openssl` crate (for cert generation) is already a regular dependency.

### Test File

Create `tests/e2e_full_stack.rs` as a cargo integration test.

### Self-Signed Certificate Generation

Use the `openssl` crate (already in deps) to generate RSA 2048 + X509 at test time:

```rust
fn generate_test_keypair() -> (Vec<u8>, Vec<u8>) {
    use openssl::rsa::Rsa;
    use openssl::x509::{X509Builder, X509NameBuilder};
    use openssl::pkey::PKey;
    use openssl::hash::MessageDigest;
    use openssl::asn1::Asn1Time;

    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();

    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "scamp-test").unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(1).unwrap()).unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();

    let cert_pem = builder.build().to_pem().unwrap();
    let key_pem = pkey.private_key_to_pem_pkcs8().unwrap();
    (key_pem, cert_pem)
}
```

### Synthetic Cache File

Build the announcement from the running service, write to a temp file:

```rust
let announcement_bytes = service.build_announcement_packet(true)?;
let announcement_str = String::from_utf8(announcement_bytes)?;

let mut cache_file = NamedTempFile::new()?;
write!(cache_file, "{}\n%%%\n", announcement_str)?;
```

### Synthetic Authorized Services File

```rust
let fingerprint = cert_pem_fingerprint(&cert_pem_str)?;
let mut auth_file = NamedTempFile::new()?;
writeln!(auth_file, "{} main:ALL", fingerprint)?;
```

### Minimal Config

The test needs a Config with `discovery.cache_path` and `bus.authorized_services`
pointing to the temp files. Options:
- Write a temp soa.conf file and use `Config::new(Some(path))`
- Or add a `Config::from_str()` / `Config::builder()` for tests

### Test Scenarios (Phase 1)

| # | Test | What It Proves |
|---|------|---------------|
| 1 | Echo roundtrip | Full stack: TLS + discovery + request + response |
| 2 | Large body (> 2048 bytes) | DATA chunking + reassembly through TLS |
| 3 | Unknown action | Error response propagation |
| 4 | Multiple sequential requests | Connection reuse, sequential msgno |
| 5 | Concurrent connections | Server handles multiple clients |
| 6 | Announcement signature verification | Parsed announcement has valid RSA signature |
| 7 | Authorized services filtering | Unauthorized action is rejected at lookup |

### API Gaps to Fix Before Phase 1

These are small fixes needed to make the E2E test work:

1. **Config from string/map**: Need a way to create Config in tests without a
   real file. Either `Config::from_str(content)` or a builder pattern.

2. **Public exports**: Some types used by the test may need `pub` visibility:
   - `AnnouncementPacket::parse` — already pub
   - `cert_pem_fingerprint` — already pub
   - `ServiceRegistry::new_from_cache` — already pub
   - `AuthorizedServices` — only used indirectly via config

### GH Actions Integration

No changes needed to `.github/workflows/ci.yml` — the E2E test runs as part of
`cargo test`. The `tempfile` dev-dependency is downloaded automatically.

---

## Phase 2: JS Interop (via scamp-js)

### Why JS First

scamp-js has minimal dependencies (1 runtime dep: `argparser`). It's easy to
containerize and provides a high-confidence interop check since JS is the most
featureful implementation (PING/PONG, flow control, dispatch_failure, etc.).

### Architecture

```
docker-compose.yml:
  rust-service:    # scamp-rs serve (echo handler)
  js-client:       # node script that calls Rust service
  js-service:      # scamp-js service (echo handler)
  rust-client:     # scamp-rs request to JS service
```

### Test Scenarios (Phase 2)

| # | Test | What It Proves |
|---|------|---------------|
| 1 | JS client → Rust service | Rust correctly handles JS wire format |
| 2 | Rust client → JS service | Rust correctly produces JS-compatible wire format |
| 3 | Large body both directions | Chunking interop |
| 4 | error_data propagation | Structured error metadata works cross-impl |
| 5 | Ticket in request header | Null ticket handling (Perl sends null) |

### What's Needed

- Dockerfile for scamp-js service (node:lts + npm install)
- Shared TLS certs (generated at compose-up time)
- Shared discovery cache file (written by one side, read by the other)
- docker-compose.yml with shared network
- GH Actions workflow step that runs `docker compose up --exit-code-from test-runner`

---

## Phase 3: Perl Interop

### Challenges

Perl GTSOA has dependencies on the full backplane (config, certs, cache service,
auth service). Standing up a minimal Perl test environment is heavier than JS.

### Options

**Option A: Minimal Perl container**
- Install only GTSOA::Transport::BEEPish and dependencies
- Write a small Perl script that connects to a Rust service and sends a request
- Avoids needing the full backplane

**Option B: Use existing dev environment**
- `gud dev` containers already run the full stack
- Run interop tests as `#[ignore]` tests that require the dev environment
- Not suitable for GH Actions but validates against production Perl

**Option C: Perl in the Rust E2E test**
- Spawn a Perl process from the Rust test
- Requires Perl + GTSOA installed in the CI environment
- Most complex but most thorough

### Recommendation

Start with Option B (use existing dev environment for local verification).
If CI coverage is needed, move to Option A (minimal Perl container).

---

## Test Backplane Architecture (Future)

Eventually, for full CI coverage including multicast:

```
┌─────────────┐    multicast     ┌─────────────┐
│ Rust Service │ ──────────────→ │ Rust Cache   │
│ (announcer)  │    UDP 5555     │ Writer       │
└──────┬───────┘                 └──────┬───────┘
       │ TLS                            │ writes
       │                                ▼
┌──────┴───────┐              ┌─────────────────┐
│ Rust Client  │ ←── reads ── │ discovery cache  │
│ (requester)  │              │ file             │
└──────────────┘              └─────────────────┘
```

This requires porting the cache writer to Rust (a small daemon that listens on
multicast, decompresses announcements, writes the `\n%%%\n`-delimited cache
file). This is a separate project but would make scamp-rs fully self-hosting.

### Minimal Rust Cache Writer

The pieces already exist:
- `discovery::observer::run_observer()` receives multicast announcements
- `discovery::packet::AnnouncementPacket::parse()` parses them
- Cache file format is simple: `announcement\n%%%\n` repeated

The cache writer would be ~50 lines: listen on multicast, accumulate
announcements, periodically write the cache file. This could live in
`src/bin/scamp/cache_writer.rs` or as a separate binary.
