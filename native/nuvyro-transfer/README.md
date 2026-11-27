# Nuvyro transfer engine

This Cargo workspace lives in the Nuvyro repository. Android and Rust sources
share the same Git history; there is no Git submodule or separate Rust remote.

## Status

This is the initial workspace scaffold. It contains a portable core library and
a JNI bridge crate configured to produce `libnuvyro_transfer_jni.so` in a future
native build. The only shared Rust API currently exposes the package version.
That version does not define the wire protocol or JNI ABI.

JNI entry points, Android loading/packaging, TLS, pairing, and file transfer are
not implemented. The Android app does not load or build these crates yet.

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
```

The workspace currently has no external dependencies. The bridge depends only
on the local core crate. Commit `Cargo.lock`; keep `target/` and generated `.so`
files out of Git. The lockfile and toolchain pin do not prove a build succeeds.

Both crates have `publish = false`. No SPDX license is declared because the
repository does not currently contain a license file; resolve licensing before
distribution rather than inferring a grant from the scaffold.

## Next steps

Define the protocol, state, trust boundaries, resource limits, and completion
contract first. Then implement JNI lifecycle and explicit Android build/packaging.
Add network and crypto dependencies only when those implementations need them.
