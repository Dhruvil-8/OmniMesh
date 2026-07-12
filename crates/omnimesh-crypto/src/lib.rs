//! # OmniMesh Crypto
//!
//! Cryptographic primitives for OmniMesh. **No custom cryptography** — all
//! algorithms come from audited libraries (RustCrypto, snow).
//!
//! ## Modules
//!
//! - [`aead`] — ChaCha20-Poly1305 authenticated encryption
//! - [`kdf`] — HKDF-based key derivation with domain separation
//! - [`noise`] — Noise_XX handshake via `snow` for authenticated key exchange
//! - [`channel`] — Secure bidirectional channel with key ratcheting
//!
//! ## Design Principles
//!
//! 1. Every module is independent and testable in isolation
//! 2. Key material is zeroized on drop
//! 3. Domain-separated key derivation prevents cross-protocol attacks
//! 4. Session keys are ratcheted after each message (forward secrecy)

pub mod aead;
pub mod channel;
pub mod kdf;
pub mod noise;
