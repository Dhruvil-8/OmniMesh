# Observability

## Structured Logging

All OmniMesh crates use the `tracing` crate for structured, filterable logging.

### Log Levels

| Level | Usage |
|---|---|
| `ERROR` | Unrecoverable failures, security violations |
| `WARN` | Retryable errors, deprecation notices, unusual conditions |
| `INFO` | Lifecycle events: node start/stop, connection established |
| `DEBUG` | Protocol details: handshake steps, routing decisions |
| `TRACE` | Packet-level: every send/recv, every crypto operation |

### Output Formats

- **Human-readable** (default): colored terminal output for development
- **JSON**: structured logs for production (log aggregation systems)

```bash
# Human-readable
OMNIMESH_LOG_LEVEL=debug cargo run -p omnimesh-node

# JSON for production
OMNIMESH_LOG_LEVEL=info omnimesh run --json-logs
```

### Span Hierarchy

```
omnimesh
├── identity
│   ├── key_generate
│   ├── key_load
│   └── key_store
├── crypto
│   ├── noise_handshake
│   └── channel_encrypt
├── transport
│   ├── quic_listen
│   ├── quic_connect
│   └── quic_send
└── routing
    ├── route_compute
    └── metric_update
```

## Metrics (Future)

Export via OpenTelemetry / Prometheus:

- `omnimesh_connections_active` — gauge
- `omnimesh_bytes_sent_total` — counter
- `omnimesh_bytes_received_total` — counter
- `omnimesh_handshake_duration_seconds` — histogram
- `omnimesh_route_computation_seconds` — histogram
- `omnimesh_peer_count` — gauge

## Health Checks (Future)

HTTP endpoint at `/health`:
```json
{
  "status": "healthy",
  "peer_id": "abc12345",
  "uptime_seconds": 3600,
  "connections": 12,
  "version": "0.1.0"
}
```
