#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
cd "$ici/../components/printf"
: "${NOVAOS_TMPDIR:=$(cd ../../.. && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/printf-0240/"
export CARGO_TARGET_DIR="$TMPDIR/cargo" RUSTC="$(rustup which --toolchain stable rustc)"
mkdir -p "$TMPDIR" "$ici/bin"
test "$(cat VERSION)" = "$(sed -n '/^version = /s/.*"\(.*\)"/\1/p' Cargo.toml)"
for cible in aarch64-apple-darwin aarch64-unknown-none-softfloat x86_64-unknown-none; do
    case "$cible" in
        *darwin) unset RUSTFLAGS; nom=printf ;;
        *) export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=--strip-all -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T../../novaOS/user/novafs/service.ld"
           nom="printf-${cible%%-*}.elf" ;;
    esac
    rustup run stable cargo build --release --offline --locked --target "$cible"
    cp "$CARGO_TARGET_DIR/$cible/release/printf" "$ici/bin/$nom"
    file "$ici/bin/$nom"
    if [ "${1:-}" = macos ]; then break; fi
done
