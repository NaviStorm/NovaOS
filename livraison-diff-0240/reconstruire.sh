#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$here")
: "${NOVAOS_TMPDIR:=$(dirname "$root")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/" CARGO_TARGET_DIR="$NOVAOS_TMPDIR/diff-arc027-target"
mkdir -p "$TMPDIR" "$here/bin"
export RUSTC=$(rustup which --toolchain stable rustc)
cd "$root/components/diff"
for target in aarch64-apple-darwin aarch64-unknown-none-softfloat x86_64-unknown-none; do
 case "$target" in
 *darwin) unset RUSTFLAGS; name=diff ;;
 *) export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=--strip-all -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T../../novaOS/user/novafs/service.ld"; name="diff-${target%%-*}.elf" ;;
 esac
 rustup run stable cargo build --offline --locked --release --target "$target"
 cp "$CARGO_TARGET_DIR/$target/release/diff" "$here/bin/$name"
done
file "$here"/bin/*
