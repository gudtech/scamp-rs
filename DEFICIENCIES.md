# scamp-rs Deficiency Report

Comprehensive parity audit against all four reference implementations.
Generated 2026-04-12. Updated after M1-M3 completion and 4-agent verification pass.

Agents: verify-vs-perl, verify-vs-js, verify-vs-go, verify-vs-csharp.

## Verified Correct (no action needed)

These areas were confirmed as matching across all implementations:

- Packet framing: `TYPE MSGNO SIZE\r\n<body>END\r\n` format
- PacketHeader JSON: field names, `"type"` rename, FlexInt, EnvelopeFormat, MessageType
- Message assembly: HEADER→DATA*→EOF/TXERR, sequential msgno from 0
- Request correlation: sequential request_id from 1, pending map by request_id
- Reply: `type="reply"`, request_id copied from request
- ACK format: decimal string of cumulative bytes
- EOF body: validated as empty
- PING/PONG: responds to PING, disabled by default (correct: Perl/Go don't support)
- TLS fingerprint verification: SHA1 of DER cert, colon-separated uppercase hex
- Natural corking: verification before stream split (equivalent to Perl _corked)
- RSA PKCS1v15 SHA256 signatures: verified against live Perl-generated cache
- authorized_services: file parsing, regex matching, `_meta.*` exception, `:` rejection, hot-reload
- Action index key: `sector:action.vVERSION` (lowercased)
- CRUD aliases: `_destroy` tag (fixed from `_delete`)
- V4 accompat filter: enabled (fixed from commented-out)
- RLE decoding for v4 action vectors
- Service identity: `name:base64(18 random bytes)`
- Weight=0 filtering in registry lookups
- Envelope filtering at lookup time
- Connection pooling by URI

## Remaining Deficiencies

### Tier 1: ~~Blocks next milestone (multicast announcing)~~ RESOLVED

| ID | Description | Status |
|----|-------------|--------|
| **D1** | ~~No multicast announcement sending~~ | **Fixed**: UDP multicast via socket2, periodic sending |
| **D2** | ~~No zlib compression~~ | **Fixed**: flate2 Compression::best() |
| **D3** | ~~Config keys not read~~ | **Fixed**: `discovery.multicast_address`, `discovery.port` |
| **D4** | ~~No V4 extension hash~~ | **Fixed**: RLE-encoded action vectors in envelopes array |

### Tier 2: Needed for production correctness

| ID | Description | Confirmed by | Perl ref |
|----|-------------|-------------|----------|
| **D5** | No send-side flow control (validate ACKs, pause at 65536, resume) | Perl, JS, C# | Connection.pm:177-183 |
| **D6** | No connection idle timeout (`_adj_timeout`: busy/pending→no timeout, idle→configured) | Perl, JS | Connection.pm:131-135 |
| **D7** | No cache staleness check (`discovery.cache_max_age` default 120s) | Perl, JS, C# | ServiceManager.pm:83-88 |
| **D8** | No announcement TTL/expiry (`now + sendInterval * 2.1`) | Perl, JS | ServiceManager.pm:38 |
| **D9** | No timestamp replay protection (reject older timestamps per identity) | Perl, JS | ServiceManager.pm:33-35 |
| **D10** | No graceful shutdown (weight=0 announce, drain, close, SIGTERM/SIGINT) | Perl, JS, C# | Announcer.pm:96-101 |
| **D11** | No ticket verification (parse, sig verify, expiry, privileges) | JS, Go, C# | Go ticket.go, JS ticket.js |
| **D12** | Reader task doesn't set `closed` flag on exit | Perl agent | client.rs:299-373 |
| **D13** | ~~Unknown packet types: Drop instead of Fatal~~ | **Fixed**: now Fatal | Connection.pm:187 |
| **D14** | ~~Malformed HEADER JSON: Drop instead of Fatal~~ | **Fixed**: now Fatal | Connection.pm:148-149 |
| **D15** | `ticket`/`identifying_token`/`action` omitted from JSON when empty (Go always serializes) | Go agent | packetheader.go:27,33-34 |

### Tier 3: Behavioral parity / config

| ID | Description | Confirmed by | Perl ref |
|----|-------------|-------------|----------|
| **D16** | Header line parsing accepts bare `\n` (Perl requires `\r\n`) | Perl agent | Connection.pm:46 |
| **D17** | Three timeouts conflated: `beepish.server_timeout` (120s), `beepish.client_timeout` (90s), `rpc.timeout` (75s) | Perl, C# | Client.pm:40, Server.pm:58 |
| **D18** | Per-action timeout from `t600` flags not extracted/used (value + 5s) | Perl, JS | ServiceInfo.pm:257-258 |
| **D19** | Config: duplicate key handling (Rust last-wins vs Perl first-wins) | Perl agent | Config.pm:30-31 |
| **D20** | Config: `GTSOA` env var not checked (Perl canonical env var) | Perl agent | Config.pm:40 |
| **D21** | Config: inline `# comments` not stripped mid-line | Perl agent | Config.pm:20 |
| **D22** | No `bus_info()` interface resolution (`if:ethN` syntax, auto-detect private IP) | Perl, C# | Config.pm:59-101 |
| **D23** | Server binds to `0.0.0.0` instead of `service.address` config | Perl, C# | Server.pm:34 |
| **D24** | No multicast receiver/observer | Perl, JS | Observer.pm |
| **D25** | No cache refresh/reload mechanism (registry is static after construction) | Perl, JS | ServiceManager.pm:68-72 |
| **D26** | No service deduplication (fingerprint+identity key) | C# agent | DiscoveryBase.cs:131-146 |
| **D27** | TXERR body not validated (empty/"0" should error) | JS agent | connection.js:229 |
| **D28** | No high-level Requester API (lookup+connect+request+JSON in one call) | Perl, JS, C# | Requester.pm:20-43 |
| **D29** | Flags not filtered to announceable set during announcement building | Perl agent | Announcer.pm:103 |
| **D30** | Per-packet flush inefficiency (should batch) | Perl agent | Connection.pm:200 |
| **D31** | No `dispatch_failure` / retry on failed dispatch | JS, C# | requester.js:50-58 |
| **D32** | No service failure tracking / backoff | JS agent | serviceMgr.js:43-52 |
