#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
: "${NOVAOS_TMPDIR:=$(cd .. && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
bin_dir=${NOVA_TR_BIN_DIR:-livraison-tr-0240/bin}
mkdir -p "$TMPDIR" "$bin_dir"
case "${1:-}" in
  macOS)
    CARGO_PKG_VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' components/Cargo.toml | head -1)
    export CARGO_PKG_VERSION
    rustup run stable rustc --edition 2024 -A dead_code -C opt-level=3 --target aarch64-apple-darwin components/coreutils/tr/host.rs -o "$bin_dir/tr"
    ;;
  aarch64|x86_64)
    NOVA_ARCH=$1
    . ./novaOS/scripts/target.sh
    RUSTC=$(rustup which --toolchain stable rustc)
    RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$PWD/novaOS/user/novafs/service.ld"
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$NOVAOS_TMPDIR/tr-production}"
    export RUSTC RUSTFLAGS CARGO_TARGET_DIR
    rustup run stable cargo build --manifest-path components/Cargo.toml --bin tr --release --locked --target "$rust_target"
    cp "$CARGO_TARGET_DIR/$rust_target/release/tr" "$bin_dir/tr-$1.elf"
    ;;
  *) echo "Usage : $0 macOS|aarch64|x86_64" >&2; exit 2 ;;
esac
