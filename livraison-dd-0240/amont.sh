#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")" && pwd)
up=${1:?checkout de coreutils 9.12}
out=${2:?répertoire neuf des journaux}
test "$(git -C "$up" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd "$root/../.." && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dd-arc027/" LC_ALL=C AWK=awk built_programs="dd printf" host_os=darwin
mkdir -p "$TMPDIR"
mkdir "$out"
dir=$(mktemp -d "${TMPDIR}gnu.XXXXXXXX")
export srcdir="$dir/source" abs_top_srcdir="$up"
mkdir -p "$srcdir/tests"
git -C "$up/gnulib" show 106e9b2384d08a1696fcbd40cbab52237943f208:tests/init.sh > "$srcdir/tests/init.sh"
cp "$up/init.cfg" "$srcdir/init.cfg"
test "$(df -Pk "$dir" | awk 'END {print $4}')" -gt 65536
failed=0
for tool in nova gnu; do
 work="$dir/$tool"
 mkdir -p "$work/src"
 case $tool in nova) exe="$root/bin/dd";; gnu) exe=/opt/homebrew/bin/gdd;; esac
 ln -s "$exe" "$work/src/dd"
 for test in ascii bytes conv-case direct nocache partial-write reblock sparse not-rewound skip-seek2 unblock-sync; do
   rc=0
   (cd "$work" && PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH" abs_top_builddir="$work" sh "$up/tests/dd/$test.sh" 9>&2) >"$out/$tool-$test.log" 2>&1 || rc=$?
   echo "$tool $test $rc" | tee -a "$out/resultats.txt"
   case "$rc" in 0|77) ;; *) failed=1;; esac
 done
 for suite in skip-seek unblock; do
   rc=0
   (cd "$work" && PATH="$work/src:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH" perl -w -I"$up/tests" -MCuSkip -MCoreutils "$up/tests/dd/$suite.pl") >"$out/$tool-$suite-perl.log" 2>&1 || rc=$?
   echo "$tool $suite.pl $rc" | tee -a "$out/resultats.txt"
   case "$rc" in 0|77) ;; *) failed=1;; esac
 done
done
exit "$failed"
