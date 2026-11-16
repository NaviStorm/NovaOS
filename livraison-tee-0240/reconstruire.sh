#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
: "${NOVAOS_TMPDIR:=$(cd .. && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/tee-arc027/"
mkdir -p "$TMPDIR" livraison-tee-0240/bin
case "${1:-}" in
macOS)
    export CARGO_PKG_VERSION=$(awk '/^version = /{gsub(/"/,"",$3);print $3;exit}' components/Cargo.toml)
    src=components/coreutils/tee
    rustup run stable rustc --edition 2024 --target aarch64-apple-darwin --crate-type staticlib -C panic=abort -C opt-level=3 "$src/host.rs" -o "${TMPDIR}libtee.a"
    cc -arch arm64 -O2 -Wall -Wextra -Werror "$src/darwin.c" "${TMPDIR}libtee.a" -o livraison-tee-0240/bin/tee
    ;;
aarch64|x86_64)
    NOVA_ARCH=$1
    . ./novaOS/scripts/target.sh
    export RUSTC=$(rustup which --toolchain stable rustc)
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$PWD/novaOS/user/novafs/service.ld"
    export CARGO_TARGET_DIR="${TMPDIR}cargo"
    rustup run stable cargo build --offline --locked --release --manifest-path components/Cargo.toml --bin tee --target "$rust_target"
    cp "$CARGO_TARGET_DIR/$rust_target/release/tee" "livraison-tee-0240/bin/tee-$1.elf"
    ;;
*) echo "Usage : $0 macOS|aarch64|x86_64" >&2; exit 2;;
esac
