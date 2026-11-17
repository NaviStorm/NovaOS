#!/bin/sh
set -eu
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
: "${NOVAOS_TMPDIR:=$(dirname "$root")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC=$(rustup which --toolchain stable rustc)
: "${1:?Indiquer un dossier de rejeu neuf.}"
mkdir "$1"
dest=$(cd "$1" && pwd)
(cd "$root/novaFS" && rustup run stable cargo build --offline --locked --release --bin novafs --target aarch64-apple-darwin)
for isa in aarch64 x86_64; do
 case $isa in aarch64) target=aarch64-unknown-none-softfloat ;; *) target=x86_64-unknown-none ;; esac
 (cd "$root/novaOS" && rustup run stable cargo build --offline --locked --release --target "$target")
 python3 -B "$root/livraison-diff-0240/preparer-recette.py" "$isa" "$dest/lab-$isa"
 for secteur in 512 4096; do
  python3 -B "$ici/requalifier.py" invite "$isa" "$secteur" ordinaire "$dest/ord-$isa-$secteur"
  python3 -B "$ici/requalifier.py" invite "$isa" "$secteur" recette "$dest/$isa-$secteur" --lab "$dest/lab-$isa"
 done
done
python3 -B "$ici/requalifier.py" synthese "$dest"
