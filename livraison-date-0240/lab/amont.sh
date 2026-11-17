#!/bin/sh
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=${1:-$NOVAOS_TMPDIR/sort-arc027-gnu}
out=${2:-$NOVAOS_TMPDIR/date-arc027/amont}
test -e "$src/src/date.c"
mkdir -p "$out"
out=$(cd "$out" && pwd)
test "$(git -C "$src" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
work=$(mktemp -d "$TMPDIR/date-amont.XXXXXXXX")
trap 'cd / && rm -rf -- "$work"' EXIT
mkdir -p "$work/tests" "$work/src"
git -C "$src/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$work/tests/init.sh"
cp "$src/init.cfg" "$work/"
ln -s "$here/bin/date" "$work/src/date"
cat > "$work/limits.c" <<'EOF'
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>
int main(void) {
    printf("INT_MAX=%d\nINTMAX_MAX=%lld\nTIME_T_MAX=%lld\n", INT_MAX, (long long)INTMAX_MAX,
           (long long)((UINT64_C(1) << (sizeof(time_t)*CHAR_BIT-1))-1));
}
EOF
cc "$work/limits.c" -o "$work/src/getlimits"
export PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH"
export srcdir="$work" abs_top_srcdir="$src" abs_top_builddir="$work" built_programs="date printf"
export SHELL=/bin/bash LC_ALL=C LOCALE_FR_UTF8=fr_FR.UTF-8 VERBOSE=no
cd "$work"
exec 9>&2
failed=0
for test in "$src"/tests/date/*.pl "$src"/tests/date/*.sh; do
    name=$(basename "$test"); code=0
    case "$test" in
    *.pl) perl -I"$src/tests" -MCuSkip -MCoreutils "$test" > "$out/$name.log" 2>&1 || code=$? ;;
    *) sh "$test" > "$out/$name.log" 2>&1 || code=$? ;;
    esac
    echo "$name $code"
    case $code in 0|77) ;; *) failed=1 ;; esac
done > "$out/statuts.txt"
cat "$out/statuts.txt"
exit "$failed"
