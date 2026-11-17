#!/bin/sh
set -eu
case "${1:-}" in
 --rapide) isas=aarch64; secteurs=512 ;;
 --complet) isas="aarch64 x86_64"; secteurs="512 4096" ;;
 --help|-h) echo "Usage : rejouer-invites.sh --rapide|--complet [répertoire neuf]"; exit 0 ;;
 *) echo "Utiliser --rapide, --complet ou --help" >&2; exit 2 ;;
esac
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
: "${NOVAOS_TMPDIR:=$(dirname "$root")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC=$(rustup which --toolchain stable rustc)
mkdir -p "$NOVAOS_TMPDIR"
if [ "$#" -eq 2 ]; then mkdir "$2"; sortie=$(cd "$2" && pwd); else sortie=$(mktemp -d "$NOVAOS_TMPDIR/diff-rejeu-XXXXXX"); fi
echo "$sortie"
(cd "$root/novaFS" && rustup run stable cargo build --offline --locked --release --bin novafs --target aarch64-apple-darwin)
for isa in $isas; do
 case "$isa" in aarch64) cible=aarch64-unknown-none-softfloat ;; x86_64) cible=x86_64-unknown-none ;; esac
 (cd "$root/novaOS" && rustup run stable cargo build --offline --locked --release --target "$cible")
 python3 -B "$ici/preparer-recette.py" "$isa" "$sortie/lab-$isa"
 for secteur in $secteurs; do
  python3 -B "$ici/invite.py" "$isa" "$secteur" ordinaire "$sortie/ord-$isa-$secteur"
  python3 -B "$ici/invite.py" "$isa" "$secteur" recette "$sortie/$isa-$secteur" --lab "$sortie/lab-$isa"
 done
done
