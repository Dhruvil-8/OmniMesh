# OmniMesh Threat Model

## Trust Boundaries

```
┌─────────────────────────────────────────────────┐
│                 Application Layer                │
│  (Services, SDK — trusted code)                 │
├─────────────────────────────────────────────────┤
│              Mesh Protocol Layer                 │  ← TRUST BOUNDARY
│  (Routing, Discovery — semi-trusted peers)      │
├─────────────────────────────────────────────────┤
│              Network Layer                       │  ← TRUST BOUNDARY
│  (Transport, Internet — untrusted)              │
└─────────────────────────────────────────────────┘
```

## Threat Categories

### 1. Sybil Attacks
- **Threat:** Attacker creates many fake identities to dominate routing/discovery
- **Mitigation:** PeerId is derived from Ed25519 key (computationally expensive to generate many), reputation tracking, proof-of-work for bootstrap

### 2. Eclipse Attacks
- **Threat:** Attacker surrounds a target node with malicious peers
- **Mitigation:** Diverse peer selection, random routing path segments, outbound connection limits

### 3. Replay Attacks
- **Threat:** Attacker captures and re-sends valid encrypted packets
- **Mitigation:** Nonce-based encryption (ChaCha20-Poly1305), session ratcheting invalidates old keys, monotonic counters

### 4. Man-in-the-Middle
- **Threat:** Attacker intercepts and modifies traffic between two peers
- **Mitigation:** Noise_XX mutual authentication, static key binding to PeerId

### 5. Denial of Service (DoS)
- **Threat:** Flood attacks, connection storms, resource exhaustion
- **Mitigation:** Connection limits, rate limiting, QUIC amplification protection, memory-bounded buffers

### 6. Traffic Analysis
- **Threat:** Observer analyzes packet timing/size to infer communication patterns
- **Mitigation:** Padding (future), relay mixing (future), uniform packet sizes (future)

### 7. Key Compromise
- **Threat:** Private key is stolen from disk or memory
- **Mitigation:** Encrypted key storage (ChaCha20-Poly1305 + HKDF), key rotation, zeroize on drop, forward secrecy via session ratcheting

### 8. Timing Attacks
- **Threat:** Attacker measures operation timing to extract key material
- **Mitigation:** Constant-time operations in crypto (RustCrypto guarantees), no secret-dependent branching

### 9. Identity Spoofing
- **Threat:** Attacker claims to be a different peer
- **Mitigation:** PeerId = hash(public_key), Noise handshake proves key possession

### 10. Protocol Downgrade
- **Threat:** Attacker forces use of weaker protocol version
- **Mitigation:** Minimum version enforcement, version negotiation in authenticated handshake

## Assumptions

1. The OS RNG (`OsRng`) produces cryptographically secure random numbers
2. Ed25519, ChaCha20-Poly1305, BLAKE2s, and Noise_XX are secure as specified
3. Rust's type system prevents memory corruption (no `unsafe` in application code)
4. The attacker has full control of the network between any two peers
