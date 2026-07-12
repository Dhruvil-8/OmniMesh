# OmniMesh Vision

## Mission

OmniMesh is a production-grade, modular P2P mesh networking framework that enables
**any device to communicate with any other device** — regardless of network topology,
NAT configuration, or transport layer.

## Why OmniMesh?

The internet was designed for client-server communication. P2P is bolted on as an
afterthought. Existing solutions are either:

- **Too monolithic** — libp2p is powerful but tightly coupled, making it hard
  to swap components or maintain independently
- **Too narrow** — WireGuard is VPN-only, ZeroTier is overlay-only
- **Too opinionated** — Tailscale requires a coordination server

OmniMesh takes a **modular, layered approach** where each concern (identity, crypto,
transport, discovery, routing, services) is a separate crate with a clear trait boundary.

## Non-Goals

- **Not a VPN** — OmniMesh is a library/framework, not a turnkey VPN product
- **Not a blockchain** — No consensus, no tokens, no mining
- **Not a CDN** — Not optimized for content distribution at scale
- **Not inventing crypto** — All cryptography comes from audited libraries

## Competitive Landscape

| Project | Type | Strengths | Limitations |
|---|---|---|---|
| WireGuard | VPN | Fast, simple, audited | Point-to-point only, no mesh routing |
| ZeroTier | Overlay | Easy setup, virtual L2 | Centralized controller, closed protocol |
| Tailscale | VPN mesh | Great UX, WireGuard-based | Requires coordination server |
| Yggdrasil | Overlay | Fully decentralized, IPv6 | No service layer, limited ecosystem |
| Nebula | Overlay | Certificate-based, Slack-backed | No dynamic discovery |
| Reticulum | Mesh | Works on any medium | Python-only, not production-scale |
| libp2p | Framework | Massive ecosystem | Monolithic, complex, hard to customize |

## Target Platforms

- Linux (x86_64, aarch64)
- macOS (x86_64, aarch64)
- Windows (x86_64)
- Android (aarch64) — via FFI/Kotlin
- iOS (aarch64) — via FFI/Swift
- WASM (browser) — via wasm-bindgen (limited)

## Core Values

1. **Modularity over monolith** — Every layer is independently replaceable
2. **Security by default** — No unencrypted paths, no custom crypto
3. **AI-maintainable** — Small files, clear interfaces, comprehensive tests
4. **Performance matters** — Benchmark every commit, zero-copy where possible
5. **Developer experience** — Same API across all languages
