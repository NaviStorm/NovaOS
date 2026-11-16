#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dirname-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
case "${1:-}" in
    --amont)
        gnu="${GNU_SOURCE:-$NOVAOS_TMPDIR/sort-arc027-gnu}"
        test "$(git -C "$gnu" rev-parse HEAD)" = c0f8514d989184921d9b12a4d103a7b23abc5af8
        essai=$(mktemp -d "$TMPDIR/perl.XXXXXXXX")
        cd "$essai"
        export srcdir="$gnu" built_programs=dirname PATH="$ici/bin:$PATH" VERBOSE=1
        exec perl -I"$gnu/tests" -MCuSkip -MCoreutils "$gnu/tests/misc/dirname.pl"
        ;;
    --qemu) exec "$ici/qualifier-qemu.sh" ;;
    *) exec python3 "$ici/lab/options.py" "${1:-$TMPDIR/options.json}" ;;
esac
