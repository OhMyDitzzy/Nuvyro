# Nuvyro transfer engine

This Cargo workspace lives in the Nuvyro repository. Android and Rust sources
share the same Git history; there is no Git submodule or separate Rust remote.

## Status

The workspace contains the portable core and a JNI lifecycle bridge that targets
`libnuvyro_transfer_jni.so`. P02 adds wire 1.0 limits/message codes, allocation-free
six-byte header validation, ABI 1 constants, and public contract fixtures. P03
adds entry points, an opaque engine registry, bounded local lifecycle events,
Kotlin ownership/loading, and an explicit opt-in Android build entry. The package
version remains independent of the wire version and JNI ABI.

Nothing here has been compiled, loaded into a JVM, or tested through an APK.
Only local session creation/close is implemented; SessionStarted explicitly has
`transport_ready:false`. No existing screen creates an engine or loads the library.
Full wire body/stream/state parsing, TLS/pairing, approved FD I/O, and file transfer
remain unimplemented. FD entry points refuse ownership with InvalidState.

The implementation contracts are [wire 1.0](../../docs/transfer-protocol-v1.md),
[security](../../docs/transfer-security-v1.md), and
[JNI ABI 1](../../docs/transfer-jni-v1.md). These define behavior to implement,
not an audited or operational transfer protocol.
See [native build/verification](../../docs/native-transfer-build.md) for the P03
implemented subset, safe checks, opt-in packaging, and remaining runtime gates.

## Layout

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
crates/
  nuvyro-transfer-core/   Portable session and transfer logic
  nuvyro-transfer-jni/    Android bridge and native boundary
```

The core must remain independent of Android and JNI. Kotlin continues to own UI,
permissions, discovery, Android network selection, and URI/SAF integration.
Add protocol, transport, pairing, and storage adapters when their contracts are
implemented. Keep the existing discovery profile endpoint separate from transfer.

## Toolchain

The development toolchain is pinned to Rust `1.100.0`, matching the local Termux
installation inspected when this scaffold was added. The manifests require Rust
`1.100` or later. Native compilation and linking have not been verified.

With rustup, `rust-toolchain.toml` selects the development toolchain when commands
are run from this directory. Termux currently uses its packaged Rust directly;
check `rustc --version --verbose` and `cargo --version` explicitly there.

The local inventory found Android NDK `29.0.14206865` with an AArch64 host toolchain
and Rust standard-library files for four Android targets. The initial device
target is `aarch64-linux-android` / `arm64-v8a`, with Android API 24 as the app's
minimum. This inventory does not establish native build or device compatibility.
Linker configuration, additional ABIs, and 16 KiB page support remain to be tested
when Android packaging is implemented. Do not assume the Termux setup is identical
to a desktop NDK installation.

## Checks without compilation

Run these from `native/nuvyro-transfer`:

```sh
cargo metadata --format-version 1 --no-deps --frozen
cargo fmt --all -- --check
python3 tools/check_contract_vectors.py
python3 -B tools/test_android_elf.py
```

The Python command checks artifact encodings against an independent test oracle;
it does not execute Rust or test TLS/JNI/storage. The Rust integration fixture test
is written but has not been compiled or run. See the fixture README for scope.

The core remains independent of JNI/Android and external crates. The bridge pins
jni 0.21.1, getrandom 0.3.4, serde 1.0.228, and serde_json 1.0.149 plus the local
core. Locked dependencies were fetched without compiling them. Commit Cargo.lock;
keep generated native outputs out of Git. Pins do not prove a build succeeds.

Both crates have `publish = false`. No SPDX license is declared because the
repository does not currently contain a license file; resolve licensing before
distribution rather than inferring a grant from the scaffold.

## Next steps

P03 source implementation is present, but native compilation, JVM calls, APK
packaging/page size, Rust/Kotlin tests, FD ownership, and crypto backend validation
remain open. P04 completes stream/body/state parsing and Network-bound transport.
Add network/crypto dependencies when those implementations need them; never
disable checks to hide an unsupported build host or missing library.
