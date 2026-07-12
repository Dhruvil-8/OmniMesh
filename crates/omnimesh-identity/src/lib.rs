//! # OmniMesh Identity
//!
//! Provides Ed25519 key generation, PeerId derivation, virtual IPv6 addresses,
//! and encrypted key storage for node identity management.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │   Keypair     │  Ed25519 key generation & signing
//! ├──────────────┤
//! │   PeerId      │  SHA-256 hash of public key (32 bytes)
//! ├──────────────┤
//! │  VirtualIp    │  Deterministic IPv6 from PeerId
//! ├──────────────┤
//! │  KeyStore     │  Encrypted at-rest storage (ChaCha20-Poly1305)
//! └──────────────┘
//! ```
//!
//! ## Modularity
//!
//! Each sub-module handles exactly one concern:
//! - `keypair` — Key generation, signing, verification
//! - `peer_id` — PeerId derivation from public keys
//! - `virtual_ip` — Deterministic IPv6 address mapping
//! - `keystore` — Encrypted key persistence via the `KeyStore` trait

pub mod keypair;
pub mod keystore;
pub mod peer_id;
pub mod virtual_ip;

pub use keypair::Keypair;
pub use keystore::{FileKeyStore, KeyStore};
pub use peer_id::PeerIdExt;
pub use virtual_ip::VirtualIp;
