#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
usage() {
    echo "Usage : $0 --rapide|--complet [dossier-résultats]"
    echo "        $0 --rapide|--complet --novaos aarch64|x86_64 dossier-neuf"
}
case ${1:-} in
    --help) usage; exit 0 ;;
    --rapide|--complet) mode=${1#--}; shift ;;
    *) usage >&2; exit 2 ;;
esac
if [ "${1:-}" = --novaos ]; then
    [ "$#" -eq 3 ] || { usage >&2; exit 2; }
    exec "$here/tester-invite-env.sh" "$2" "$3" --mode "$mode"
fi
[ "$#" -le 1 ] || { usage >&2; exit 2; }
export TMPDIR="$(dirname "$(dirname "$here")")/tmp/" PYTHONDONTWRITEBYTECODE=1
exec python3 -B "$here/lab/mesures.py" "$mode" "${1:-$TMPDIR/env-arc027/resultats-$mode}"
