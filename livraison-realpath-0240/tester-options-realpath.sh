#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/" PYTHONDONTWRITEBYTECODE=1
cible=macos
case "${1:-}" in --novaos|--macos) cible=${1#--};shift;; esac
mode=${1:---rapide}
case "$mode" in --help|-h) echo "tester-options-realpath.sh [--macos|--novaos] [--rapide|--complet] [rapport.json]";exit;; --rapide|--complet) profil=${mode#--};; *) exit 2;; esac
mkdir -p "$TMPDIR"
if [ "$cible" = novaos ]; then exec "$ici/qualifier-qemu.sh" "$profil" tout; fi
if [ "$profil" = complet ]; then python3 "$ici/lab/flux.py"; fi
exec python3 "$ici/lab/options.py" "${profil:-rapide}" "${2:-$TMPDIR/options.json}"
