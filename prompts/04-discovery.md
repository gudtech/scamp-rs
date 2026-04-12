# Prompt: Phase 4 — Discovery (Multicast Announce, Cache Refresh, Routing)

## Context

You are working on scamp-rs at `/Users/daniel/GT/repo/scamp-rs/`. Phases 0-3 should be complete. Now we need the discovery layer: multicast announcement sending, cache file watching, and proper sector/envelope-aware routing.

Reference implementations:
- **scamp-go**: `/Users/daniel/GT/repo/scamp-go/scamp/` — `discoveryannounce.go`, `servicecache.go`, `serviceproxy.go`
- **scamp-js**: `/Users/daniel/GT/repo/scamp-js/lib/` — `discovery/announce.js`, `discovery/observe.js`, `util/serviceMgr.js`
- Gap analysis: `/Users/daniel/GT/repo/retailops-rs/migration-research/07-scamp-rs-parity-analysis.md`

## Tasks

### 1. Multicast announcement sending

When a `ScampService` is running, it must periodically announce itself via UDP multicast so other services can discover it.

Implement an `Announcer` that:
1. Builds the announcement packet (from Phase 2's `to_announcement_packet()`)
2. Sends it via UDP multicast to the configured group address (default `239.63.248.106:5555`)
3. Repeats at the configured interval (default 5 seconds)
4. Supports suspend: send with `weight=0` to indicate the service is shutting down
5. Supports resume: send with normal weight

Reference: Go `discoveryannounce.go` `Announce()` loop; JS `announce.js` `_announce()`.

**Cross-impl difference**: JS compresses multicast packets with zlib before sending. Go does not compress. Implement without compression (matching Go) — the discovery cache manager handles decompression if needed. Add a note/TODO about zlib compatibility.

**Networking**: Use `tokio::net::UdpSocket` with multicast group join. The multicast interface should be determined from config (`bus.address` or `discovery.address`) or default to all interfaces.

### 2. Discovery cache file watching

Currently scamp-rs only reads the discovery cache file at startup. Implement live refresh:

1. Use the `notify` crate to watch `discovery.cache_path` for modifications
2. On modification, re-parse the cache file
3. Update the `ServiceRegistry` with new/changed/expired services
4. Thread-safe: the registry must be readable from request-making threads while being updated

```rust
pub struct LiveServiceRegistry {
    inner: Arc<RwLock<ServiceRegistry>>,
    _watcher: notify::RecommendedWatcher,
}
```

Reference: JS `observe.js` watches cache file. Go reads cache on each `MakeJSONRequest` call via `DefaultCache.Refresh()` — a simpler but less efficient approach.

### 3. Announcement expiry / TTL

Services that stop announcing should be removed from the registry. JS uses `sendInterval * 2.1` as the TTL — if no announcement is received within that window, the service is considered stale.

Implement expiry checking:
- Each service entry gets a `last_seen` timestamp (from the announcement)
- Periodically (or on cache refresh), remove entries whose `last_seen + ttl` is in the past
- Default TTL: `5s * 2.1 = 10.5s`

Reference: JS sets `timeout = sendInterval * 2.1` in `serviceMgr.js`.

### 4. Sector prefix in action index key

Currently `ServiceRegistry` indexes actions by `action~version` only. It should include the sector:

```
sector:ClassName.method~version
```

This matches the Go convention. When looking up an action, the caller specifies a sector. Actions not in the requested sector are not returned.

Read Go `servicecache.go` to understand the exact key format. Read JS `serviceMgr.js` `_index()` for the JS variant (`sector:class.action.vVERSION`). Pick the Go format for Rust.

### 5. Envelope-based filtering at lookup

When finding an action, the caller may specify a preferred envelope format. The lookup should prefer services that support the requested envelope.

Go includes envelope in the index key (`sector:class.action~version#envelope`). JS filters at lookup time. The JS approach is more flexible — implement that: store all envelopes per action entry, filter at `find_actions()` time.

### 6. CRUD tag aliases

JS creates alias entries for CRUD operations. For example, an action `Product.Sku` with flags containing `_read` gets an additional index entry as if it were `Product.Sku._read`. This allows looking up actions by CRUD operation type.

Read JS `serviceMgr.js` `_index()` `alias_tags` to understand the exact behavior. Implement if it's used by the RetailOps codebase (check how `intapi-go` or `gt-main-service` looks up actions).

### 7. Weight-based load balancing

When multiple services announce the same action, the current registry picks randomly. Improve this:

- Go's approach: shuffle candidates, then sort by queue depth (most sophisticated)
- JS: random selection
- For Rust: start with random (matching JS), with a TODO for queue-depth-aware selection

The `weight` field from announcements should be respected: weight=0 services are never selected (they're in graceful shutdown).

## Success Criteria

- [ ] Service sends multicast announcements on configured interval
- [ ] Other scamp services (Go/JS) can discover the Rust service via cache file
- [ ] Discovery cache file is watched for changes; registry updates live
- [ ] Stale services are expired from the registry
- [ ] Action lookup includes sector matching
- [ ] Action lookup filters by envelope when requested
- [ ] Weight=0 services are excluded from routing
- [ ] Integration test: start Rust service, verify it appears in discovery cache, verify Go/JS client can find it
- [ ] `cargo test` passes
