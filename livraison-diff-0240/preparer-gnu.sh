#!/bin/sh
set -eu
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
: "${NOVAOS_TMPDIR:=$(dirname "$(dirname "$ici")")/tmp}"
mkdir -p "$NOVAOS_TMPDIR"
dest="$NOVAOS_TMPDIR/diff-gnu-3.12"
if [ -x "$dest/bin/diff" ]; then "$dest/bin/diff" --version; exit 0; fi
d=$(mktemp -d "$NOVAOS_TMPDIR/diff-gnu-build-XXXXXX")
cd "$d"
if [ -f "$NOVAOS_TMPDIR/cmp-0240/diffutils-3.12.tar.xz" ]; then
 cp "$NOVAOS_TMPDIR/cmp-0240/diffutils-3.12.tar.xz" .
else
 curl --fail --location --output diffutils-3.12.tar.xz https://ftp.gnu.org/gnu/diffutils/diffutils-3.12.tar.xz
fi
echo '7c8b7f9fc8609141fdea9cece85249d308624391ff61dedaf528fcb337727dfd  diffutils-3.12.tar.xz' | shasum -a 256 -c -
tar xf diffutils-3.12.tar.xz
cd diffutils-3.12
./configure --prefix="$dest"
make -j2
make install
printf '%s\n' "$PWD" > "$dest/source-dir"
"$dest/bin/diff" --version
