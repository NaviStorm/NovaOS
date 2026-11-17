#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
up=${1:?checkout GNU épinglé}
out=${2:?répertoire des journaux}
test "$(git -C "$up" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
: "${NOVAOS_TMPDIR:=$(cd "$root/../.." && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/"
mkdir -p "$TMPDIR" "$out"
dir=$(mktemp -d "${TMPDIR}gnu.XXXXXXXX")
mkdir -p "$dir/source/tests"
if git -C "$up/gnulib" cat-file -e 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh 2>/dev/null; then
    git -C "$up/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$dir/source/tests/init.sh"
else
    curl -fsSL https://raw.githubusercontent.com/coreutils/gnulib/106e9b2384d08a1696fcbd40cbab52237943f208/tests/init.sh -o "$dir/source/tests/init.sh"
fi
cp "$up/init.cfg" "$dir/source/init.cfg"
export AWK=awk LC_ALL=C built_programs=realpath srcdir="$dir/source" abs_top_srcdir="$up" host_os=darwin
for exe in "$root/bin/realpath" /opt/homebrew/bin/grealpath; do
    label=nova
    if [ "$exe" = /opt/homebrew/bin/grealpath ]; then label=gnu; fi
    work="$dir/$label"
    mkdir -p "$work/src"
    cp "$exe" "$work/src/realpath"
    for script in "$up"/tests/misc/realpath.sh; do
        suite=$(basename "$script" .sh)
        rc=0
        (cd "$work" && PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH" abs_top_builddir="$work" sh "$script" 9>&2) >"$out/$label-$suite.log" 2>&1 || rc=$?
        printf '%s %s %s\n' "$label" "$suite" "$rc"
        [ "$rc" = 0 ] || exit "$rc"
    done
done
