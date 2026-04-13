# scamp-rs Deficiency Report

Comprehensive parity audit against all four reference implementations.
Last audit: 2026-04-13, after M1-M5 completion. 6 agents: Perl, JS, Go, C# parity + test review + code review.

## Verified Correct (no action needed)

Confirmed matching across all implementations:

- Packet framing: `TYPE MSGNO SIZE\r\n<body>END\r\n` format
- PacketHeader JSON: field names, `"type"` rename, FlexInt, EnvelopeFormat, MessageType
- Nullable fields: `ticket: null`, `identifying_token: null` handled correctly
- Message assembly: HEADER→DATA*→EOF/TXERR, sequential msgno from 0
- Request correlation: sequential request_id from 1, pending map by request_id
- Reply: `type="reply"`, request_id copied from request
- ACK format: decimal string of cumulative bytes
- EOF body: validated as empty
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
| D1 | No multicast announcement sending | Fixed: UDP multicast via socket2 |
| D2 | No zlib compression | Fixed: flate2 Compression::best() |
| D3 | Config keys not read | Fixed: `discovery.multicast_address`, `discovery.port` |
| D4 | No V4 extension hash | Fixed: RLE-encoded action vectors in envelopes array |
| D12 | Reader task doesn't set closed flag | Fixed: set on reader exit |
| D13 | Unknown packet types: Drop instead of Fatal | Fixed: now Fatal |
| D14 | Malformed HEADER JSON: Drop instead of Fatal | Fixed: now Fatal |
| D15 | Empty fields omitted from header JSON | Fixed: always serialize action/ticket/identifying_token |
| D16 | Header line accepts bare `\n` | Fixed: require `\r\n` |
| D17 | Three timeouts conflated | Fixed: distinct constants (75/90/120s) |
| D19 | Config last-wins for duplicates | Fixed: first-wins |
| D20 | GTSOA env var not checked | Fixed: checked after SCAMP_CONFIG |
| D21 | Inline `#` comments not stripped | Fixed: strip after `#` |
| D27 | TXERR body not validated | Fixed: reject empty/"0" |
| D30 | DATA chunk size 131072 | Fixed: 2048 to match Perl |
| D5b | No send-side flow control watermark | Fixed: client-side pause at 65536 unacked bytes, Notify on ACK |
| T1 | Zero tests for server hot path | Fixed: 6 tests via duplex streams (echo, error, ping, ACK, multi-chunk) |
| T2 | Zero tests for client request sending | Fixed: 4 tests (echo, error, large body, timeout) |
| Q5 | listener.rs exceeds 300-line limit | Fixed: extracted server_connection.rs (299 prod lines + tests) |
| BUG | Packet::parse failed on binary body data | Fixed: find \r\n in raw bytes before UTF-8 decode |

## Remaining Deficiencies

### Code Quality

| ID | Severity | Description | File:Line |
|----|----------|-------------|-----------|
| **Q5** | Low | `list.rs` (314) exceeds 300-line limit | coding standards |

### Remaining Discovery

| ID | Description | Ref |
|----|-------------|-----|
| **D24** | No multicast receiver/observer | Observer.pm |
| **D25** | No cache refresh/reload mechanism (registry is static) | ServiceManager.pm:68-72 |

### Remaining Service Lifecycle

| ID | Description | Ref |
|----|-------------|-----|
| **D10** | No graceful shutdown (drain active requests before close) | service.js:78-91 |
| **D11** | No ticket verification (parse, sig verify, expiry, privileges) | ticket.go, ticket.js |
| **D29** | Flags not filtered to announceable set during announcement building | Announcer.pm:103 |

### Remaining Config / API

| ID | Description | Ref |
|----|-------------|-----|
| **D22** | No `bus_info()` interface resolution (`if:ethN`, auto-detect) | Config.pm:59-101 |
| **D23** | Server binds to `0.0.0.0` instead of `service.address` | Server.pm:34 |
| **D28** | No high-level Requester API | Requester.pm:20-43 |
| **D31** | No `dispatch_failure` / retry on failed dispatch | requester.js:50-58 |
| **D32** | No service failure tracking / backoff | serviceMgr.js:43-52 |

### Remaining Test Coverage

| ID | Description | Impact |
|----|-------------|--------|
| **T4** | `make_auth()` duplicates production logic | Medium |
| **T6** | `test_fingerprint_of_dev_cert` silently passes when cert missing | Low |
| **T8** | No shared test helpers for common patterns | Medium |
| **T10** | No tests for RLE decode edge cases | Medium |
