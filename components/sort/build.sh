#!/bin/sh
set -eu
cd "$(dirname "$0")"
export TMPDIR="${NOVAOS_TMPDIR:-${TMPDIR:?}}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$TMPDIR/arc027-sort-target}"
export RUSTC="$(rustup which --toolchain stable rustc)"
case "${1:-macos}" in
macos) target=aarch64-apple-darwin ;;
aarch64) target=aarch64-unknown-none-softfloat ;;
x86_64) target=x86_64-unknown-none ;;
*) exit 2 ;;
esac
if [ "$target" != aarch64-apple-darwin ]; then
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$(cd ../../novaOS/user/novafs && pwd -P)/service.ld"
fi
if [ "$#" -gt 0 ]; then shift; fi
rustup run stable cargo build --release --locked --target "$target" "$@"
echo "$CARGO_TARGET_DIR/$target/release/sort"
