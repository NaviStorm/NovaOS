#!/bin/sh
# Exécuter depuis n’importe où, avec le checkout GNU v9.12 fourni en argument.
set -eu
src=$(cd "$1" && pwd -P)
bin=$(cd "$(dirname "$2")" && pwd -P)/$(basename "$2")
work=$(mktemp -d "${TMPDIR:?}/sort-amont.XXXXXX")
mkdir "$work/bin"
ln -s "$bin" "$work/bin/sort"
# sort.pl consulte uniquement les deux dépassements suivants sur un hôte 64 bits.
cat > "$work/bin/getlimits" <<'LIMITS'
#!/bin/sh
echo UINTMAX_OFLOW=18446744073709551616
echo SIZE_OFLOW=18446744073709551616
LIMITS
chmod +x "$work/bin/getlimits"
cd "$work"
export PATH="$work/bin:/opt/homebrew/opt/coreutils/libexec/gnubin:$PATH" built_programs=sort LC_ALL=C srcdir="$src"
rc=0
for suite in sort.pl sort-files0-from.pl sort-merge.pl; do
    if perl -w -I"$src/tests" -MCuSkip -MCoreutils "$src/tests/sort/$suite"; then
        echo "$suite : succès"
    else
        echo "$suite : échec"
        rc=1
    fi
done
exit "$rc"
