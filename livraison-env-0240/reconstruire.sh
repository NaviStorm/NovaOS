#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
. "$here/lab/environnement.sh"
export RUSTC="$(rustup which --toolchain stable rustc)"
export CARGO_TARGET_DIR="$TMPDIR/env-arc027/cargo"
mkdir -p "$CARGO_TARGET_DIR" "$here/bin"
case ${1:-} in
macOS) target=aarch64-apple-darwin; suffix=darwin-arm64;;
aarch64|x86_64) suffix=$1.elf
    case $1 in aarch64) target=aarch64-unknown-none-softfloat;; x86_64) target=x86_64-unknown-none;; esac
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld";;
*) echo "Usage : $0 macOS|aarch64|x86_64"; exit 2;;
esac
rustup run stable cargo build --release --locked --offline --target "$target" --manifest-path "$root/components/env/Cargo.toml"
for bin in env env-witness; do cp "$CARGO_TARGET_DIR/$target/release/$bin" "$here/bin/$bin-$suffix"; done
