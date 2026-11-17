#!/bin/sh
# GNU_SOURCE pointe sur le tag v9.12. GNU_INIT peut désigner le init.sh généré.
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
: "${GNU_SOURCE:?fournir les sources de GNU Coreutils 9.12}"
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$here")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
work=$(mktemp -d "$TMPDIR/stat-amont.XXXXXXXX")
mkdir -p "$work/source/tests"
if [ -n "${GNU_INIT:-}" ] || [ -f "$GNU_SOURCE/tests/init.sh" ]; then
  cp "${GNU_INIT:-$GNU_SOURCE/tests/init.sh}" "$work/source/tests/init.sh"
else
  revision=$(git -C "$GNU_SOURCE" rev-parse HEAD:gnulib)
  git -C "$GNU_SOURCE/gnulib" show "$revision:tests/init.sh" > "$work/source/tests/init.sh"
fi
cp "$GNU_SOURCE/init.cfg" "$work/source/init.cfg"
export srcdir="$work/source" LC_ALL=C TZ=UTC built_programs=stat AWK=awk
unset QUOTING_STYLE
exec 9>&2
status=0
for provider in GNU Nova; do
  cd "$work"
  mkdir -p "$provider/src"
  case $provider in GNU) binary=/opt/homebrew/opt/coreutils/libexec/gnubin/stat;; Nova) binary=$here/bin/stat-darwin-arm64;; esac
  ln -s "$binary" "$provider/src/stat"
  cd "$provider"
  export PATH="$PWD/src:/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:/usr/bin:/bin"
  for test in "$GNU_SOURCE"/tests/stat/*; do
    rc=0
    case $test in *.pl) perl -I"$GNU_SOURCE/tests" -MCuSkip -MCoreutils "$test" || rc=$?;; *) sh "$test" || rc=$?;; esac
    echo "$provider $(basename "$test") $rc"
    [ "$rc" = 0 ] || status=1
  done
done
exit "$status"
