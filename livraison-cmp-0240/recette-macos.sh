#!/bin/sh
set -eu
case "$#:${1:-}" in
    0:) profil=rapide ;;
    1:--large) profil=complet ;;
    *) echo "Usage : $0 [--large]" >&2; exit 2 ;;
esac
d=$(CDPATH= cd "$(dirname "$0")" && pwd)
archives=${CMP_RESULTATS:-"$d/../../livraisons"}
mkdir -p "$archives"
p=$(mktemp -d "$archives/cmp-$profil.XXXXXX")
printf 'Résultats de la recette %s : %s\n' "$profil" "$p"
"$d/tester-options-cmp.sh" "$@" --json "$p/options.json"
python3 -B "$d/flux-darwin.py" --json "$p/flux.json"
"$d/mesurer-performances-cmp.sh" "$@" --json "$p/performances.json" > "$p/tableau.txt"
cat "$p/tableau.txt"
(cd "$p" && shasum -a 256 options.json flux.json performances.json tableau.txt > SHA256SUMS && shasum -a 256 -c SHA256SUMS)
