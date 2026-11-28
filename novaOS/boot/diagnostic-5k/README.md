# Reproduire la qualification 5K

Les deux bancs utilisent des VM x86_64 Q35 à un vCPU sous TCG. Le premier exerce le retrait d’un demandeur EDU et l’exception APIC du firmware 5J. Le second réutilise les ELF NovaOS et le lecteur NovaFS P9 du banc NIC/NVMe historique. Les deux prennent le verrou partagé des worktrees. [Rapport et mesures](../../docs/diagnostic-uefi-5k.md)

## Vérification du lot publié

Depuis la racine du worktree sur le Mac de laboratoire, avec le répertoire temporaire commun déjà créé, exécuter les commandes suivantes. La contre-lecture de base utilise la bibliothèque standard Python 3.9 ou ultérieure. Le dernier contrôle facultatif rouvre les cinq systèmes de fichiers avec le lecteur NovaFS natif épinglé.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5k
v=novaOS/docs/validation/diagnostic-uefi-5k
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "$d" -v
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/verifier.py"
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s novaOS/boot/diagnostic-5j -q
  /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5i/test_domaines.py -q
done
make -C novaOS abi-check
python3 -B novaOS/boot/diagnostic-5j/verifier.py
python3 -B "$d/verifier.py" --lecteur-p9 ../pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs
```

## Nouvelles captures

Avec les dépendances de qualification conservées dans les worktrees frères, choisir des destinations neuves. Les horodatages, les époques, les adresses de l’observateur et le nombre d’interruptions avant l’arrêt varient entre exécutions. Les contrôles portent sur leurs relations et sur l’origine de chaque accès.

```sh
mkdir -p novaOS/build
python3 -B "$d/banc.py" ../diagnostic-uefi-phase5j/novaOS/build novaOS/build/rejeu-edu-5k
bash "$d/rejouer-stockage.sh" novaOS/build/rejeu-duo-5k
python3 -B "$d/archiver.py" novaOS/build/rejeu-edu-5k novaOS/build/rejeu-duo-5k novaOS/build/preuves-rejeu-5k
```

Pour reconstituer les supports dans un répertoire neuf, extraire les deux archives publiques ci-dessous. Le modèle QEMU **11.1.3 avec le correctif PIO 5J**, son exécutable Darwin qualifié et les dépendances de compilation sont décrits dans la recette 5J. Le banc vérifie le SHA du binaire avant exécution. Un binaire reconstruit de SHA différent exige une qualification de provenance distincte. Les cinq ELF historiques sont vérifiés contre le manifeste de leur campagne et le lecteur P9 contre son empreinte. [Recette du modèle et du firmware](../diagnostic-5j/README.md) · [Reproduction NIC/NVMe](../../tests/ethernet_v1/rejouer_phase3d.py)

```sh
j=novaOS/build/supports-rejeu-5k
mkdir "$j"
v5j=novaOS/docs/validation/diagnostic-uefi-5j
(cd "$v5j" && shasum -a 256 -c SHA256SUMS)
tar -xzf "$v5j/laboratoire.tar.gz" -C "$j"
tar -xzf "$v/modules.tar.gz" -C "$j"
mkdir -p "$j/qualification5j" "$j/modele5j/build"
for f in diagnostic.img nominal.fd inactif.fd; do cp "$j/supports5j-b/$f" "$j/qualification5j/$f"; done
# Le lecteur PIO 5J attend la copie FAT complétée de zéros à 34 Mio.
python3 -B -c 'import os, sys; os.truncate(sys.argv[1], 34 << 20)' "$j/qualification5j/diagnostic.img"
cp ../diagnostic-uefi-phase5j/novaOS/build/modele5j/build/qemu-system-x86_64 "$j/modele5j/build/"
python3 -B "$d/banc.py" "$j" novaOS/build/rejeu-supports-5k
```

L’archive d’observations contient les tables avant/après, la file QI, le mot WAIT, les registres, les compteurs QMP, les reçus EFI, les traces CPU/périphériques, les témoins GDB, les cinq images NVMe fictives et leurs vérifications P9. La RAM générale utilisée temporairement pour chercher le reçu EFI reste locale au banc. Les métadonnées locales de lancement, les chemins du Mac et le répertoire de signature sont exclus du lot.
