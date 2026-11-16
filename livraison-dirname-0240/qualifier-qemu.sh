#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$root")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dirname-arc027/"
export RUSTC="$(rustup which --toolchain stable rustc)" PYTHONDONTWRITEBYTECODE=1
export CARGO_NET_OFFLINE=true NOVA_RAM=256 NOVA_VCPUS=2
mkdir -p "$TMPDIR"
out=$(mktemp -d "$TMPDIR/recette.XXXXXXXX")
printf '%s\n' "$out"
"$ici/tester-options-dirname.sh" "$out/options.json"
(cd "$root/novaFS" && rustup run stable cargo build --offline --release --locked --bin novafs --target aarch64-apple-darwin)
python3 "$ici/lab/elf.py" preparer "$out/base-elf"
for arch in aarch64 x86_64; do
    case "$arch" in aarch64) target=aarch64-unknown-none-softfloat;; *) target=x86_64-unknown-none;; esac
    export CARGO_TARGET_DIR="$TMPDIR/kernel-$arch"
    make -C "$root/novaOS" ARCH="$arch" kernel
    export NOVA_KERNEL_ELF="$CARGO_TARGET_DIR/$target/release/novaos"
    for sector in 512 4096; do
        python3 "$ici/lab/invite.py" "$arch" "$sector" "$out/nsh-$arch-$sector"
        python3 "$ici/lab/sonde.py" "$arch" "$sector" "$out/sonde-$arch-$sector" "$out/options.json"
        python3 "$ici/lab/elf.py" "$arch" "$sector" "$out/elf-$arch-$sector" "$out/base-elf"
    done
done
printf 'Douze démarrages vérifiés dans %s\n' "$out"
