#!/usr/bin/env bash
# Depuis la racine de la branche de qualification, vers un dossier neuf.
set -eu
: "${NOVAOS_TMPDIR:?Répertoire temporaire NovaOS requis}"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
sortie=${1:?Répertoire de sortie neuf requis}
mkdir "$sortie"
builder=novaOS/tests/ethernet_v1/port_construire_q35.py
runner=novaOS/tests/ethernet_v1/port_q35_stockage.py
base=58e387267ea900f18e4c0620a4b737721ac3eb39
deps=(../pilote-virtio-net ../archives-virtio-net ../archives-ethernet-phase3/reproduction-8bd3f9c/limine-enroll
      ../pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs)
python3 -B "$builder" "$sortie/elf-co" --revision "$base" --phase3d --sonder-5l --cas duo-wait-absent duo-wait-tardif
python3 -B "$builder" "$sortie/elf-actif" --revision "$base" --phase3d --chrono-5l --cas duo-retrait
python3 -B "$builder" "$sortie/elf-iqe" --revision "$base" --phase3d --iqe-5l --cas duo-retrait
for cas in duo-wait-absent duo-wait-tardif; do
    python3 -B "$runner" "$cas" "$sortie/elf-co/$cas.elf" "$sortie/$cas" "${deps[@]}"
done
python3 -B "$runner" duo-retrait "$sortie/elf-actif/duo-retrait.elf" "$sortie/actif-wait" "${deps[@]}" --panne-stockage-actif wait
python3 -B "$runner" duo-retrait "$sortie/elf-iqe/duo-retrait.elf" "$sortie/actif-iqe" "${deps[@]}" --panne-stockage-actif iqe
python3 -B "$runner" duo-retrait "$sortie/elf-actif/duo-retrait.elf" "$sortie/temoin" "${deps[@]}"
