#!/bin/sh
set -eu
cd "$(dirname "$0")"
: "${NOVAOS_TMPDIR:=$(cd ../.. && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/tee-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
case "${1:-macOS}" in
macOS) shift "$(( $# > 0 ))"; exec python3 outils/options.py "$@";;
aarch64|x86_64)
    test "$#" = 3 || { echo "Architecture, secteur et nouveau répertoire de preuves attendus" >&2; exit 2; }
    case "$2" in 512|4096) ;; *) exit 2;; esac
    test ! -e "$3" || { echo "Le répertoire de preuves doit être neuf" >&2; exit 2; }
    TMPDIR=$(mktemp -d "${TMPDIR}qualification-$1.XXXXXX")/
    export TMPDIR
    make -C ../novaOS ARCH="$1" kernel
    ../novaFS/lab/run build --offline --bin novafs
    python3 outils/construire-labo.py "$1"
    python3 outils/invite.py "$@"
    exec python3 outils/elf.py "$@";;
*) echo "Usage : $0 macOS [rapport] | aarch64|x86_64 512|4096 dossier" >&2; exit 2;;
esac
