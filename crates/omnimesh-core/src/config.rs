//! TOML-based configuration with environment variable overrides.
//!
//! Configuration is loaded in order of priority (highest first):
//! 1. Environment variables (prefixed with `OMNIMESH_`)
//! 2. Config file (TOML)
//! 3. Defaults

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::error::{OmniMeshError, Result};

/// Top-level OmniMesh configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Node configuration.
    #[serde(default)]
    pub node: NodeConfig,

    /// Transport layer configuration.
    #[serde(default)]
    pub transport: TransportConfig,

    /// Identity and key storage configuration.
    #[serde(default)]
    pub identity: IdentityConfig,

    /// Logging and telemetry configuration.
    #[serde(default)]
    pub telemetry: TelemetryConfig,

    /// Persistent storage configuration.
    #[serde(default)]
    pub storage: StorageConfig,
}

/// Node-level settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Human-readable name for this node.
    #[serde(default = "default_node_name")]
    pub name: String,

    /// Data directory for persistent state.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Enable relay mode (this node acts as a relay for others).
    #[serde(default)]
    pub relay_enabled: bool,

    /// Maximum number of concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

/// Transport layer settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Listen address for QUIC transport.
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,

    /// Bootstrap nodes for initial peer discovery.
    #[serde(default)]
    pub bootstrap_nodes: Vec<String>,

    /// Connection timeout in milliseconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,

    /// Keep-alive interval in seconds.
    #[serde(default = "default_keepalive")]
    pub keepalive_secs: u64,
}

/// Identity and key management settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// Path to the private key file.
    #[serde(default = "default_key_path")]
    pub key_path: PathBuf,

    /// Enable automatic key rotation.
    #[serde(default)]
    pub auto_rotate: bool,

    /// Key rotation interval in hours (if auto_rotate is true).
    #[serde(default = "default_rotation_interval")]
    pub rotation_interval_hours: u64,
}

/// Telemetry and observability settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Log level filter (e.g., "info", "debug", "omnimesh=trace").
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Enable JSON-formatted log output.
    #[serde(default)]
    pub json_logs: bool,

    /// Enable OpenTelemetry metrics export.
    #[serde(default)]
    pub metrics_enabled: bool,

    /// Metrics export endpoint (if metrics_enabled).
    #[serde(default)]
    pub metrics_endpoint: Option<String>,
}

/// Persistent storage settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the database file.
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    /// Maximum database size in megabytes.
    #[serde(default = "default_max_db_size")]
    pub max_size_mb: u64,
}

// ── Defaults ─────────────────────────────────────────────────────────

fn default_node_name() -> String {
    format!("omnimesh-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

fn default_data_dir() -> PathBuf {
    dirs_default("omnimesh")
}

fn default_max_connections() -> usize {
    256
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:4433".parse().unwrap()
}

fn default_connect_timeout() -> u64 {
    10_000
}

fn default_keepalive() -> u64 {
    15
}

fn default_key_path() -> PathBuf {
    dirs_default("omnimesh").join("identity.key")
}

fn default_rotation_interval() -> u64 {
    720 // 30 days
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_db_path() -> PathBuf {
    dirs_default("omnimesh").join("state.redb")
}

fn default_max_db_size() -> u64 {
    512
}

fn dirs_default(app: &str) -> PathBuf {
    // Use platform-appropriate data directory
    if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join(app)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
            .join("Library")
            .join("Application Support")
            .join(app)
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
            .join(".local")
            .join("share")
            .join(app)
    }
}

// ── Default impls ────────────────────────────────────────────────────

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            name: default_node_name(),
            data_dir: default_data_dir(),
            relay_enabled: false,
            max_connections: default_max_connections(),
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            bootstrap_nodes: Vec::new(),
            connect_timeout_ms: default_connect_timeout(),
            keepalive_secs: default_keepalive(),
        }
    }
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            key_path: default_key_path(),
            auto_rotate: false,
            rotation_interval_hours: default_rotation_interval(),
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            json_logs: false,
            metrics_enabled: false,
            metrics_endpoint: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            max_size_mb: default_max_db_size(),
        }
    }
}

// ── Loading ──────────────────────────────────────────────────────────

impl Config {
    /// Load configuration from a TOML file, falling back to defaults.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            OmniMeshError::Config(format!(
                "failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| OmniMeshError::Config(format!("invalid config TOML: {}", e)))?;
        Ok(config)
    }

    /// Load configuration from a TOML file with environment variable overrides.
    ///
    /// Environment variables are prefixed with `OMNIMESH_` and use underscores
    /// for nesting. For example:
    /// - `OMNIMESH_NODE_NAME` → `node.name`
    /// - `OMNIMESH_TRANSPORT_LISTEN_ADDR` → `transport.listen_addr`
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut config = match path {
            Some(p) if p.exists() => Self::from_file(p)?,
            _ => Self::default(),
        };

        // Apply environment variable overrides
        config.apply_env_overrides();
        Ok(config)
    }

    /// Apply environment variable overrides to the config.
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("OMNIMESH_NODE_NAME") {
            self.node.name = val;
        }
        if let Ok(val) = std::env::var("OMNIMESH_TRANSPORT_LISTEN_ADDR") {
            if let Ok(addr) = val.parse() {
                self.transport.listen_addr = addr;
            }
        }
        if let Ok(val) = std::env::var("OMNIMESH_LOG_LEVEL") {
            self.telemetry.log_level = val;
        }
        if let Ok(val) = std::env::var("OMNIMESH_DATA_DIR") {
            self.node.data_dir = PathBuf::from(val);
        }
    }

    /// Serialize the config to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| OmniMeshError::Config(format!("failed to serialize config: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.transport.listen_addr.port(), 4433);
        assert_eq!(config.node.max_connections, 256);
        assert_eq!(config.telemetry.log_level, "info");
    }

    #[test]
    fn test_config_round_trip() {
        let config = Config::default();
        let toml_str = config.to_toml().unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.transport.listen_addr.port(), 4433);
    }

    #[test]
    fn test_config_from_toml_string() {
        let toml_str = r#"
[node]
name = "test-node"
max_connections = 128

[transport]
listen_addr = "127.0.0.1:5555"
connect_timeout_ms = 5000

[telemetry]
log_level = "debug"
json_logs = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.node.name, "test-node");
        assert_eq!(config.node.max_connections, 128);
        assert_eq!(config.transport.listen_addr.port(), 5555);
        assert_eq!(config.telemetry.log_level, "debug");
        assert!(config.telemetry.json_logs);
    }
}
