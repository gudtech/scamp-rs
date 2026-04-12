# scamp-rs Completion Punchlist

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done, `[!]` blocked

> **Reference priority**: Perl (gt-soa/perl) > scamp-js > gt-soa/js > scamp-go.
> Where this punchlist was originally written referencing scamp-go, it has been
> revised against the canonical Perl implementation. Items marked with ⚠️ were
> corrected during the Perl cross-check.

## Phase 0: Critical Interop Fix + Cleanup

- [x] **P0-1** Remove `BEEP\r\n` handshake from `src/transport/beepish/client.rs`
- [x] **P0-2** Remove dead code: `src/message/`, `src/common/`, `src/error.rs`, `src/agent/`, `src/action.rs`, `src/transport/beepish/tcp.rs`
- [x] **P0-3** Clean `src/lib.rs`: remove all commented-out module declarations and re-exports
- [x] **P0-4** Remove unused deps from Cargo.toml: `pnet`, `net2`, `atty`
- [x] **P0-5** Verify `cargo build && cargo test && cargo clippy` pass

## Phase 1: Transport Core

- [x] **P1-1** ⚠️ Fix PacketHeader serde: JSON field `"type"` (not `"message_type"`), lowercase EnvelopeFormat/MessageType, optional field omission
- [x] **P1-2** Implement `FlexInt` type for `client_id` (deserializes from both JSON string `"42"` and integer `42`)
- [x] **P1-3** ⚠️ Inbound message assembly: HEADER → DATA* → EOF/TXERR. Message numbers start at 0. Sequential validation.
- [x] **P1-4** Outbound message serialization: HEADER + DATA chunks (131072 bytes) + EOF (empty body)
- [x] **P1-5** ⚠️ Request-response correlation: sequential request_id starting from 1, pending map by request_id
- [x] **P1-6** Timeout per request via `tokio::time::timeout` (default 75s)
- [x] **P1-7** ⚠️ Flow control: ACK sent as decimal string of cumulative bytes. Send-side pause/resume deferred.
- [x] **P1-8** ⚠️ PING/PONG: disabled by default, responds to PING with PONG
- [x] **P1-9** Connection architecture: mpsc writer channel, reader task, ConnectionHandle with pending map

## Phase 2: Service Infrastructure

- [x] **P2-1** TLS server listener on random port in 30100-30399 range with retry
- [x] **P2-2** Action registration: `service.register("Name.action", version, handler_fn)`
- [x] **P2-3** ⚠️ Request dispatch with `type = 'reply'` and `request_id` copy from request
- [x] **P2-4** Handler via async closures returning ScampReply
- [x] **P2-5** Service identity: `name:base64(random 18 bytes)`
- [ ] **P2-6** ⚠️ Announcement packet generation (needed for discovery via multicast/cache)

## Phase 3: Security

- [ ] **P3-1** ⚠️ RSA SHA256 announcement signature verification. Perl `ServiceInfo.pm:99-100` uses `use_sha256_hash` + `use_pkcs1_oaep_padding`. Go `verify.go` uses `rsa.VerifyPKCS1v15`. JS uses `crypto.createVerify('sha256')`. The Perl OAEP padding call is likely a no-op for verification (OAEP is an encryption scheme; `Crypt::OpenSSL::RSA->verify()` uses PKCS1v15 for signatures regardless). **Verify empirically** by checking real signatures from Perl services against PKCS1v15.
- [ ] **P3-2** ⚠️ SHA1 certificate fingerprinting. Format: uppercase hex, colon-separated. Perl `ServiceInfo.pm:83-86`: SHA1 of DER-encoded cert bytes, hex digest uppercased, then insert colons: `$hash =~ s/(..)(?!$)/$1:/g`. Go `cert.go` produces same format.
- [ ] **P3-3** ⚠️ `authorized_services` file parsing: Perl `ServiceInfo.pm:122-138`. Format: `fingerprint tokens` per line. Tokens are comma-separated. If token contains `:`, replace `:ALL` with `:.*`. If no `:`, prefix with `main:`. Patterns are `quotemeta`-escaped then compiled as regex: `/^(?:tok1|tok2)(?:\.|$)/i`. Matching: `"$sector:$action" =~ /$rx/`. Special case: `_meta.*` actions always authorized.
- [ ] **P3-4** Action authorization filtering in ServiceRegistry.
- [ ] **P3-5** ⚠️ Ticket format: `version,userId,clientId,timestamp,ttl,privs,signature`. Privileges use **`+` separator** (not comma). Go `ticket.go:106`: `strings.Split(parts[5], "+")`. JS `ticket.js:43`: `parts[5].split('+')`. Signature is the LAST comma-separated field (split on comma, pop last element).
- [ ] **P3-6** ⚠️ Ticket signature: the signed content is `version,userId,clientId,timestamp,ttl,privs` (everything before the last comma). Go uses `base64.RawURLEncoding` to decode the signature. JS converts URL-safe base64 to standard before decoding (`replace(/-/g,'+').replace(/_/g,'/')`). Both use PKCS1v15 SHA256.
- [ ] **P3-7** Ticket expiry: `timestamp + ttl < now()` (Go `ticket.go:123-124`). Timestamps in Unix seconds.

## Phase 4: Discovery

- [ ] **P4-1** ⚠️ UDP multicast announcement sending. Perl `Announcer.pm`: sends **zlib-compressed** packets. Default interval: 5 seconds. Multicast group: `239.63.248.106:5555` (Perl `Config.pm:109-110`). On shutdown: sets weight=0, sends at 1-second intervals for 10 rounds (`Announcer.pm:96-100`).
- [ ] **P4-2** ⚠️ Discovery cache file: the file has a **header section before the first `%%%`** that should be discarded. Perl `ServiceManager.pm:91`: `my ($header, @anns) = split /\n%%%\n/, $data;`. Also check cache staleness: `discovery.cache_max_age` (default 120 seconds).
- [ ] **P4-3** Announcement expiry/TTL: `sendInterval * 2.1` (Perl `ServiceManager.pm:37`). `sendInterval` is in **seconds** (Perl `ServiceInfo.pm:33`: divides the millisecond value from JSON by 1000).
- [ ] **P4-4** ⚠️ Action index key format: **`sector:action.vVERSION`** (lowercased). Both Perl (`ServiceInfo.pm:188`: `"\L$sector:$aname.v$vers"`) and JS (`serviceMgr.js:221`: `info.sector + ':' + aname + '.v' + info.version`) agree. NOT `action~version` as scamp-rs currently uses.
- [ ] **P4-5** Envelope-based action filtering at lookup time. Perl `ServiceInfo.pm:254`: `grep { $_ eq $envelope } @{$blk->[4]}`.
- [ ] **P4-6** ⚠️ CRUD tag aliases: both Perl and JS create alias entries for CRUD tags. Perl `ServiceInfo.pm:191-192`: `$map{ "\L$sector:$ns._$tag.v$vers" } = $info if $tag =~ /^(?:create|read|update|destroy)$/`. JS `serviceMgr.js:223-225`: same pattern.
- [ ] **P4-7** Suspend (weight=0) for graceful shutdown. Weight=0 services excluded from routing. Perl `ServiceManager.pm:57`: `$sv->weight or next`.

## Phase 5: Hardening

- [ ] **P5-1** Typed error enum: `ScampError { Transport, Auth, Timeout, ActionNotFound, Remote, ConnectionLost, Io, Tls }`
- [ ] **P5-2** ⚠️ TXERR handling: Perl `Connection.pm:170-175` — TXERR body is UTF-8 error text, delivered via `finish(error_text)`. JS `connection.js:229` — validates body is non-empty and not `"0"`. Propagate as error to pending request.
- [ ] **P5-3** ⚠️ Connection reconnection: Perl `ConnectionManager.pm` — simple: on `on_lost` callback, delete from pool; next request creates new connection. No exponential backoff in Perl. JS has failure tracking with minute-based backoff (`Registration.prototype.connectFailed()`).
- [ ] **P5-4** ⚠️ Graceful shutdown: Perl `Announcer.pm:96-100` — set active=false (weight=0), announce at 1s interval for 10 rounds. JS `service.js:77-90` — suspend announcer, wait 5s, wait for active requests to drain, then 1s delay.
- [ ] **P5-5** Running service file (liveness indicator).
- [ ] **P5-6** ⚠️ TLS certificate fingerprint verification on client connections. Perl `Connection.pm:61-73`: on TLS handshake completion, verify peer certificate SHA1 fingerprint matches announced fingerprint. Connection is **corked** (writes buffered) until verification succeeds. Go `connection.go:38-39`: `InsecureSkipVerify: true` (no verification!).

## Phase 6: Cleanup

- [ ] **P6-1** Remove dead code: `src/message/`, `src/common/`, `src/error.rs`, `src/agent/`, `src/action.rs`, `src/transport/beepish/tcp.rs`
- [ ] **P6-2** Remove unused deps: `net2`, `atty`, `pnet`
- [ ] **P6-3** Add deps: `rustls` + `tokio-rustls` (replace `tokio-native-tls`), `ring`, `base64`, `notify`, `flate2` (zlib)
- [ ] **P6-4** New module structure: `src/service.rs`, `src/client.rs`, `src/auth/ticket.rs`, `src/auth/authorized_services.rs`, `src/crypto.rs`

## Testing

### Unit Tests
- [x] **T1** Packet parse/write roundtrip for all packet types
- [x] **T2** PacketHeader serde roundtrip — verify `"type"` field name, `"json"` envelope, `"request"`/`"reply"` types
- [x] **T3** FlexInt deserialization from string and integer
- [ ] **T4** Config parsing with real soa.conf files
- [ ] **T5** Announcement parsing with real discovery cache data from dev environment
- [ ] **T6** Ticket parsing and verification with known-good tickets

### Live Interop Tests (against running dev environment via `gud dev`)

**Prerequisite**: Dev environment must be running (`gud dev status -g` shows UP for main, auth, cache, soabridge).

**Phase 1 validation (after transport core):**
- [x] **T7** Parse the **live discovery cache file** from the running dev environment. ✓ All records parse. Verified via Docker on gtnet.
- [ ] **T8** Validate **announcement signature verification** against real cache data.
- [x] **T9** **Rust client → Perl service**: ✓ Successfully sent requests to gt-main-service. `_meta.documentation~1` returned 400KB+ response. `api.status.health_check~1` returned clean 0-byte response. Verified via `docker run --network gtnet`.
- [ ] **T10** **Rust client → Go service**: Connect to soabridge (scamp-go). Send a request. Verify response. Verify NO PING packets are sent.

**Phase 2 validation (after service infrastructure):**
- [~] **T11** **Perl client → Rust service via discovery**:
  - ✓ `lssoa` shows Rust service with correct identity, address, sector, fingerprint
  - ✓ Perl `ServiceManager->lookup` discovers Rust service through cache
  - ✓ Signature verification passes (PKCS1 SHA256)
  - ✓ authorized_services check passes (dev cert fingerprint)
  - ✓ Direct `BEEPish::Client` with fingerprint verification → echo works
  - ✗ `Requester->simple_request` path times out — connection accepted but no packets sent. Likely Perl AnyEvent event loop timing issue with corked writes, not a wire protocol bug. Needs investigation.
- [x] **T12** **lssoa validation**: ✓ `docker exec main perl /service/main/gt-soa/perl/script/lssoa` shows the Rust service with correct identity, sector, weight, address, envelope, and fingerprint.

**Phase 3+ validation:**
- [ ] **T13** Connection multiplexing: concurrent requests on one connection to a Perl service
- [ ] **T14** Flow control under load: large message body to/from Perl service
- [ ] **T15** Graceful shutdown: start Rust service, verify it appears in cache, shut down, verify weight=0 announcement sent, verify it disappears from cache
- [ ] **T16** Certificate fingerprint verification: verify Rust client checks peer cert fingerprint against announced fingerprint when connecting to Perl services

## Cross-Implementation Differences (decisions)

- [ ] **D1** ⚠️ Timestamp format: Perl=`Time::HiRes::time` (float seconds), Go=seconds.microseconds, JS=milliseconds. Rust parses as f64, which handles all formats. For **generating**, use float seconds (matching Perl).
- [ ] **D2** ⚠️ PING/PONG: Perl AND Go don't support. Only JS does. **Default: disabled.** Only enable for known JS peers.
- [ ] **D3** ⚠️ Multicast compression: **Perl uses zlib** (not just JS). Go doesn't compress. Rust must compress outgoing and handle both compressed/uncompressed incoming (Perl Observer strips 'R'/'D' prefix bytes then tries uncompress, falls back to raw).
- [ ] **D4** ⚠️ Action index key: Use `sector:action.vVERSION` (Perl and JS agree). NOT Go's `~version#envelope` format.
- [ ] **D5** `error_data` field: JS-only. Skip for now, add later for forward compat.
- [ ] **D6** Pub/sub message types (event/subscribe/notify): JS-only. Not needed for Perl interop.
- [ ] **D7** ⚠️ DATA chunk size: Perl=2048, Go=128KB, JS=131072. Use 131072 for sending (all receivers handle it). Accept any size ≤131072 on receive.
- [ ] **D8** ⚠️ Signing padding: Perl code calls `use_pkcs1_oaep_padding` but Go/JS use PKCS1v15. Empirically verify against real Perl-generated signatures. If PKCS1v15 works, use it.
- [ ] **D9** ⚠️ EOF packet body: Perl validates body MUST be empty (`Connection.pm:162`). JS validates same (`connection.js:217`). Go doesn't validate. Rust should validate on receive and send empty.
