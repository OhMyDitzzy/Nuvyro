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

## Core decoding and integrity

Feed TLS plaintext to `StreamDecoder::push`, then process its single returned
frame before feeding the unconsumed input suffix. Partial headers and bodies
are retained within the wire ceilings; invalid input, EOF, and cancellation
close the decoder. A clean EOF succeeds only at a frame boundary.
The transport must cap its downstream data queue at four chunks.

`Frame::decode` returns a borrowed `Message`; keep the frame alive while using
its strings or chunk bytes. `Manifest::decode` preserves the original encoding
and order and calculates the domain-prefixed BLAKE3 digest. Message and frame
debug formatting omits tokens, names, and data.

`FileStreamVerifier` validates one expected file context, byte count, sequential
offsets, and incremental BLAKE3. Its local `destination_ready` transition follows
`FileBegin`; a peer frame cannot make that transition. `IntegrityChecked` covers
received bytes only. It does not assert staging readback, destination export,
receipt persistence, or completion. The owning coordinator must enforce
authentication, generation, consent, deadlines, file order, and storage policy,
and close the connection on decoding or sequence failure.

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
