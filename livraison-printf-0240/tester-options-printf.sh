#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = --toutes-cibles ]; then
    shift
    if [ "$#" != 2 ] || [ "$1" != --sortie ]; then
        echo "Usage : $0 --toutes-cibles --sortie DOSSIER_NEUF" >&2
        exit 2
    fi
    mkdir "$2"
    dest=$(cd "$2" && pwd)
    python3 -B "$ici/options.py" --sortie "$dest/macos"
    python3 -B "$ici/amont.py" "$dest/amont"
    for arch in aarch64 x86_64; do
        python3 -B "$ici/invites.py" "$arch" "$dest/macos" "$dest/$arch"
    done
else
    exec python3 -B "$ici/options.py" "$@"
fi
