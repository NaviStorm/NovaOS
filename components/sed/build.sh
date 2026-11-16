#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
: "${NOVAOS_TMPDIR:?Définir NOVAOS_TMPDIR selon AGENTS.md}"
export TMPDIR="$NOVAOS_TMPDIR/"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$NOVAOS_TMPDIR/arc027-sed-target}"
export RUSTC="$(rustup which --toolchain stable rustc)"
cd "$here"
case "${1:-macos}" in
    macos) target=aarch64-apple-darwin; export MACOSX_DEPLOYMENT_TARGET=11.0 ;;
    aarch64) target=aarch64-unknown-none-softfloat ;;
    x86_64) target=x86_64-unknown-none ;;
    *) exit 2 ;;
esac
if [ "$target" != aarch64-apple-darwin ]; then
    : "${AR:=$(command -v llvm-ar || xcrun --find llvm-ar)}"
    : "${RANLIB:=$(command -v llvm-ranlib || xcrun --find llvm-ranlib)}"
    export AR RANLIB
    linker=$(cd ../../novaOS/user/novafs && pwd -P)/service.ld
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$linker"
fi
shift "$(( $# > 0 ))"
rustup run stable cargo build --release --locked --offline --target "$target" "$@"
echo "$CARGO_TARGET_DIR/$target/release/sed"
