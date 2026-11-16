#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
. "$here/lab/environnement.sh"
version=$(awk -F'"' '/^version = / {print $2; exit}' "$root/components/basename/Cargo.toml")
test "$version" = "$(cat "$root/components/basename/VERSION")"
case "${1:-}" in
  macOS) target=aarch64-apple-darwin; name=basename-darwin-arm64 ;;
  aarch64|x86_64)
    case "$1" in aarch64) target=aarch64-unknown-none-softfloat ;; x86_64) target=x86_64-unknown-none ;; esac
    name=basename-$1.elf
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld" ;;
  *) echo "Usage : $0 macOS|aarch64|x86_64" >&2; exit 2 ;;
esac
export CARGO_TARGET_DIR="$TMPDIR/cargo"
rustup run stable cargo build --locked --offline --release --target "$target" --manifest-path "$root/components/basename/Cargo.toml"
mkdir -p "$here/bin"
cp "$CARGO_TARGET_DIR/$target/release/basename" "$here/bin/$name"
file "$here/bin/$name"
