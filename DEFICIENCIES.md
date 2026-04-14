# scamp-rs Deficiency Report

Comprehensive parity audit against all four reference implementations.
Last audit: 2026-04-13, v3 (3rd full 6-agent audit). 87 tests (82 lib + 5 E2E).

## Verified Correct (no action needed)

Confirmed matching across all implementations:

- Packet framing: `TYPE MSGNO SIZE\r\n<body>END\r\n` format
- PacketHeader JSON: field names, `"type"` rename, FlexInt, EnvelopeFormat, MessageType
- Nullable fields: `ticket: null`, `identifying_token: null` handled correctly
- Message assembly: HEADER→DATA*→EOF/TXERR, sequential msgno from 0
- Request correlation: sequential request_id from 1, pending map by request_id
- Reply: `type="reply"`, request_id copied from request
- ACK format: decimal string of cumulative bytes
- EOF body: validated as empty (client reader)
- Unknown packet types: Fatal (matches Perl Connection.pm:187)
- Malformed HEADER JSON: Fatal (matches Perl Connection.pm:148-149)
- PING/PONG: responds to PING, disabled by default (Perl/Go don't support)
- TLS fingerprint verification: SHA1 of DER cert, colon-separated uppercase hex
- Natural corking: verification before stream split (equivalent to Perl _corked)
- RSA PKCS1v15 SHA256 signatures: verified against live Perl-generated cache
- authorized_services: file parsing, regex matching, `_meta.*` exception, `:` rejection
- Action index key: `sector:action.vVERSION` (lowercased)
- CRUD aliases: `_destroy` tag
- V4 accompat filter: enabled
- RLE encoding/decoding for v4 action vectors
- Service identity: `name:base64(18 random bytes)`
- Weight=0 filtering in registry lookups
- Envelope filtering at lookup time
- Connection pooling by URI
- Multicast announcing: zlib compression, periodic 5s, socket2 with interface bind
- V4 extension hash in envelopes array
- Base64 76-char line wrapping matching Perl MIME::Base64
- Shutdown announcing: weight=0, 10 rounds at 1s (matches Perl)
- Full bidirectional interop: soatest + simple_request through discovery pipeline
- Announcement timestamp: seconds as f64 (matches Perl Time::HiRes::time)
- dispatch_failure: client-side retry (matches JS; A3 was mis-stated, no server change needed)

## Resolved Deficiencies

| ID | Description | Resolution |
|----|-------------|------------|
| D1 | No multicast announcement sending | Fixed: UDP multicast via socket2 |
| D2 | No zlib compression | Fixed: flate2 Compression::best() |
| D3 | Config keys not read | Fixed: `discovery.multicast_address`, `discovery.port` |
| D4 | No V4 extension hash | Fixed: RLE-encoded action vectors in envelopes array |
| D5 | No send-side ACK validation | Fixed: validate format, monotonic, not-past-end |
| D6 | No connection idle timeout | Fixed: 120s server idle timeout |
| D7 | No cache staleness check | Fixed: warn on stale cache (120s default) |
| D8 | No announcement TTL/expiry | Fixed: skip expired (ts + interval*2.1) |
| D9 | No timestamp replay protection | Fixed: reject older timestamps per identity |
| D12 | Reader closed flag not set | Fixed: set on reader exit |
| D13 | Unknown packet types: Drop | Fixed: now Fatal |
| D14 | Malformed HEADER JSON: Drop | Fixed: now Fatal |
| D15 | Empty fields omitted from JSON | Fixed: always serialize |
| D16 | Bare `\n` accepted | Fixed: require `\r\n` |
| D17 | Timeouts conflated | Fixed: 75/90/120s constants |
| D18 | Per-action timeout not extracted | Fixed: ActionEntry::timeout_secs() |
| D19 | Config last-wins | Fixed: first-wins |
| D20 | GTSOA env var missing | Fixed: checked after SCAMP_CONFIG |
| D21 | Inline `#` comments not stripped | Fixed: strip after `#` |
| D26 | No service deduplication | Fixed: fingerprint+identity key |
| D27 | TXERR body not validated | Fixed: reject empty/"0" |
| D30 | DATA chunk 131072 | Fixed: 2048 to match Perl |
| Q1 | socket_addr() panics | Fixed: returns Result |
| Q2 | Non-deterministic v3 ordering | Fixed: BTreeMap |
| Q3 | println! in library | Fixed: log macros |
| Q4 | Unused futures dep | Fixed: removed |
| T3 | No wire fixtures | Fixed: fixtures.rs + 12 tests |
| T5 | Cache test no assertions | Fixed: added assertions |
| T7 | Dead integration test | Fixed: removed |
| T9 | Unused fixture | Fixed: removed |
| D5b | No send-side flow control watermark | Fixed: client-side pause at 65536 unacked bytes, Notify on ACK |
| T1 | Zero tests for server hot path | Fixed: 6 tests via duplex streams (echo, error, ping, ACK, multi-chunk) |
| T2 | Zero tests for client request sending | Fixed: 4 tests (echo, error, large body, timeout) |
| Q5 | listener.rs exceeds 300-line limit | Fixed: extracted server_connection.rs |
| BUG | Packet::parse failed on binary body data | Fixed: find \r\n in raw bytes before UTF-8 decode |
| D10 | No graceful shutdown | Fixed: tokio::select on shutdown watch, 30s drain timeout |
| D22 | No bus_info() interface resolution | Fixed: bus_info module with getifaddrs, if:ethN, private IP auto-detect |
| D23 | Server binds to 0.0.0.0 | Fixed: bind_pem takes bind_ip from BusInfo |
| D28 | No high-level Requester API | Fixed: Requester::request() combines lookup+connect+send |
| D29 | Flags not filtered to announceable set | Fixed: filter to ANNOUNCEABLE set in announcement building |
| D11 | No ticket verification | Fixed: auth::ticket with RSA verify, expiry, privileges |
| D31 | No dispatch_failure / retry | Fixed: Requester retries once on dispatch_failure |
| D32 | No service failure tracking | Fixed: mark_failed() with exponential backoff, prefer healthy |
| Q5 | list.rs exceeds 300-line limit | Fixed: compacted from 314 to 212 lines |
| T4 | make_auth() duplicates production logic | Fixed: extracted parse_content(), shared by reload and tests |
| T6 | test_fingerprint_of_dev_cert silently passes | Fixed: now #[ignore] with panic on missing cert |
| T10 | No RLE decode edge cases | Fixed: 6 tests for unrle edge cases |
| D24 | No multicast receiver/observer | Fixed: observer.rs — joins group, decompresses, injects |
| D25 | No cache refresh/reload | Fixed: inject_packet + reload_from_cache on ServiceRegistry |
| T8 | No shared test helpers | Fixed: test_helpers.rs with echo_actions, write_request, etc. |
| C1 | No Auth.getAuthzTable privilege checking | Fixed: auth::authz module, integrated into dispatch pipeline |
| C2 | No error_data header field | Fixed: added to PacketHeader, dispatch_failure checks it |
| C3 | Blocking UDP send_to in async | Fixed: tokio::net::UdpSocket with async .await |
| C4 | Silent write error swallowing | Fixed: errors logged, early return on broken pipe |
| I1 | Outgoing flow-control state removed too early | Fixed: removed after response received |
| L3 | Unused thiserror dependency | Fixed: removed from Cargo.toml |
| A1 | ScampReply has no error_data field | Fixed: added error_data to ScampReply + send_reply |
| A2 | register() always sets empty flags | Fixed: register_with_flags() |
| S1 | Inline tests push files over 300 lines | Fixed: extracted to separate test files |
| BUG2 | BufReader busy-loop on partial TLS packets | Fixed: manual Vec buffer + AsyncReadExt::read() (server + client) |
| BUG3 | tokio::io::split deadlock on TLS streams | Fixed: copy_bidirectional proxy through duplex |
| A3 | Server never sets dispatch_failure | Non-issue: JS reviewer clarified dispatch_failure is client-side |

## All 46 Original + 6 Post-Audit + 3 Session Deficiencies Resolved

## v3 Audit Findings (2026-04-13, 6-agent review)

### High — fix now

| ID | Severity | Description | Source |
|----|----------|-------------|--------|
| H1 | High | Proxy task handle leak: from_stream spawns copy_bidirectional but never stores JoinHandle. Orphan tasks + FD leaks on drop. | Elegance-v3 |
| H2 | High | Server-side flow control dead code: OutgoingReplyState tracks sent/acked but server never waits on ACKs. Large replies sent unbounded. | Elegance-v3 |
| P1 | High | Out-of-sequence HEADER doesn't close connection (Perl closes, Rust logs and continues) | Perl-v3 |

### Medium — fix before stress testing

| ID | Severity | Description | Source |
|----|----------|-------------|--------|
| M1 | Medium | No cap on server/client read buf growth — adversarial stream could OOM | Elegance-v3 |
| M5 | Medium | Empty ticket bypasses auth (AuthzChecker skips check when ticket is empty) | Elegance-v3 |
| M6 | Medium | CacheFileAnnouncementIterator drops final record if file lacks trailing delimiter | Elegance-v3 |
| JS1 | Medium | AuthzChecker allows access for actions missing from authz table (JS denies with "Unconfigured action") | JS-v3 |
| P2 | Medium | Announcement builder doesn't emit `t<N>` timeout flags | Perl-v3 |
| P3 | Medium | Server doesn't validate EOF body is empty (client reader does) | Perl-v3 |
| P4 | Medium | AuthzChecker not plumbed into ScampService::run() — always None, authz is dead code in production | Perl-v3, C#-v3 |
| F1 | Medium | proto/tests.rs at 396 lines (over 300-line limit) | Standards-v3 |
| F2 | Medium | service_info/mod.rs has parsing logic (should be in parse.rs) | Standards-v3 |

### Low — polish

| ID | Severity | Description | Source |
|----|----------|-------------|--------|
| F3 | Low | Redundant eprintln! alongside log::error! in signature_is_valid() | Standards-v3 |
| F4 | Low | Three unwrap() on SystemTime that should be unwrap_or_default() | Standards-v3 |
| M4 | Low | closed AtomicBool uses Relaxed ordering (should be Acquire/Release) | Elegance-v3 |
| E1 | Low | E2E test 50ms sleep is a race (should use readiness signal) | JS-v3 |
| E2 | Low | E2E missing 2 of 7 planned scenarios (concurrent connections, auth filtering) | Standards-v3 |
| A5 | Low | V4 action vectors always empty in announcements | Perl-v2, Go-v3 |
| A6 | Low | No heartbeat initiation (responds to PING, never sends) | JS-v2, JS-v3 |
| A7 | Low | Connection pool grows without bound (no eviction) | JS-v2, Elegance-v3 |
| A4 | Low | Config keys hardcoded (rpc.timeout, beepish.* timeouts, port range) | Perl-v2 |
| A8 | Low | Stale cache behavior divergence (Rust serves, Perl fails) | Perl-v2 |
| S2 | Low | HANDOFF.md has stale statements about already-fixed issues | Standards-v3 |
| M8 | Low | RSA signature verification blocks ServiceRegistry write lock | Elegance-v3 |
| S3 | Low | service_registry.rs at ~275 lines (resolved from 322, under limit) | Standards-v3 |
