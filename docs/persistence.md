# Persistence & State Management

## What Gets Persisted

| Data | Storage | Lifetime |
|---|---|---|
| Node identity (keypair) | Encrypted file (ChaCha20) | Permanent |
| Known peers cache | redb (embedded KV) | Across restarts |
| Routing table snapshot | redb | Across restarts |
| Service registrations | redb | Across restarts |
| Configuration | TOML file | Permanent |

## Storage Backend

**redb** — a pure-Rust, embedded, ACID key-value store.

Why redb over alternatives:
- **vs sled**: redb is simpler, fewer bugs, actively maintained
- **vs SQLite**: no C dependency, no FFI overhead
- **vs RocksDB**: pure Rust, no cmake/C++ build chain

## Schema

```
Table: peers
  Key:   PeerId (32 bytes)
  Value: NodeInfo (bincode-encoded)

Table: routes
  Key:   PeerId (destination)
  Value: Vec<Route> (bincode-encoded)

Table: services
  Key:   ServiceId (string)
  Value: ServiceRegistration (bincode-encoded)

Table: metadata
  Key:   "last_seen_epoch" | "db_version"
  Value: u64
```

## Recovery Strategy

- On startup, load cached peers and routing table
- If DB is corrupted, delete and rebuild from network
- Identity key is stored separately (not in DB) for safety
- DB migrations via version field in metadata table
