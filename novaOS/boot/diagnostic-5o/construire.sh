#!/bin/sh
set -eu
: "${NOVAOS_TMPDIR:?répertoire temporaire du projet requis}"
export TMPDIR="$NOVAOS_TMPDIR/" RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTFLAGS="--remap-path-prefix=$PWD=/nova --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup -C link-arg=/timestamp:0 -C link-arg=/debug:none"
exec python3 -B novaOS/boot/diagnostic-5o/construire.py "${1:-novaOS/build/5o/rust}"
