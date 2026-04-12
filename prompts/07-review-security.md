# Prompt: Review — Security Implementation Audit

## Context

You are reviewing the security implementation in scamp-rs at `/Users/daniel/GT/repo/scamp-rs/` for correctness against the production implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` — `verify.go`, `cert.go`, `ticket.go`, `authorizedservices.go`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/` — `util/ticket.js`, `handle/service.js`, `util/serviceMgr.js`

This is a security-focused review. The consequences of bugs here are: accepting forged announcements, accepting invalid tickets (auth bypass), or failing to enforce service authorization.

## Review Checklist

### 1. Announcement Signature Verification

Read `src/crypto.rs` and `src/discovery/packet.rs` `signature_is_valid()`.

- [ ] **What bytes are signed?** Read Go `verify.go` — it signs `classRecordString` which is... what exactly? The raw JSON? A normalized form? Read the exact code path from `serviceproxy.go` `MarshalText()` through `signSHA256()`.
- [ ] **Signature algorithm**: RSA PKCS1v15 with SHA256. Is Rust using the exact same padding scheme? (`ring` uses `RSA_PKCS1_2048_8192_SHA256` — is this compatible with Go's `crypto/rsa` `PKCS1v15`?)
- [ ] **Base64 encoding**: Is the signature standard base64 or URL-safe base64? With or without padding?
- [ ] **Certificate parsing**: Does Rust extract the public key the same way Go does? Go uses `x509.ParseCertificate()` then `.PublicKey`. What if the cert has multiple keys or an unusual format?
- [ ] **Negative test**: Does Rust correctly reject a tampered announcement (modified JSON with valid cert but mismatched signature)?
- [ ] **Positive test**: Can Rust verify a real announcement from the discovery cache?

### 2. Certificate Fingerprinting

Read `src/crypto.rs` `cert_sha1_fingerprint()`.

- [ ] **Input**: Is it the DER-encoded certificate bytes (not PEM)? Read Go `cert.go` `sha1FingerPrint()`.
- [ ] **Output format**: Colon-separated uppercase hex? (`"AB:CD:EF:..."`) Or lowercase? Or no colons? Compare Go and JS output for the same cert.
- [ ] **Test**: Compute fingerprint of a known cert and compare with Go/JS output.

### 3. Ticket Verification

Read `src/auth/ticket.rs`.

- [ ] **Parsing**: The ticket format is `version,userId,clientId,timestamp,ttl,privs,signature`. But `privs` itself may contain commas (it's a comma-separated list of privilege names). How is the boundary between privs and signature determined? Read Go `ticket.go` carefully — it likely splits on the LAST comma or uses a fixed field count.
- [ ] **Signed content**: What bytes does the signature cover? The entire string up to (but not including) the signature? Or a specific subset of fields? Read Go `VerifyTicket()` to see what `message` is passed to `verifySHA256()`.
- [ ] **Public key source**: Where does the verification public key come from? Is it `ticket_verify_public_key.pem`? Read Go.
- [ ] **Timestamp handling**: Is `timestamp` Unix seconds? And `ttl` is seconds to add? What's the tolerance for clock skew?
- [ ] **Privilege format**: Are privileges stored as strings? Integers? Read the format in Go.
- [ ] **Negative tests**: Invalid signature, expired ticket, missing privilege — all correctly rejected?

### 4. Authorized Services

Read `src/auth/authorized_services.rs`.

- [ ] **File format**: One line per entry? What's the exact delimiter between fingerprint and action patterns?
- [ ] **Action patterns**: Are they exact matches? Glob patterns (`Product.*`)? Regexes? Read Go and JS — they may differ.
- [ ] **Wildcard handling**: Is `*` a valid pattern meaning "all actions"? Read the implementations.
- [ ] **Integration**: Is authorization checked at the right point — when building the service registry from cache? Or when routing a request?

### 5. TLS Security

- [ ] **Certificate verification**: Is the service's TLS certificate verified against the announced certificate fingerprint? Read JS `client.js` which does `cert.fingerprint` comparison.
- [ ] **Dev mode**: Is there a way to disable cert verification for development? Is it appropriately gated?
- [ ] **Cipher suites**: Are the chosen ciphers compatible with Go/JS services?

## Output

Write a report to `/Users/daniel/GT/repo/scamp-rs/REVIEW-security.md` with:

1. **Critical** — security bypass, must fix before any production use
2. **Important** — incorrect behavior that could cause issues
3. **Verified** — confirmed correct against both implementations
4. **Recommendations** — hardening suggestions

For each finding, cite specific code in all three implementations and explain the expected vs actual behavior.
