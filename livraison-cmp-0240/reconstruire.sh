#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(cd "$ici/.." && pwd)
: "${NOVAOS_TMPDIR:=$(cd "$root/.." && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR" "$ici/bin"
cd "$root/components/cmp"
manifest=Cargo.toml
export RUSTC=$(rustup which --toolchain stable rustc)
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/cmp-cargo"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$manifest")
test "$version" = "$(cat "$root/components/cmp/VERSION")"
for target in aarch64-apple-darwin aarch64-unknown-none-softfloat x86_64-unknown-none; do
    case "$target" in
        *darwin) unset RUSTFLAGS; nom=cmp ;;
        *) export RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=--strip-all -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T../../novaOS/user/novafs/service.ld"
           nom="cmp-${target%%-*}.elf" ;;
    esac
    rustup run stable cargo build --offline --locked --release --manifest-path "$manifest" --target "$target"
    cp "$CARGO_TARGET_DIR/$target/release/cmp" "$ici/bin/$nom"
done
file "$ici"/bin/cmp*
