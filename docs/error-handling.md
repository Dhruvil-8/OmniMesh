# Error Handling Strategy

## Error Taxonomy

All errors flow through the unified `OmniMeshError` enum in `omnimesh-core`.

### Classification

| Category | Retryable? | Security? | Example |
|---|---|---|---|
| `Connection` | ✅ | ❌ | TCP reset, QUIC timeout |
| `Timeout` | ✅ | ❌ | Connect/read timeout |
| `Transport` | ✅ | ❌ | Send/recv failure |
| `Discovery` | ✅ | ❌ | DHT lookup failed |
| `Io` | ✅ | ❌ | File not found |
| `Crypto` | ❌ | ✅ | Decryption failed |
| `Handshake` | ❌ | ✅ | Noise auth failed |
| `KeyStore` | ❌ | ✅ | Wrong passphrase |
| `InvalidPacket` | ❌ | ✅ | Malformed data |
| `Config` | ❌ | ❌ | Bad TOML |
| `NoRoute` | ❌ | ❌ | No path to peer |
| `ProtocolMismatch` | ❌ | ❌ | Version incompatible |

### Retry Policy

Transient errors (retryable = true) are automatically retried with exponential backoff:

```
Default:      5 retries,  100ms → 30s,  ×2.0 multiplier
Aggressive:  10 retries,   50ms →  5s,  ×1.5 multiplier
Conservative: 3 retries,    1s → 60s,  ×3.0 multiplier
```

Non-retryable errors fail immediately (crypto, config, protocol).

### Circuit Breaker (Future)

For persistent failures against a specific peer:
1. **Closed** — Normal operation, track failure count
2. **Open** — After N consecutive failures, reject immediately for cooldown period
3. **Half-open** — After cooldown, allow one probe; if success → Closed, if fail → Open

### Graceful Degradation

- Connection loss → buffer outgoing messages, retry in background
- Discovery failure → use cached peer list
- Routing failure → fall back to direct connection or relay
- Key rotation failure → continue with current key, retry later
