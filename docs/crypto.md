# Cryptography Layer Design

## Principle: No Custom Cryptography

OmniMesh does **not** invent any cryptographic algorithms. All primitives come
from audited, widely-used libraries:

| Primitive | Library | Algorithm |
|---|---|---|
| Key exchange | `snow` | Noise_XX_25519_ChaChaPoly_BLAKE2s |
| Symmetric encryption | `chacha20poly1305` | ChaCha20-Poly1305 (IETF) |
| Key derivation | `hkdf` + `sha2` | HKDF-SHA256 |
| Hashing | `blake3` | BLAKE3 |
| Signing | `ed25519-dalek` | Ed25519 |
| Secure memory | `zeroize` | Compiler-fence zeroization |

## Components

### AEAD (`aead.rs`)
- ChaCha20-Poly1305 authenticated encryption
- Random 12-byte nonce prepended to ciphertext
- Wire format: `[nonce:12][ciphertext:N][tag:16]`
- Explicit-nonce variant for counter-based schemes

### Key Derivation (`kdf.rs`)
- HKDF-SHA256 with domain separation labels
- Domains: `TRANSPORT`, `SESSION_RATCHET`, `KEYSTORE`, `NOISE_PROLOGUE`, `MESSAGE_AUTH`
- Key pair splitting (encryption key + authentication key)
- Key ratcheting for forward secrecy

### Noise Protocol (`noise.rs`)
- Noise_XX pattern: mutual authentication, forward secrecy
- 3-message handshake: `e → e,ee,s,es → s,se`
- Both sides learn each other's static key
- Transitions to `NoiseTransport` for application data

### Secure Channel (`channel.rs`)
- Combines AEAD + HKDF ratcheting
- Separate send/recv keys (initiator/responder asymmetric)
- Key ratcheted after every message (forward secrecy)
- Mandatory rekeying after 1M messages (nonce exhaustion protection)
- Replay protection: ratcheted keys invalidate old ciphertexts
- Inspired by QuantumVault's `PQSession` ratcheting pattern

## Post-Quantum Migration Path

1. **Current:** X25519 + Ed25519 (classical)
2. **Phase 1:** Hybrid X25519 + ML-KEM-768 (parallel key exchange)
3. **Phase 2:** Pure ML-KEM + ML-DSA when ecosystem matures
4. **Design:** `CryptoProvider` trait allows swapping implementations
   without changing upper layers

## Security Properties

- Forward secrecy via ephemeral DH + session ratcheting
- No secret-dependent branching (constant-time crypto)
- All key material zeroized on drop
- Domain-separated key derivation prevents cross-protocol attacks
- Authenticated encryption prevents tampering
