#!/bin/sh
set -eu
base=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$base")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
gnu=${GNU_COREUTILS_SRC:-$NOVAOS_TMPDIR/sort-arc027-gnu}
case "${1:-}" in
--macOS) exe=/opt/homebrew/bin/gcut ;;
--novaOS) exe=$base/bin/cut-darwin-arm64 ;;
*) echo "Usage : $0 --macOS|--novaOS (GNU_COREUTILS_SRC désigne le checkout v9.12)"; exit 2 ;;
esac
[ "$(git -C "$gnu" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8 ]
dir=$(mktemp -d "$TMPDIR/cut-gnu.XXXXXXXX")
cd "$dir"
ln -s "$exe" cut
cat > getlimits <<'SH'
#!/bin/sh
echo IO_BUFSIZE=262144
echo UINTMAX_MAX=18446744073709551615
echo UINTMAX_OFLOW=18446744073709551616
SH
chmod +x getlimits
export PATH="$dir:$PATH" built_programs=cut LOCALE_FR_UTF8=fr_FR.UTF-8 VERBOSE=yes
perl -I"$gnu/tests" -MCuSkip -MCoreutils "$gnu/tests/cut/cut.pl"
