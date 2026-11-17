#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd .. && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/date-arc027/target"
export RUSTC=$(rustup which --toolchain stable rustc)

mkdir -p "$CARGO_TARGET_DIR" livraison-date-0240/bin
case "${1:-}" in
macOS) target=aarch64-apple-darwin; suffix= ;;
aarch64|x86_64)
  NOVA_ARCH=$1
  . ./novaOS/scripts/target.sh
  target=$rust_target; suffix="-$1.elf"
  export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$PWD/novaOS/user/novafs/service.ld"
  ;;
*) echo "Usage : $0 macOS|aarch64|x86_64" >&2; exit 2 ;;
esac
rustup run stable cargo build --locked --offline --release --target "$target" --manifest-path components/coreutils/date/Cargo.toml
cp "$CARGO_TARGET_DIR/$target/release/date" "livraison-date-0240/bin/date$suffix"
