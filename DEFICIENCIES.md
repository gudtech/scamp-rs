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

## Remaining Deficiencies

### Code Quality (from code review agent)

| ID | Severity | Description | File:Line |
|----|----------|-------------|-----------|
| **Q1** | Critical | `ServiceInfo::socket_addr()` panics on invalid URIs — multiple `.unwrap()` on network data | `service_info/mod.rs:22-28` |
| **Q2** | Medium | `HashMap` in announce.rs causes non-deterministic v3 class ordering in signed JSON | `announce.rs:39` |
| **Q3** | Medium | `println!` in library code (should be `log::` macros) | `config.rs:42`, `packet.rs:57` |
| **Q4** | Low | Unused `futures` dependency | `Cargo.toml:8` |
| **Q5** | Low | `listener.rs` (342) and `list.rs` (314) exceed 300-line limit | coding standards |

### Wire Protocol (Tier 2 — production correctness)

| ID | Description | Confirmed by | Ref |
|----|-------------|-------------|-----|
| **D5** | No send-side flow control (validate ACKs, pause at 65536, resume) | Perl, JS, C# | Connection.pm:177-183, connection.js:237-250 |
| **D6** | No connection idle timeout (`_adj_timeout`: busy/pending→no timeout, idle→configured) | Perl, JS | Connection.pm:131-135 |

### Discovery / Registry (Tier 2)

| ID | Description | Confirmed by | Ref |
|----|-------------|-------------|-----|
| **D7** | No cache staleness check (`discovery.cache_max_age` default 120s) | Perl, JS, C# | ServiceManager.pm:83-88 |
| **D8** | No announcement TTL/expiry (`now + sendInterval * 2.1`) | Perl, JS | ServiceManager.pm:38 |
| **D9** | No timestamp replay protection (reject older timestamps per identity) | Perl, JS | ServiceManager.pm:33-35 |
| **D26** | No service deduplication (fingerprint+identity key) | C#, JS | DiscoveryBase.cs:131-146 |
| **D24** | No multicast receiver/observer | Perl, JS | Observer.pm |
| **D25** | No cache refresh/reload mechanism (registry is static) | Perl, JS | ServiceManager.pm:68-72 |

### Service Lifecycle (Tier 2)

| ID | Description | Confirmed by | Ref |
|----|-------------|-------------|-----|
| **D10** | No graceful shutdown (drain active requests before close) | Perl, JS, C# | service.js:78-91 |
| **D11** | No ticket verification (parse, sig verify, expiry, privileges) | JS, Go, C# | ticket.go, ticket.js |
| **D18** | Per-action timeout from `t600` flags not used (value + 5s) | Perl, JS | ServiceInfo.pm:257-258 |
| **D29** | Flags not filtered to announceable set during announcement building | Perl | Announcer.pm:103 |

### Config / Behavioral (Tier 3)

| ID | Description | Confirmed by | Ref |
|----|-------------|-------------|-----|
| **D22** | No `bus_info()` interface resolution (`if:ethN`, auto-detect private IP) | Perl, C# | Config.pm:59-101 |
| **D23** | Server binds to `0.0.0.0` instead of `service.address` config | Perl, C# | Server.pm:34 |
| **D28** | No high-level Requester API (lookup+connect+request+JSON) | Perl, JS, C# | Requester.pm:20-43 |
| **D31** | No `dispatch_failure` / retry on failed dispatch | JS, C# | requester.js:50-58 |
| **D32** | No service failure tracking / backoff | JS | serviceMgr.js:43-52 |

### Test Coverage (from test review agent)

| ID | Description | Impact |
|----|-------------|--------|
| **T1** | Zero tests for server hot path (handle_connection, route_packet, dispatch_and_reply) | Critical: server correctness unverified |
| **T2** | Zero tests for client request sending (TLS connect, chunking, correlation) | Critical: client correctness unverified |
| **T3** | No wire protocol packet captures from Perl as test vectors | Critical: no byte-level proof of wire compat |
| **T4** | `make_auth()` duplicates production logic instead of testing through `load()` | Medium: false confidence |
| **T5** | `test_cache_file_announcement_iterator` makes zero assertions (always passes) | Medium: dead test |
| **T6** | `test_fingerprint_of_dev_cert` silently passes when cert missing | Low: should be `#[ignore]` |
| **T7** | `tests/basic_service.rs` entirely commented out | Low: dead code |
| **T8** | No shared test helpers/macros for common patterns (packet building, etc.) | Medium: test duplication |
| **T9** | `service_info_packet_v3_data_parsed.json` fixture unused by any test | Low: dead fixture |
| **T10** | No tests for RLE decode edge cases in `unrle()` | Medium: complex parser untested |
