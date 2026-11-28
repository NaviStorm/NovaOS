#!/bin/sh
set -eu
: "${NOVAOS_TMPDIR:?répertoire temporaire du projet requis}"
export TMPDIR="$NOVAOS_TMPDIR/" RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTFLAGS="--remap-path-prefix=$PWD=/nova --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
exec rustup run stable cargo build --offline --locked --release --target x86_64-unknown-uefi --manifest-path novaOS/boot/diagnostic-5r/Cargo.toml --target-dir "${1:-novaOS/build/5r/rust}"
