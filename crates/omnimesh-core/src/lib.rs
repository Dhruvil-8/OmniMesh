//! # OmniMesh Core
//!
//! Foundation crate providing shared types, error handling, configuration,
//! observability setup, and utility functions used across the OmniMesh workspace.
//!
//! ## Modules
//!
//! - [`error`] — Unified error types for all OmniMesh crates
//! - [`config`] — TOML-based configuration with environment variable overrides
//! - [`types`] — Common types (PeerId, NodeInfo, ProtocolVersion)
//! - [`telemetry`] — Structured logging and tracing initialization
//! - [`retry`] — Retry policies with exponential backoff

pub mod config;
pub mod error;
pub mod retry;
pub mod telemetry;
pub mod types;

pub use config::Config;
pub use error::{OmniMeshError, Result};
pub use types::{PeerId, ProtocolVersion};
