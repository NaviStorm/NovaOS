#!/bin/sh
set -eu
cd "$(dirname "$0")"
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd ../.. && pwd)/tmp/seq-arc027}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
case "${1:-}" in
--help|-h)
    cat <<'EOF'
tester-options-seq.sh [--out DOSSIER]
    Compare le Mach-O à GNU 9.12. Conserve les flux, les arguments et le statut.
tester-options-seq.sh --amont [DOSSIER]
    Exécute la suite Perl et les six scripts GNU depuis la copie de référence.
tester-options-seq.sh --qemu aarch64|x86_64 [--out DOSSIER] [--vecteurs OPTIONS.jsonl.gz]
    Qualifie les ELF livrés puis nsh sur P9/512 et P9/4096.
Les sorties de rejeu vont par défaut dans le TMPDIR privé du chantier.
EOF
    ;;
--amont) shift; exec lab/amont.sh "$@" ;;
--qemu)
    shift
    arch=${1:?architecture requise}
    shift
    case "$arch" in aarch64|x86_64) ;; *) exit 2 ;; esac
    NOVAFS_TARGET=aarch64-apple-darwin ../novaFS/lab/run build --offline --bin novafs
    make -C ../novaOS ARCH="$arch" kernel
    python3 -B lab/invite.py "$arch" sdk "$@"
    exec python3 -B lab/invite.py "$arch" nsh "$@"
    ;;
*) exec python3 -B lab/options.py "$@" ;;
esac
