#!/bin/sh
set -eu
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$ici")
if [ "$#" -ne 1 ]; then echo "Usage : $0 dossier-neuf" >&2; exit 2; fi
: "${NOVAOS_TMPDIR:=$(dirname "$root")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
mkdir "$1"
dest=$(cd "$1" && pwd)
src=$(mktemp -d "$TMPDIR/diff-perf-src-XXXXXX")
git -C "$root" archive --output="$src/export.tar" HEAD components novaOS/user novaOS/abi livraison-diff-0240/reconstruire.sh
tar xf "$src/export.tar" -C "$src"
"$src/livraison-diff-0240/reconstruire.sh"
cp "$src"/livraison-diff-0240/bin/* "$dest/"
