#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(cd "$ici/.." && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/" CARGO_TARGET_DIR="$NOVAOS_TMPDIR/realpath-arc027/cargo"
mkdir -p "$TMPDIR" "$ici/bin"
export RUSTC="$(rustup which --toolchain stable rustc)"
cd "$root/components/realpath"
test "$(cat VERSION)" = "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)"
build() {
    rustup run stable cargo build --bin realpath --release --locked --offline --target "$1"
    cp "$CARGO_TARGET_DIR/$1/release/realpath" "$ici/bin/$2"
    file "$ici/bin/$2"
}
case "${1:-tout}" in
    mac|elf|tout) ;;
    *) exit 2 ;;
esac
if [ "${1:-tout}" != elf ]; then
    unset RUSTFLAGS
    build aarch64-apple-darwin realpath
fi
if [ "${1:-tout}" != mac ]; then
    export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T$root/novaOS/user/novafs/service.ld"
    build aarch64-unknown-none-softfloat realpath-aarch64.elf
    build x86_64-unknown-none realpath-x86_64.elf
fi
