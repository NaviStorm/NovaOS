#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/" CARGO_TARGET_DIR="$NOVAOS_TMPDIR/stat-arc027/cargo"
mkdir -p "$CARGO_TARGET_DIR"
export RUSTC="$(rustup which --toolchain stable rustc)"
for arch in "${@}"; do
case "$arch" in
macOS) target=aarch64-apple-darwin; dest=stat-darwin-arm64; unset RUSTFLAGS ;;
aarch64|x86_64) case $arch in aarch64) target=aarch64-unknown-none-softfloat;; *) target=x86_64-unknown-none;; esac
dest=stat-$arch.elf
export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld";;
*) exit 2;;
esac
rustup run stable cargo build --manifest-path "$root/components/stat/Cargo.toml" --offline --locked --release --target "$target"
cp "$CARGO_TARGET_DIR/$target/release/stat" "$here/bin/$dest"
done
