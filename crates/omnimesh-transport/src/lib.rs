//! # OmniMesh Transport
//!
//! Pluggable transport layer with trait-based abstraction.
//! Applications never know which transport is used underneath.
//!
//! ## Modules
//!
//! - [`traits`] — `Transport` and `Connection` traits
//! - [`quic`] — QUIC transport via quinn
//! - [`mock`] — In-memory mock transport for testing
//! - [`connection`] — Connection pooling and management

pub mod connection;
pub mod mock;
pub mod quic;
pub mod traits;

pub use traits::{Connection, Transport, TransportEvent};
