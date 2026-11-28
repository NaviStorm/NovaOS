#!/bin/sh
set -eu
: "${NOVAOS_TMPDIR:?répertoire de travail nécessaire}"
export TMPDIR="$NOVAOS_TMPDIR/" RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTFLAGS="--remap-path-prefix=$PWD=/nova --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
rustup run stable cargo build --bin diagnostic-5m --features diagnostic-5m --release --locked --offline --target x86_64-unknown-uefi --manifest-path novaOS/boot/diagnostic-5e/Cargo.toml --target-dir "${1:-novaOS/build/target5m}"
