# Perl Parity Review

## Summary

scamp-rs is a substantial and largely correct Rust reimplementation of the canonical Perl SCAMP stack (GTSOA). The wire protocol, discovery, config parsing, authorization, and service lifecycle are all present and well-tested. However, several behavioral gaps remain that range from minor interop risks to critical missing features that could cause runtime failures in production.

**Overall assessment**: PARTIAL parity. Core request/response flow works and has been verified against live Perl services. Several edge cases and one major feature (Auth.getAuthzTable privilege checking) are missing.

---

## Detailed Findings

### 1. Wire Protocol (Packet Framing, Header JSON, ACK, EOF, TXERR, PING/PONG): MATCH

The Rust implementation closely matches Perl `Connection.pm` for packet framing.

**Matching behaviors:**
- Frame format `TYPE MSGNO SIZE\r\n<body>END\r\n` — exact match (Perl Connection.pm:194-195)
- Header line length limit of 80 bytes — matches Perl Connection.pm:46
- `\r\n` required (bare `\n` rejected) — matches Perl, tested
- HEADER body is JSON, malformed JSON is fatal — matches Perl Connection.pm:148-149
- DATA chunking at 2048 bytes — matches Perl Connection.pm:218
- EOF body must be empty — matches Perl Connection.pm:162
- ACK body is decimal byte count string — matches Perl Connection.pm:179
- ACK pointer validation (no backward, no past end) — matches Perl Connection.pm:177-183
- TXERR terminates the message with error text — matches Perl Connection.pm:171-173
- Unknown packet type is fatal — matches Perl Connection.pm:187
- Sequential message numbering enforced — matches Perl Connection.pm:141
- JSON field name `"type"` (not `"message_type"`) — correct for wire compat
- `FlexInt` accepts both JSON integers and strings — matches Go's `flexInt`
- `null` ticket/identifying_token handled via `nullable_string` deserializer — matches Perl's `undef` encoding

**PING/PONG:**
- Rust implements PING/PONG response (server replies PONG to PING) — this is NOT in the Perl Connection.pm (which treats unknown types as fatal). PING/PONG originates from the JS/Go implementations. The Perl impl would reject PING with a fatal error. This is **safe** since Perl never sends PING, but the Rust server would handle it if a JS/Go client sends one.

**Minor divergence:**
- Perl Connection.pm sends `TXERR` when a message has an error (`defined($msg->error)`); Rust checks `body_str.is_empty() || body_str == "0"` for invalid TXERR and ignores it. Perl does NOT validate TXERR body content; it just passes it through. The Rust validation is stricter than Perl.
- MAX_PACKET_SIZE is 131072 in Rust. Perl has no explicit max — it uses the regex match to parse, which implicitly handles arbitrarily large packets. In practice 128KB is fine.

### 2. Connection Handling (TLS, Fingerprint Verify, Idle Timeout, Flow Control): PARTIAL

**Matching behaviors:**
- TLS with `danger_accept_invalid_certs(true)` — matches Perl's self-signed cert usage
- SHA1 fingerprint verification after TLS handshake — matches Perl Connection.pm:61-68
- Client connection timeout: 90s default — matches Perl Client.pm `beepish.client_timeout` default of 90
- Server connection timeout: 120s default — matches Perl Server.pm `beepish.server_timeout` default of 120
- Idle timeout disabled when busy (incoming or outgoing messages active) — matches Perl Connection.pm:131-135 `_adj_timeout`
- `TCP_NODELAY` enabled — matches Perl Connection.pm `no_delay => 1`

**Gaps:**
- **No corking**: Perl Connection.pm:196-201 implements write corking during TLS handshake — data is buffered until `on_starttls` fires and fingerprint is verified, then flushed. Rust connects, verifies fingerprint, then creates the `ConnectionHandle`, so there is natural corking (no packets sent before fingerprint check), but this is achieved differently. The Rust approach is correct but subtly different in timing.
- **No `on_lost` callback propagation**: Perl Client.pm:45-52 delivers "Connection lost" errors to all pending requests when the connection drops. Rust does this via `notify_all_pending` in the reader task — functionally equivalent.
- **No `busy` flag management**: Perl Connection.pm:24 has a `busy` attribute that controls timeout adjustment. Rust checks `!incoming.is_empty() || !outgoing.is_empty()` directly — functionally equivalent.
- **Flow control watermark (65536 bytes)**: Rust implements send-side flow control matching JS `connection.js:4` watermark. Perl does NOT implement send-side flow control — it relies on TCP backpressure. This is an improvement, not a regression.
- **No connection reconnection**: Perl Client.pm uses `on_lost` callback to remove connections from the pool. Rust does the same via the `closed` AtomicBool. Neither implementation attempts automatic reconnection. Match.

### 3. Config Parsing (First-Wins, Inline Comments, Env Vars, if:ethN): PARTIAL

**Matching behaviors:**
- `#` comment stripping (both full-line and inline) — matches Perl Config.pm:20
- First-wins for duplicate keys — matches Perl Config.pm:30-31
- `key = value` format with whitespace trimming — matches Perl Config.pm:23
- `GTSOA` env var for config path — matches Perl Config.pm:40
- `if:ethN` interface resolution — matches Perl Config.pm:89-93
- Default multicast group `239.63.248.106` and port `5555` — matches Perl Config.pm:109-110
- Private IP preference (10.x > 192.168.x) — matches Perl Config.pm:71

**Gaps:**
- **Flat vs hierarchical storage**: Perl stores config as a flat `%hash` with dotted keys (e.g., `$hash{"bus.address"}`). Rust parses into a nested tree structure with `.` as a path separator. For standard lookups this is equivalent, but the Perl implementation treats `foo.bar` as a single opaque key, while Rust navigates a tree. If a config file has both `foo.bar = x` and `foo = y`, Perl treats these as two independent keys; Rust would create a node `foo` with a value and a child `bar`. This could cause subtle differences for unusual config files.
- **Numeric path segments**: Rust treats numeric segments as array indices (`current.list[num]`). Perl has no such concept — `0.thing = val` would be key `"0.thing"`. This is a divergence, though unlikely to matter in practice.
- **UTF-8 handling**: Perl Config.pm:15 explicitly reads with `:encoding(utf8)`. Rust uses `read_to_string` which requires valid UTF-8 but doesn't do any encoding conversion. Match in practice for well-formed files.
- **`SCAMP_CONFIG` env var**: Rust adds a `SCAMP_CONFIG` env var and `dotenv` support that Perl does not have. This is an extension, not a breaking change.
- **`bus.address` vs `service.address`**: Perl Config.pm:103-108 resolves `bus.address` as the common fallback, then checks `discovery.address` and `service.address` separately. Rust `BusInfo::from_config` reads `bus.address` for service and `discovery.address` for discovery, with the same private-IP fallback. The Perl config key `service.address` is NOT checked by Rust — it only reads `bus.address`. This could matter if someone sets `service.address` separately.

### 4. Discovery (Cache Loading, Announcement Format, Signatures, TTL, Replay Protection): PARTIAL

**Matching behaviors:**
- Cache file format: `\n%%%\n` delimiter — matches Perl ServiceManager.pm:92
- Announcement format: `json_blob\n\ncert_pem\nsig_base64\n` — matches Perl ServiceInfo.pm:42
- v3 announcement array: `[3, ident, sector, weight, interval_ms, uri, [envelopes..., v4_hash], v3_actions, timestamp]` — matches Perl Announcer.pm:179-190
- SHA1 fingerprint computation (uppercase hex, colon-separated) — matches Perl ServiceInfo.pm:82-87
- RSA PKCS1v15 SHA256 signature verification — matches Perl (note: Perl's `use_pkcs1_oaep_padding` is a no-op for verify; PKCS1v15 is the actual padding used)
- RLE decoding for v4 action vectors — matches Perl ServiceInfo.pm:233-248
- v4 `accompat != 1` filtering — matches Perl ServiceInfo.pm:220
- Replay protection via `fingerprint + identity` timestamp dedup — matches Perl ServiceManager.pm:29-35
- TTL expiry: `now > timestamp + interval * 2.1` — matches Perl ServiceManager.pm:38
- Cache staleness check via mtime — matches Perl ServiceManager.pm:83-88 (but Perl treats stale cache as an error and stops; Rust logs a warning and continues)
- CRUD aliases (`_create`, `_read`, `_update`, `_destroy`) — matches Perl ServiceInfo.pm:191-192
- v3 action path construction: `namespace.name` lowercased — matches Perl ServiceInfo.pm:185-186

**Gaps:**
- **Zlib decompression prefix stripping**: Rust strips `R` or `D` prefix byte before decompressing — matches Perl Observer.pm:51-52. However, Perl falls back to treating uncompressable data as raw text (Observer.pm:57: `$ubuffer = $cbuffer`). Rust does NOT have this fallback — it would error on non-zlib data after stripping the prefix.
- **Announcement building: v3actions=null vs missing**: When Rust has no v3 actions, it sends `Value::Null` at position 7. Perl sends `undef` (which JSON-encodes as `null`). However, the Perl code only populates `v3actions` conditionally: `@v3classes ? (v3actions => \@v3classes) : ()` — when there are no v3 classes, the hash key is absent, and `delete $pkthash->{v3actions}` returns `undef` which becomes JSON `null`. So both produce `null` at position 7. Match.
- **Announcement building: actions only go to v3 zone**: The Rust announcer puts ALL actions into the v3 compatibility zone and produces EMPTY v4 vectors. Perl Announcer.pm:140-156 splits actions between v3 (when no custom sector/envelopes) and v4. In practice the Rust approach means all actions appear in v3 format and the v4 extension is empty — receivers that parse v3 will see all actions, receivers that only parse v4 will see none. This is safe for backward compat but means v4-only consumers would miss actions. Since no known consumer is v4-only, this is low risk.
- **Dynamic observer injection**: Rust observer injects into the registry but does NOT reuse previously parsed announcements. Perl ServiceManager.pm:94 passes `\%old` for caching. Minor performance difference, not a correctness issue.
- **Cache reload throttling**: Perl ServiceManager.pm:68-72 throttles `fill_from_cache` to at most once per second. Rust has no such throttle.
- **`discovery.cache_max_age` error behavior**: Perl ServiceManager.pm:86-88 sets `fill_error` and returns early when cache is stale, preventing any lookups. Rust logs a warning but continues to load the stale cache. This means Rust will serve from a stale cache while Perl would fail requests.

### 5. Service Lifecycle (Bind, Accept, Dispatch, Graceful Shutdown, Announcing): PARTIAL

**Matching behaviors:**
- Random port binding in 30100-30399 range, 20 tries — matches Perl Server.pm:27-29
- URI format `beepish+tls://ip:port` — matches Perl Server.pm:36
- Identity format `name:base64(18 random bytes)` — matches Perl Announcer.pm:53
- Announcement interval: 5 seconds default — matches Perl Announcer.pm:40
- Shutdown: 10 rounds of weight=0 at 1s intervals — matches Perl Announcer.pm:93, 82-94
- Reply sends HEADER + DATA chunks + EOF — matches Perl Connection.pm:210-228
- Zlib level 9 compression for multicast — matches Perl Announcer.pm:203

**Gaps:**
- **No per-action `sector` or `envelopes` override**: Perl Announcer.pm:140-156 supports per-action sector and envelope overrides (for v4 actions). Rust `RegisteredAction` has no sector/envelope fields — all actions inherit the service's sector and envelopes. This means Rust cannot announce actions in multiple sectors from a single service.
- **No `busy` tracking in server**: Perl Server.pm:64-66 tracks `$count` of in-flight requests and sets `conn->busy(!!$count)`, which controls timeout behavior. Rust server_connection.rs checks `!incoming.is_empty() || !outgoing.is_empty()` for the timeout, which is functionally equivalent but at the connection level rather than the service level.
- **Connection draining on shutdown**: Rust ScampService::run has a 30-second drain timeout. Perl does not have explicit connection draining — it relies on the announcer's shutdown rounds to let in-flight requests complete. The Rust approach is more robust.
- **Announcement weight/interval not configurable per-service**: Rust hardcodes weight=1, interval=5 in `build_announcement_packet`. Perl Announcer.pm has configurable `weight` and `interval` attributes.

### 6. Authorization (authorized_services Parsing, _meta Exception, Regex Matching): MATCH

**Matching behaviors:**
- File format: `FINGERPRINT token1, token2` — matches Perl ServiceInfo.pm:128-130
- Comment stripping (`#`) — matches Perl ServiceInfo.pm:124-126
- `quotemeta` equivalent: `regex::escape` — matches Perl ServiceInfo.pm:131
- `:ALL` replaced with `:.*` — matches Perl ServiceInfo.pm:132
- No colon → prepend `main:` — matches Perl ServiceInfo.pm:132
- Full regex: `(?i)^(?:pattern)(?:\.|$)` — matches Perl ServiceInfo.pm:135
- `_meta.*` actions always authorized — matches Perl ServiceInfo.pm:146-147
- Actions/sectors containing `:` rejected — matches Perl ServiceInfo.pm:149
- Check: `"$sector:$action" =~ /$rx/` — matches Perl ServiceInfo.pm:163
- Case insensitive matching — matches Perl ServiceInfo.pm:135 `/i`
- Hot-reload on mtime change — matches Perl ServiceInfo.pm:117-118

**One difference in authorization flow:**
- Perl ServiceInfo.pm:150-153: If the service has an invalid signature (`!$self->verified`), the action is NOT authorized regardless of the authorized_services file. Rust `inject_packet` in ServiceRegistry rejects packets with invalid signatures entirely (line 82-84), so unauthorized unverified services never make it into the registry. Functionally equivalent — unsigned services are rejected in both, just at different points.

### 7. Ticket Verification (Format, RSA Verify, Expiry, Privileges): PARTIAL

**Matching behaviors:**
- CSV format: `version,user_id,client_id,validity_start,ttl,privs,signature` — matches JS ticket.js
- Version must be 1 — matches JS ticket.js:23
- Privileges are `+`-separated IDs — matches JS ticket.js:43
- RSA PKCS1v15 SHA256 verification — matches JS ticket.js:27
- Base64URL decoding for signature — matches JS ticket.js:24
- Expiry check: `now < validity_start` and `now >= validity_start + ttl` — matches JS ticket.js `expired()` function
- Signed data is everything before the last comma — matches JS ticket.js:26

**Gaps:**
- **Public key source**: Rust `Ticket::verify` takes a `public_key_pem` parameter. JS ticket.js:26 hardcodes `/etc/GTSOA/auth/ticket_verify_public_key.pem`. Rust does not read this file automatically — the caller must provide the key. There is no config integration for the ticket verify key path.
- **No `checkAccess` / Auth.getAuthzTable integration**: This is the CRITICAL gap (see Critical Gaps below). Rust can parse and verify ticket signatures, but there is no mechanism to check whether a ticket's privileges grant access to a specific action. JS ticket.js:51-93 calls `Auth.getAuthzTable` to get a mapping of actions to required privilege IDs, then checks the ticket's privileges against that table. Rust has `has_privilege()` and `has_all_privileges()` but there is no code to determine WHICH privileges an action requires.

### 8. Requester API (Lookup, Connect, Timeout, Retry on dispatch_failure): PARTIAL

**Matching behaviors:**
- Discovery lookup by sector:action.vVERSION — matches Perl ServiceManager.pm lookup
- Default sector "main" — matches Perl Requester.pm:15
- Default RPC timeout 75s — matches Perl ServiceInfo.pm:257 `GTSOA::Config->val('rpc.timeout', 75)`
- Per-action timeout from `tN` flags with +5s padding — matches Perl ServiceInfo.pm:258
- Connection pooling by URI — matches Perl ConnectionManager.pm:14-28
- Retry on `dispatch_failure` error code — matches JS behavior

**Gaps:**
- **No `ident` parameter for targeted routing**: Perl Requester.pm:25 passes `$rqparams->{ident}` to lookup, which can target a specific worker identity. Rust Requester has no equivalent — it always routes randomly.
- **No `fill_error` handling**: Perl Requester.pm:28-29 returns the `fill_error` if the cache failed to load (stale cache). Rust does not propagate this — it would return "Action not found" instead of a more specific stale-cache error.
- **`simple_request` is synchronous in Perl**: Perl Requester.pm:78 blocks with `$reply->recv`. Rust is fully async. This is fine for API design but means usage patterns differ.
- **No `binary_send` or `raw_recv` modes**: Perl simple_async_request supports `binary_send` (skip JSON encoding) and `raw_recv` (return raw bytes instead of JSON-decoding). Rust always sends raw bytes and returns raw bytes — the caller is responsible for encoding/decoding. This is arguably better design but differs from Perl's convenience API.

### 9. Perl Behaviors NOT Implemented in Rust: MISSING

1. **Auth.getAuthzTable privilege checking**: The JS/C# implementations call `Auth.getAuthzTable~1` to get a table mapping actions to required privilege IDs, cache it for 5 minutes, and use it to verify that a ticket holder has the required privileges. This is the standard way to enforce authorization on incoming requests. Rust has no equivalent — it can verify ticket signatures but cannot enforce privilege-based access control.

2. **PING/PONG in Perl**: Perl Connection.pm does NOT handle PING/PONG packets — it would die with "Unexpected packet of type PING". Rust handles them (responds PONG to PING). This is forward-compatible with JS/Go clients that send PING, but means Perl clients and Rust servers are fine, while Perl servers would reject PING from Rust clients. Not an issue since Rust client does not send PING.

3. **`rpc.timeout` config key**: Perl ServiceInfo.pm:257 reads `GTSOA::Config->val('rpc.timeout', 75)` as the base timeout. Rust hardcodes `DEFAULT_RPC_TIMEOUT_SECS = 75`. If someone configures a different `rpc.timeout` in soa.conf, Perl would respect it but Rust would not.

4. **`beepish.client_timeout` / `beepish.server_timeout` config keys**: Perl reads these from config (Client.pm:40, Server.pm:58). Rust hardcodes the defaults (90s and 120s). Not configurable at runtime.

5. **`beepish.bind_tries`, `beepish.first_port`**: Perl reads these from config (Server.pm:27-29). Rust hardcodes them (20 tries, 30100-30399).

6. **Multicast socket binding differences**: Perl Observer.pm:37-38 calls `mcast_add` for each discovery interface separately. Rust creates a single socket and calls `join_multicast_v4` once. For single-interface setups this is equivalent; multi-homed configurations may differ.

7. **`service.address` config key**: Perl Config.pm:106 checks `service.address` separately from `bus.address`. Rust only reads `bus.address` and applies it to service binding.

8. **Logging infrastructure**: Perl has a structured `GTSOA::Logger` with configurable log files, severity levels, and child mode. Rust uses `env_logger` (environment-variable based). Not a correctness issue but differs operationally.

9. **v5 pure-hash announcement format**: Perl Announcer.pm:125 supports `GTSOA_ANNOUNCE_V4ONLY` env var to skip the v3 wrapper and emit a pure hash. Rust always emits v3-wrapped format. Not needed for current interop.

10. **Weight=0 service exclusion during lookup**: Both implementations exclude weight=0 services from lookup results. Match. However, Perl does this during `lookup()` (ServiceManager.pm:59), while Rust does it in `get_action()` / `find_action_with_envelope()`. Equivalent.

---

## Critical Gaps

### 1. Auth.getAuthzTable Privilege Checking — MISSING (HIGH SEVERITY)

**What it is**: When a service receives an incoming request with a ticket, it needs to verify not just that the ticket is valid (signature, expiry), but also that the ticket grants the required privileges for the specific action being called. The canonical approach is:

1. Service calls `Auth.getAuthzTable~1` via SCAMP to get a table: `{ "action.name": [required_priv_id, ...], ... }`
2. Table is cached for 5 minutes
3. On each incoming request, look up the action in the table, check that the ticket has all required privilege IDs

**Current state in Rust**: `Ticket::verify()` can parse and cryptographically verify tickets. `Ticket::has_privilege()` / `has_all_privileges()` exist. But there is NO code to:
- Call `Auth.getAuthzTable` via the requester
- Cache the returned table
- Look up required privileges for an action
- Integrate privilege checking into the request dispatch pipeline

**Impact**: Any Rust service that uses tickets for authorization will accept ALL valid tickets regardless of whether the user has the required privileges. This means a valid ticket with zero privileges would grant access to any action.

**Recommendation**: Implement an `AuthzTable` module that:
- Uses the `Requester` to call `Auth.getAuthzTable~1`
- Caches results for 5 minutes (matching JS ticket.js:53)
- Provides `check_access(action, ticket) -> Result<()>`
- Is integrated into `server_connection::dispatch_and_reply` for actions without the `noauth` flag

### 2. Stale Cache Behavior Divergence — DIVERGENT (MEDIUM SEVERITY)

Perl stops serving requests when the discovery cache is stale (sets `fill_error` and returns it). Rust logs a warning and continues to serve from the stale cache. In production, a stale cache means the discovery writer has stopped (likely a system problem). Perl's behavior is more conservative and prevents routing to potentially-dead services.

### 3. Config Keys Not Read — MISSING (LOW SEVERITY)

Several Perl config keys are hardcoded in Rust instead of being read from the config file:
- `rpc.timeout` (base RPC timeout)
- `beepish.client_timeout` (client idle timeout)
- `beepish.server_timeout` (server idle timeout)
- `beepish.bind_tries` (bind attempt count)
- `beepish.first_port` / last port (port range)
- `service.address` (separate service bind address)

---

## Recommendations

1. **Implement Auth.getAuthzTable** (Priority: CRITICAL). This is the only gap that could cause a security issue in production. Without it, ticket-based authorization is incomplete — valid tickets pass regardless of privilege requirements.

2. **Read `rpc.timeout` from config** (Priority: MEDIUM). Some deployments configure this to longer values for slow actions. Hardcoding 75s could cause premature timeouts.

3. **Read `beepish.*` timeouts from config** (Priority: LOW). The defaults match Perl, but operational flexibility is lost.

4. **Consider stale-cache error behavior** (Priority: MEDIUM). Decide whether to match Perl's fail-fast behavior on stale cache or document the current graceful-degradation approach as intentional.

5. **Add `service.address` config support** (Priority: LOW). Only matters for deployments that set service address separately from bus address.

6. **Consider `ident` parameter for targeted routing** (Priority: LOW). Some Perl callers use this for sticky routing to specific worker instances.
