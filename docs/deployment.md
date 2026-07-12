# Deployment Guide

## CLI Interface

```bash
# Initialize a new node
omnimesh init

# Show identity
omnimesh identity show

# Generate new identity
omnimesh identity generate

# Start the node
omnimesh run

# Start with custom config
omnimesh --config /path/to/config.toml run
```

## Configuration File (TOML)

```toml
[node]
name = "my-node"
data_dir = "/var/lib/omnimesh"
relay_enabled = false
max_connections = 256

[transport]
listen_addr = "0.0.0.0:4433"
bootstrap_nodes = ["1.2.3.4:4433", "5.6.7.8:4433"]
connect_timeout_ms = 10000
keepalive_secs = 15

[identity]
key_path = "/var/lib/omnimesh/identity.key"
auto_rotate = false
rotation_interval_hours = 720

[telemetry]
log_level = "info"
json_logs = true
metrics_enabled = false

[storage]
db_path = "/var/lib/omnimesh/state.redb"
max_size_mb = 512
```

## Environment Variables

All config values can be overridden with `OMNIMESH_` prefix:
- `OMNIMESH_NODE_NAME`
- `OMNIMESH_TRANSPORT_LISTEN_ADDR`
- `OMNIMESH_LOG_LEVEL`
- `OMNIMESH_DATA_DIR`

## Docker

```dockerfile
FROM rust:1.75 AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p omnimesh-node

FROM debian:bookworm-slim
COPY --from=builder /src/target/release/omnimesh /usr/local/bin/
EXPOSE 4433/udp
ENTRYPOINT ["omnimesh"]
CMD ["run"]
```

## Cross-Compilation Targets

| Target | Status |
|---|---|
| x86_64-unknown-linux-gnu | ✅ Primary |
| x86_64-apple-darwin | ✅ Supported |
| aarch64-apple-darwin | ✅ Supported |
| x86_64-pc-windows-msvc | ✅ Supported |
| aarch64-linux-android | 🔜 Planned |
| aarch64-apple-ios | 🔜 Planned |
| wasm32-unknown-unknown | 🔜 Planned (limited) |

## Systemd Service

```ini
[Unit]
Description=OmniMesh Node
After=network.target

[Service]
Type=simple
User=omnimesh
ExecStart=/usr/local/bin/omnimesh --config /etc/omnimesh/config.toml run
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```
