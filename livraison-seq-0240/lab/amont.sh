#!/bin/sh
set -eu
delivery=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
root=$(CDPATH= cd "$delivery/../.." && pwd)
ref=${GNU_SEQ_SOURCE:-$root/tmp/sort-arc027-gnu}
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$root/tmp/seq-arc027}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
out=${1:-"$TMPDIR/rejeu"}
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d "$TMPDIR/amont.XXXXXX")
mkdir -p "$work/tests" "$work/src"
ln -s "$delivery/bin/seq" "$work/src/seq"
# Limites du compilateur hôte réellement employées par les scripts seq.
cat > "$work/limites.c" <<'EOF'
#include <limits.h>
#include <inttypes.h>
#include <stdio.h>
int main(void) {
    printf("INT_MAX=%d\nLONG_MIN=%ld\nINTMAX_MAX=%" PRIdMAX "\nINTMAX_OFLOW=%" PRIuMAX "\n",
           INT_MAX, LONG_MIN, INTMAX_MAX, (uintmax_t)INTMAX_MAX+1);
    return 0;
}
EOF
cc "$work/limites.c" -o "$work/src/getlimits"
gnulib=$(git -C "$ref" rev-parse HEAD:gnulib)
git -C "$ref/gnulib" show "$gnulib:tests/init.sh" > "$work/tests/init.sh"
cp "$ref/init.cfg" "$work/init.cfg"
export srcdir="$work" abs_top_srcdir="$ref" abs_top_builddir="$work" CC=cc
export built_programs=seq LC_ALL=C LOCALE_FR_UTF8=fr_FR.UTF-8 VERBOSE=yes SHELL=/bin/bash
export PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH"
cd "$work"
exec 9>&2
perl -I"$ref/tests" -MCuSkip -MCoreutils "$ref/tests/seq/seq.pl" > "$out/seq-pl.txt" 2>&1
rc=0
for test in "$ref"/tests/seq/*.sh; do
    name=$(basename "$test")
    code=0
    sh "$test" > "$out/$name.txt" 2>&1 || code=$?
    printf '%s\t%s\n' "$name" "$code"
    case $code in 0|77) ;; *) rc=1 ;; esac
done > "$out/amont.tsv"
cat "$out/amont.tsv"
exit "$rc"
