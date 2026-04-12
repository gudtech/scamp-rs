# Prompt: Review — Async Architecture and Correctness

## Context

You are reviewing the async/concurrency architecture of scamp-rs at `/Users/daniel/GT/repo/scamp-rs/` for correctness, deadlock potential, resource leaks, and idiomatic Rust async patterns.

## Review Checklist

### 1. Connection lifecycle

- [ ] **Task ownership**: Are spawned tasks (reader, writer, heartbeat) properly tracked? Do they get cancelled when the connection drops?
- [ ] **Drop behavior**: When a `ConnectionHandle` is dropped, are all associated tasks cleaned up? Is there a risk of zombie tasks?
- [ ] **Shutdown ordering**: Reader task, writer task, heartbeat task — is there a defined shutdown order? Can a writer task panic if the reader detects disconnect and drops shared state?

### 2. Channel patterns

- [ ] **mpsc for writes**: Is the writer channel bounded? If so, what happens under backpressure? If unbounded, is there a memory leak risk?
- [ ] **oneshot for responses**: Are oneshot senders cleaned up if the connection drops before the response arrives? Does the requester get a meaningful error (not just a RecvError)?
- [ ] **Pending map cleanup**: If a request times out, is the oneshot sender removed from the pending map? Stale entries = memory leak.

### 3. Lock contention

- [ ] **Mutex on pending map**: Is the lock held briefly (insert/remove only) or during packet processing? Long-held locks block other requests.
- [ ] **RwLock on ServiceRegistry**: Is there a risk of write starvation (frequent reads blocking rare updates)?
- [ ] **Any lock held across an `.await`?** This is a classic async Rust bug — `tokio::sync::Mutex` is OK, `std::sync::Mutex` across `.await` will deadlock.

### 4. Resource management

- [ ] **Connection pool limits**: Is there a maximum number of connections? What happens if a service has 1000 actions all being called concurrently?
- [ ] **File descriptor leaks**: Are TLS streams properly closed on error? Do `drop` impls handle cleanup?
- [ ] **Task limits**: Is there a maximum number of spawned tasks? A service under load could spawn thousands of handler tasks.

### 5. Error propagation

- [ ] **Panic safety**: If a handler panics, does it bring down the connection? The service? The entire process? Handlers should be wrapped in `std::panic::catch_unwind` or `tokio::task::JoinHandle` error handling.
- [ ] **Error recovery**: If one request fails (bad parse, handler error), does the connection remain usable for subsequent requests?
- [ ] **IO error handling**: Are `BrokenPipe`, `ConnectionReset`, `UnexpectedEof` all handled as connection loss (not fatal)?

### 6. Cancellation safety

- [ ] **Timeout + cancellation**: When `tokio::time::timeout` fires, the inner future is dropped. Are there any side effects from dropping mid-execution? (E.g., partially sent packets, partially updated state)
- [ ] **Select arms**: If `tokio::select!` is used, are all branches cancellation-safe?

### 7. Testing

- [ ] **Are there tests for concurrent access?** Multiple requests on one connection, multiple connections to one service, connection drop during request.
- [ ] **Are there tests for error paths?** Connection loss mid-request, handler panic, malformed packet.

## Output

Write a report to `/Users/daniel/GT/repo/scamp-rs/REVIEW-async-architecture.md` with:

1. **Bugs** — will cause incorrect behavior (deadlocks, leaks, lost responses)
2. **Risks** — correct under normal conditions but could fail under stress
3. **Design Suggestions** — idiomatic improvements
4. **Confirmed Good** — patterns verified as correct

Include code snippets showing the problematic patterns and suggested fixes.
