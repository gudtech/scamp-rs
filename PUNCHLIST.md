# scamp-rs Completion Punchlist

Status legend: `[ ]` todo, `[~]` in progress, `[x]` done, `[!]` blocked

> **Reference priority**: Perl (gt-soa/perl) > scamp-js > gt-soa/js > scamp-go.
> Where this punchlist was originally written referencing scamp-go, it has been
> revised against the canonical Perl implementation. Items marked with ⚠️ were
> corrected during the Perl cross-check.

## Phase 0: Critical Interop Fix + Cleanup

- [ ] **P0-1** Remove `BEEP\r\n` handshake from `src/transport/beepish/client.rs` (lines ~136-147). No other implementation does this. Breaks all interop.
- [ ] **P0-2** Remove dead code: `src/message/`, `src/common/`, `src/error.rs`, `src/agent/`, `src/action.rs`, `src/transport/beepish/tcp.rs`
- [ ] **P0-3** Clean `src/lib.rs`: remove all commented-out module declarations and re-exports
- [ ] **P0-4** Remove unused deps from Cargo.toml: `pnet`, `net2`, `atty`
- [ ] **P0-5** Verify `cargo build && cargo test && cargo clippy` pass

## Phase 1: Transport Core

- [ ] **P1-1** ⚠️ Fix PacketHeader serde: the JSON field name MUST be `"type"` (not `"message_type"`). Perl `Client.pm:87` sets `$request->header->{type} = 'request'`, Go uses `json:"type"` tag. Custom serde for `EnvelopeFormat` (→ `"json"`, `"jsonstore"`) and `MessageType` (→ `"request"`, `"reply"`) as lowercase strings. Rename struct field to `type` with `#[serde(rename = "type")]`.
- [ ] **P1-2** Implement `FlexInt` type for `client_id` (deserializes from both JSON string `"42"` and integer `42`). Reference: Go `packetheader.go` flexInt type.
- [ ] **P1-3** ⚠️ Inbound message assembly: HEADER → DATA* → EOF/TXERR. Message numbers start at **0** (not 1). Perl `Connection.pm:97-98`: `_next_message_in = 0; _next_message_out = 0`. Track per-direction counters; validate sequential HEADER msgno.
- [ ] **P1-4** Outbound message serialization: Message → HEADER + DATA chunks + EOF. Chunk size: **131072 bytes** (matches JS). Note: Perl uses 2048-byte chunks (`Connection.pm:218`) but all receivers handle any size ≤131072.
- [ ] **P1-5** ⚠️ Request-response correlation: Perl `Client.pm` uses sequential integer `request_id` starting from 1 (`_nextcorr = 1`). Reply carries back the same `request_id` in its header. Map pending requests by `request_id`. Server copies `request_id` from request to reply (`Server.pm:66`).
- [ ] **P1-6** Timeout per request via `tokio::time::timeout`. Default: **75 seconds** from `rpc.timeout` config (Perl `ServiceInfo.pm:256`). Per-action timeouts from `t600` flags add 5 seconds (`ServiceInfo.pm:257`).
- [ ] **P1-7** ⚠️ Flow control: ACK body is a **decimal string** of cumulative bytes received (Perl `Connection.pm:179`: validates `/^[1-9][0-9]*$/`). ACK value must strictly advance. Pause sending when `sent - acked >= 65536`. Resume on ACK receipt.
- [ ] **P1-8** ⚠️ PING/PONG heartbeat: **MUST be disabled by default.** Perl does NOT support PING/PONG — unknown packet types cause connection error (`Connection.pm:186`). Go also does not support them. Only scamp-js supports PING/PONG. Heartbeat should only be enabled when explicitly connecting to a JS service.
- [ ] **P1-9** Connection architecture: mpsc channel for serialized writes, reader task for packet dispatch, `ConnectionHandle` with pending requests map.

## Phase 2: Service Infrastructure

- [ ] **P2-1** TLS server listener: accept connections on random port in configured range. Perl `Server.pm`: `beepish.first_port` (30100), `beepish.first_port` (30399 — note Perl has a bug using first_port for both), `beepish.bind_tries` (20). URI format: `beepish+tls://addr:port`.
- [ ] **P2-2** Action registration: `service.register("Name.action", version, handler_fn)` with sector, flags, envelope types.
- [ ] **P2-3** ⚠️ Request dispatch: route by action name + version. Server sets `type = 'reply'` and copies `request_id` from request to reply (Perl `Server.pm:66-68`). Note: Perl uses `'reply'` not `'response'` (MESSAGE.pod incorrectly says "response").
- [ ] **P2-4** Handler trait: `async fn handle(request: ScampRequest) -> Result<ScampResponse, ScampError>`
- [ ] **P2-5** Service identity: `name:base64(random 18 bytes)` — Perl `Announcer.pm:52`.
- [ ] **P2-6** ⚠️ Announcement packet generation: v3 JSON array format is `[3, ident, sector, weight, interval_ms, uri, [envelopes..., v4_hash], v3_actions, timestamp]`. Note: `interval` is in **milliseconds** in the JSON (Perl `Announcer.pm:163`: `intvl => $self->interval * 1000`). Signing: RSA SHA256, sign the JSON blob bytes. Full packet: `json_blob\n\ncert_pem\nbase64(sig)\n`. Then **zlib compress** before multicast sending (Perl `Announcer.pm:202`).

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
- [ ] **T1** Packet parse/write roundtrip for all packet types
- [ ] **T2** PacketHeader serde roundtrip — verify `"type"` field name, `"json"` envelope, `"request"`/`"reply"` types
- [ ] **T3** FlexInt deserialization from string and integer
- [ ] **T4** Config parsing with real soa.conf files
- [ ] **T5** Announcement parsing with real discovery cache data from dev environment
- [ ] **T6** Ticket parsing and verification with known-good tickets

### Live Interop Tests (against running dev environment via `gud dev`)

**Prerequisite**: Dev environment must be running (`gud dev status -g` shows UP for main, auth, cache, soabridge).

**Phase 1 validation (after transport core):**
- [ ] **T7** Parse the **live discovery cache file** from the running dev environment. Verify all records parse without error. Print service count, action count, sectors. This validates announcement parsing against real Perl-generated data.
- [ ] **T8** Validate **announcement signature verification** against real cache data. Every signed announcement from Perl services must verify successfully with PKCS1v15 SHA256.
- [ ] **T9** **Rust client → Perl service**: Connect to gt-main-service (running in `main` container) via TLS. Send a real SCAMP request (e.g., `API.Status.health_check~1`). Verify valid response received. This is the #1 interop validation.
- [ ] **T10** **Rust client → Go service**: Connect to soabridge (scamp-go). Send a request. Verify response. Verify NO PING packets are sent.

**Phase 2 validation (after service infrastructure):**
- [ ] **T11** **Perl client → Rust service**: Start a Rust service, wait for it to appear in the discovery cache. Use `docker exec main` to run Perl tools (`lssoa`, or a simple Perl script using GTSOA::Requester) to discover and call the Rust service. This validates: announcement generation, signature, TLS server, request dispatch, and reply format.
- [ ] **T12** **lssoa validation**: Run `docker exec main lssoa` (or equivalent) and verify the Rust service appears with correct identity, address, sector, and action list.

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
