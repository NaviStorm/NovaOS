#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd .. && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dd-arc027/"
export CARGO_TARGET_DIR="${DD_BUILD:-${TMPDIR}cargo}"
export RUSTC=$(rustup which --toolchain stable rustc)
mkdir -p "$TMPDIR" livraison-dd-0240/bin
case "${1:-}" in
 macOS) rust_target=aarch64-apple-darwin; name=dd;;
 aarch64|x86_64) NOVA_ARCH=$1; . ./novaOS/scripts/target.sh
 export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$PWD/novaOS/user/novafs/service.ld"
 name=dd-$1.elf;;
 *) echo "Usage : $0 macOS|aarch64|x86_64"; exit 2;;
esac
rustup run stable cargo build --manifest-path components/dd/Cargo.toml --release --offline --locked --target "$rust_target"
cp "$CARGO_TARGET_DIR/$rust_target/release/dd" "livraison-dd-0240/bin/$name"
