#!/bin/sh
# Construit le premier étage UEFI pour le profil QEMU ArmVirt qualifié.
set -eu
[ "$#" -eq 1 ] || { echo "Usage : $0 chemin/BOOTAA64.EFI" >&2; exit 2; }
racine=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cible="$racine/build/relais-bootid"
RUSTC=$(rustup which --toolchain stable rustc)
export RUSTC
rustup run stable cargo build --manifest-path "$racine/boot/relais-bootid/Cargo.toml" --locked \
    --release --target aarch64-unknown-uefi --target-dir "$cible"
cp "$cible/aarch64-unknown-uefi/release/relais-bootid.efi" "$1"
