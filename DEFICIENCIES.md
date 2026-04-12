# scamp-rs Deficiency Report

Comprehensive parity audit against the canonical Perl implementation (gt-soa/perl),
cross-referenced with scamp-js and scamp-go. Generated 2026-04-12.

## Wire Protocol (proto.rs, client.rs)

### CRITICAL

**W-1. Header line accepts bare `\n`; Perl requires `\r\n`**
- Rust `proto.rs:262`: searches for `\n`, accepts bare LF
- Perl `Connection.pm:46`: regex requires `\015\012` (CR+LF)
- All other implementations require `\r\n`. Fix: search for `\r\n` explicitly.

**W-2. No TLS certificate fingerprint verification**
- Rust `client.rs:115-117`: `danger_accept_invalid_certs(true)`, no fingerprint check
- Perl `Connection.pm:61-68`: after TLS handshake, extracts peer cert SHA1 fingerprint and compares against announced fingerprint. Mismatch is fatal.
- Security vulnerability: MITM possible. Must extract peer cert fingerprint post-handshake and verify.

### HIGH

**W-3. `ticket`/`identifying_token`/`action` omitted from JSON when empty**
- Rust `proto.rs:33,51,55`: `skip_serializing_if = "String::is_empty"` on action, ticket, identifying_token
- Go `packetheader.go:27,33-34`: no `omitempty` — always serializes these fields
- Receivers may rely on fields being present. Remove skip_serializing_if from these fields.

**W-4. No send-side flow control (ACK validation, pause/resume)**
- Rust `client.rs:492-498`: ACK packets for outgoing messages completely ignored
- Perl `Connection.pm:177-183`: validates ACK format `/^[1-9][0-9]*$/`, checks monotonic advance, checks not past bytes sent, calls `ack()` to resume stream
- JS `connection.js:237-250`: pause when `sent - acked >= maxflow`, resume on ACK
- Without this, Rust can overwhelm slow receivers with unbounded data.

**W-5. No corking mechanism (writes sent before fingerprint verification)**
- Rust: packets written to TLS stream immediately
- Perl `Connection.pm:33,196-201`: `_corked` flag buffers writes until TLS fingerprint verified
- JS `connection.js:14,41-48`: `_outBuf` array buffers until `start()` after TLS verification
- Credentials/tickets could be sent to unverified peer. Implement corking.

**W-6. No proactive connection-lost notification / pool cleanup**
- Rust `client.rs:73-77`: lazy cleanup — checks `closed` only on next `get_connection` call
- Perl `ConnectionManager.pm:26`: `on_lost` removes from pool immediately
- Stale connections linger in pool until next request.

**W-7. Reader task does not set `closed` flag on exit**
- Rust `client.rs:299-373`: reader task exits but never sets `closed = true`
- Only set in `Drop::drop()`. Race: send_request may try to use a dead connection.

### MEDIUM

**W-8. Unknown packet types silently dropped; should be fatal**
- Rust `proto.rs:315-319`: returns `ParseResult::Drop`
- Perl `Connection.pm:187`: calls `_error()` which destroys connection
- JS `connection.js:144`: also fatal. Go `packet.go:87`: also fatal.

**W-9. Malformed HEADER JSON silently dropped; should be fatal**
- Rust `proto.rs:326-330`: returns `ParseResult::Drop`
- Perl `Connection.pm:148-149`: fatal error, destroys connection

**W-10. TXERR body not validated (empty accepted)**
- Rust accepts any TXERR body including empty
- JS `connection.js:229`: rejects empty or `"0"`

**W-11. No idle timeout management**
- Rust: no idle timeout at all
- Perl `Connection.pm:131-135`: `_adj_timeout` dynamically adjusts: busy/pending → no timeout, idle → configured timeout
- Default: `beepish.client_timeout` = 90s (Perl), not read by Rust

**W-12. No `busy` flag for timeout adjustment**
- Perl `Client.pm:66,79,93`: tracks pending requests, adjusts connection busy state
- Rust: no equivalent

**W-13. Default timeout 75s vs Perl's 90s**
- Rust `client.rs:21`: `DEFAULT_TIMEOUT_SECS = 75`
- Perl `Client.pm:40`: `beepish.client_timeout` default 90s. Should read from config.

**W-14. DATA chunk size 131072 vs Perl's 2048**
- Rust `proto.rs:11`: 131072 bytes
- Perl `Connection.pm:218`: 2048 bytes
- Large chunks defeat flow control granularity. Consider reducing or making configurable.

**W-15. ACK sent for every DATA packet; Perl/JS batch/defer**
- Rust `client.rs:430-439`: immediate ACK per DATA packet
- Perl: deferred to stream consumer. JS: immediate if consumed, deferred on backpressure.
- Functionally correct for buffering receiver but inefficient.

**W-16. Per-packet flush inefficiency**
- Rust `client.rs:289-293`: flushes after every packet write
- Perl/JS: kernel/event-loop level batching
- With TCP_NODELAY, each flush = separate TCP packet. Consider batching.

---

## Service Infrastructure & Discovery (service.rs, discovery/)

### CRITICAL

**S-1. No multicast announcing**
- Rust: no UDP socket, no periodic sending, no zlib compression
- Perl `Announcer.pm:55-94`: creates IO::Socket::Multicast, joins group 239.63.248.106:5555, sends compressed packet every 5s
- Services cannot be discovered live without this.

**S-2. No multicast receiving**
- Rust: no multicast observer
- Perl `Observer.pm`: listens on multicast group, decompresses, injects into ServiceManager
- Includes prefix byte handling: strips leading 'R' or 'D' before decompression (Observer.pm:51-53)

**S-3. No zlib compression of announcements**
- Perl `Announcer.pm:203`: `compress($uc_packet, 9)`
- JS also compresses. Go does not.
- Must compress outgoing, handle both compressed/uncompressed incoming.

**S-4. No v4 extension hash in announcements**
- Rust `service.rs:283`: only puts envelopes array at position 6, no v4 hash appended
- Perl `Announcer.pm:187`: builds `[ @{envelopes}, $v4_hash ]` — the v4 extension hash with RLE-encoded vectors is the last element of the envelopes array
- Actions with per-action sector/envelope overrides cannot be announced.

**S-5. No RLE encoding for v4 action vectors**
- Perl `Announcer.pm:105-119`: `__torle` function for run-length encoding
- Applied to acname, acns, acver, acflag, acsec, acenv vectors

**S-6. No signature verification (stub returns true)**
- Rust `packet.rs:67-69`: always returns true
- Perl `ServiceInfo.pm:90-108`: full RSA SHA256 verification using cert public key
- Any forged announcement would be accepted.

**S-7. No authorized_services implementation**
- Rust: no file, no module, no checking. `authorized` hardcoded to true.
- Perl `ServiceInfo.pm:111-168`: reads config path, parses fingerprint→token regex mapping, matches `"$sector:$action"`, special-cases `_meta.*`, rejects `:` in sector/action, hot-reloads on mtime change.

**S-8. No graceful shutdown**
- Rust `service.rs:329-348`: infinite loop, no way to stop, no signal handling
- Perl `Announcer.pm:97-101`: sets weight=0, sends at 1s interval for 10 rounds
- JS `service.js:78-91`: suspend announcer, wait 5s, drain active requests

### HIGH

**S-9. No cache file header stripping**
- Rust `cache_file.rs`: processes all chunks from `\n%%%\n` split
- Perl `ServiceManager.pm:92`: `my ($header, @anns) = split /\n%%%\n/, $data` — first chunk is header, discarded
- JS `serviceMgr.js:87`: `chunks.shift()` — same
- Rust will try to parse the header as an announcement, which will fail.

**S-10. No cache staleness check**
- Rust: opens cache file without checking mtime
- Perl `ServiceManager.pm:83-88`: checks mtime against `discovery.cache_max_age` (default 120s). Returns error if stale.

**S-11. No announcement expiry/TTL**
- Rust: entries never expire
- Perl `ServiceManager.pm:38`: `expires = now + 2.1 * send_interval`. Checked on every lookup.

**S-12. No timestamp replay protection**
- Rust: no timestamp tracking
- Perl `ServiceManager.pm:33-35`: tracks per-service timestamps, rejects stale packets

**S-13. No fingerprint computation**
- Rust: no SHA1 fingerprint of certificates anywhere
- Perl `ServiceInfo.pm:83-87`: SHA1 of DER cert, uppercase hex, colon-separated
- Without fingerprints, authorized_services matching is impossible.

**S-14. CRUD alias tag uses "delete" instead of "destroy"**
- Rust `service_registry.rs:84`: maps `CrudOp::Delete` to `"delete"`
- Perl `ServiceInfo.pm:193`: matches `destroy` (not `delete`): `$tag =~ /^(?:create|read|update|destroy)$/`
- Mismatched aliases means CRUD lookups won't resolve correctly.

**S-15. V4 accompat filtering not applied**
- Rust `service_info.rs:319-324`: reads accompat but never filters on it (code commented out)
- Perl `ServiceInfo.pm:210`: `next if $compatver != 1`
- Incompatible v4 actions would be incorrectly registered.

**S-16. No signal handling**
- Rust: no SIGTERM/SIGINT handling
- JS `service.js:55-63`: handles both, triggers graceful shutdown

### MEDIUM

**S-17. No server connection timeout**
- Rust `service.rs`: no idle timeout on accepted connections
- Perl `Server.pm:59`: `beepish.server_timeout` default 120s

**S-18. No request concurrency tracking**
- Perl `Server.pm:64`: tracks active count, sets `busy` flag
- Needed for graceful shutdown drain.

**S-19. Announcement packet separator format**
- Rust: single-line base64 for signature
- Perl `encode_base64`: wraps at 76 characters
- May cause issues if receiver expects line-wrapped base64.

**S-20. Flags not filtered to announceable set**
- Perl `Announcer.pm:103`: only announces `read, update, destroy, create, noauth, secret` flags
- Rust: no flag filtering during announcement.

**S-21. Weight hardcoded to 1**
- Should be configurable for load balancing and shutdown.

**S-22. Lookup timeout not computed from action flags**
- Perl `ServiceInfo.pm:258`: extracts `t600` → timeout = 605. Default from `rpc.timeout` config.
- Rust: hardcoded default, doesn't use flag timeouts.

### LOW

**S-23. Bind address from config**
- Perl `Server.pm:34`: binds to `bus_info->{service}[0]`, not `0.0.0.0`
- Rust: hardcodes `0.0.0.0`

**S-24. No cache refresh/reload mechanism**
- Perl reloads on every `fill_from_cache` call (rate-limited to 1/sec)

**S-25. No deduplication of cache entries**
- JS `serviceMgr.js:98-99`: deduplicates by blob content

**S-26. No failed-service backoff**
- JS `serviceMgr.js:43-51`: exponential backoff per failure

---

## Configuration, Security & Behavioral (config.rs, client.rs, service.rs)

### CRITICAL

**C-1. 12 of 14 Perl config keys unread**
- Rust only reads `discovery.cache_path`. Missing: `bus.address`, `bus.authorized_services`, `discovery.address`, `service.address`, `discovery.port`, `discovery.multicast_address`, `discovery.cache_max_age`, `beepish.server_timeout`, `beepish.client_timeout`, `beepish.bind_tries`, `beepish.first_port`, `rpc.timeout`.
- scamp-rs cannot be configured via soa.conf for networking, timeouts, security, or discovery.

**C-2. No TLS certificate fingerprint verification on client**
- Rust `client.rs:115-117`: `danger_accept_invalid_certs(true)` — no verification at all
- Perl `Connection.pm:61-68`: SHA1 fingerprint comparison during TLS handshake. Mismatch is fatal.
- MITM vulnerability. SCAMP uses self-signed certs with fingerprint pinning instead of chain validation.

**C-3. Signature verification stubbed (returns true)**
- Rust `packet.rs:67-69`: always returns true
- Perl `ServiceInfo.pm:91-109`: full RSA SHA256 verification
- Forged announcements accepted.

**C-4. No certificate fingerprinting**
- Rust: no SHA1 fingerprint computation anywhere
- Perl `ServiceInfo.pm:82-88`: SHA1 of DER cert, uppercase hex, colon-separated
- Required for authorized_services lookup and client connection verification.

**C-5. No authorized_services implementation**
- Rust: `authorized` hardcoded to `true` in service_registry.rs:66
- Perl `ServiceInfo.pm:111-168`: full implementation with hot-reload, regex matching, _meta.* exception
- Any service can claim any action.

### HIGH

**C-6. No bus_info() / interface resolution**
- Perl `Config.pm:103-112`: resolves bus.address to IP, supports `if:eth0` syntax, auto-detects 10.x/192.168.x
- Rust: no equivalent. Server binds to 0.0.0.0 instead of configured interface.

**C-7. No ticket verification**
- Format: `version,userId,clientId,timestamp,ttl,privs,signature`
- Privs: `+` separated integers
- Sig: base64url-encoded RSA PKCS1v15 SHA256 over preceding fields (last comma-delimited field)
- Key: `/etc/GT/auth/ticket_verify_public_key.pem` (Go) or `/etc/scamp/auth/ticket_verify_public_key.pem` (JS)
- Rust: ticket passed through but never verified.

**C-8. Connections made without fingerprint context**
- Perl `ConnectionManager.pm:24-26`: passes `fingerprint => $svc->fingerprint` when creating client
- Rust `BeepishClient.get_connection()`: ServiceInfo has no fingerprint field, nothing to verify against.

### MEDIUM

**C-9. GTSOA env var not checked**
- Perl `Config.pm:40`: reads `$ENV{GTSOA}`, defaults to `/etc/GTSOA/soa.conf`
- Rust: reads `$ENV{SCAMP_CONFIG}`, does NOT check `GTSOA`

**C-10. No write corking during TLS handshake**
- Perl: buffers writes until fingerprint verified
- Rust: sends immediately. Credentials could go to unverified peer.

**C-11. Three timeout values conflated**
- Perl has distinct: `beepish.server_timeout` (120s), `beepish.client_timeout` (90s), `rpc.timeout` (75s)
- Rust: single `DEFAULT_TIMEOUT_SECS = 75` for everything.

**C-12. Per-action timeout flags ignored**
- Perl `ServiceInfo.pm:257-258`: `t600` flag → timeout = 605s (flag value + 5s padding)
- Rust: parses `Flag::Timeout` but never uses it.

**C-13. No connection idle timeout**
- Perl `Connection.pm:131-135`: `_adj_timeout` — disables timeout when busy, enables when idle
- Rust: connections persist indefinitely.

**C-14. No high-level Requester API**
- Perl `Requester.pm`: `make_request`, `simple_request`, `simple_async_request`
- Handles cache fill, action lookup, connection management, JSON encode/decode, error normalization
- Rust has pieces but no integrated API.

### LOW

**C-15. Config duplicate key handling (last wins vs first wins)**
- Perl: first wins, logs error
- Rust: last wins, silent

**C-16. Inline comment stripping**
- Perl: strips `# comment` mid-line
- Rust: only skips full-line `#` comments

**C-17. Signing padding analysis: all impls agree on PKCS1v15**
- Perl calls `use_pkcs1_oaep_padding` but this is a no-op for `sign()`/`verify()` — OAEP is encryption-only
- Go: `rsa.VerifyPKCS1v15` explicitly. JS: `crypto.createVerify('sha256')` = PKCS1v15 by default.
- Rust `service.rs:297`: `Padding::PKCS1` = correct. Confirmed: no actual padding mismatch.

---

## Summary

| Severity | Wire | Service/Discovery | Config/Security | Total |
|----------|------|-------------------|-----------------|-------|
| CRITICAL | 2 | 8 | 5 | 15 |
| HIGH | 5 | 8 | 3 | 16 |
| MEDIUM | 9 | 5 | 6 | 20 |
| LOW | — | 4 | 3 | 7 |
| **Total** | **16** | **25** | **17** | **58** |
