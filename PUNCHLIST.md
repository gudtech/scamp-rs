# scamp-rs Completion Punchlist

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done

> **Reference priority**: Perl (gt-soa/perl) > scamp-js > gt-soa/js > scamp-go.
> Comprehensive deficiency audit in DEFICIENCIES.md (58 items across wire protocol,
> service/discovery, and config/security).

---

## Phase 0: Cleanup ✓

- [x] Remove `BEEP\r\n` handshake
- [x] Delete dead modules
- [x] Clean lib.rs
- [x] Remove unused deps (pnet, net2, atty)
- [x] cargo build/test/clippy pass

## Phase 1: Wire Protocol Correctness

### Done
- [x] PacketHeader serde: `"type"` field name, lowercase enums, FlexInt
- [x] Message assembly (inbound): HEADER→DATA*→EOF/TXERR, msgno from 0
- [x] Message serialization (outbound): HEADER + DATA chunks + EOF
- [x] Request-response correlation: sequential request_id from 1
- [x] ACK sending on DATA receipt (decimal string format)
- [x] PING/PONG disabled by default, PONG response

### Remaining
- [ ] **W-1** Header line parsing: require `\r\n` explicitly (Perl regex requires `\015\012`)
- [ ] **W-3** Always serialize `action`, `ticket`, `identifying_token` in JSON (remove `skip_serializing_if`)
- [ ] **W-4** Send-side flow control: validate incoming ACKs (format `/^[1-9][0-9]*$/`, monotonic, not past sent), pause when `sent - acked >= 65536`, resume on ACK
- [ ] **W-7** Reader task must set `closed` flag on exit (currently only set in Drop)
- [ ] **W-8** Unknown packet types: change from silent Drop to Fatal (matches Perl/JS/Go)
- [ ] **W-9** Malformed HEADER JSON: change from silent Drop to Fatal (matches Perl/JS/Go)
- [ ] **W-11** Connection idle timeout: implement `_adj_timeout` logic (busy/pending → no timeout, idle → configured timeout). Read `beepish.client_timeout` (default 90s) and `beepish.server_timeout` (default 120s).
- [ ] **W-13** Read `rpc.timeout` from config (default 75s). Read `beepish.client_timeout` (default 90s).

## Phase 2: TLS & Certificate Security

- [ ] **C-2** Client-side TLS fingerprint verification: after TLS handshake, extract peer cert, compute SHA1 fingerprint, compare against announced fingerprint. Mismatch = fatal.
- [ ] **C-4** Certificate fingerprinting: SHA1 of DER cert → uppercase hex colon-separated (`XX:XX:XX:...`). Add to ServiceInfo.
- [ ] **W-5** Write corking: buffer outbound packets until fingerprint verification completes. Flush on success, destroy on failure. (Matches Perl Connection.pm:33-74)
- [ ] **C-8** Pass fingerprint through from ServiceRegistry → ConnectionHandle for verification
- [ ] Consider migrating from `tokio-native-tls` to `tokio-rustls` for peer cert access and pure-Rust TLS

## Phase 3: Signature Verification & Authorization

- [ ] **C-3** Implement RSA PKCS1v15 SHA256 signature verification in `packet.rs` (replace stub). Verify against real Perl-generated announcements from dev cache.
- [ ] **S-13** Compute SHA1 fingerprint during announcement parsing and store in ServiceInfo/ActionEntry
- [ ] **C-5** Implement `authorized_services` file parsing:
  - Read `bus.authorized_services` config path
  - Format: `fingerprint tokens` per line, `#` comments
  - Tokens comma-separated, `quotemeta`-escaped, `:ALL` → `:.*`, no `:` → `main:` prefix
  - Regex: `/^(?:tok1|tok2)(?:\.|$)/i`
  - `_meta.*` always authorized
  - Reject `:` in sector or action name
  - Hot-reload on file mtime change
- [ ] **C-7** Ticket verification:
  - Parse `version,userId,clientId,timestamp,ttl,privs,signature`
  - Privs: `+` separated
  - Sig: pop last comma-field, base64url decode, PKCS1v15 SHA256 verify
  - Expiry: `timestamp + ttl < now`
  - Key from config or `/etc/GT/auth/ticket_verify_public_key.pem`

## Phase 4: Discovery — Multicast & Cache

- [ ] **S-1** UDP multicast announcement sending:
  - Create UDP socket, join multicast group (config `discovery.multicast_address` default `239.63.248.106`, port `discovery.port` default `5555`)
  - Bind to interface from `bus.address` / `discovery.address` config
  - Send compressed announcement every `interval` seconds (default 5s)
  - **S-3** Zlib compress packets before sending (Perl `compress($pkt, 9)`)
- [ ] **S-2** UDP multicast receiving (for live discovery):
  - Listen on multicast group
  - **S-6.2** Strip leading `R`/`D` prefix bytes
  - **S-6.3** Decompress (zlib), fallback to raw
  - Inject into ServiceRegistry
- [ ] **S-4** V4 extension hash in announcements: append RLE-encoded action vectors as last element of envelopes array
- [ ] **S-5** Implement `__torle` RLE encoding for v4 vectors
- [ ] **S-9** Cache file header stripping: discard first chunk before `%%%` (Perl `ServiceManager.pm:92`)
- [ ] **S-10** Cache staleness check: verify mtime < `discovery.cache_max_age` (default 120s)
- [ ] **S-11** Announcement expiry/TTL: `now + send_interval_sec * 2.1` for dynamic entries
- [ ] **S-12** Timestamp replay protection: track per-identity timestamps, reject stale
- [ ] **S-24** Cache refresh: periodic reload (Perl reloads each `fill_from_cache`, rate-limited 1/sec)

## Phase 5: Service Infrastructure Fixes

- [ ] **S-8** Graceful shutdown:
  - Send weight=0 announcements (10 rounds at 1s interval — Perl, or rapid 4x200ms — JS)
  - Stop accepting new connections
  - Wait for active handlers to complete
  - Close connections, stop announcer
  - **S-16** Handle SIGTERM/SIGINT via `tokio::signal`
- [ ] **S-14** Fix CRUD alias tag: use `"destroy"` not `"delete"` (matches Perl/JS)
- [ ] **S-15** Filter v4 actions where `accompat != 1` (uncomment and fix existing code)
- [ ] **S-17** Server connection idle timeout (default 120s from `beepish.server_timeout`)
- [ ] **S-18** Track active request count per connection (needed for graceful shutdown, busy flag)
- [ ] **S-19** Base64 line-wrapping: use 76-char lines for signature encoding (match Perl `encode_base64`)
- [ ] **S-20** Filter flags to announceable set during packet building (`read, update, destroy, create, noauth, secret`)
- [ ] **S-21** Make weight configurable (not hardcoded 1)
- [ ] **S-22** Compute per-action timeout from `t600` flags (value + 5s padding) and return to caller

## Phase 6: Configuration & Behavioral Parity

- [ ] **C-1** Read all config keys: `bus.address`, `bus.authorized_services`, `discovery.*`, `beepish.*`, `rpc.timeout`
- [ ] **C-6** Implement bus_info() interface resolution: `bus.address` → IP, support `if:ethN` syntax, auto-detect 10.x/192.168.x
- [ ] **C-9** Check `GTSOA` env var in addition to `SCAMP_CONFIG`
- [ ] **C-11** Separate three timeout values: `beepish.server_timeout` (120s), `beepish.client_timeout` (90s), `rpc.timeout` (75s)
- [ ] **C-14** High-level Requester API: cache fill → action lookup → connect → request → JSON decode → error normalization
- [ ] **C-15** Config: first-wins for duplicate keys (match Perl)
- [ ] **C-16** Config: strip inline `# comments` (match Perl)
- [ ] **S-23** Bind to configured `service.address` interface, not `0.0.0.0`

## Testing

### Unit
- [x] Packet parse/write roundtrip
- [x] PacketHeader serde (`"type"`, lowercase enums, FlexInt)
- [ ] Config parsing with inline comments and duplicate keys
- [ ] Announcement signature verification against real dev cache data
- [ ] Certificate fingerprint matches `openssl x509 -fingerprint -sha1`
- [ ] authorized_services regex matching
- [ ] Ticket parsing and verification

### Live Interop (Docker on gtnet via `gud dev`)
- [x] **Rust client → Perl service**: health_check and _meta.documentation (400KB+)
- [ ] **Rust client → Go service**: soabridge request (verify no PING sent)
- [~] **Perl client → Rust service via discovery**: Direct BEEPish::Client works. Full Requester path needs: multicast announcing (S-1), so cache service picks up Rust service, then Perl Requester discovers and calls it through normal pipeline. No cache file hacks.
- [ ] **lssoa validation**: `docker exec main perl .../lssoa` shows Rust service discovered via multicast (not cache injection)
- [ ] Connection multiplexing: concurrent requests on one connection
- [ ] Flow control: large message with ACK-based pause/resume
- [ ] Graceful shutdown: weight=0 announcement, drain, disappear from cache

---

## Cross-Implementation Decisions

- [x] **D1** Timestamp: use float seconds (matching Perl `Time::HiRes::time`)
- [x] **D2** PING/PONG: disabled by default (Perl/Go don't support)
- [ ] **D3** Multicast compression: must compress (Perl does). Handle both compressed/uncompressed incoming.
- [x] **D4** Action index key: `sector:action.vVERSION` (matching Perl/JS)
- [ ] **D7** DATA chunk size: 131072 for sending (all receivers handle it). Consider reducing for flow control.
- [x] **D8** Signing: PKCS1v15 SHA256 confirmed across all impls (Perl's OAEP call is no-op for signatures)
- [x] **D9** EOF body: must be empty (validated)
