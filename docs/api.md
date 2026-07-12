# Public API Reference

## Core Types

### `PeerId`
- 32-byte identifier derived from SHA-256(public_key)
- `from_bytes([u8; 32])`, `to_hex()`, `from_hex()`, `to_base64()`, `short()`

### `ProtocolVersion`
- `major: u16`, `minor: u16`, `patch: u16`
- `CURRENT` — current protocol version
- `is_compatible_with(&other)` — same major = compatible

### `Config`
- `load(path)` — load from TOML file with env overrides
- `to_toml()` — serialize to TOML string
- Sections: `node`, `transport`, `identity`, `telemetry`, `storage`

### `OmniMeshError`
- Unified error enum: `Identity`, `Crypto`, `Transport`, `Routing`, `Config`, etc.
- `is_retryable()` — transient failures (connection, timeout)
- `is_security_error()` — crypto, handshake, spoofing

## Identity API

### `Keypair`
- `generate()` — new random Ed25519 keypair
- `from_seed(&[u8; 32])` — deterministic from seed
- `public_key()` → `PublicKey`
- `sign(message)` → `Signature`
- `verify(message, signature)` → `Result<()>`

### `KeyStore` (trait)
- `store(keypair, passphrase)`, `load(passphrase)`, `exists()`, `delete()`
- Implementation: `FileKeyStore` (ChaCha20-Poly1305 encrypted)

### `VirtualIp`
- `from_peer_id(&PeerId)` — deterministic IPv6 in `fd4f:4d00::/32`

## Crypto API

### `aead`
- `encrypt(key, plaintext)` → `Vec<u8>` (nonce-prepended)
- `decrypt(key, ciphertext)` → `Vec<u8>`

### `kdf`
- `derive_key(ikm, salt, info, len)` → `Vec<u8>`
- `derive_key_256(ikm, salt, info)` → `[u8; 32]`
- `ratchet_key(current)` → `[u8; 32]`

### `NoiseHandshake`
- `new(role)`, `with_keypair(role, key)`
- `write_message(payload)`, `read_message(msg)`
- `into_transport()` → `NoiseTransport`

### `SecureChannel`
- `new(shared_secret, is_initiator)` — split send/recv keys
- `encrypt(plaintext)`, `decrypt(ciphertext)` — auto-ratchet
- `messages_remaining()` — before mandatory rekey

## Transport API

### `Transport` (trait)
- `listen(addr)`, `accept()`, `connect(addr)`, `shutdown()`
- Implementations: `QuicTransport`, `MockTransport`

### `Connection` (trait)
- `send(data)`, `recv()`, `close()`
- `remote_addr()`, `is_connected()`

## Stability Guarantees

- `0.x` — API is experimental, may change between minor versions
- `1.0+` — semver: patch = fixes, minor = additions, major = breaking
- Deprecated items carry `#[deprecated(since, note)]` for 2 minor versions
