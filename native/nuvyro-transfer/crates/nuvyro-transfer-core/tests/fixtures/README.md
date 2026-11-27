# Contract fixtures

All IDs, tokens, and pins are deterministic public examples. They cannot prove
pairing or identify a real device; never use them to create a live invitation.

- `frame-headers.txt`: six-byte headers, including exact per-type boundaries,
  one-byte-too-small/large cases, zero/overflow-sized lengths, and unknown type.
  `ok:type:body_len` means header validation only; no body/state/authentication
  check has occurred. The Rust integration test consumes this file directly.
- `contract-vectors.json`: canonical QR examples, canonical manifest bytes with
  UTF-8/name/size/count failures, complete example frame encodings, the official
  empty-file BLAKE3 digest, and deferred lifecycle/security/storage scenarios.
  No manifest digest is supplied: manifest hashing is not implemented or verified
  by the P02 fixture checker. The valid manifests test encoding/metadata only.

Detailed manifest fixture outcomes `UnsafeName` and `DuplicateId` map to the
remote `MalformedFrame` code; they are not additional wire error codes.
Deferred scenarios are requirements for the named future milestones, not tests
that the current script or engine executes.

From the workspace directory, the standard-library-only artifact checker is:

```sh
python3 tools/check_contract_vectors.py
```

This checks fixture encoding and expected structural outcomes independently of
Rust, without compiling or loading the core. It does not establish Rust runtime
behavior, TLS/pinning, invitation races, state enforcement, JNI, hashing, SAF
finalization, or on-device transfer. Run the Rust fixture test when compilation
is explicitly allowed; complete body/stream/state tests belong to P04 and later.
