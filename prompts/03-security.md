# Prompt: Phase 3 — Security (Signatures, Tickets, Authorization)

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`. Phases 0-2 should be complete. Now we need all security features: announcement signature verification, ticket verification, and service authorization.

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` — `verify.go`, `cert.go`, `ticket.go`, `authorizedservices.go`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/` — `util/ticket.js`, `handle/service.js` (authorized_keys), `util/serviceMgr.js` (registerService signature check)
- Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Dependency Changes

This phase requires adding crypto capabilities. Recommended approach:

- Add `rustls` + `tokio-rustls` to replace `tokio-native-tls` (pure Rust TLS, access to peer certificates)
- Add `ring` for RSA PKCS1v15 SHA256 verification and SHA1 fingerprinting
- Add `base64` (if not already present) for signature encoding/decoding
- Add `x509-parser` or `rustls-pemfile` for PEM certificate parsing

Update Cargo.toml and migrate the TLS connection code in `src/transport/beepish/client.rs` from `tokio-native-tls` to `tokio-rustls`.

## Tasks

### 1. Crypto module (`src/crypto.rs` — new file)

Implement low-level crypto operations:

```rust
/// Verify an RSA PKCS1v15 SHA256 signature
pub fn verify_rsa_sha256(public_key_der: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, CryptoError>;

/// Sign a message with RSA PKCS1v15 SHA256
pub fn sign_rsa_sha256(private_key_der: &[u8], message: &[u8]) -> Result<Vec<u8>, CryptoError>;

/// Compute SHA1 fingerprint of a DER-encoded certificate (format: "XX:XX:XX:..." hex pairs)
pub fn cert_sha1_fingerprint(cert_der: &[u8]) -> String;

/// Parse a PEM certificate and extract the DER-encoded public key
pub fn extract_public_key_from_pem(pem: &str) -> Result<Vec<u8>, CryptoError>;
```

Reference: Go `verify.go` `verifySHA256()`, `cert.go` `sha1FingerPrint()`.

### 2. Announcement signature verification (`src/discovery/packet.rs`)

The current `signature_is_valid()` method always returns `true`. Replace with real verification:

1. Parse the PEM certificate from the announcement packet
2. Extract the RSA public key
3. The signed message is the JSON class records (the first section of the announcement, before the certificate)
4. Verify the base64-decoded signature against the message using RSA PKCS1v15 SHA256

Read Go `scamp/verify.go` and JS `serviceMgr.js` `registerService()` to understand exactly what bytes are signed. This is critical — a mismatch means legitimate announcements will be rejected.

Write tests using real announcement data from a discovery cache file. You can find one at the path configured in soa.conf (`discovery.cache_path`), or examine the test fixtures.

### 3. Certificate fingerprinting

Compute the SHA1 fingerprint of announcement certificates for use in authorization matching. Format should match Go/JS output: colon-separated uppercase hex pairs (e.g., `"AB:CD:12:34:..."`).

Read Go `cert.go` `sha1FingerPrint()` for the exact format.

### 4. Authorized services (`src/auth/authorized_services.rs` — new file)

Parse the `bus.authorized_services` file. Format is one entry per line:

```
<sha1_fingerprint> <action_pattern> [<action_pattern> ...]
```

Where action patterns can include wildcards (globs or regexes — read Go `authorizedservices.go` and JS `service.js` `authorized_keys()` to determine the exact syntax).

Implement:
```rust
pub struct AuthorizedServices {
    entries: Vec<AuthEntry>,
}

pub struct AuthEntry {
    fingerprint: String,
    patterns: Vec<ActionPattern>,
}

impl AuthorizedServices {
    pub fn load(path: &str) -> Result<Self, io::Error>;
    pub fn is_authorized(&self, fingerprint: &str, action: &str) -> bool;
}
```

Integrate into `ServiceRegistry` — when building the registry from cache, filter actions based on authorization.

### 5. Ticket verification (`src/auth/ticket.rs` — new file)

SCAMP tickets have the format:
```
version,userId,clientId,timestamp,ttl,privs,signature
```

Where `signature` is a base64-encoded RSA PKCS1v15 SHA256 signature of `version,userId,clientId,timestamp,ttl,privs` using the ticket signing key.

Implement:
```rust
pub struct Ticket {
    pub version: i32,
    pub user_id: i64,
    pub client_id: i64,
    pub timestamp: i64,
    pub ttl: i64,
    pub privileges: Vec<String>,
}

impl Ticket {
    /// Parse and verify a ticket string. Requires the public key for signature verification.
    pub fn verify(ticket_str: &str, public_key: &[u8]) -> Result<Self, TicketError>;
    
    /// Check if the ticket has expired
    pub fn is_expired(&self) -> bool;
    
    /// Check if the ticket has the required privilege
    pub fn has_privilege(&self, priv_name: &str) -> bool;
}
```

The public key is loaded from `ticket_verify_public_key.pem` (path from config or well-known location).

Reference: Go `ticket.go` `VerifyTicket()`, `Expired()`, `CheckPrivs()`; JS `ticket.js` `verify()`, `expired()`, `checkAccess()`.

### 6. Integrate ticket verification into request dispatch

In the service's request dispatch (Phase 2), before invoking the handler:
1. If the action has the `noauth` flag, skip ticket verification
2. Otherwise, extract the `ticket` field from the request header
3. Verify the ticket (parse, check signature, check expiry)
4. Make `ticket.user_id` and `ticket.client_id` available to the handler via `ScampRequest`

## Success Criteria

- [ ] Announcement signatures verified against real discovery cache data
- [ ] Invalid signatures correctly rejected
- [ ] Certificate SHA1 fingerprints match Go/JS output for same cert
- [ ] `authorized_services` file parsed and used for action filtering
- [ ] Tickets parsed from the correct string format
- [ ] Ticket signatures verified
- [ ] Expired tickets rejected
- [ ] Privilege checking works
- [ ] `noauth` actions skip ticket verification
- [ ] TLS migrated from `tokio-native-tls` to `tokio-rustls`
- [ ] All new code has unit tests
- [ ] `cargo test` passes
