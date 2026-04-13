# C# Parity Review

## Summary

Comparing scamp-rs (Rust) against gt-soa/csharp (C#) implementation. The Rust implementation covers the core SCAMP protocol with strong wire compatibility but lacks several C# features related to the higher-level service framework.

**Overall:** The Rust implementation provides solid coverage of the wire protocol, discovery, config, and crypto layers. The main gaps are in the service-side framework features (attribute-based action scanning, permission checking, RPC exception propagation) and the streaming Message abstraction that C# uses for flow-controlled data transfer.

| Area | Status |
|------|--------|
| Wire protocol (packet framing) | MATCH |
| Wire protocol (header JSON) | MATCH |
| Connection handling (client) | MATCH |
| Connection handling (server) | PARTIAL |
| Discovery (announcement parsing) | MATCH |
| Discovery (cache file / pinboard) | MATCH |
| Discovery (multicast observer) | MATCH |
| Discovery (multicast announcer) | MATCH |
| Config parsing | MATCH |
| Ticket verification | PARTIAL |
| Authorized services | MATCH |
| Crypto utilities | MATCH |
| Requester (high-level API) | PARTIAL |
| Message streaming abstraction | MISSING |
| ServiceAgent / attribute scanning | MISSING |
| RPC exception model | MISSING |
| ActionName structured type | DIVERGENT |
| WorkQueue concurrency primitive | MISSING |
| Permission checking framework | MISSING |
| Auth service communications | MISSING |

## Detailed Findings

### Wire Protocol (Packet Framing): MATCH

Both implementations use identical packet framing: `TYPE MSGNO SIZE\r\n<body>END\r\n`.

**C# (`PacketLayer.ProcessRead`):**
- Scans first 80 bytes for `\r\n` header line
- Parses 3 space-separated parts: tag, msgno, length
- Validates msgno and length as non-negative integers with canonical string representation
- Checks for `END\r\n` trailer
- MaxReadPacket = 131072

**Rust (`Packet::parse`):**
- Scans first 80 bytes for `\r\n` (matches C# 80-byte limit)
- Parses 3 whitespace-separated parts: cmd, msg_no, siz
- Validates message number and size as integers
- Checks for `END\r\n` trailer
- MAX_PACKET_SIZE = 131072

**Parity note:** Both reject bare `\n` (Rust explicitly, C# via requiring `\r\n`). Both close the connection on malformed packets. The Rust version has slightly richer error reporting (distinguishes TooShort vs NeedBytes vs Fatal) whereas C# just returns false or calls Close().

### Wire Protocol (Header JSON): MATCH

Both serialize/deserialize the packet header as JSON with the same field names.

**C# (`Protocol.packet_OnPacket`):**
- Parses header body as JSON object via `JSON.Parse().AsObject()`
- Fields: `type`, `request_id`, `action`, `version`, `envelope`, `ticket`, `identifying_token`, `error`, `error_code`, `error_data`, `client_id`
- Sends header via `JSON.Stringify(message.Header)`

**Rust (`PacketHeader`):**
- Serde-based JSON with `#[serde(rename = "type")]` for `message_type` field
- Same fields: `type`, `request_id`, `action`, `version`, `envelope`, `ticket`, `identifying_token`, `error`, `error_code`, `client_id`
- `FlexInt` handles both integer and string JSON for `request_id`/`client_id`
- `nullable_string` handles `null` values for `ticket`/`identifying_token`
- `error`/`error_code` use `skip_serializing_if = "Option::is_none"`

**Difference:** C# has `error_data` as a JObject field on RPCException headers. Rust does not serialize `error_data`. This is a minor gap; `error_data` is used for structured error details and dispatch_failure signaling in C#.

### Connection Handling (Client): MATCH

**C# (`BEEPishSOAClient`):**
- Connection pooling by URI (`connections` dictionary)
- TLS with certificate thumbprint verification
- Sequential request IDs (`nextid++`)
- Pending requests tracked by request_id string
- Timeout per request with `Timer`
- Handles `dispatch_failure` via `RPCException.DispatchFailure()`

**Rust (`BeepishClient` + `ConnectionHandle`):**
- Connection pooling by URI (`connections` HashMap)
- TLS with certificate fingerprint verification
- Sequential request IDs (`next_request_id.fetch_add(1)`)
- Pending requests tracked by request_id i64
- Timeout via `tokio::time::timeout`
- Flow control watermark (65536 bytes) with ACK-based backpressure

**Parity note:** Both implement connection pooling, TLS fingerprint verification, request timeouts, and pending request tracking. The Rust version additionally implements send-side flow control (D5b watermark), which C# handles through the Message.StreamData/Ack abstraction instead.

### Connection Handling (Server): PARTIAL

**C# (`BEEPishSOAServer`):**
- Binds to random port in 30100-30399 range (configurable via `beepish.first_port`, `beepish.bind_tries`)
- Accepts TLS connections with `SslStream.BeginAuthenticateAsServer`
- Uses `Protocol` class for packet handling
- Delegates request handling to `reqh` callback
- Sets `NoDelay = true` on accepted sockets

**Rust (`ScampService`):**
- Binds to random port in 30100-30399 range (hardcoded, 20 tries)
- Accepts TLS connections with `TlsAcceptor`
- Inline packet handling in `server_connection::handle_connection`
- Registered action handlers via HashMap
- Sets `set_nodelay(true)` on accepted sockets
- Graceful shutdown with 30s drain timeout

**Gaps:**
- C# bind port range is configurable (`beepish.first_port`, `beepish.bind_tries`); Rust hardcodes these
- C# server idle timeout is not explicitly implemented (relies on OS); Rust implements 120s idle timeout matching Perl
- C# `Protocol` has a general-purpose `OnMessage` event; Rust dispatches directly to registered handlers

### Discovery (Announcement Parsing): MATCH

**C# (`ServiceInfo` constructor):**
- Splits on `\n\n` into 3+ hunks: JSON, cert PEM, signature
- Parses JSON as 9-element array: [version, identity, sector, weight, sendInterval, uri, envelopes+extensions, actions, timestamp]
- v3 actions: array of [namespace, [name, flags, ?version], ...]
- Signature verification: `RSA.VerifyData(signedData, "SHA256", signature)`
- Fingerprint via `CryptoUtils.Fingerprint(cert)` (colon-separated hex)

**Rust (`AnnouncementPacket::parse` + `AnnouncementBody::parse`):**
- Splits on `\n\n` into 3 parts: JSON blob, cert PEM, signature
- Parses JSON as 9-element array with identical field positions
- v3 actions parsed in `parse_v3_actions` with same structure
- v4 actions parsed with RLE decoding (`unrle`)
- Signature verification via `verify_rsa_sha256` (RSA PKCS1v15 SHA256)
- Fingerprint via `cert_sha1_fingerprint` (colon-separated uppercase hex)

**Difference:** Rust also implements v4 action parsing with full RLE decode support. C# stores v4 extensions as a raw JObject but does not decode v4 action vectors. This makes Rust slightly more capable for v4 announcements.

### Discovery (Cache File / Pinboard): MATCH

**C# (`PinboardDiscovery`):**
- Reads `discovery.cache_path` from config
- Splits on `\n%%%\n` delimiter
- Staleness check: `discovery.cache_max_age` (default 120s)
- Stash/restore pattern to reuse existing ServiceInfo objects

**Rust (`CacheFileAnnouncementIterator` + `ServiceRegistry`):**
- Reads `discovery.cache_path` from config
- Streaming iterator splitting on `\n%%%\n` delimiter
- Staleness check: `discovery.cache_max_age` (default 120s)
- Replay protection via `seen_timestamps` HashMap
- TTL/expiry check: `timestamp + interval * 2.1`

**Difference:** Rust implements replay protection and TTL expiry which C# does not (C# re-reads the full cache each time). Rust uses a streaming iterator instead of reading the entire file into memory.

### Discovery (Multicast Observer): MATCH

**C# does not have a standalone multicast observer** -- `DiscoveryBase` is abstract and `PinboardDiscovery` reads from a file. However, the `MulticastAnnouncer` handles the send side.

**Rust (`observer.rs`):**
- Joins multicast group on specified interface
- Receives UDP packets, strips R/D prefix byte
- Zlib decompresses
- Parses announcement and injects into registry
- Graceful shutdown via watch channel

This is an area where Rust has **more** than C# -- the C# implementation relies on an external process (likely Perl) to maintain the discovery cache file, while Rust can receive multicast directly.

### Discovery (Multicast Announcer): MATCH

**C# (`MulticastAnnouncer`):**
- Creates UdpClient per discovery address
- Joins multicast group
- Timer-based sending at `SendInterval` (default 5000ms)
- Zlib compression via `CryptoUtils.ZlibCompress`
- Shutdown mode: sets weight=0, changes interval to 100ms

**Rust (`multicast.rs`):**
- Creates UDP socket via socket2
- Sets multicast interface
- Async loop with configurable interval (default 5s)
- Zlib compression via flate2
- Shutdown mode: sends weight=0 for 10 rounds at 1s intervals

**Difference:** C# shutdown uses 100ms interval (aggressive), Rust uses 1s for 10 rounds (matching Perl). Both produce zlib-compressed signed announcement packets.

### Config Parsing: MATCH

**C# (`ConfigFile`):**
- Reads from file, strips `#` comments
- Splits on `=` for key/value
- First-wins for duplicate keys (logs warning)
- `Get(key, default)` and `GetInt(key, default)` accessors

**Rust (`config.rs`):**
- Reads from file, strips `#` comments (including inline)
- Splits on `=` for key/value
- First-wins for duplicate keys
- Hierarchical key navigation via dot-separated segments
- Supports numeric indices for array-like config
- `get<T>(key)` generic accessor with `FromStr` parsing
- Config path resolution: `SCAMP_CONFIG` env, `GTSOA` env, default paths, `~/GT/backplane/etc/soa.conf`

**Difference:** Rust supports hierarchical/nested config tree and value rewrites. C# uses flat key-value. Both handle comments and first-wins semantics identically.

### Ticket Verification: PARTIAL

**C# (`Ticket`):**
- Parses CSV: `version,user_id,client_id,timestamp,ttl,privs,signature`
- RSA SHA256 signature verification via `CryptoUtils.ParseX509PublicKey`
- Base64URL decoding for signature
- Privilege checking: `HasPrivilege(uint)` and `HasPrivilege(string)` (via authz table lookup)
- AuthzTable: fetches privilege name-to-ID mapping from Auth.getAuthzTable service
- API key support: `ForApiKey(string)` calls User.login and Auth.authorize
- FromFile: reads ticket from file

**Rust (`ticket.rs`):**
- Parses CSV with same field order
- RSA SHA256 PKCS1v15 signature verification via openssl
- Base64URL decoding for signature
- Privilege checking: `has_privilege(u64)` and `has_all_privileges(&[u64])`
- Expiry validation (not-yet-valid and expired checks)

**Gaps:**
- No AuthzTable integration (cannot look up privilege names, only IDs)
- No API key support
- No `FromFile` utility
- No string-based privilege checking (only numeric IDs)
- C# uses `uint` for IDs; Rust uses `u64` (compatible but different)

### Authorized Services: MATCH

**C# (`AuthorizedServices`):**
- Parses fingerprint + token list from file
- Tokens with `:` split into sector:name; without `:` default to `main:`
- `:ALL` becomes `.*`; others get `(?:\.|$)` suffix
- Case-insensitive regex matching
- Hot-reload via mtime check

**Rust (`authorized_services.rs`):**
- Same parsing logic: fingerprint + comma-separated tokens
- Same `:` handling: with colon splits sector:name; without defaults to `main:`
- `:ALL` replaced with `:.*`; `(?:\.|$)` suffix on others
- Case-insensitive regex matching
- Hot-reload via mtime check
- `_meta.*` actions always authorized (matches Perl)
- Rejects actions/sectors containing `:`

**Parity note:** Nearly identical logic. Rust additionally handles `_meta.*` always-authorized and colon-rejection rules (from Perl spec). C# does not have these checks but may rely on other layers.

### Crypto Utilities: MATCH

**C# (`CryptoUtils`):**
- `Fingerprint`: cert thumbprint with colon separation
- `StripPEM`: removes PEM headers, base64 decodes
- `ZlibCompress`: manual zlib header + DeflateStream + Adler32
- `Base64Folded`: line-wrapped base64 with optional PEM headers
- `ToBase64URL` / `FromBase64URL`: URL-safe base64
- `ParseX509PublicKey` / `ParseX509PrivateKey`: ASN.1 parsing of PEM keys
- `LoadKeyPair`: loads cert+key from files

**Rust (`crypto.rs`):**
- `cert_sha1_fingerprint`: SHA1 of DER, colon-separated uppercase hex
- `pem_to_der`: strips PEM headers, base64 decodes
- `verify_rsa_sha256`: RSA PKCS1v15 SHA256 signature verification
- `cert_pem_fingerprint`: PEM -> DER -> SHA1 fingerprint

**Difference:** C# has more crypto utilities (ASN.1 parser, key pair loading, Adler32). Rust delegates to openssl for key operations. Both produce the same fingerprint format.

### Requester (High-Level API): PARTIAL

**C# (`Requester`):**
- Static class with connection pooling
- `MakeRequest`: discovery lookup -> connect -> send
- `MakeJsonRequest`: JSON encode/decode convenience
- `SyncJsonRequest`: blocking synchronous wrapper
- Handles `dispatch_failure` in RPCException error_data
- Sets `action`, `version`, `envelope` on request header
- Timeout from ActionInfo with +5s padding on client side

**Rust (`Requester`):**
- Instance-based with connection pooling
- `request`: discovery lookup -> connect -> send
- `request_with_opts`: full parameter control
- D31 retry on `dispatch_failure` (marks service failed, retries once)
- Timeout from ActionInfo flags (t600 -> 605s) with default 75s

**Gaps:**
- No JSON convenience methods (MakeJsonRequest/SyncJsonRequest)
- No blocking synchronous API (async-only)
- Missing `error_data` field in response handling
- C# has `TargetIdent` option for routing to specific service identity; Rust does not

### Message Streaming Abstraction: MISSING

**C#** has a sophisticated `Message` class that provides:
- Streaming production: `AddData(buf, offset, len)` + `End(error)`
- Streaming consumption: `Consume(DataDelegate, EndDelegate)`
- All-at-once consumption: `Consume(maxBuffer, FullDelegate)` with buffer limit protection
- Flow control: `BeginStream(AckDelegate)` with windowed ACK-driven sending (`WINDOW = 131072`)
- `StreamData(byte[], error)` convenience for immediate data
- `Discard()` for unwanted messages

**Rust** does not have an equivalent abstraction. The Rust implementation:
- Accumulates the entire request body in memory before dispatching
- Sends the entire reply body in DATA chunks after handler completes
- Has flow control (ACK watermark) but at the connection level, not per-message

This is the **most significant architectural difference**. The C# Message class allows streaming both directions, enabling processing of arbitrarily large payloads without buffering the entire body. Rust buffers everything.

### ServiceAgent / Attribute Scanning: MISSING

**C# (`ServiceAgent`):**
- Reflection-based action scanning via `[RPC]`, `[RPCNamespace]`, `[RPCService]` attributes
- Automatic action registration from assembly metadata
- `Execute()` method with JSON request/response parsing
- `CheckPermissions()` with ticket verification and privilege demands
- Signal handling (SIGTERM/SIGINT) for graceful shutdown
- Running service file creation/deletion
- WorkQueue-based request processing with bounded concurrency

**Rust:**
- Manual action registration via `service.register("action", version, handler)`
- No attribute/macro-based scanning
- No automatic permission checking framework
- Signal handling not built-in (relies on caller)

### RPC Exception Model: MISSING

**C# (`RPCException`):**
- Structured error type with `ErrorCode`, `ErrorMessage`, `ErrorData`
- Serializes to JSON header with `error_code`, `error`, `error_data`
- `DispatchFailure()` factory for dispatch_failure error_data
- `Wrap(Exception)` for converting arbitrary exceptions
- Deserializes from header: `new RPCException(JObject header)`

**Rust:**
- Uses `anyhow::Error` for general errors
- `ScampReply::error(message, code)` for reply errors
- No structured error_data support
- No dispatch_failure data structure

### ActionName Structured Type: DIVERGENT

**C# (`ActionName`):**
- Structured type with `Sector`, `Namespace`, `Name`, `Version` fields
- Case-insensitive equality via lowercased `identity` string
- Text format: `sector:namespace.name~version` (main sector omits `sector:`)
- `TryParse` / `Parse` static methods
- Used throughout for type-safe action identification

**Rust:**
- Actions identified by flat strings: `path` (e.g., `product.sku.fetch`), `version`, `sector`
- Registry key format: `sector:path.vVERSION`
- Pathver format: `path~version`
- No structured ActionName type

**Impact:** The C# approach provides stronger type safety and consistent parsing. The Rust approach is simpler but requires manual string formatting for lookups.

### WorkQueue Concurrency Primitive: MISSING

**C# (`WorkQueue`):**
- Bounded-concurrency work queue on thread pool
- Configurable concurrency limit and max queue length
- Used by ServiceAgent (concurrency=20) and Protocol (concurrency=1 for sequencing)

**Rust:**
- Uses tokio channels (mpsc) and tasks for concurrency
- No explicit bounded-concurrency work queue abstraction
- Server connection handles requests sequentially (single-task reader loop)

### Permission Checking Framework: MISSING

**C# (`ServiceAgent.CheckPermissions`):**
- Verifies ticket on each request
- Looks up permission demands from Auth.getAuthzTable
- Checks ticket privileges against demands
- Handles `noauth` flag to skip auth
- Extracts `identifying_token` and `client_id` from ticket/header

**Rust:**
- No built-in permission checking
- Ticket parsing and verification exists but is not integrated into request dispatch
- Handler functions receive raw `ScampRequest` with ticket string; checking is left to the handler

## Critical Gaps

1. **Message Streaming Abstraction** -- The C# Message class enables streaming large payloads without full buffering. Rust buffers entire request/response bodies in memory. For large payloads this could be a memory concern.

2. **RPC Exception with error_data** -- The C# error model carries structured `error_data` including `dispatch_failure` signaling. Rust checks `error_code == "dispatch_failure"` but cannot propagate or receive `error_data`.

3. **Permission Checking Integration** -- C# automatically verifies tickets and checks privilege demands before handler execution. Rust leaves this entirely to handler implementations.

4. **ServiceAgent Framework** -- C# provides a complete service hosting framework with attribute scanning, signal handling, running-service file management, and bounded concurrency. Rust requires manual setup of all these pieces.

## Recommendations

1. **Add `error_data` to PacketHeader** -- Add an optional `error_data` field (serde_json::Value) to support structured error propagation and dispatch_failure signaling. Low effort, high compatibility impact.

2. **Consider streaming body support** -- For large payloads, consider a streaming body API (e.g., `AsyncRead`/`AsyncWrite` on request/response bodies) instead of buffering everything. This is a significant architectural change but important for parity with C#'s Message class.

3. **Integrate ticket verification into server dispatch** -- Add an optional middleware/wrapper that verifies tickets and checks privileges before calling the handler. This doesn't need to be mandatory but should be available.

4. **Add ActionName structured type** -- A structured `ActionName { sector, namespace, name, version }` type would improve type safety and make the codebase more consistent with other SCAMP implementations.

5. **Make server bind config configurable** -- The hardcoded port range (30100-30399) and bind tries (20) should be read from config (`beepish.first_port`, `beepish.bind_tries`) to match C#.

6. **Add RPCException equivalent** -- A structured SCAMP error type with error_code, error_message, and error_data would make error handling more ergonomic and wire-compatible.
