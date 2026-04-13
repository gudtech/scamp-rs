# scamp-rs Completion Punchlist

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done

> **Reference priority**: Perl (gt-soa/perl) > scamp-js > gt-soa/js > scamp-go.
>
> Structured around **verification milestones** — each milestone has a dependency
> chain and a concrete test that proves it works. After each milestone, we will
> re-dispatch audit agents (one per reference implementation) to verify parity
> by direct comparison to the canonical implementations.
>
> Deficiency audit: DEFICIENCIES.md (32 remaining items after M1-M3 + audit fixes).
> Verified by 4 agents: Perl, JS, Go, C#. Last audit: 2026-04-12.

---

## Completed Work

- [x] Phase 0: BEEP handshake removed, dead code deleted, deps cleaned
- [x] PacketHeader serde: `"type"` field, lowercase enums, FlexInt
- [x] Message assembly (inbound): HEADER→DATA*→EOF/TXERR, msgno from 0
- [x] Message serialization (outbound): HEADER + DATA chunks (131072) + EOF
- [x] Request-response correlation: sequential request_id from 1, pending map
- [x] ACK sending on DATA receipt (decimal string)
- [x] PING/PONG: disabled by default, responds with PONG
- [x] Connection architecture: mpsc writer, reader task, ConnectionHandle
- [x] TLS server listener, action registration, request dispatch
- [x] Announcement packet generation (v3 JSON, RSA PKCS1v15 SHA256 signing)
- [x] Action index key: `sector:action.vVERSION` with CRUD aliases (`_destroy` not `_delete`)
- [x] Docker build pipeline for x86_64 interop testing
- [x] **M1**: TLS fingerprint verification + natural corking (crypto.rs)
- [x] **M2**: Announcement signature verification against live Perl cache (PKCS1v15 SHA256)
- [x] **M3**: authorized_services parsing, regex matching, registry filtering
- [x] V4 accompat filter enabled (was commented out)
- [x] Interop verified: Rust client → Perl mainapi with fingerprint check
- [x] **M4**: UDP multicast announcing (zlib, v4 extension hash, 76-char base64, periodic send)
- [x] Verified: `lssoa` shows Rust service via normal discovery pipeline
- [x] **M5**: Full bidirectional interop (Perl Requester → Rust service via discovery)
- [x] Fix: null JSON values in PacketHeader (`ticket: null`, `identifying_token: null`)
- [x] Fix: malformed HEADER JSON → Fatal (was Drop); unknown packet types → Fatal (was Drop)

---

## Milestone 1: Secure Client Connection

**Goal**: Rust client connects to Perl services with real TLS fingerprint
verification instead of `danger_accept_invalid_certs`.

**Dependency chain**:
1. [ ] Crypto: SHA1 fingerprint of DER-encoded certificate → uppercase hex colon-separated (`XX:XX:XX:...`). Matches Perl `ServiceInfo.pm:82-87` and Go `cert.go:14-31`.
2. [ ] Store fingerprint + cert_pem in `ServiceInfo` during announcement parsing (in `packet.rs` and `service_info.rs`)
3. [ ] Propagate fingerprint through `ActionEntry` → `BeepishClient.get_connection()`
4. [ ] After TLS handshake: extract peer certificate, compute SHA1 fingerprint, compare against announced fingerprint. Mismatch = close connection with error.
5. [ ] Write corking: buffer outbound packets until fingerprint verification succeeds. Flush on success, destroy on mismatch. (Perl `Connection.pm:33-74`)

**Verification**:
```bash
# Must still work — but now with real fingerprint verification:
docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro \
  -e SCAMP_CONFIG=/backplane/etc/soa.conf \
  scamp-rs-test request --action "api.status.health_check~1" --body '{}'
```

**Files**: `src/transport/beepish/client.rs`, `src/discovery/packet.rs`, `src/discovery/service_info.rs`, `src/discovery/service_registry.rs`, new `src/crypto.rs`

---

## Milestone 2: Announcement Signature Verification

**Goal**: Rust correctly verifies RSA PKCS1v15 SHA256 signatures on all
Perl-generated announcements in the discovery cache.

**Dependency chain** (builds on M1 crypto):
1. [ ] Implement `verify_rsa_sha256(public_key_der, message, signature)` in `crypto.rs`
2. [ ] In `packet.rs`: replace `signature_is_valid()` stub with real verification — extract public key from cert PEM, verify signature over JSON blob bytes
3. [ ] Handle base64 decoding of signature (Perl `encode_base64` wraps at 76 chars — handle both wrapped and unwrapped)

**Verification**:
```bash
# Parse live dev cache — every announcement must verify:
cargo test -- --ignored test_verify_real_cache_signatures

# Also: an announcement with a tampered JSON blob must fail verification
```

**Files**: `src/crypto.rs`, `src/discovery/packet.rs`

---

## Milestone 3: Authorized Services Filtering

**Goal**: Rust service registry only includes actions from services whose
certificate fingerprint is authorized for those actions.

**Dependency chain** (builds on M1 fingerprint + M2 signature verification):
1. [ ] Read `bus.authorized_services` path from config
2. [ ] Parse file: `fingerprint tokens` per line, `#` comments, whitespace trimmed
3. [ ] Token processing: comma-separated, `quotemeta`-escaped, `:ALL` → `:.*`, no `:` → `main:` prefix
4. [ ] Build regex per fingerprint: `/^(?:tok1|tok2)(?:\.|$)/i`
5. [ ] Special cases: `_meta.*` always authorized; reject `:` in sector/action
6. [ ] Hot-reload: re-read file when mtime changes
7. [ ] Integrate into `ServiceRegistry`: set `authorized` based on fingerprint + action match
8. [ ] Filter unauthorized entries from `get_action()` / `find_action()` results

**Verification**:
```bash
# Actions list should match what Perl's lssoa shows (same filtering):
docker run --rm --network gtnet -v ~/GT/backplane:/backplane:ro \
  -e SCAMP_CONFIG=/backplane/etc/soa.conf \
  scamp-rs-test list actions --name health_check

# Compare output with:
docker exec main perl /service/main/gt-soa/perl/script/lssoa | grep health_check
```

**Files**: new `src/auth/authorized_services.rs`, `src/discovery/service_registry.rs`, `src/config.rs`

---

## Milestone 4: Multicast Announcing (Rust Service Discoverable)

**Goal**: Rust service sends real UDP multicast announcements that the
cache service picks up. `lssoa` shows the Rust service.

**Dependency chain**:
1. [x] Read config: `discovery.multicast_address` (default `239.63.248.106`), `discovery.port` (default `5555`)
2. [x] Create UDP socket, join multicast group, set multicast interface (socket2)
3. [x] Zlib compress announcement packet before sending (flate2, level 9)
4. [x] Send on interval (default 5s)
5. [x] Fix announcement format: v4 extension hash in envelopes array, RLE encoding for action vectors
6. [x] Fix base64 line-wrapping (76-char lines to match Perl `encode_base64`)
7. [~] Fix flags: filter to announceable set — constant defined, not yet applied (actions don't have flags yet)
8. [x] Shutdown: weight=0, send 10 rounds at 1s interval, then stop

**Verification**:
```bash
# Start Rust service with multicast:
docker run -d --name scamp-rs-service --network gtnet \
  -v ~/GT/backplane:/backplane:ro \
  -e SCAMP_CONFIG=/backplane/etc/soa.conf \
  scamp-rs-test serve --key /backplane/devkeys/dev.key --cert /backplane/devkeys/dev.crt

# Wait ~10s for cache service to pick up the announcement, then:
docker exec main perl /service/main/gt-soa/perl/script/lssoa | grep scamp-rs

# Must show Rust service with correct identity, address, sector, fingerprint
```

**Files**: `src/service.rs` (announcer module), `Cargo.toml` (add `flate2`), `src/config.rs`

---

## Milestone 5: Full Bidirectional Interop via Discovery

**Goal**: Perl `Requester->simple_request` discovers and successfully calls
the Rust service through the normal discovery pipeline. No hacks.

**Dependency chain** (builds on M4):
1. [x] Diagnose Requester timeout: root cause was `ticket: null` in JSON header failing String deserialization
2. [x] Fix: `nullable_string` deserializer for ticket, identifying_token, action fields
3. [x] Verify: Rust service → multicast → cache → Perl Requester discovers → sends request → Rust handles → Perl receives response

**Verification**:
```bash
# The definitive bidirectional interop test:
docker exec main perl -e '
  use GTSOA::Requester;
  use JSON;
  my @r = GTSOA::Requester->simple_request(
    action => "ScampRsTest.echo", version => 1,
    envelope => "json", data => {test => "full interop"},
  );
  die "FAILED: " . encode_json($r[1]) unless $r[0];
  print "SUCCESS: " . encode_json($r[1]) . "\n";
'
```

**Files**: `src/service.rs`, `src/transport/beepish/client.rs`

---

## Milestone 6: Wire Protocol Hardening + Test Infrastructure

**Goal**: Wire protocol matches Perl exactly, backed by Perl-captured test vectors.

Items:
- [ ] **T-1** Capture wire packets from Perl (HEADER+DATA+EOF+ACK) as test fixtures
- [ ] **T-2** Server hot path tests (handle_connection, route_packet, dispatch_and_reply)
- [ ] **T-3** Shared test helpers: packet builders, default headers, sample keypair loader
- [ ] **W-1** Require `\r\n` in header line parsing (not bare `\n`) — D16
- [ ] **W-4** Send-side flow control: validate ACKs, pause/resume at 65536 bytes — D5
- [ ] **W-7** Reader task sets `closed` flag on exit — D12
- [ ] **W-10** Validate TXERR body non-empty — D27
- [ ] **W-11** Connection idle timeout with `_adj_timeout` logic — D6
- [ ] **W-12** Busy flag: track pending requests, adjust timeout
- [x] ~~W-8 Unknown packet types → Fatal~~ (done)
- [x] ~~W-9 Malformed HEADER JSON → Fatal~~ (done)
- [x] ~~S-14 CRUD alias: `"destroy"` not `"delete"`~~ (done)
- [x] ~~S-15 Filter v4 accompat != 1~~ (done)

**Verification**:
```bash
cargo test  # all unit tests pass
# Then: re-dispatch audit agents against Perl, JS, Go to verify parity
```

---

## Milestone 7: Config & Behavioral Parity

**Goal**: Config parsing matches Perl exactly, timeouts are correct.

- [ ] **C-15** Config: first-wins for duplicate keys — D19
- [ ] **C-16** Config: strip inline `# comments` — D21
- [ ] **C-9** Check `GTSOA` env var (Perl canonical) in addition to `SCAMP_CONFIG` — D20
- [ ] **C-11** Three distinct timeouts: server=120s, client=90s, rpc=75s — D17
- [ ] **C-12** Per-action timeout from `t600` flags (value + 5s padding) — D18
- [ ] **C-6** bus_info(): resolve `bus.address` → IP, `if:ethN`, auto-detect — D22
- [ ] **S-23** Bind to `service.address` interface, not `0.0.0.0` — D23

**Verification**:
```bash
cargo test
# Re-dispatch audit agents for final parity check
```

---

## Milestone 8: Discovery Hardening

- [ ] Cache staleness check (`discovery.cache_max_age`, default 120s) — D7
- [ ] Announcement TTL/expiry (`now + sendInterval * 2.1`) — D8
- [ ] Timestamp replay protection (reject older per identity) — D9
- [ ] Service deduplication (fingerprint+identity key) — D26
- [ ] Cache file watching (live refresh via `notify` crate) — D25
- [ ] Multicast receiver/observer — D24

## Milestone 9: Production Hardening

- [ ] Graceful shutdown: drain active requests before close — D10
- [ ] Ticket verification (parse, sig verify, expiry, privileges) — D11
- [ ] High-level Requester API (lookup+connect+request+JSON) — D28
- [ ] Service failure tracking / backoff — D32
- [ ] dispatch_failure / retry — D31
- [ ] Typed error enum (`ScampError`)
- [ ] Connection reconnection with backoff

---

## Audit Schedule

After each milestone, re-dispatch verification agents:

| After | Audit scope | Agents |
|-------|-------------|--------|
| M1 | TLS/fingerprint correctness | 1 agent per: Perl, JS, Go |
| M2 | Signature verification | 1 agent: Perl (canonical signer) |
| M3 | Authorization filtering | 1 agent: Perl (canonical authorized_services) |
| M4 | Announcement format | 1 agent per: Perl, JS (announcement parsers) |
| M5 | Full interop | Live test from Perl container |
| M6 | Wire protocol | 1 agent per: Perl, JS, Go |
| M7 | Config/behavior | 1 agent: Perl (canonical config consumer) |
