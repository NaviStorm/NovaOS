#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
ref=${GNU_UNAME_SOURCE:-$NOVAOS_TMPDIR/sort-arc027-gnu}
test "$(git -C "$ref" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
work=$(mktemp -d "$TMPDIR/amont.XXXXXXXX")
mkdir -p "$work/tests" "$work/src"
ln -s "$ici/bin/uname" "$work/src/uname"
gnulib=$(git -C "$ref" rev-parse HEAD:gnulib)
git -C "$ref/gnulib" show "$gnulib:tests/init.sh" > "$work/tests/init.sh"
cp "$ref/init.cfg" "$work/"
export UNAME_NODENAME=noeud-recette
export srcdir="$work" abs_top_srcdir="$ref" abs_top_builddir="$work" built_programs=uname LC_ALL=C SHELL=/bin/bash
export PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH"
cd "$work"
exec sh "$ref/tests/misc/uname-labeled.sh"
