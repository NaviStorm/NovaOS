#!/bin/sh
# Rejoue le script distribué par GNU sans en intégrer les sources au produit.
set -eu
here=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
ref=${GNU_COREUTILS_SOURCE:-"$(dirname "$(dirname "$here")")/tmp/sort-arc027-gnu"}
test "$(git -C "$ref" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
gnu=$(command -v gsleep)
out=${1:-"$TMPDIR/sleep-amont"}
mkdir -p "$out"
out=$(cd "$out" && pwd)
for tool in nova gnu; do
 work=$(mktemp -d "$TMPDIR/sleep-amont.XXXXXXXX")
 mkdir -p "$work/tests" "$work/src"
 git -C "$ref/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$work/tests/init.sh"
 cp "$ref/init.cfg" "$work/init.cfg"
 case "$tool" in nova) exe=$here/bin/sleep;; gnu) exe=$gnu;; esac
 ln -s "$exe" "$work/src/sleep"
 cat > "$work/limits.c" <<'EOF'
#include <float.h>
#include <limits.h>
#include <stdio.h>
int main(void) { printf("INT_MAX=%d\nLDBL_MAX=%La\n", INT_MAX, LDBL_MAX); }
EOF
 cc "$work/limits.c" -o "$work/src/getlimits"
 (
  cd "$work"
  export srcdir="$work" abs_top_srcdir="$ref" abs_top_builddir="$work" built_programs='sleep printf timeout'
  export LC_ALL=C LOCALE_FR_UTF8=fr_FR.UTF-8 VERBOSE=yes SHELL=/bin/bash
  export PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH"
  exec 9>&2
  sh -x "$ref/tests/misc/sleep.sh"
 ) > "$out/amont-$tool.log" 2>&1
done
