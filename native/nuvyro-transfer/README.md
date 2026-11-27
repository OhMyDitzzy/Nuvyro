# Nuvyro transfer engine

This Cargo workspace shares the Nuvyro Git repository with the Android app.

## Components

- `nuvyro-transfer-core`: portable protocol definitions, validation, and limits.
- `nuvyro-transfer-jni`: Android JNI lifecycle and bounded event delivery.

Package version, wire version 1.0, and JNI ABI 1 are independent. Kotlin owns UI,
permissions, discovery, Android Network selection, and URI/SAF integration.
The profile endpoint is separate from file transfer. The local-session API
reports `transport_ready:false`; it does not advertise or authorize file access.
FD attachment requires an approved transfer/storage phase.

## Contracts and build

- [Wire protocol](../../docs/transfer-protocol-v1.md)
- [Security contract](../../docs/transfer-security-v1.md)
- [Android boundary](../../docs/transfer-jni-v1.md)
- [Native build](../../docs/native-transfer-build.md)

The workspace pins Rust 1.100.0 and requires Rust 1.100 or later, edition 2024.
Use Cargo.lock for dependency resolution. Keep generated target directories and
shared libraries out of Git. Both crates have `publish = false`; no SPDX license
is declared in their manifests.

## Local checks

Run from this directory:

```sh
cargo metadata --format-version 1 --no-deps --frozen
cargo fmt --all -- --check
python3 -B tools/check_contract_vectors.py
python3 -B tools/test_android_elf.py
```

The Python commands validate fixture encodings and synthetic artifact headers;
they do not execute the Rust implementation or establish TLS/device compatibility.
