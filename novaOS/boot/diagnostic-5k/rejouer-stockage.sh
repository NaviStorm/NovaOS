#!/usr/bin/env bash
# Depuis la racine du worktree. L'oracle historique prend le verrou QEMU pour chaque VM.
set -eu
: "${NOVAOS_TMPDIR:?Définir le répertoire temporaire partagé NovaOS}"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
export PYTHONDONTWRITEBYTECODE=1
sortie=${1:?Indiquer un nouveau répertoire de sortie}
mkdir "$sortie"
for cas in duo-retrait duo-differe duo-sans-reset duo-wait-absent duo-wait-tardif; do
    opts=()
    if [[ "$cas" == duo-differe ]]; then opts+=(--instantane); fi
    python3 -B novaOS/tests/ethernet_v1/port_q35_stockage.py "$cas" \
        "../archives-ethernet-phase3d/images-duo-58e3872/$cas.elf" "$sortie/$cas" \
        ../pilote-virtio-net ../archives-virtio-net \
        ../archives-ethernet-phase3/reproduction-8bd3f9c/limine-enroll \
        ../pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs "${opts[@]}"
done
