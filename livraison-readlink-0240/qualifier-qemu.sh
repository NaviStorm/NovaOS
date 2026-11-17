#!/bin/sh
set -eu
base=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR=$(dirname "$(dirname "$base")")/tmp
export TMPDIR="$NOVAOS_TMPDIR/readlink-arc027/" PYTHONDONTWRITEBYTECODE=1
export RUSTC="$(rustup which --toolchain stable rustc)"
mkdir -p "$TMPDIR"
out=$(mktemp -d "$TMPDIR/recette.XXXXXXXX")
echo "$out"
for arch in aarch64 x86_64; do
    python3 -B "$base/lab/invite.py" "$arch" "$out/$arch"
    for secteur in 512 4096; do
        python3 -B "$base/lab/sonde.py" "$arch" "$secteur" "$out/$arch/$secteur/moteur"
    done
done
