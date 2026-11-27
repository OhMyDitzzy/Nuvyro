"""Explicit, opt-in native build; --plan validates configuration without compiling."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tomllib

from android_elf import inspect_library


WORKSPACE = Path(__file__).resolve().parents[1]
NDK_VERSION = "29.0.14206865"
TARGET = "aarch64-linux-android"
ABI = "arm64-v8a"
LIBRARY = "libnuvyro_transfer_jni.so"


def configuration(sdk, output):
    sdk, output = sdk.resolve(), output.resolve()
    if not output.is_relative_to(WORKSPACE.parents[1] / "app/build"):
        raise ValueError("Native output must be under app/build")
    pin = tomllib.loads((WORKSPACE / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    probe_environment = {**os.environ, "RUSTUP_AUTO_INSTALL": "0"}
    for tool in ("rustc", "cargo"):
        version = subprocess.run([tool, "--version"], cwd=WORKSPACE, env=probe_environment,
            check=True, text=True, capture_output=True).stdout.split()
        if len(version) < 2 or version[1] != pin:
            raise ValueError(f"{tool} must match the pinned toolchain {pin}")
    ndk = sdk / "ndk" / NDK_VERSION
    properties = (ndk / "source.properties").read_text()
    metadata = dict(line.split("=", 1) for line in properties.splitlines() if "=" in line)
    metadata = {key.strip(): value.strip() for key, value in metadata.items()}
    if metadata.get("Pkg.Revision") != NDK_VERSION:
        raise ValueError("NDK metadata does not match the pinned revision")
    machine = os.uname().machine
    host = {"aarch64": "linux-arm64", "arm64": "linux-arm64", "x86_64": "linux-x86_64"}.get(machine)
    if sys.platform not in ("linux", "android") or host is None:
        raise ValueError("This build entry currently supports Linux/Termux arm64 or x86_64 hosts")
    binaries = ndk / "toolchains/llvm/prebuilt" / host / "bin"
    linker = binaries / "aarch64-linux-android24-clang"
    archiver = binaries / "llvm-ar"
    if not linker.is_file() or not archiver.is_file():
        raise ValueError("Pinned NDK has no API 24 arm64 linker/archiver for this host")
    target_directory = output.parent / "cargo-target"
    environment = os.environ.copy()
    environment["RUSTUP_AUTO_INSTALL"] = "0"
    existing = environment.get("CARGO_ENCODED_RUSTFLAGS")
    flags = existing.split("\x1f") if existing else shlex.split(environment.get("RUSTFLAGS", ""))
    flags.extend(["-C", "panic=unwind", "-C", "prefer-dynamic=no", "-C", "link-arg=-Wl,-z,max-page-size=16384", "-C", "link-arg=-Wl,-z,common-page-size=16384"])
    environment.pop("RUSTFLAGS", None)
    environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    environment["CARGO_TARGET_DIR"] = str(target_directory)
    environment["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = str(linker)
    environment["CC_aarch64_linux_android"] = str(linker)
    environment["AR_aarch64_linux_android"] = str(archiver)
    command = ["cargo", "build", "--frozen", "--release", "--target", TARGET, "--package", "nuvyro-transfer-jni"]
    source = target_directory / TARGET / "release" / LIBRARY
    destination = output / ABI / LIBRARY
    return command, environment, source, destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--plan", action="store_true")
    arguments = parser.parse_args()
    command, environment, source, destination = configuration(arguments.sdk, arguments.output)
    if arguments.plan:
        print(json.dumps({"compile": False, "command": command, "linker": environment["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"],
            "target": TARGET, "destination": str(destination), "ndk": NDK_VERSION}, indent=2))
        return
    destination.unlink(missing_ok=True)
    subprocess.run(command, cwd=WORKSPACE, env=environment, check=True)
    details = inspect_library(source.read_bytes())
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".so.tmp")
    try:
        shutil.copyfile(source, temporary)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    print(json.dumps({"library": str(destination), **details}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Native build failed: {error}") from error
