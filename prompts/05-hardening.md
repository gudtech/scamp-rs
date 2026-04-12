# Prompt: Phase 5 — Hardening (Errors, Reconnection, Graceful Shutdown)

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`. Phases 0-4 should be complete. Now we need production hardening: proper error types, connection recovery, graceful shutdown, and liveness indicators.

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/`
- Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Tasks

### 1. Typed error enum

Replace ad-hoc `anyhow::Error` usage with a structured error type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ScampError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("action not found: {action}")]
    ActionNotFound { action: String },

    #[error("request timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("remote error ({code}): {message}")]
    Remote { code: String, message: String },

    #[error("connection lost")]
    ConnectionLost,

    #[error("service not available")]
    ServiceUnavailable,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Tls(Box<dyn std::error::Error + Send + Sync>),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

Audit all existing code to use this error type consistently. Convert `anyhow::Result` returns to `Result<T, ScampError>` for public APIs. Keep `anyhow` for internal convenience where appropriate.

### 2. TXERR handling

When a TXERR packet is received:
- The body contains the error text (must be non-empty — validate this, matching JS behavior)
- Map it to `ScampError::Remote` with the error text and any error_code from the header
- Deliver to the pending request as an error

When sending a TXERR (from a handler that returns an error):
- Serialize the error message as the TXERR body
- Set `error` and `error_code` in the reply HEADER
- Reference: Go replies with JSON `{"error": "..."}` in body; JS puts error in header fields

### 3. Connection reconnection

Implement connection recovery in the client:

- When a request fails due to connection loss (`ConnectionLost`, `Io` error), mark the connection as dead
- On the next request to the same service, create a new connection
- Implement backoff on repeated connection failures: `min(60, failure_count)` seconds (match JS `Registration.prototype.connectFailed()`)
- Track connection health: if a connection hasn't been used in N seconds and fails a health check, proactively reconnect

Reference: Go `serviceProxy.go` `GetClient()` checks `isClosed`; JS `Registration.prototype.lost` event triggers reconnection.

### 4. Graceful shutdown

Implement `ScampService::shutdown()`:

1. Send a weight=0 announcement (tells discovery to stop routing to this service)
2. Stop accepting new connections on the listener
3. Wait for all active request handlers to complete (with a timeout)
4. Close all connections
5. Stop the announcer task
6. Return

Reference: JS `service.js` `stopService()` — sends suspend, waits for active requests, closes.

Also handle SIGTERM/SIGINT via `tokio::signal`:
```rust
tokio::select! {
    _ = service.accept_loop() => {},
    _ = tokio::signal::ctrl_c() => {
        service.shutdown().await;
    }
}
```

### 5. Running service file (liveness indicator)

Write a file to a well-known path when the service starts, remove it on shutdown. This lets monitoring tools quickly check if a service is running.

Format: Write the service's listen address and identity to the file.
Path: `running_service_file_dir_path` from config (Go convention).

Reference: Go `service.go` `createRunningServiceFile()`.

### 6. Client convenience API

Implement a high-level `ScampClient` with a `MakeJSONRequest`-equivalent:

```rust
impl ScampClient {
    /// Find the action in the registry, connect to a suitable service,
    /// send the request, wait for response, deserialize JSON.
    pub async fn json_request<T, R>(
        &self,
        sector: &str,
        action: &str,
        version: i32,
        payload: &T,
    ) -> Result<R, ScampError>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        // 1. Look up action in registry (with sector + envelope=json)
        // 2. Select a service instance (random, excluding weight=0)
        // 3. Get or create connection to that service
        // 4. Send request with ticket from context
        // 5. Await response with timeout
        // 6. Deserialize response body as R
    }
}
```

Also provide a builder pattern for more control:
```rust
client.request()
    .sector("main")
    .action("Product.Sku.search")
    .version(1)
    .envelope(EnvelopeFormat::Json)
    .ticket(&ticket)
    .timeout(Duration::from_secs(30))
    .body(&payload)
    .send()
    .await?
```

Reference: Go `requester.go` `MakeJSONRequest()`; JS `requester.js` `makeJsonRequest()`.

## Success Criteria

- [ ] All public APIs return typed `ScampError` (not `anyhow::Error`)
- [ ] TXERR packets correctly propagated as errors
- [ ] Client automatically reconnects after connection loss
- [ ] Connection backoff prevents rapid reconnection loops
- [ ] Service shuts down gracefully: drain requests, stop announcer, close connections
- [ ] Running service file written and cleaned up
- [ ] `json_request()` convenience works end-to-end
- [ ] Builder pattern API works
- [ ] `cargo test` passes
- [ ] `cargo clippy` clean
