#!/bin/sh
set -eu
base=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
repo=$(dirname "$base")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC="$(rustup which --toolchain stable rustc)"
mkdir -p "$TMPDIR"
case "${1:-}" in
macOS)
    export CARGO_TARGET_DIR="$TMPDIR/arc027-uniq-target"
    rustup run stable cargo build --manifest-path "$repo/components/uniq/Cargo.toml" --release --locked --offline
    cp "$CARGO_TARGET_DIR/release/uniq" "$base/bin/uniq-darwin-arm64"
    ;;
aarch64|x86_64)
    arch=$1
    case "$arch" in aarch64) target=aarch64-unknown-none-softfloat ;; x86_64) target=x86_64-unknown-none ;; esac
    case "${2:-ordinaire}" in
    ordinaire) unset NOVA_DOC_TRACE; profile=ordinaire ;;
    documentation) export NOVA_DOC_TRACE=1; profile=documentation ;;
    *) exit 2 ;;
    esac
    export CARGO_TARGET_DIR="$TMPDIR/uniq-system-$profile-$arch"
    make -C "$repo/novaOS" ARCH="$arch" kernel
    if [ "$profile" = ordinaire ]; then
        set -- "$CARGO_TARGET_DIR/$target/release/build"/novaos-*/out/uniq.elf
        [ "$#" -eq 1 ]
        cp "$1" "$base/bin/uniq-$arch.elf"
        cmp "$1" "$base/bin/uniq-$arch.elf"
    fi
    echo "$CARGO_TARGET_DIR/$target/release/novaos"
    ;;
*) echo 'reconstruire.sh macOS | aarch64|x86_64 [ordinaire|documentation]' ;;
esac
