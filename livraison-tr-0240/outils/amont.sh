#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
up=${1:?chemin du checkout GNU Coreutils v9.12}
out=${2:?répertoire de preuves}
test "$(git -C "$up" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
: "${NOVAOS_TMPDIR:=$(cd "$root/.." && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
dir=$(mktemp -d "$NOVAOS_TMPDIR/tr-amont.XXXXXXXX")
mkdir -p "$dir/source/tests" "$dir/src"
ln -s "$root/livraison-tr-0240/bin/tr" "$dir/src/tr"
git -C "$up/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$dir/source/tests/init.sh"
cp "$up/init.cfg" "$dir/source/init.cfg"
cd "$dir"
export PATH="$dir/src:$PATH" LC_ALL=C built_programs=tr
srcdir="$up" VERBOSE=yes perl -w -I"$up/tests" -MCuSkip -MCoreutils "$up/tests/tr/tr.pl" > "$out/tr-pl.log" 2>&1
srcdir="$dir/source" abs_top_builddir="$dir" abs_top_srcdir="$up" sh -x "$up/tests/tr/tr-case-class.sh" > "$out/tr-case-class.log" 2>&1 9>&2
