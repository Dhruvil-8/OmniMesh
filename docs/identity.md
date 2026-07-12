# Identity Layer Design

## Overview

Every node in the OmniMesh network has a unique cryptographic identity derived
from an Ed25519 keypair. This identity is the foundation for authentication,
addressing, and trust.

## Identity Derivation Chain

```
Ed25519 Keypair
      │
      ▼
  Public Key (32 bytes)
      │
      ▼ SHA-256
  PeerId (32 bytes)
      │
      ▼ ULA mapping
  Virtual IPv6 (fd4f:4d00::/32)
```

## Components

### Keypair (`keypair.rs`)
- Ed25519 via `ed25519-dalek`
- Generation from OS CSPRNG (`OsRng`)
- Deterministic reconstruction from 32-byte seed
- Signing and verification
- Private key zeroized on drop
- Debug output redacts secret material

### PeerId (`peer_id.rs`)
- SHA-256 hash of the public key
- 32 bytes — fixed size, compact
- Does not reveal the public key (one-way hash)
- Hex and base64 encoding
- Short form for display (first 8 hex chars)

### Virtual IPv6 (`virtual_ip.rs`)
- Deterministic mapping from PeerId to IPv6
- Uses ULA prefix `fd4f:4d00::/32` ("OM" = OmniMesh in hex)
- Enables standard socket programming with mesh addresses
- No external coordination needed

### KeyStore (`keystore.rs`)
- Trait-based: `KeyStore` trait for pluggable backends
- `FileKeyStore`: encrypted at rest with ChaCha20-Poly1305
- HKDF-derived encryption key from passphrase + random salt
- Atomic writes (write to temp, then rename)
- Zeroize before delete

## Key Rotation

- Each key has an epoch number
- Rotation generates a new keypair and increments the epoch
- Old key is kept for a grace period (signature verification)
- Peers are notified of rotation via signed announcement
- PeerId changes on rotation (new public key → new hash)

## Key Backup & Restore

- Export: encrypted seed → base64 string (human-copyable)
- Import: base64 string → decrypt → reconstruct keypair
- Backup includes metadata (creation time, epoch)

## Security Properties

- Private key never leaves encrypted storage
- No key in memory longer than necessary (zeroize on drop)
- Passphrase-derived encryption key (HKDF with random salt)
- Each storage operation uses a fresh random nonce
