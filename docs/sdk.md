# SDK Design

## Uniform API

The OmniMesh SDK provides the same high-level API across all languages:

```rust
// Rust
let mesh = OmniMesh::builder()
    .name("my-node")
    .listen("0.0.0.0:4433")
    .build()
    .await?;

mesh.connect(peer_id).await?;
mesh.publish("topic", data).await?;
let response = mesh.rpc(peer_id, request).await?;
let peers = mesh.discover().await?;
```

```python
# Python (via PyO3)
mesh = OmniMesh.builder().name("my-node").listen("0.0.0.0:4433").build()
mesh.connect(peer_id)
mesh.publish("topic", data)
response = mesh.rpc(peer_id, request)
```

## FFI Strategy

Inspired by QuantumVault's FFI patterns:

```
Rust Core (omnimesh-sdk)
       │
       ├──▶ C ABI (cbindgen) ──▶ C/C++ header
       │
       ├──▶ UniFFI ──▶ Swift bindings
       │            ──▶ Kotlin bindings
       │
       ├──▶ PyO3 ──▶ Python package
       │
       └──▶ cgo ──▶ Go bindings
```

### Opaque Handle Pattern (from QuantumVault)

```rust
// FFI layer uses opaque handles + catch_unwind
pub struct MeshHandle(OmniMesh);

#[no_mangle]
pub extern "C" fn omnimesh_create() -> *mut MeshHandle {
    match panic::catch_unwind(|| {
        // ... create mesh ...
    }) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null_mut(),
    }
}
```

## SDK Layers

1. **omnimesh-sdk** — Rust-native high-level API (builder pattern)
2. **omnimesh-ffi** — C ABI exports with opaque handles
3. **Language bindings** — Generated from FFI or UniFFI

## API Stability

- Public API follows semver
- Deprecated APIs survive 2 minor versions before removal
- All breaking changes documented in CHANGELOG.md
