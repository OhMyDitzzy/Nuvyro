# Native transfer build

The arm64 JNI library is built explicitly and packaged only with the native
build property enabled. Generated artifacts remain in the Android build directory.

## Toolchain and dependencies

- Pinned Rust/Cargo development version: 1.100.0, edition 2024.
- Pinned NDK: 29.0.14206865. Initial target: arm64-v8a /
  aarch64-linux-android, minimum API 24. Other ABIs are not claimed supported.
- Python 3.11+ for the builder/inspector. Linux x86_64 and Linux/Termux AArch64
  hosts are recognized; recognizing a host is not a successful build test.
- JNI implementation uses jni 0.21.1 with default features disabled, getrandom
  0.3.4, serde 1.0.228, serde_json 1.0.149, and the local core. These are exact
  API pins. Cargo.lock pins transitive resolution.
- JUnit 4.13.2 is test-only for Kotlin loader tests. Gradle resolves it when those
  tests are run.

The bridge targets the jni 0.21 API. Update the bridge and its tests together
when changing that dependency.

## Checks without native compilation

From the workspace, these inspect source/configuration or Python test artifacts:

```sh
cargo fmt --all -- --check
cargo metadata --format-version 1 --no-deps --frozen
python3 -B tools/check_contract_vectors.py
python3 -B tools/test_android_elf.py
python3 -B tools/build_android.py --sdk "$ANDROID_HOME" --output ../../app/build/generated/nuvyroTransfer/jniLibs --plan
```

`--plan` invokes only rustc/cargo version queries and checks installed NDK metadata
and tool paths. It prints the future command but does not execute it, create
build outputs, or install a toolchain. rustup auto-install is explicitly disabled.
The helper tests use synthetic ELF bytes; they do not inspect a compiled library.

## Explicit native build and APK opt-in

A fresh checkout needs its pinned toolchain/NDK installed and locked Cargo sources
fetched; the build helper uses --frozen and will not fetch them implicitly:

```sh
# From native/nuvyro-transfer, fetch locked source dependencies without compiling:
cargo fetch --locked --target aarch64-linux-android

# From the Nuvyro root, build only the native artifact:
./gradlew :app:buildTransferNative

# Build/package the Android app with the arm64 library:
./gradlew :app:assembleDebug -PnuvyroNative=true
```

Without the property, normal app builds neither invoke Cargo nor package the
generated library, even after an earlier native build. Missing/incompatible
libraries are represented by the lazy loader as Unavailable. The engine factory
probes ABI before create, runs on dispatcher IO, and closes an acquired handle
if coroutine cancellation prevents its return. There is no eager UI startup load.

The task declares Rust/configuration/script inputs and generated outputs.
Up-to-date skipping is disabled because external packaged compiler/NDK state
is not fully fingerprinted. Cargo may still reuse its own target artifacts.
Only the opt-in app configuration restricts ABI to arm64-v8a and registers the
generated jniLibs source with uncompressed packaging. It adds an explicit preBuild
dependency; Cargo never runs merely because a screen is opened.

## Artifact ownership and validation

All generated content is under `app/build/generated/nuvyroTransfer/`:

```text
cargo-target/aarch64-linux-android/release/libnuvyro_transfer_jni.so
jniLibs/arm64-v8a/libnuvyro_transfer_jni.so
```

The builder uses the selected NDK's API 24 clang wrapper/llvm-ar, environment-local
linker configuration, required panic=unwind, static Rust standard library preference, and explicit
16 KiB linker alignment flags. No absolute SDK path is committed in Cargo config.
It removes a stale packaged output before compiling and copies the new library
only after validating ELF64/AArch64/ET_DYN, load segment alignment, dynamic string
tables, and a platform DT_NEEDED allowlist. RPATH/RUNPATH and implicit dependencies
such as libandroid-support.so, libstd-*.so, or unbundled C++ runtimes are rejected.

Termux's packaged Rust standard libraries may still reference Termux-specific
libraries or fail NDK linking. File presence and --plan cannot establish their
portability. If the build fails, repair the toolchain or use a verified build host;
do not weaken artifact validation or copy a host .so into the APK. Additional
native dependencies require an explicit packaging/license decision.

ELF checks alone do not prove 16 KiB APK ZIP alignment, installability, or runtime
behavior. Those require the actual artifact, APK/bundle inspection, and devices
with the relevant page size.
