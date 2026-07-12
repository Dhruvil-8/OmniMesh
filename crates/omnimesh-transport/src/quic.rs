//! QUIC transport implementation using quinn.
//!
//! Provides a production-ready transport over QUIC with:
//! - TLS 1.3 encryption (via rustls)
//! - Multiplexed streams (no head-of-line blocking)
//! - Connection migration support
//! - 0-RTT handshakes (future)
//!
//! NAT traversal is handled at a higher layer (discovery/relay).

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use quinn::{Endpoint, RecvStream, SendStream};
use tracing::{debug, info};

use omnimesh_core::error::{OmniMeshError, Result};

use crate::traits::{Connection, Transport};

/// QUIC-based transport implementation.
#[derive(Default)]
pub struct QuicTransport {
    /// The quinn endpoint (handles both client and server roles).
    endpoint: Option<Endpoint>,
    /// Local bind address.
    local_addr: Option<SocketAddr>,
}

/// A QUIC connection to a remote peer.
pub struct QuicConnection {
    /// The quinn connection handle.
    connection: quinn::Connection,
    /// Send stream (opened lazily on first send).
    send_stream: Option<SendStream>,
    /// Receive stream (accepted lazily on first recv).
    recv_stream: Option<RecvStream>,
    /// Whether the connection is still active.
    connected: bool,
}

impl QuicTransport {
    /// Create a new QUIC transport.
    pub fn new() -> Self {
        Self {
            endpoint: None,
            local_addr: None,
        }
    }

    /// Create a self-signed TLS configuration for development/testing.
    ///
    /// In production, this should be replaced with proper certificate
    /// management integrated with the identity layer.
    fn make_server_config() -> Result<quinn::ServerConfig> {
        let cert = rcgen::generate_simple_self_signed(vec!["omnimesh".to_string()])
            .map_err(|e| OmniMeshError::Transport(format!("cert generation: {}", e)))?;

        let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der())
            .map_err(|e| OmniMeshError::Transport(format!("key serialization: {}", e)))?;

        let server_config = quinn::ServerConfig::with_single_cert(vec![cert_der], key_der)
            .map_err(|e| OmniMeshError::Transport(format!("server config: {}", e)))?;

        Ok(server_config)
    }

    /// Create a client TLS configuration that accepts any certificate.
    ///
    /// Authentication is handled by the Noise layer on top, so we skip
    /// TLS certificate verification here.
    fn make_client_config() -> quinn::ClientConfig {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .expect("failed to create QUIC client config"),
        ))
    }
}

/// Skip TLS certificate verification.
///
/// OmniMesh uses Noise protocol for authentication on top of QUIC,
/// so the TLS layer is only used for transport encryption and
/// multiplexing. Peer authentication happens at the Noise layer.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[async_trait]
impl Transport for QuicTransport {
    type Conn = QuicConnection;

    async fn listen(&mut self, addr: SocketAddr) -> Result<()> {
        let server_config = Self::make_server_config()?;

        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| OmniMeshError::Transport(format!("bind failed: {}", e)))?;

        let local = endpoint
            .local_addr()
            .map_err(|e| OmniMeshError::Transport(format!("local addr: {}", e)))?;

        self.endpoint = Some(endpoint);
        self.local_addr = Some(local);

        info!(addr = %local, "QUIC transport listening");
        Ok(())
    }

    async fn accept(&mut self) -> Result<QuicConnection> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| OmniMeshError::Transport("not listening".into()))?;

        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| OmniMeshError::Transport("endpoint closed".into()))?;

        let connection = incoming
            .await
            .map_err(|e| OmniMeshError::Connection(format!("accept failed: {}", e)))?;

        let remote = connection.remote_address();
        debug!(remote = %remote, "accepted QUIC connection");

        Ok(QuicConnection {
            connection,
            send_stream: None,
            recv_stream: None,
            connected: true,
        })
    }

    async fn connect(&mut self, addr: SocketAddr) -> Result<QuicConnection> {
        // Create or reuse endpoint
        let mut endpoint = if let Some(ep) = &self.endpoint {
            ep.clone()
        } else {
            let ep = Endpoint::client("0.0.0.0:0".parse().unwrap())
                .map_err(|e| OmniMeshError::Transport(format!("client bind: {}", e)))?;
            self.endpoint = Some(ep.clone());
            ep
        };

        // Set client config
        endpoint.set_default_client_config(Self::make_client_config());

        let connection = endpoint
            .connect(addr, "omnimesh")
            .map_err(|e| OmniMeshError::Connection(format!("connect init: {}", e)))?
            .await
            .map_err(|e| OmniMeshError::Connection(format!("connect failed: {}", e)))?;

        debug!(remote = %addr, "QUIC connection established");

        Ok(QuicConnection {
            connection,
            send_stream: None,
            recv_stream: None,
            connected: true,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close(0u32.into(), b"shutdown");
            endpoint.wait_idle().await;
            info!("QUIC transport shut down");
        }
        self.local_addr = None;
        Ok(())
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
}

#[async_trait]
impl Connection for QuicConnection {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        if !self.connected {
            return Err(OmniMeshError::Connection("not connected".into()));
        }

        // Open a send stream if we don't have one
        let stream = match &mut self.send_stream {
            Some(s) => s,
            None => {
                let (send, _recv) = self
                    .connection
                    .open_bi()
                    .await
                    .map_err(|e| OmniMeshError::Connection(format!("open stream: {}", e)))?;
                self.send_stream = Some(send);
                self.recv_stream = Some(_recv);
                self.send_stream.as_mut().unwrap()
            }
        };

        // Write length prefix (4 bytes, big-endian) + data
        let len = (data.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .await
            .map_err(|e| OmniMeshError::Transport(format!("write len: {}", e)))?;
        stream
            .write_all(&data)
            .await
            .map_err(|e| OmniMeshError::Transport(format!("write data: {}", e)))?;

        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Bytes>> {
        if !self.connected {
            return Ok(None);
        }

        // Accept a recv stream if we don't have one
        let stream = match &mut self.recv_stream {
            Some(s) => s,
            None => {
                let (_send, recv) = self
                    .connection
                    .accept_bi()
                    .await
                    .map_err(|e| OmniMeshError::Connection(format!("accept stream: {}", e)))?;
                self.send_stream = Some(_send);
                self.recv_stream = Some(recv);
                self.recv_stream.as_mut().unwrap()
            }
        };

        // Read length prefix
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf).await {
            Ok(()) => {}
            Err(e) => {
                // Stream closed gracefully
                if matches!(e, quinn::ReadExactError::FinishedEarly(_)) {
                    return Ok(None);
                }
                return Err(OmniMeshError::Transport(format!("read len: {}", e)));
            }
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read data
        let mut data = vec![0u8; len];
        stream
            .read_exact(&mut data)
            .await
            .map_err(|e| OmniMeshError::Transport(format!("read data: {}", e)))?;

        Ok(Some(Bytes::from(data)))
    }

    async fn close(&mut self) -> Result<()> {
        self.connected = false;
        self.connection.close(0u32.into(), b"close");
        Ok(())
    }

    fn remote_addr(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    fn is_connected(&self) -> bool {
        self.connected && self.connection.close_reason().is_none()
    }
}
