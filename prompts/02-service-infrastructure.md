# Prompt: Phase 2 — Service Infrastructure

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`. Phase 0 (cleanup) and Phase 1 (transport core) should be complete. Now we need the service side: accepting connections, dispatching requests to handlers, and generating announcement packets.

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` — `service.go`, `requester.go`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/` — `actor/service.js`, `transport/beepish/server.js`, `discovery/announce.js`
- Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Tasks

### 1. TLS Server Listener (`src/service.rs` — new file)

Create a `ScampService` struct:

```rust
pub struct ScampService {
    name: String,          // human-readable name
    identity: String,      // "name:base64(random18bytes)"
    sector: String,        // e.g., "main"
    config: Config,
    actions: HashMap<String, RegisteredAction>,
    listener: Option<TlsListener>,  // set after bind
    address: Option<SocketAddr>,
}
```

Implement service startup:
1. Load TLS cert and key from config paths (`<service>.soa_key` / `<service>.soa_cert` in Go convention, or `<service>.key` / `<service>.cert` in JS convention — support both)
2. Select a random port in the configured range (default 30100-30399). Try binding; if it fails, pick another. Retry up to `beepish.bind_tries` times (default 20). Reference: JS `server.js` `_listen()`.
3. Accept incoming TLS connections in a loop
4. For each connection, spawn a handler task that reads packets and dispatches requests

### 2. Action Registration

```rust
pub struct RegisteredAction {
    pub name: String,        // e.g., "Product.Sku.fetch"
    pub version: i32,
    pub envelopes: Vec<EnvelopeFormat>,
    pub flags: Vec<String>,  // e.g., ["noauth"], CRUD tags
    handler: Arc<dyn ActionHandler>,
}

#[async_trait]
pub trait ActionHandler: Send + Sync {
    async fn handle(&self, request: ScampRequest) -> Result<ScampResponse, ScampError>;
}
```

Provide a convenience method that takes a closure:

```rust
impl ScampService {
    pub fn register<F, Fut>(&mut self, action: &str, version: i32, handler: F)
    where
        F: Fn(ScampRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ScampResponse, ScampError>> + Send,
    { ... }
}
```

Reference: Go `service.go` `Register()`; JS `service.js` `registerAction()`.

### 3. Request Dispatch

When a complete inbound message is assembled (from Phase 1's reader):

1. Parse the HEADER to extract `action`, `version`, `envelope`
2. Look up in the registered actions map. Key format should be `action~version` (or similar — read how Go does it in `service.go` `Handle()`)
3. If found: invoke the handler, send the response as HEADER + DATA + EOF packets with the same `request_id` and `message_type: "reply"`
4. If not found: send an error reply with `error: "no such action"` (match Go's behavior from `service.go`)

Reference: Go `service.go` `Handle()`; JS `service.js` `handleRequest()`.

### 4. Service Identity

Generate a service identity string: `"humanName:base64(random_18_bytes)"`. This is used in discovery announcements and logging.

Reference: Go `service.go` `generateRandomName()`; JS uses `crypto.randomBytes(18).toString('base64')`.

### 5. Announcement Packet Generation

Implement `ScampService::to_announcement_packet()` that serializes the service into the discovery announcement format:

The announcement body is a JSON array: `[version, identity, sector, weight, interval, address, envelopes, actions, timestamp]`

Where `actions` is an array of `[ClassName, [methodName, flags, version]]` pairs.

Then the full announcement packet is:
```
<json_body>\n\n<pem_certificate>\n\n<rsa_sha256_signature_base64>
```

Reference: Go `service.go` `MarshalText()`, `serviceAsServiceProxy()`; JS `announce.js` `_makePacket()`.

### 6. High-level `run()` method

```rust
impl ScampService {
    pub async fn run(&mut self) -> Result<(), ScampError> {
        self.bind()?;                    // Select port, create TLS listener
        self.start_announcer().await?;   // Start periodic announcement (Phase 4, stub for now)
        self.accept_loop().await         // Accept connections and dispatch
    }
}
```

For now, the announcer can be a stub (Phase 4 implements multicast sending). The accept loop and dispatch should be fully functional.

## Success Criteria

- [ ] `ScampService` can bind to a port and accept TLS connections
- [ ] Actions can be registered with name, version, and async handler
- [ ] Incoming requests are dispatched to the correct handler
- [ ] Responses are sent back with correct request_id correlation
- [ ] Unknown actions get an error reply
- [ ] Service identity is generated correctly
- [ ] Announcement packet can be serialized (signing verified in Phase 3)
- [ ] Integration test: Rust service with a test action, Rust client sends request, gets response
- [ ] `cargo test` passes
