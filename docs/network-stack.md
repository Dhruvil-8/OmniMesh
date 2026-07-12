# Network Stack & Wire Protocol

## Packet Format

Every OmniMesh packet on the wire follows this structure:

```
┌──────────┬──────────┬──────────┬───────────┬──────────────┐
│ Magic    │ Version  │ Type     │ Length    │ Payload      │
│ 4 bytes  │ 3 bytes  │ 1 byte   │ 4 bytes  │ variable     │
│ "OMSH"   │ M.m.p    │ enum     │ BE u32   │              │
└──────────┴──────────┴──────────┴───────────┴──────────────┘
```

## Packet Types

| Type | Value | Description |
|---|---|---|
| `Handshake` | 0x01 | Noise handshake messages |
| `Data` | 0x02 | Encrypted application data |
| `Control` | 0x03 | Routing updates, keepalive |
| `Discovery` | 0x04 | Peer discovery messages |
| `ServiceAd` | 0x05 | Service advertisement |
| `Ping` | 0x06 | Latency measurement |
| `Pong` | 0x07 | Latency measurement reply |

## Version Negotiation

1. Initiator sends `Handshake` with its `ProtocolVersion`
2. Responder checks compatibility (`same major = compatible`)
3. If compatible, responder replies with its version → use minimum
4. If incompatible, responder sends `ProtocolMismatch` error

## Backward Compatibility

- Minor version additions are backward-compatible
- Unknown fields are ignored by older versions
- New packet types are ignored by older versions (logged as warning)
- Major version bump = breaking change, requires upgrade

## Encoding

- Control plane: MessagePack (compact, schema-less)
- Data plane: raw bytes (application-defined)
- Metadata: bincode (fast, Rust-native, for internal serialization)

## MTU & Fragmentation

- Default MTU: 1200 bytes (QUIC minimum for IPv6)
- QUIC handles fragmentation/reassembly transparently
- Application sees a stream abstraction, not raw packets
