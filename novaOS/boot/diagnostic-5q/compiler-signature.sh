#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")" && pwd)
export RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTFLAGS="--remap-path-prefix=$root=/nova/5q --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C relocation-model=pic -C no-redzone=yes"
rustup run stable cargo build --locked --offline --release --target x86_64-unknown-none --manifest-path "$root/Cargo.toml" --target-dir "$1"
