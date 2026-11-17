#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/"
export RUSTC="$(rustup which --toolchain stable rustc)" PYTHONDONTWRITEBYTECODE=1
export CARGO_NET_OFFLINE=true NOVA_RAM=256 NOVA_VCPUS=2
mkdir -p "$TMPDIR"
case "${1:-complet}" in rapide) arches=aarch64;secteurs=512;; complet) arches="aarch64 x86_64";secteurs="512 4096";; *) exit 2;; esac
scope=${2:-tout}
out=$(mktemp -d "$TMPDIR/recette.XXXXXXXX")
printf '%s\n' "$out"

(cd "$root/novaFS" && rustup run stable cargo build --offline --release --locked --bin novafs --target aarch64-apple-darwin)
if [ "$scope" = tout ]; then "$ici/reconstruire.sh" elf; python3 "$ici/lab/elf.py" preparer "$out/base-elf"; fi
for arch in $arches; do
    case "$arch" in aarch64) target=aarch64-unknown-none-softfloat;; *) target=x86_64-unknown-none;; esac
    export CARGO_TARGET_DIR="$TMPDIR/kernel-$arch"
    make -C "$root/novaOS" ARCH="$arch" kernel
    export NOVA_KERNEL_ELF="$CARGO_TARGET_DIR/$target/release/novaos"
    for sector in $secteurs; do
        if [ "$scope" = tout ]; then python3 "$ici/lab/invite.py" "$arch" "$sector" "$out/nsh-$arch-$sector"; fi
        python3 "$ici/lab/sonde.py" "$arch" "$sector" "$out/sonde-$arch-$sector"
        if [ "$scope" = tout ]; then python3 "$ici/lab/elf.py" "$arch" "$sector" "$out/elf-$arch-$sector" "$out/base-elf"; fi
    done
done
printf 'Résultats QEMU dans %s\n' "$out"
