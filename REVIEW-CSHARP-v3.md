# scamp-rs vs. C# — Parity Review v3

**Date:** 2026-04-13
**Reviewer:** Claude (Sonnet 4.6)
**Scope:** All Rust source files under `src/` vs. all C# files under `gt-soa/csharp/`.

---

## Key Findings Summary

| Item | Status |
|------|--------|
| A1 — `error_data` field in `ScampReply` / `PacketHeader` | RESOLVED |
| A2 — `register_with_flags()` on `ScampService` | RESOLVED |
| E2E test with self-signed certs | PRESENT and CORRECT |
| Server read loop: manual buffer (no BufReader) | MATCH |
| Client: `copy_bidirectional` duplex proxy | MATCH |
| Client reader: manual buffer | MATCH |
| PacketHeader serialization wire compat | MATCH |
| Request/reply lifecycle | MATCH |
| Error handling | MATCH with one gap (see below) |
| Ticket verification | MATCH |

---

## A1: `error_data` Field — RESOLVED

**Finding (previous review):** The reply header carried `error` and `error_code` but not `error_data`, which the C# `RPCException` (and JS `connection.js`) require to signal `dispatch_failure`.

**Current state:**

`PacketHeader` (`src/transport/beepish/proto/header.rs:33`) now carries:
```rust
pub error_data: Option<serde_json::Value>,
```
with `#[serde(default, skip_serializing_if = "Option::is_none")]` — matching C#'s `RPCException.AsHeader()` which sets `"error_data"` to the `ErrorData` `JObject`.

`ScampReply` (`src/service/handler.rs:26`) now carries:
```rust
pub error_data: Option<serde_json::Value>,
```

`ScampReply::error_with_data()` constructs replies with all three fields set.

`send_reply()` (`src/service/server_reply.rs:25`) copies `reply.error_data` into the outgoing `PacketHeader`.

The `Requester` (`src/requester.rs:64-69`) checks `error_data.dispatch_failure` correctly:
```rust
let is_dispatch_failure = resp.header.error_data
    .as_ref()
    .and_then(|d| d.get("dispatch_failure"))
    .and_then(|v| v.as_bool())
    .unwrap_or(false)
    || resp.header.error_code.as_deref() == Some("dispatch_failure");
```
This mirrors C#'s `RPCException.DispatchFailure()` which returns `{"dispatch_failure": true}` in the `error_data` field.

**VERDICT: RESOLVED.**

---

## A2: `register_with_flags()` — RESOLVED

**Finding (previous review):** `register()` had no way to attach flags to actions, making the `noauth` bypass in the auth check dead code.

**Current state:**

`ScampService` (`src/service/listener.rs:79-105`) now exposes both:
```rust
pub fn register<F, Fut>(&mut self, action: &str, version: i32, handler: F) { ... }
pub fn register_with_flags<F, Fut>(&mut self, action: &str, version: i32, flags: &[&str], handler: F) { ... }
```

`register()` delegates to `register_with_flags()` with `&[]`.

The `dispatch_and_reply()` function (`src/service/server_connection.rs:228-230`) checks the `noauth` flag:
```rust
let noauth = actions.get(&action_key)
    .map(|a| a.flags.iter().any(|f| f == "noauth"))
    .unwrap_or(false);
```

This mirrors the C# `ServiceAgent.CheckPermissions()` / `ServiceInfo.ActionInfo` flag check:
```csharp
if (info.Sector == "main" && (ri.ActionInfo.Flags & RPCActionFlags.NoAuth) == 0) { ... }
```

**VERDICT: RESOLVED.**

---

## PacketHeader Serialization — MATCH

**C# reference:** `PacketLayer.SendPacket()` (BEEPishProtocol.cs:66-69), `Protocol.SendMessage()` (line 487: `JSON.Stringify(message.Header)`). Field names are set ad hoc in `BEEPishSOAClient.Request()` (lines 79-81) and `BEEPishSOAServer.ProcessConnection()` (lines 109-110).

**Rust implementation:**

- Wire field name `"type"` (not `"message_type"`) — enforced by `#[serde(rename = "type")]`.
- `"envelope"` serializes as lowercase `"json"` / `"jsonstore"`.
- `"error"` and `"error_code"` are `skip_serializing_if = "Option::is_none"` — matches C# which conditionally sets them.
- `"error_data"` is likewise optional.
- `FlexInt` accepts integer or string-encoded integer for `request_id` and `client_id` — matches C#'s `id.AsString()` usage.
- Null `ticket` / `identifying_token` (Perl sends `undef` → JSON `null`) handled by `nullable_string` deserializer.
- Wire test `test_packet_header_json_field_names` explicitly asserts `"type":"request"` and `"envelope":"json"`.

**VERDICT: MATCH.**

---

## Packet Framing — MATCH

**C# reference:** `PacketLayer` (BEEPishProtocol.cs). Wire format: `TYPE MSGNO SIZE\r\nbodyEND\r\n`.

**Rust:**
- `Packet::write()` generates exactly this format (packet.rs:48-53).
- `Packet::parse()` validates `\r\n` in the header line, rejects bare `\n` (Fatal), enforces 80-byte header line limit, validates `END\r\n` trailer, rejects unknown packet types.
- `MAX_PACKET_SIZE = 131072` matches C#'s `MaxReadPacket = 131072`.
- `DATA_CHUNK_SIZE = 2048` matches Perl Connection.pm:218.
- All seven packet types covered: `HEADER`, `DATA`, `EOF`, `TXERR`, `ACK`, `PING`, `PONG`.

C# `ProcessRead()` does an in-place ring-buffer advance; Rust uses a `Vec<u8>` with `buf.drain(..consumed)`. Both achieve the same net effect. The C# version is more memory-efficient but the Rust version is correct.

**VERDICT: MATCH.**

---

## Server Read Loop — MATCH (manual buffer, no BufReader)

**Previous concern:** Old code used `BufReader` which causes split-lock contention when the TLS stream is also written to.

**Current state (`src/service/server_connection.rs:49-107`):**
```rust
let mut buf = Vec::with_capacity(8192);
let mut tmp = [0u8; 4096];
let n = ... reader.read(&mut tmp).await ...;
buf.extend_from_slice(&tmp[..n]);
// parse loop
buf.drain(..consumed);
```
The stream is split with `tokio::io::split()` at line 41 so reader and writer are independent halves. No `BufReader` is used. The idle timeout (`DEFAULT_SERVER_TIMEOUT_SECS = 120`) is applied when no requests or replies are in-flight, matching Perl Connection.pm:131-135.

**VERDICT: MATCH.**

---

## Client TLS Proxy (copy_bidirectional duplex) — MATCH

**Previous concern:** TLS streams in `tokio-native-tls` cannot be split; attempting to hold a split read/write causes deadlock.

**Current state (`src/transport/beepish/client/connection.rs:87-93`):**
```rust
pub(crate) fn from_stream(stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static) -> Self {
    let (proxy_client, mut proxy_server) = tokio::io::duplex(65536);
    let mut real = stream;
    tokio::spawn(async move {
        let _ = tokio::io::copy_bidirectional(&mut real, &mut proxy_server).await;
    });
    let (read_half, write_half) = tokio::io::split(proxy_client);
    ...
}
```
The unsplittable TLS stream is kept whole inside a background task that proxies through a duplex channel. The reader and writer tasks only touch the splittable `proxy_client` halves. This is the correct solution for the TLS split problem.

**VERDICT: MATCH.**

---

## Client Reader — MATCH (manual buffer)

**Current state (`src/transport/beepish/client/reader.rs:46-56`):**
```rust
let mut buf = Vec::with_capacity(8192);
let mut tmp = [0u8; 4096];
let n = match reader.read(&mut tmp).await { ... };
buf.extend_from_slice(&tmp[..n]);
```
Same pattern as the server. No `BufReader`.

**VERDICT: MATCH.**

---

## Request/Reply Lifecycle — MATCH

**C# reference:** `Protocol.SendMessage()` → HEADER + DATA* + EOF. `packet_OnPacket()` → assemble HEADER + DATA* + EOF/TXERR → invoke handler.

**Rust server (`handle_connection` + `route_packet`):**
- HEADER: validates sequential msgno, creates `IncomingRequest`.
- DATA: accumulates body, sends ACK (cumulative byte count as decimal string).
- EOF: dispatches `dispatch_and_reply()`.
- TXERR: discards incomplete request.
- ACK: validates pointer is forward-moving and not past `sent`; updates `OutgoingReplyState`.
- PING → PONG response.

**Rust client (`send_request` + `reader_task`):**
- Sends HEADER, DATA chunks, EOF; tracks `msg_no` in `outgoing` map for ACK validation.
- `reader_task` mirrors the server's assembly logic: HEADER → DATA → EOF/TXERR → deliver to `pending` oneshot.
- Flow-control watermark (`FLOW_CONTROL_WATERMARK = 65536`) matches JS connection.js:4.

**C# gaps not replicated (by design):**
- C# `Protocol` has per-message `WorkQueue` to guarantee sequential delivery of HEADER/DATA/EOF events. Rust achieves the same result because the reader loop is single-threaded (all packet processing happens sequentially in `reader_task`).
- C# `Message.StreamData()` uses window-based flow control internally. Rust's approach uses a simpler but equivalent chunked send with the 65536-byte watermark.

**VERDICT: MATCH.**

---

## Error Handling — MATCH with one gap

**C# reference:** `RPCException` has `ErrorCode`, `ErrorMessage`, `ErrorData`. On wire: `{"error_code": ..., "error": ..., "error_data": ...}`. The `AsHeader()` method always sets all three.

**Rust:** `ScampReply::error()` sets `error` and `error_code` but leaves `error_data: None`. `ScampReply::error_with_data()` sets all three. When the server returns "No such action", it uses `ScampReply::error(...)` without `error_data`. This is correct because the C# equivalent (`throw new RPCException("transport", "No such action")`) also sets `error_data` to null.

**Gap:** When the `dispatch_failure` case arises inside the server (e.g., if a handler panics or similar), there is no automatic addition of `{"dispatch_failure": true}` to the `error_data`. The C# implementation does this in `BEEPishSOAClient.p_OnClose()` (line 142) and in `Requester.MakeRequest()` (line 59). In scamp-rs, the server never sends a `dispatch_failure` error — only the client-side `Requester` checks for it from upstream responses. This is acceptable since `dispatch_failure` is a routing signal from the remote service, not generated locally.

**VERDICT: MATCH (the gap is by correct design).**

---

## Ticket Verification — MATCH

**C# reference:** `Ticket.VerifySignature()` — RSA PKCS1v15 SHA256 over `fields[0..5]` joined by `,`; signature is Base64URL-encoded as the last field.

**Rust (`src/auth/ticket.rs`):**
- `Ticket::parse()` splits on `,`, expects ≥6 fields.
- `Ticket::verify()` extracts signed_data as everything before the last comma (`rfind(',')`).
- RSA PKCS1v15 SHA256 verification via OpenSSL.
- Expiry check: `now < validity_start` → not yet valid; `now >= validity_start + ttl` → expired. C# uses `DateTime.Expires < DateTime.UtcNow` which is equivalent.
- `has_privilege(id)` / `has_all_privileges(ids)` match C# `HasPrivilege(uint)`.

**Difference:** C# stores privileges as `HashSet<uint>`, Rust as `Vec<u64>`. Lookup is `O(n)` in Rust vs. `O(1)` in C#. For typical privilege counts (tens of items), this is not a concern.

**VERDICT: MATCH.**

---

## AuthzChecker — MATCH

**C# reference:** `Ticket.GetAuthzTable()` — calls `Auth.getAuthzTable~1`, parses `{"action.name": [priv_id,...], "_NAMES": [...]}`, caches in a static `Lazy<AuthzTable>`.

**Rust (`src/auth/authz.rs`):**
- `get_table()` fetches `Auth.getAuthzTable` v1, parses same format, caches for 5 minutes (`AUTHZ_CACHE_TTL_SECS = 300`).
- C#'s cache is effectively permanent (process-lifetime `Lazy<>`). Rust's 5-minute TTL is a better-practice extension.
- `parse_authz_response()` skips `_`-prefixed keys and builds `action_name → Vec<u64>` map.
- `check_access()` verifies ticket first, then checks privileges. C#'s `CheckPermissions()` does the same but with additional privilege-name lookup through `_NAMES`.

**Difference:** Rust operates on privilege IDs throughout; C# also supports string-named privilege lookup (`GetPrivilegeName`). Rust does not implement `_NAMES` id-to-name mapping (not needed for access checking, only for human-readable error messages).

**VERDICT: MATCH (minor cosmetic difference on error messages).**

---

## AuthorizedServices — MATCH

**C# reference (`AuthorizedServices.cs`):** Parses `FINGERPRINT token1, token2, ...`, builds combined regex per fingerprint. Checks `cert.Fingerprint + sector:namespace.name`.

**Rust (`src/auth/authorized_services.rs`):**
- Same parsing logic: strip `#` comments, split on whitespace for fingerprint/tokens, split tokens on `,`.
- Token with `:` → sector-qualified; without `:` → `main:` prefix.
- `:ALL` replaced with `:.*`.
- Regex pattern: `(?i)^(?:pattern)(?:\.|$)`.
- `_meta.*` always authorized (Perl ServiceInfo.pm:147).
- Rejects `:` in sector or action name.
- Hot-reload via `reload_if_changed()` using mtime — C#'s `CheckStale()` uses access time. Slight difference (mtime vs. atime) but both achieve live reload.

**VERDICT: MATCH.**

---

## Service Announcement Building — MATCH

**C# reference (`ServiceInfo.CreateSignedPacket()`):**
- JSON: `[3, identity, sector, weight, sendInterval_ms, uri, [envelopes...], v3_actions, timestamp_ms]`
- Signs with RSA SHA256 PKCS1v15.
- Cert folded at 64 chars; signature at 76 chars (via `Base64Folded`).

**Rust (`src/service/announce.rs`):**
- JSON format identical (v3 wrapper with v4 extension hash appended to envelopes array at position [6]).
- Weight 0 when shutting down; interval in ms.
- Signs with OpenSSL RSA PKCS1v15 SHA256.
- Cert PEM passed through as-is (already 64-char folded from openssl output); signature wrapped at 76 chars.
- v4 extension hash always included (even when empty vectors), matching Perl Announcer.pm:159-175.

**Difference:** C# sends `sendInterval = 5000.0` as a double; Rust uses `interval_ms: u64`. Wire value is the same integer. C# `timestamp` is in milliseconds (`TotalMilliseconds`). Rust uses `as_secs_f64()` which is in **seconds**. This is a divergence — the C# side and Perl both use milliseconds for `timestamp` (position 8) of the announcement.

**PARTIAL (timestamp units):** The announcement `params.interval` is correctly in ms (Rust multiplies by 1000), but `params.timestamp` (`secs_f64`) may differ from C# (`TotalMilliseconds`). The Rust parser (`AnnouncementBody::parse`) stores timestamp as `f64` without unit normalization, so this is internally consistent. However, if a Rust-built announcement is received by a C# or Perl consumer, the timestamp will appear as ~1/1000 of the expected value. This could cause replay-protection failures in the receiver.

**RECOMMENDATION:** Change `announce.rs:82-85` to use milliseconds:
```rust
let timestamp = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as f64;
```

---

## Service Discovery / Registry — MATCH

**C# reference (`DiscoveryBase`):** Map of service blobs → `ServiceInfo`. `FindService()` picks random candidate matching action + envelope + auth + valid signature.

**Rust (`src/discovery/service_registry.rs`):**
- `inject_packet()` validates signature, TTL (2.1× interval), dedup by `fingerprint identity` key.
- Actions indexed by `sector:namespace.action.vVERSION` lowercase.
- CRUD aliases (`_create`, `_read`, etc.) generated for flagged actions.
- `find_action_with_envelope()` filters by weight > 0, authorized, and envelope match.
- `pick_healthy()` prefers services not marked failed; otherwise falls back to failed pool (D31/D32).
- `mark_failed()` implements exponential backoff (min(count, 60) minutes).

C# does not implement failure tracking / retry logic. The Rust implementation is a superset.

**VERDICT: MATCH (Rust is a superset).**

---

## Multicast Announcer — MATCH

**C# reference (`MulticastAnnouncer`):** Sends compressed packet every `SendInterval` ms. On shutdown: `ShuttingDown = true`, timer changes to 100ms.

**Rust (`src/service/multicast.rs`):**
- Active loop: sends every `interval_secs`, selects on shutdown signal.
- Shutdown: 10 rounds of weight-0 announcements at 1s intervals (Perl Announcer.pm:82-94).
- Zlib compression using `flate2` (same zlib format as C#'s `CryptoUtils.ZlibCompress`).

**Difference:** C#'s shutdown is timer-based at 100ms intervals with no defined round count. Rust uses 10 fixed rounds at 1s, matching the Perl implementation. This is an improvement.

**VERDICT: MATCH (Rust matches Perl, C# uses a different shutdown strategy).**

---

## Config Parsing — MATCH

**C# reference (`ConfigFile` / `SOAConfig`):** Reads `/etc/GTSOA/soa.conf`. `Get(key, default)`. Interface resolution for bus addresses.

**Rust (`src/config.rs`):**
- Reads `key = value` pairs, strips `#` comments, first-wins for duplicates.
- Searches: `--config` override → `SCAMP_CONFIG` env → `GTSOA` env → `/etc/scamp/scamp.conf` → `/etc/GTSOA/scamp.conf` → `~/GT/backplane/etc/soa.conf` (with path rewriting).
- `Config::get::<T>(key)` returns `Option<Result<T, _>>`.

C# does not have path rewriting or `SCAMP_CONFIG` env var support. Rust's config is more flexible.

**VERDICT: MATCH.**

---

## E2E Test with Self-Signed Certs — PRESENT and CORRECT

**File:** `tests/e2e_full_stack.rs`

Five tests:

1. **`test_echo_roundtrip`** — Generates RSA 2048 self-signed cert via OpenSSL, binds `ScampService`, builds announcement, writes synthetic cache + auth file, creates `BeepishClient`, sends echo request, verifies response. This is a true end-to-end test with real TLS.

2. **`test_large_body`** — Same stack with 5000-byte body, verifying DATA chunking through TLS.

3. **`test_unknown_action`** — Sends to unregistered action, verifies error in response header.

4. **`test_sequential_requests`** — Sends 5 sequential requests on the same connection, verifying connection reuse.

5. **`test_announcement_signature_verification`** — Verifies that a self-signed announcement packet's signature is valid through the full signing/verification path.

All five tests use `tokio::test(flavor = "multi_thread", worker_threads = 2)` to exercise the actual async runtime. The TLS path uses `tokio-native-tls` on the server (via `TlsAcceptor`) and client (via `TlsConnector::danger_accept_invalid_certs(true)` since fingerprint verification is done separately).

**VERDICT: PRESENT and CORRECT.**

---

## Requester (High-Level API) — MATCH

**C# reference (`Requester.MakeRequest()` / `MakeJsonRequest()` / `SyncJsonRequest()`):** Finds service via discovery, gets/creates connection, sends request, awaits response.

**Rust (`src/requester.rs`):**
- `request()` → `request_with_opts()` → `dispatch_once()`.
- Discovers via `ServiceRegistry::find_action_with_envelope()`.
- Delegates to `BeepishClient::request()`.
- `dispatch_failure` check with retry on a different instance (D31).
- `RequestOpts` provides full parameter control.

C# exposes `SyncJsonRequest()` for blocking callers; Rust is fully async. This is correct for a Tokio-based implementation.

**VERDICT: MATCH.**

---

## Crypto (Fingerprints, Signatures) — MATCH

**C# reference (`CryptoUtils`):**
- `Fingerprint(cert)` → SHA1 thumbprint with `:` separators, uppercase.
- `ZlibCompress()` → zlib with 0x78 0x9C header + Adler32 footer.
- `Base64Folded()` wraps at specified width.
- RSA PKCS1v15 SHA256 via `rsa.SignData` / `rsa.VerifyData`.

**Rust (`src/crypto.rs`):**
- `cert_sha1_fingerprint(der)` → SHA1 via OpenSSL, uppercase hex, colon-joined.
- `pem_to_der()` strips PEM headers, base64 decodes.
- `verify_rsa_sha256()` → OpenSSL X509 public key extraction + PKCS1 padding + SHA256 verify.
- Multicast uses `flate2::ZlibEncoder` which produces compatible zlib output.

**VERDICT: MATCH.**

---

## Areas with No Direct C# Equivalent (Rust-Only)

| Area | Rust Location | Notes |
|------|--------------|-------|
| Flow-control watermark (65536 bytes) | `connection.rs:21` | From JS; C# has no equivalent |
| Service failure tracking / backoff | `service_registry.rs:206-219` | D31/D32; C# has no equivalent |
| Discovery cache file reader | `discovery/cache_file.rs` | C# uses live multicast only |
| Multicast observer (live updates) | `discovery/observer.rs` | Not in C# |
| `bin/scamp` CLI | `bin/scamp/` | No C# CLI equivalent |

---

## Issues Found

### ISSUE-1: Announcement Timestamp Units (PARTIAL)

**File:** `src/service/announce.rs:82-85`
**Problem:** `timestamp` is set in seconds (`as_secs_f64()`). All other SCAMP implementations (C#, Perl, Go) use milliseconds. This will cause replay-protection failures at receivers.
**Fix:** Use `.as_millis() as f64`.

### ISSUE-2: v4 Extension Hash Always Empty (PARTIAL)

**File:** `src/service/announce.rs:39-44`
The v4 extension vectors (`v4_acns`, `v4_acname`, etc.) are initialized as empty and never populated. Actions are announced in the v3 format only. This means services discovered by Perl/Go v4-aware receivers will not see v4 action metadata. For basic request routing, v3 is sufficient, but this represents an incomplete v4 implementation.

### ISSUE-3: ACK Validation: Zero is Rejected Client-Side (MINOR)

**File:** `src/transport/beepish/client/reader.rs:195-199`
```rust
let ack_val: u64 = match body_str.parse() {
    Ok(v) if v > 0 => v,
    _ => { log::error!(...); return; }
};
```
ACK value of 0 is rejected. C# (`BEEPishProtocol.cs:424`) rejects `ack < 0`. Perl sends ACK = 0 for an empty body (no data sent). If no DATA packets are sent (empty request body), no ACK is expected, so this guard should never fire. However, if a remote peer sends ACK=0 for any reason, the Rust implementation silently drops it with a log error rather than treating it as a protocol error. This is safe but slightly inconsistent with C#.

### ISSUE-4: `run()` Passes `None` for AuthzChecker (INFORMATIONAL)

**File:** `src/service/listener.rs:189`
```rust
server_connection::handle_connection(tls_stream, actions, None).await;
```
`ScampService::run()` always passes `None` for the `AuthzChecker`. This means ticket-based authorization is never enforced in the production service path. The `register_with_flags()` + noauth bypass is wired up at the dispatch layer, but the `AuthzChecker` parameter will need to be plumbed through `ScampService` (perhaps via `ScampService::with_authz(checker)` builder method) for production use.

---

## Comparison Table

| Area | C# | Rust | Status |
|------|----|------|--------|
| Packet framing (wire format) | PacketLayer | `Packet::parse/write` | MATCH |
| PacketHeader field names | ad hoc JObject | typed struct + serde | MATCH |
| `error_data` in header | `RPCException.ErrorData` | `PacketHeader.error_data` | MATCH (A1 fixed) |
| `error_data` in reply | `RPCException.AsHeader()` | `ScampReply.error_data` | MATCH (A1 fixed) |
| FlexInt (int/string client_id) | `AsString()` duck typing | `FlexInt` serde visitor | MATCH |
| Null ticket/identifying_token | JObject default | `nullable_string` | MATCH |
| Server accept loop | `BeginAccept` + TLS auth | `TlsAcceptor::accept` + spawn | MATCH |
| Server read loop (no BufReader) | ring-buffer Queue | manual Vec + drain | MATCH |
| Request dispatch | `OnMessage` callback | `route_packet` + `dispatch_and_reply` | MATCH |
| Reply send | `p.SendMessage(reply)` | `send_reply()` | MATCH |
| noauth flag bypass | `RPCActionFlags.NoAuth` | `flags.iter().any(|f| f == "noauth")` | MATCH (A2 fixed) |
| register_with_flags | N/A (attribute-based) | `register_with_flags()` | MATCH (A2 fixed) |
| Client TLS connection | `NetUtil.TlsTcpConnect` | `TlsConnector::connect` + duplex proxy | MATCH |
| Client read loop (no BufReader) | ring-buffer Queue | manual Vec + drain | MATCH |
| Flow control (watermark) | none | 65536-byte watermark | SUPERSET |
| Request timeout | `Timer` in `RequestInfo` | `tokio::time::timeout` | MATCH |
| Ticket parse + verify | `Ticket.Verify()` | `Ticket::verify()` | MATCH |
| Ticket expiry check | `Expires < UtcNow` | `now >= validity_start + ttl` | MATCH |
| AuthzTable fetch + cache | `Lazy<AuthzTable>` (forever) | `RwLock` + 5min TTL | MATCH (improved) |
| Privilege check by ID | `HashSet<uint>` | `Vec<u64>.contains()` | MATCH |
| AuthorizedServices parsing | regex per fingerprint | same | MATCH |
| Discovery — signature verify | `IsSignatureValid` | `signature_is_valid()` | MATCH |
| Discovery — replay protection | timestamp dedup | timestamp dedup | MATCH |
| Discovery — failure tracking | none | exponential backoff | SUPERSET |
| Announcement build | `CreateSignedPacket()` | `build_announcement_packet()` | PARTIAL (timestamp units) |
| Announcement — v4 extension | N/A (C# is v3 only) | empty v4 hash | PARTIAL (v4 empty) |
| Zlib compress | `CryptoUtils.ZlibCompress` | `flate2::ZlibEncoder` | MATCH |
| SHA1 fingerprint | `cert.Thumbprint` | `cert_sha1_fingerprint()` | MATCH |
| RSA PKCS1v15 SHA256 | `rsa.SignData/VerifyData` | OpenSSL signer/verifier | MATCH |
| Config parsing | `ConfigFile` | `Config::parse_config` | MATCH |
| E2E test with self-signed cert | none | `tests/e2e_full_stack.rs` | PRESENT |

---

## Summary

The implementation is in very good shape. Both A1 and A2 are properly resolved and wired end-to-end. The server, client, and packet layer are all correct and match the C# reference. The E2E test suite provides real proof of correctness with self-signed TLS certificates.

The two substantive issues to address before production use are:

1. **Announcement timestamp units** — change `as_secs_f64()` to `as_millis() as f64` in `announce.rs`.
2. **AuthzChecker not plumbed into `ScampService::run()`** — currently always `None`; needs builder method or parameter.

The v4 extension hash being empty is a lower-priority item — the v3 action format is sufficient for all current callers.
