# C# Parity Review v2

Re-review of scamp-rs against gt-soa/csharp, following up on REVIEW-CSHARP.md.

## Previously Flagged Items

### 1. `error_data` on PacketHeader -- FIXED

`PacketHeader` now has `pub error_data: Option<serde_json::Value>` with `skip_serializing_if = "Option::is_none"`. The field is used in `Requester::request_with_opts` to detect `dispatch_failure` from `error_data.dispatch_failure`, matching C# `RPCException.DispatchFailure()` and JS `requester.js:50-58`.

### 2. Auth.getAuthzTable integration -- FIXED

New module `auth/authz.rs` implements `AuthzChecker` which:
- Fetches `Auth.getAuthzTable~1` via SCAMP RPC (matching C# `Ticket.GetAuthzTable`).
- Caches the result for 5 minutes (matching JS `ticket.js:53`).
- Parses the response JSON, skipping `_NAMES` and metadata keys.
- Checks ticket privileges against per-action demands.
- Integrated into `server_connection::dispatch_and_reply` which calls `checker.check_access()` before handler dispatch, respecting `noauth` flags. This matches C# `ServiceAgent.CheckPermissions`.

### 3. Connection Handling (Server) port config -- STILL OPEN

Port range (30100-30399) and bind tries (20) remain hardcoded in `listener.rs:117-119`. C# reads `beepish.first_port` and `beepish.bind_tries` from config.

### 4. Message Streaming Abstraction -- STILL OPEN (by design)

Rust buffers entire request/response bodies in memory. C# `Message` class supports streaming production (`AddData`/`End`) and consumption (`Consume(DataDelegate, EndDelegate)`) with ACK-driven flow control (`BeginStream`, `WINDOW = 131072`). This remains the largest architectural difference.

### 5. ServiceAgent / Attribute Scanning -- STILL OPEN (by design)

C# uses reflection-based action scanning via `[RPC]`, `[RPCNamespace]`, `[RPCService]` attributes. Rust uses manual `service.register("action", version, handler)`. This is an idiomatic Rust approach and not a gap that needs to be closed -- Rust procedural macros could optionally provide similar functionality in the future, but manual registration is the standard pattern.

### 6. RPC Exception Model -- PARTIALLY FIXED

Rust `ScampReply::error(message, code)` now has `error_data` on `PacketHeader` for deserialization. However, `ScampReply` itself does not have an `error_data` field, so server-side handlers cannot attach structured error data to replies. The `send_reply` function in `server_connection.rs:298` always sets `error_data: None` on the reply header. C# `RPCException` carries `ErrorData` and serializes it via `AsHeader()` including into reply headers (ServiceAgent.cs:91-93).

### 7. ActionName Structured Type -- STILL OPEN (by design)

Rust uses flat strings (`path`, `version`, `sector`) rather than C#'s structured `ActionName { Sector, Namespace, Name, Version }`. The Rust approach is simpler and consistently used throughout. Not a compatibility issue.

### 8. WorkQueue Concurrency Primitive -- STILL OPEN (by design)

C# `WorkQueue` provides bounded-concurrency task dispatch (used at concurrency=20 in ServiceAgent, concurrency=1 for Protocol sequencing). Rust uses tokio tasks and channels. Server connections process requests sequentially within a single task. For high-concurrency workloads, Rust would need a semaphore or similar mechanism, but tokio's task model inherently provides concurrency across connections.

### 9. Permission Checking Framework -- FIXED

`AuthzChecker` is now integrated into `server_connection.rs` dispatch flow. When an `AuthzChecker` is configured, ticket verification and privilege checking happen before handler invocation. Actions with `noauth` flag bypass the check. This matches C# `ServiceAgent.CheckPermissions`.

### 10. Requester gaps (JSON convenience, sync API, TargetIdent) -- STILL OPEN

- No `MakeJsonRequest`/`SyncJsonRequest` convenience methods (C# `Requester.MakeJsonRequest`, `SyncJsonRequest`).
- No blocking synchronous API (async-only). Acceptable for Rust.
- No `TargetIdent` option for routing to a specific service identity (C# `RequestLocalOptions.TargetIdent`).
- No `identifying_token` ticket verification on the requester side (C# `ServiceAgent.CheckPermissions` line 173 verifies `identifying_token` as a secondary ticket).

## New Gaps Found

### N1. ScampReply missing error_data propagation -- NEW

`ScampReply` struct has `body`, `error`, `error_code` but no `error_data`. The `send_reply` function (`server_connection.rs:298`) hardcodes `error_data: None`. C# propagates `RPCException.ErrorData` into reply headers (ServiceAgent.cs:93 sets `error_data` on the response header). A handler returning a dispatch_failure or structured error cannot express it.

**Impact:** Medium. Server-side handlers cannot send structured error metadata.

**Fix:** Add `pub error_data: Option<serde_json::Value>` to `ScampReply` and wire it through `send_reply`.

### N2. No PING/keepalive initiation -- NEW

Both client and server respond to PING with PONG (`reader.rs:225-233`, `server_connection.rs:202-218`), but neither side ever initiates a PING. C# `Protocol` also does not initiate PINGs (it is connection-close driven), so this is parity, but worth noting that no implementation has heartbeat initiation.

**Impact:** None for C# parity.

### N3. C# timestamp in announcements uses milliseconds since epoch, not seconds -- MINOR DISCREPANCY

C# `ServiceInfo.GetAnnounceJson` (line 278):
```csharp
(DateTime.UtcNow - new DateTime(1970, 1, 1)).TotalMilliseconds
```

Rust `announce.rs:82-85` uses `as_secs_f64()` (seconds, not milliseconds).

Perl also uses seconds (`time()` returns seconds). C# is the outlier here; Rust matches Perl. The parsing side handles both since it just reads the value as `f64`. Not a bug but noted for completeness.

### N4. Server does not propagate error_data from received dispatch_failure to client -- MINOR

When the server itself cannot dispatch (unknown action), `ScampReply::error` is used, but it cannot include `error_data: {"dispatch_failure": true}` because `ScampReply` has no `error_data` field. C# sets `dispatch_failure` in `RPCException.DispatchFailure()` and propagates it through `error_data` so the client-side requester can detect it and retry. Rust's requester checks both `error_data.dispatch_failure` and `error_code == "dispatch_failure"` so the fallback works, but proper structured signaling is missing from the server side.

### N5. No `register_with_flags` for action flags -- NEW

`ScampService::register()` always sets `flags: vec![]` on the `RegisteredAction`. There is no way to register an action with flags like `noauth`, `read`, `create`, `t600`, etc. C# gets flags from `[RPC(Flags = RPCActionFlags.NoAuth)]` attributes. The `noauth` check in `server_connection.rs:239-242` reads from `RegisteredAction.flags`, but since `register()` never sets them, `noauth` actions cannot actually be registered.

**Impact:** Medium. The authz integration works but you cannot mark actions as `noauth` through the public API.

**Fix:** Add `register_with_flags(action, version, flags, handler)` method, or add a flags parameter to `register()`.

### N6. No `identifying_token` / `RealTicket` handling -- NEW

C# `ServiceAgent.CheckPermissions` (lines 172-173) verifies `identifying_token` as a secondary ticket (`RealTicket`) and extracts `ClientID` from it. Rust passes `identifying_token` through to the handler in `ScampRequest` but does not verify it or use it for anything.

**Impact:** Low. Only matters for services that need to distinguish the real user from the effective ticket holder.

## Updated Status Table

| Area | v1 Status | v2 Status | Notes |
|------|-----------|-----------|-------|
| Wire protocol (packet framing) | MATCH | MATCH | |
| Wire protocol (header JSON) | MATCH (missing error_data) | MATCH | error_data added |
| Connection handling (client) | MATCH | MATCH | |
| Connection handling (server) | PARTIAL | PARTIAL | Port config still hardcoded |
| Discovery (announcement parsing) | MATCH | MATCH | |
| Discovery (cache file / pinboard) | MATCH | MATCH | |
| Discovery (multicast observer) | MATCH | MATCH | |
| Discovery (multicast announcer) | MATCH | MATCH | |
| Config parsing | MATCH | MATCH | |
| Ticket verification | PARTIAL | MATCH | AuthzChecker added |
| Authorized services | MATCH | MATCH | |
| Crypto utilities | MATCH | MATCH | |
| Requester (high-level API) | PARTIAL | PARTIAL | dispatch_failure fixed; still no JSON convenience or TargetIdent |
| Message streaming abstraction | MISSING | MISSING | By design |
| ServiceAgent / attribute scanning | MISSING | MISSING | By design |
| RPC exception model | MISSING | PARTIAL | error_data deserialized but not propagated from server replies |
| ActionName structured type | DIVERGENT | DIVERGENT | By design |
| WorkQueue concurrency primitive | MISSING | MISSING | By design, tokio provides alternatives |
| Permission checking framework | MISSING | FIXED | AuthzChecker integrated into dispatch |
| Auth service communications | MISSING | FIXED | Auth.getAuthzTable implemented |
| Action flag registration | N/A | NEW GAP | register() ignores flags |
| Server error_data propagation | N/A | NEW GAP | ScampReply lacks error_data |

## Remaining Actionable Items (Priority Order)

1. **Add `error_data` to `ScampReply`** and wire it through `send_reply`. Low effort, closes N1/N4.
2. **Add flags support to `register()`** (or `register_with_flags`). Low effort, closes N5 and makes `noauth` actually work.
3. **Make server bind port range configurable** from config (`beepish.first_port`, `beepish.bind_tries`). Low effort.
4. **Add `TargetIdent` option** to `RequestOpts` for routing to a specific service identity. Low effort.
5. Consider JSON convenience wrappers for the Requester if ergonomics become an issue.
