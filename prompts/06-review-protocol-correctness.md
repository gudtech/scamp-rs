# Prompt: Review — Protocol Correctness Audit

## Context

You are reviewing scamp-rs at `/Users/daniel/GT/repo/scamp-rs/` for protocol-level correctness against the canonical Perl implementation and the other production implementations:
- **gt-soa/perl** (`/Users/daniel/GT/repo/gt-soa/perl/lib/GTSOA/`): THE canonical implementation. Wire compat with Perl is the #1 priority.
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/` (most featureful)
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` (cross-reference only, least reliable)

Your job is to find every place where scamp-rs diverges from the wire protocol in ways that would cause interoperability failures, **especially with Perl services**. This is an adversarial review — assume bugs exist and hunt for them.

## Review Checklist

### 1. Packet framing

Read `src/transport/beepish/proto.rs` and compare byte-for-byte with:
- Go: `scamp/packet.go` (`ReadPacket`, `Write`)
- JS: `lib/transport/beepish/connection.js` (packet parsing in `_ondata`)

Verify:
- [ ] Header line format: `TYPE MSGNO BODYSIZE\r\n` — are spaces correct? Is `\r\n` vs `\n` correct?
- [ ] Body follows header line directly (no separator)
- [ ] `END\r\n` trailer after body
- [ ] Zero-length body packets (EOF, ACK, PING, PONG) — is BODYSIZE=0 and body empty?
- [ ] ACK body format: is it the byte count as a string? As raw bytes? Read both Go and JS.
- [ ] Max body size: Go doesn't enforce, JS enforces 131072. What does Rust do on oversized packets?
- [ ] Message number: starts at 1? 0? Is it per-direction? Read Go `incomingmsgno`/`outgoingmsgno` and JS `_nextIncomingID`/`_nextOutgoingID`.

### 2. PacketHeader JSON format

Read the `PacketHeader` struct's serde output and compare with:
- Go: `scamp/packetheader.go` (JSON field names, casing, types)
- JS: header construction in `connection.js` `sendMessage()` and parsing in `_onHeaderPacket`

Verify:
- [ ] All JSON field names match exactly (camelCase vs snake_case? Go uses `json:"action"` tags)
- [ ] `envelope` serializes as `"json"` not `"Json"` or `0`
- [ ] `type` field name — is it `"type"` or `"message_type"`? Read Go's json tag.
- [ ] `version` is integer, not string
- [ ] `request_id` — is it integer or string on the wire? Read Go's `flexInt` usage.
- [ ] `client_id` — Go uses `flexInt` (handles string or int). Does Rust handle both?
- [ ] Optional fields (`error`, `error_code`, `identifying_token`) — are they omitted when null/empty, or sent as `null`? Read Go behavior.
- [ ] Field ordering — JSON doesn't require ordering but some parsers are sensitive. Check.

### 3. Message assembly

Read the message assembly logic and compare with:
- Go: `connection.go` `routePacket()`
- JS: `connection.js` `_onpacket`, `_onHeaderPacket`, `_onDataPacket`, `_onEofPacket`

Verify:
- [ ] First packet of a message is HEADER, followed by zero or more DATA, terminated by EOF or TXERR
- [ ] Multiple messages can be interleaved on one connection (multiplexing) — does Rust track per-msgno state?
- [ ] Go checks that each packet's msgno matches expected `incomingmsgno` and increments atomically. Does Rust?
- [ ] What happens if packets arrive out of order? (They shouldn't, but defensive handling)
- [ ] What happens if a HEADER arrives without a preceding EOF for the previous message?

### 4. Discovery announcement format

Read the announcement parsing in `src/discovery/` and compare with:
- Go: `scamp/serviceproxy.go` `UnmarshalText()`
- JS: `lib/handle/service.js`, `lib/util/serviceMgr.js`

Verify:
- [ ] Three-part format: `json\n\ncert\n\nsig` — are the delimiters `\n\n` exactly? Or something else?
- [ ] JSON body is a JSON array with fields in correct positions
- [ ] Certificate is full PEM (with `-----BEGIN CERTIFICATE-----` markers)
- [ ] Signature is base64 of RSA SHA256 over... what exactly? The JSON body bytes? The JSON body + cert? Read Go `verify.go` and JS `serviceMgr.js` to determine exactly what is signed.
- [ ] Cache file delimiter: `\n%%%\n` — any edge cases with leading/trailing whitespace?

### 5. TLS configuration

- [ ] Does Rust connect with the same TLS version/cipher suites as Go/JS?
- [ ] Certificate verification: Go uses `InsecureSkipVerify: true` in dev. What does Rust do? Read Go `client.go` TLS config.
- [ ] SNI: does Rust send SNI? Does Go? Does it matter?

### 6. Edge cases

- [ ] Empty request body (no DATA packets, just HEADER + EOF)
- [ ] Very large request body (multi-megabyte — chunking correctness)
- [ ] Concurrent requests on one connection (multiplexing stress)
- [ ] Unicode in action names, ticket strings, header fields
- [ ] Numeric overflow in `request_id`, `msg_no`, `client_id`

## Output

Write a report to `/Users/daniel/GT/repo/scamp-rs/REVIEW-protocol-correctness.md` with:

1. **Critical Issues** — will cause interop failures, must fix
2. **Warnings** — might cause issues in edge cases
3. **Confirmed Correct** — verified against both Go and JS
4. **Ambiguous** — Go and JS disagree, document the decision Rust should make

For each issue, cite the specific file and line in all three implementations.
