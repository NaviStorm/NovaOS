#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
cd "$root"
version=$(awk -F'"' '/^version = / {print $2; exit}' components/cut/Cargo.toml)
[ "$version" = "$(cat components/cut/VERSION)" ]
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR" "$here/bin"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/cut-0240-target" RUSTC="$(rustup which --toolchain stable rustc)"
case "${1:-}" in
macOS) target=aarch64-apple-darwin; output=cut-darwin-arm64 ;;
aarch64|x86_64)
    case "$1" in aarch64) target=aarch64-unknown-none-softfloat ;; x86_64) target=x86_64-unknown-none ;; esac
    output=cut-$1.elf
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld" ;;
*) echo "Usage : $0 macOS|aarch64|x86_64"; exit 2 ;;
esac
rustup run stable cargo build --manifest-path "$root/components/cut/Cargo.toml" --release --locked --offline --target "$target"
cp "$CARGO_TARGET_DIR/$target/release/cut" "$here/bin/$output"
file "$here/bin/$output"
