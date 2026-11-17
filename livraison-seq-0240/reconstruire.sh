#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd .. && pwd)/tmp/seq-arc027}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR" livraison-seq-0240/bin
case "${1:-}" in
macOS)
    export CARGO_PKG_VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' components/Cargo.toml | head -1)
    rustup run stable rustc --edition 2024 -C opt-level=3 --target aarch64-apple-darwin components/coreutils/seq/host.rs -o livraison-seq-0240/bin/seq
    ;;
aarch64|x86_64)
    NOVA_ARCH=$1
    . ./novaOS/scripts/target.sh
    export RUSTC=$(rustup which --toolchain stable rustc)
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$PWD/novaOS/user/novafs/service.ld"
    export CARGO_TARGET_DIR="$TMPDIR/production"
    rustup run stable cargo build --manifest-path components/Cargo.toml --locked --offline --release --target "$rust_target" --bin seq
    cp "$CARGO_TARGET_DIR/$rust_target/release/seq" "livraison-seq-0240/bin/seq-$1.elf"
    ;;
*) echo "Usage : $0 macOS|aarch64|x86_64" >&2; exit 2 ;;
esac
