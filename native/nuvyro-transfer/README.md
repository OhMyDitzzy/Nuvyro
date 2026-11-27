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

## Pairing QR input

`PairingQr::decode` accepts borrowed scanner bytes with the exact pairing prefix,
canonical unpadded base64url, a 512-byte text ceiling, and a fixed 104-byte decode
buffer. It validates version, IDs, pin/token shape, port, and TTL hint. It performs
no network, authentication, invitation consumption, or expiry decision.

`PairingQr`, `SpkiPin`, and `InvitationToken` have redacted debug output and no
implicit cloning or serialization. Pin/token access borrows their bytes; their
owned storage and the temporary decoded buffer use zeroization on drop. Callers
must clear their own scan buffers and avoid copying secrets into logs or saved
state. This does not promise erasure of copies left by moves or other libraries.
The TTL hint must never create or extend the receiver's monotonic expiry.

## Internal TLS client policy

The transport-private client configuration uses an explicit ring provider and
TLS 1.3 only, the fixed SNI name `nuvyro.local`, and ALPN `nuvyro-transfer/1`.
Resumption, early data, key logging, and secret extraction are disabled. The
owning worker must reject every TLS/I/O error and check the negotiated version,
ALPN, and single peer certificate before queuing any application bytes.

The verifier requires one X.509 v3 leaf of at most 4 KiB, an uncompressed P-256
public key, ECDSA/SHA-256 self-signature, explicit CA=false, digitalSignature,
serverAuth, and the fixed SAN DNS name. It rejects duplicate, malformed, and
unhandled critical extensions, checks wall-time validity and a maximum validity
span of 86,700 seconds, and compares SHA-256 of the complete SPKI with the scanned
pin using constant-time comparison. TLS CertificateVerify is checked separately
through rustls and its selected crypto provider; no public CA roots are trusted.

Configuration performs no socket I/O. The worker owns Android Network binding,
generation/cancellation, monotonic handshake deadlines, and resource limits.
Successful TLS profile checks do not authenticate an invitation, match the
secure session hello, authorize a file, or replace receiver consent.

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
