# Perl Parity Review v3 (2026-04-13)

## Summary

scamp-rs is in very good shape against the Perl canonical implementation. The six key areas
flagged for review (server read loop, client connection, client reader, reply writing,
`error_data`, and `register_with_flags`) all check out correctly. Config::from_content() and
the E2E test are sound. A handful of lower-severity divergences and gaps remain, documented
below.

Overall status by area:
- Server read loop: MATCH
- Client connection: MATCH (with one minor note on `_nextcorr` start value)
- Client reader: MATCH
- Reply writing: MATCH
- error_data field: MATCH
- register_with_flags / noauth: MATCH
- Config::from_content: MATCH
- E2E test architecture: MATCH (sound and correct)
- Announcement building: PARTIAL (timeout flag `t<N>` not emitted)
- Announcement signing: DIVERGENT (Perl uses `use_pkcs1_oaep_padding` — see finding below)
- Observer: MATCH
- Authorized services: MATCH
- Ticket: MATCH
- AuthzChecker: PARTIAL (authz check only fires when ticket non-empty, not on all requests)
- Requester: MATCH
- ServiceRegistry: MATCH
- Discovery cache: MATCH
- Config parsing: MATCH
- BusInfo / interface resolution: MATCH

---

## Detailed Findings

### [MATCH] Server Read Loop (src/service/server_connection.rs)

**Idle timeout (120s / no timeout when busy):**
Perl Connection.pm:134 sets `connection->timeout(busy || incoming || outgoing ? 0 : self->timeout)`.
The Rust code mirrors this exactly: `is_busy = !incoming.is_empty() || !outgoing.is_empty()`,
then wraps `reader.read()` in a 120s `tokio::time::timeout` only when `!is_busy`. MATCH.

**Packet parsing (HEADER→DATA→EOF flow):**
Rust uses `Packet::parse()` in a consume loop over a `Vec<u8>` buf grown with `tmp.extend`.
Perl uses `AnyEvent::Handle`'s `on_read` callback on its `rbuf` string buffer. Functionally
identical: both look for a complete frame (`TYPE N LEN\r\n<body>END\r\n`), consume it, and
call `_packet()`/`route_packet()`. Max header-line length of 80 bytes enforced in both
(Perl Connection.pm:47, Rust proto/packet.rs:65-72). MATCH.

**ACK sending on DATA receipt:**
Perl Connection.pm:153: `$message->on_ack(sub { $self->_send_packet('ACK', $corrno, $_[0]) })`.
The ACK is sent with the cumulative bytes received at that point. Rust server_connection.rs:142-155
sends `ACK` immediately after each `DATA` with `msg.received.to_string()` as the body. MATCH.

**TXERR validation (empty/"0" body rejected):**
Perl Connection.pm:170-175 does not explicitly reject empty/zero TXERR; it calls `$msg->finish($body)`.
The Rust code (server_connection.rs:163-170) rejects TXERR with empty or "0" body, citing
"JS connection.js:229". This is a deliberate tightening over Perl for robustness. Not a
regression — Perl simply does not validate the body content here, but Rust's behavior is
more defensive. Note the comment in the Rust code correctly attributes this to the JS impl,
not Perl. PARTIAL match (Rust is stricter than Perl).

**Unknown packet types (fatal):**
Perl Connection.pm:187: `return $self->_error(error => 'Unexpected packet of type '.$type)`.
Rust proto/packet.rs:128-130: `ParseResult::Fatal(anyhow!("Unexpected packet of type {}", cmd))`.
Both treat unknown packet types as fatal and close the connection. MATCH.

**EOF body must be empty:**
Perl Connection.pm:162: `return $self->_error(error => 'EOF packet must be empty') if $body ne ''`.
The Rust client reader (reader.rs:152-155) enforces this with a log error and early return.
The server side (server_connection.rs) does not explicitly check EOF body emptiness — it just
calls `incoming.remove()`. This is a minor gap: a malformed EOF with a non-empty body would
be silently accepted on the server side. The client reader is correct.

**Out-of-sequence HEADER:**
Perl Connection.pm:141: `return $self->_error(error => 'Out of sequence message received') unless $corrno == $self->{_next_message_in}`.
Rust server_connection.rs:122-124: logs an error and returns (does not close connection).
Divergence: Perl closes the connection on out-of-sequence messages; Rust only logs and
continues. Low severity since this indicates a bug in the sender, but Perl's behavior of
closing is more correct. PARTIAL.

### [MATCH] Client Connection (src/transport/beepish/client/connection.rs)

**Duplex proxy approach:**
`ConnectionHandle::from_stream()` creates a `tokio::io::duplex(65536)` pair, spawns a
`copy_bidirectional` task to bridge it with the real TLS stream, then splits the proxy end
for the reader and writer tasks. This correctly avoids the deadlock where TLS internally needs
to write during a read (TLS handshake renegotiation or alert). Perl uses AnyEvent's
event-driven approach which inherently avoids this. The proxy approach is architecturally sound.

**Fingerprint verification before traffic (natural corking):**
Perl Connection.pm:117-127 sets `_corked => 1` and defers writes until `on_starttls` fires
and the fingerprint is validated. Rust client/connection.rs:142-155 validates the fingerprint
synchronously before calling `from_stream()`, so no data flows until the fingerprint check
passes. Equivalent outcome. MATCH.

**Connection pooling by URI:**
Perl ConnectionManager.pm:17-28 caches one connection per URI. Rust BeepishClient uses
`Arc<Mutex<HashMap<String, Arc<ConnectionHandle>>>>` keyed by `service_info.uri`. MATCH.

**On-lost / connection cleanup:**
Perl Client.pm:45-53 calls `on_lost` when the connection drops, delivering errors to pending
callbacks. Rust reader_task (reader.rs:86-88) calls `notify_all_pending()` with "Connection lost"
when the reader exits, which delivers errors to all pending oneshot senders. MATCH.

**`_nextcorr` starting value:**
Perl Client.pm:33: `$self->{_nextcorr} = 1` (starts at 1). Rust client/connection.rs:117:
`next_request_id: AtomicI64::new(1)`. MATCH.

**`next_outgoing_msg_no` starting value:**
Perl Connection.pm:99: `$self->{_next_message_out} = 0`. Rust client/connection.rs:118:
`next_outgoing_msg_no: AtomicU64::new(0)`. MATCH.

### [MATCH] Client Reader (src/transport/beepish/client/reader.rs)

Rewritten from BufReader to manual Vec buf + `AsyncReadExt::read()`. The pattern matches
the server connection: read into a 4096-byte tmp buf, extend the Vec, parse complete packets
in a loop, drain consumed bytes. MATCH with server_connection.rs approach.

**Empty DATA body skip:**
reader.rs:135-137: `if packet.body.is_empty() { return; }` with comment citing JS
connection.js:202. Perl Connection.pm does not explicitly skip empty DATA; it calls
`$msg->add_data($body)` which would be a no-op for an empty string. Functionally equivalent.

**ACK: cumulative bytes:**
reader.rs:141-148 sends cumulative `msg.received.to_string()` as ACK body. MATCH with Perl
Connection.pm:153 which calls `on_ack` with the cumulative count.

**TXERR delivery:**
reader.rs:170-190 removes the incoming message and delivers to the pending sender with
`error: Some(error_text)`. Perl Connection.pm:172: calls `$msg->finish($body)`. MATCH.

### [MATCH] Reply Writing (src/service/server_reply.rs)

`send_reply()` builds HEADER (with `message_type: Reply`, error fields, `request_id` copied
from the request), then DATA chunks of 2048 bytes each, then EOF. This matches Perl
Server.pm:66-72 which calls `$reply->header->{request_id} = $request->header->{request_id}`,
`$reply->header->{type} = 'reply'`, then `$conn->send_message($reply)`, which in
Connection.pm:211-228 sends HEADER, DATA chunks (2048 bytes, line 218), and EOF.

One gap: `send_reply` does not wait for ACKs from the client before considering the reply
complete — it removes the outgoing state entry immediately after writing EOF (server_reply.rs:83).
Perl Connection.pm deletes from `_outgoing_messages` only after EOF is sent (line 225), same
as Rust. The `OutgoingReplyState` with `sent`/`acknowledged` tracking exists but ACK handling
in `route_packet` only updates it — nothing blocks on it. This is acceptable for the current
design (server doesn't need to block on client ACKs) but means flow control isn't enforced
server-to-client. The client side does enforce it. MATCH for correctness.

### [MATCH] error_data Field

`PacketHeader` (proto/header.rs:33): `error_data: Option<serde_json::Value>`, serialized with
`skip_serializing_if = "Option::is_none"`. Wire field name is `error_data` (lowercase, not
renamed). `ScampReply` (handler.rs:26) carries it through. `send_reply` in server_reply.rs:25
copies it into the outgoing header. The Requester (requester.rs:63-69) reads
`error_data.dispatch_failure` for D31 retry. MATCH with JS semantics (Perl does not use
`error_data` in its implementation — it's a newer extension).

### [MATCH] register_with_flags / noauth

`ScampService::register_with_flags()` (listener.rs:89-105) stores flags as `Vec<String>`.
`dispatch_and_reply` (server_connection.rs:227-230) checks whether the registered action has
`"noauth"` in its flags and skips the `AuthzChecker` if so. This is internally consistent
and matches the Perl concept of noauth actions (which simply bypass ticket checking). MATCH.

One note: `ScampService::register()` calls `register_with_flags(action, version, &[], handler)`,
so existing callers don't need to change. The announcement builder (announce.rs:14) lists
`"noauth"` in `ANNOUNCEABLE` flags, so noauth actions are correctly announced over the wire.

### [MATCH] Config::from_content()

`Config::from_content(content)` (config.rs:51-54) calls `parse_config(content, None)` and
wraps the result in `Arc`. This is a clean testing affordance with no production path
implications. The parser logic is identical to the file-based path. Perl Config.pm has no
equivalent (it only reads from file), so this is a Rust addition. Correct and sound.

### [MATCH] E2E Test Architecture (tests/e2e_full_stack.rs)

**Self-signed cert generation:**
Uses OpenSSL directly to generate RSA 2048 + self-signed X.509, matching the kind of cert
the Perl stack uses in production (scamp uses the same cert for TLS and announcement signing).

**Synthetic cache file:**
The test writes the announcement packet (which is the uncompressed text form) to a temp file
with the `\n%%%\n` delimiter, then loads it via `ServiceRegistry::new_from_cache()`. This
exactly matches the cache file format used by Perl (ServiceManager.pm:92-93 splits on `\n%%%\n`).

**Auth file:**
The test writes `<fingerprint> main:ALL` to an auth file. This is the correct format for
`authorized_services` and matches what real dev environments use.

**Fingerprint verification:**
The client connects with `danger_accept_invalid_certs(true)` but then checks the fingerprint
explicitly. This mirrors Perl's approach: `tls_ctx` with no CA verification, but
`on_starttls` does fingerprint comparison. Sound.

**No timing races:**
The 50ms sleep after `service.run()` spawn is pragmatic. The service is bound before the
handle is returned, so TcpListener is ready; the sleep just gives the tokio runtime time to
enter the accept loop. Low risk.

**Test coverage:** Five tests cover echo roundtrip, large body (DATA chunking), unknown action
error, sequential requests (connection reuse), and announcement signature verification.
All key paths exercised. MATCH.

---

### [PARTIAL] Announcement Building: timeout flag (`t<N>`) not emitted

Perl Announcer.pm:139: `push @flags, "t".$ac->timeout if $ac->timeout;` — the per-action
timeout flag (e.g., `t600`) is appended to the announced flags.

Rust announce.rs:14: `const ANNOUNCEABLE: &[&str] = &["create", "destroy", "noauth", "read", "secret", "update"];`

The `t<N>` timeout flag is not in `ANNOUNCEABLE` and is not specially handled, so timeout
flags registered via `register_with_flags("MyAction", 1, &["t600"], handler)` would NOT be
emitted in announcements. Receivers (including scamp-rs itself via `Flag::Timeout`) rely on
this flag for per-action RPC timeout overrides (ServiceInfo.pm:258, service_registry.rs:28-30).
If no services currently use `t<N>` flags registered via scamp-rs, this has no impact, but
it is a correctness gap if they ever do.

**Fix:** In announce.rs, after the `ann_flags` collection, handle `t<N>` flags:
```rust
for f in &action.flags {
    if let Some(secs) = f.strip_prefix('t').and_then(|s| s.parse::<u32>().ok()) {
        ann_flags.push(/* formatted string */ ...);
    }
}
```
Or more simply, `action.flags.iter().find_map(|f| ...)` in the same loop.

### [DIVERGENT] Announcement Signing: PKCS1v15 vs PKCS1-OAEP

Perl Announcer.pm:196-198:
```perl
$self->_signing_key->use_sha256_hash;
$self->_signing_key->use_pkcs1_oaep_padding;
my $uc_packet = ... encode_base64($self->_signing_key->sign( $blob ));
```

Despite calling `use_pkcs1_oaep_padding`, OAEP padding applies only to RSA encryption, not
signing. `Crypt::OpenSSL::RSA->sign()` uses PKCS1v15 regardless of the padding setting when
called as a sign operation. So both Perl and Rust actually produce PKCS1v15 SHA256 signatures.
The Rust code (announce.rs:130: `signer.set_rsa_padding(openssl::rsa::Padding::PKCS1)`) is
correct. The crypto.rs comment (line 36-39) correctly explains this. MATCH in behavior, but
the comment in Perl is potentially misleading to future maintainers.

### [PARTIAL] AuthzChecker: ticket required check gap

server_connection.rs:231-239:
```rust
if let Some(checker) = authz {
    if !noauth && !msg.header.ticket.is_empty() {
        if let Err(e) = checker.check_access(...).await { ... }
    }
}
```

The condition `!msg.header.ticket.is_empty()` means: if a request arrives WITHOUT a ticket,
authz is skipped entirely. Perl ServiceInfo.pm:151: `if (!$self->verified) { return 0 }` —
Perl's authorization check is on the service certificate, not on the ticket in the request.
Ticket validation in Perl is a separate concern handled higher up.

The Rust `AuthzChecker` is modeled after JS ticket.js rather than Perl, which is intentional
(it's a newer security model). However, the current check means requests with empty tickets
bypass the authz table check entirely. Whether this matches intended behavior depends on
context: if the design intent is "no ticket = no authz check = action-level noauth wins", it
may be correct for public/anonymous actions, but it could also be a gap if actions should
require a ticket. This should be documented as a deliberate design decision.

### [MATCH] Authorized Services (src/auth/authorized_services.rs)

Pattern building matches Perl ServiceInfo.pm:130-135:
- Perl: `map { quotemeta } split /\s*,\s*/, $toks` → Rust: `regex::escape(token)` per token
- Perl: `:ALL` → `:.*` — Rust: `escaped.replace(":ALL", ":.*")`
- Perl: no `:` → `main:` prefix — Rust: `format!("main:{}", escaped)`
- Perl regex: `/^(?:$tok_rx)(?:\.|$)/i` — Rust: `(?i)^(?:{})(?:\\.|$)`
- `_meta.*` always authorized — Perl line 146, Rust line 131
- Colon in sector/action rejected — Perl line 149, Rust line 136
- Hot-reload via mtime check — Perl line 116-118, Rust `reload_if_changed()` lines 60-71

One difference: Perl checks `$ak_mtime` as a module-level singleton; Rust checks per-instance
`last_mtime`. Functionally the same for single-process use. MATCH.

### [MATCH] Ticket Verification (src/auth/ticket.rs)

Format: `version,user_id,client_id,validity_start,ttl,privs,signature` — matches all impls.
Signature: RSA PKCS1v15 SHA256 over fields 0-5 (everything before last comma). MATCH.
Base64URL decoding for signature field. MATCH.
Expiry check: `now < validity_start` (not yet valid) and `now >= validity_start + ttl`
(expired). MATCH.

### [MATCH] Discovery Cache File (src/discovery/cache_file.rs)

Delimiter `\n%%%\n` matches Perl ServiceManager.pm:92: `split /\n%%%\n/, $data`. The
first entry (before the first delimiter) is the header and is skipped by Perl
(`my ($header, @anns) = split...`). The Rust `CacheFileAnnouncementIterator` reads until the
first delimiter, skipping any empty entry (line 49: `if matched && !announcement_data.is_empty()`).
This correctly handles the header-then-announcements structure. MATCH.

### [MATCH] ServiceRegistry (src/discovery/service_registry.rs)

**Index key format:**
Rust `make_index_key`: `format!("{}:{}.v{}", sector, action_path, version).to_lowercase()`.
Perl ServiceInfo.pm:188: `"\L$sector:$aname.v$vers"` (lowercase). MATCH.

**CRUD alias keys:**
Rust `make_crud_alias_key`: `format!("{}:{}._{}.v{}", sector, namespace, tag, version)`.
Perl ServiceInfo.pm:193: `"\L$sector:$ns._$tag.v$vers"`. MATCH.

**Replay protection:**
Rust: dedup_key `fingerprint identity`, rejects if `timestamp <= prev_ts`. Perl: key is
`fingerprint worker_ident`, also rejects stale timestamps. MATCH.

**TTL check:**
Rust service_registry.rs:92: `now_f > body.params.timestamp + interval_secs * 2.1`.
Perl ServiceManager.pm:38 (for dynamic): `EV::now() + 2.1 * $info->send_interval`.
Rust interval is `body.params.interval as f64 / 1000.0` (converting ms→s). MATCH.

**Weight=0 exclusion:**
Rust get_action line 201: `e.announcement_params.weight > 0`. Perl ServiceManager.pm:59:
`$sv->weight or next`. MATCH.

**D31/D32 failure tracking with exponential backoff:**
Not present in Perl (Perl uses a simple `expires` mechanism). Rust adds this as an
improvement. No parity issue.

### [MATCH] Requester (src/requester.rs)

**simple_request equivalent:**
Perl Requester.pm:78-91 (`simple_request`) fetches from cache, looks up action, sends via
connection manager. Rust `Requester::request()` calls `request_with_opts()` which calls
`dispatch_once()` — same flow. MATCH.

**Sector:**
Perl defaults sector to `'main'` (Requester.pm:16). Rust reads `bus.default_sector` from
config, defaulting to `"main"`. MATCH.

**Error handling:**
Perl simple_request returns `(0, [$error_code, $error])` on error. Rust `dispatch_once`
converts `resp.error` (transport error from TXERR) into `Err(anyhow!(...))`. Application-level
errors (in the response header `error`/`error_code`) are passed through in `ScampResponse`.
The difference is that Rust callers must check both `Result::Err` (transport errors) and
`response.header.error` (application errors). This is correctly handled in the CLI code. MATCH.

### [PARTIAL] Announcement Building: v4-only actions not populated

Rust announce.rs:38-45 declares `v4_acns`, `v4_acname`, etc. as empty Vecs and never
populates them (all actions go into the v3 compat zone). Perl Announcer.pm:141-156 routes
actions with custom sector or custom envelopes into the v4 hash; others go into v3.

Since scamp-rs currently has no API surface for per-action sectors or envelopes (only the
service-level sector and envelopes), this is acceptable — all registered actions naturally
fall into the v3 compat path. But if a future need arises for v4-only actions (e.g., an
action in a different sector), the announce.rs v4 path is dead code that would need to be wired up.

### [MATCH] Multicast Announcer (src/service/multicast.rs)

Interval: 5 seconds (Perl Announcer.pm:40). MATCH.
Shutdown: 10 rounds of weight=0 at 1s each (Perl Announcer.pm:93). MATCH.
Compression: zlib level 9 (Perl uses `compress($uc_packet, 9)`). MATCH.
Packet prefix handling in observer: strips `'R'` or `'D'` prefix if present
(observer.rs:73-77, Perl Observer.pm:51-52). MATCH.

### [MATCH] BusInfo / Interface Resolution (src/bus_info.rs)

Perl Config.pm:66-112 resolves `if:ethN` syntax, prefers 10.x.x.x then 192.168.x.x.
Rust bus_info.rs does the same via `getifaddrs()`. Sort priority: 10→0, 192.168→1,
172.16-31→2, other→3. Perl only mentions 10.x and 192.168.x explicitly. The 172.x preference
is a bonus Rust addition (private range not in Perl). Functionally equivalent for standard
deployments. MATCH.

---

## Issues Requiring Attention

### HIGH: Server does not close on out-of-sequence HEADER

server_connection.rs:122-124 logs an error and returns from `route_packet()` without closing
the connection. Perl Connection.pm:141 calls `$self->_error(error => ...)` which destroys the
connection. The Rust connection stays open and continues processing subsequent packets. This
could lead to a confused state if a sender sends HEADER packets out of order.

**Fix:** Return a sentinel (or use a different mechanism) to signal `handle_connection()` to
break out of the loop.

### MEDIUM: Server does not validate EOF body is empty

server_connection.rs handles `PacketType::Eof` without checking `packet.body.is_empty()`.
Perl Connection.pm:162 rejects non-empty EOF bodies as fatal errors. The client reader (reader.rs:152-155)
does check this. Should be made consistent.

### MEDIUM: ACK check uses `> 0` guard that rejects ACK for msgno 0 byte 0

reader.rs:195-199 and server_connection.rs:175-179: `Ok(v) if v > 0 => v`. If the peer
sends `ACK 0 0\r\nEND\r\n` (which Perl rejects with "Malformed ACK body" for `[1-9][0-9]*`),
Rust also rejects it with "Malformed ACK body". MATCH. But this means an ACK for 0 bytes
(which shouldn't happen in practice since ACKs follow DATA) is correctly rejected.

### LOW: `listener.rs` passes `None` for `authz` in `run()`

`ScampService::run()` (listener.rs:189) passes `None` for `authz` to `handle_connection()`.
There is currently no public API to configure an `AuthzChecker` on a `ScampService`. The
`register_with_flags`/`noauth` path requires an `authz` to be Some for the noauth bypass to
matter; with `authz = None` all actions run without authorization regardless of flags.
This is an API gap — services that want to enforce auth need a way to configure `authz`.

### LOW: `next_request_id` client-side starts at 1, but `request_id` on the server is compared to `0`

The server uses `msg.header.request_id` (a `FlexInt`) for routing replies back. The client
sets `next_request_id: AtomicI64::new(1)` and the pending map is keyed by `request_id`. The
server copies `request_id` from the request header verbatim into the reply header. This is
self-consistent. No issue.

### LOW: `client_timeout` vs `server_timeout` defaults

Perl Client.pm:40: `timeout => GTSOA::Config->val('beepish.client_timeout', 90)`.
Rust: the client connection has no idle timeout at all — the reader simply blocks on `read()`
forever. This means an idle client connection (after all requests complete) will hang until
TCP keepalive closes it. The Perl client would time out after 90s idle. For the current use
case (request/response and done), this is low risk, but worth noting.
