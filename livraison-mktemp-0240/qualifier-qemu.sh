#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/mktemp-arc027/"
export CARGO_NET_OFFLINE=true PYTHONDONTWRITEBYTECODE=1 NOVA_VCPUS=2 NOVA_RAM=256
export RUSTC="$(rustup which --toolchain stable rustc)"
mkdir -p "$TMPDIR"
out=$(mktemp -d "$TMPDIR/qualification.XXXXXXXX")
echo "$out"
(cd "$root/novaFS" && rustup run stable cargo build --locked --release --offline --bin novafs --target aarch64-apple-darwin)
python3 -B "$ici/lab/elf.py" preparer "$out/elf-base"
python3 -B "$ici/lab/prive.py" preparer "$out/prive-base"
python3 -B "$ici/lab/prive.py" preparer "$out/commande-base" --commande
python3 -B "$ici/lab/prive.py" preparer "$out/duo-base" --duo
for arch in aarch64 x86_64; do
    case $arch in aarch64) target=aarch64-unknown-none-softfloat;; *) target=x86_64-unknown-none;; esac
    CARGO_TARGET_DIR="$TMPDIR/kernel-$arch" make -C "$root/novaOS" ARCH="$arch" kernel
    export NOVA_KERNEL_ELF="$TMPDIR/kernel-$arch/$target/release/novaos"
    for sector in 512 4096; do
        pair="$arch-$sector"
        python3 -B "$ici/lab/invite.py" "$arch" "$sector" "$out/nsh-$pair"
        python3 -B "$ici/lab/elf.py" "$arch" "$sector" "$out/elf-$pair" "$out/elf-base"
        python3 -B "$ici/lab/prive.py" "$arch" "$sector" "$out/prive-$pair" "$out/prive-base"
        python3 -B "$ici/lab/prive.py" "$arch" "$sector" "$out/commande-$pair" "$out/commande-base" --commande
        python3 -B "$ici/lab/prive.py" "$arch" "$sector" "$out/duo-$pair" "$out/duo-base" --duo
    done
done
echo "$out"
