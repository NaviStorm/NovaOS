#!/bin/sh
set -eu
root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
up=${1:?checkout GNU épinglé}
out=${2:?répertoire des journaux}
test "$(git -C "$up" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
: "${NOVAOS_TMPDIR:=$(cd "$root/../.." && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/tee-arc027/"
mkdir -p "$TMPDIR" "$out"
dir=$(mktemp -d "${TMPDIR}gnu.XXXXXXXX")
mkdir -p "$dir/source/tests"
git -C "$up/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$dir/source/tests/init.sh"
cp "$up/init.cfg" "$dir/source/init.cfg"
export AWK=awk LC_ALL=C built_programs=tee srcdir="$dir/source" abs_top_srcdir="$up" host_os=darwin
for exe in "$root/bin/tee" /opt/homebrew/bin/gtee; do
    label=nova
    if [ "$exe" = /opt/homebrew/bin/gtee ]; then label=gnu; fi
    work="$dir/$label"
    mkdir -p "$work/src"
    ln -s "$exe" "$work/src/tee"
    for suite in append tee short-write write-eagain; do
        rc=0
        (cd "$work" && PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH" abs_top_builddir="$work" sh "$up/tests/tee/$suite.sh" 9>&2) >"$out/$label-$suite.log" 2>&1 || rc=$?
        printf '%s %s %s\n' "$label" "$suite" "$rc"
        case "$rc" in 0|77) ;; *) exit "$rc";; esac
    done
done
