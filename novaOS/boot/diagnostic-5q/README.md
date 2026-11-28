# Reproduire la qualification USB 5Q

## Vérification hors VM

Depuis la racine du worktree, avec Python 3.9 ou ultérieur et OpenSSL 3 dans le PATH. Les archives contiennent des époques distinctes et le juge utilise la clé Ed25519 publique du projet.

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
d=novaOS/boot/diagnostic-5q
v=novaOS/docs/validation/diagnostic-uefi-5q
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "$d"
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/preuves.py" verifier "$v/observations.zip"
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/preuves.py" verifier "$v/reconstruction.zip"
done
make -C novaOS abi-check
```

Le juge décode les pages binaires VT-d, compare les mises en cache de traductions aux prêts publiés, vérifie les signatures avec OpenSSL et reconstitue les sections relogées du PE. Il contrôle les deux bornes du diagnostic, ses écrans et les interruptions avec le juge 5K. Les traces exportées conservent les positions en octets. Les pointeurs d’objets du processus hôte sont pseudonymisés. Les adresses invitées servent à la contre-lecture.

## Reconstruction du firmware de laboratoire

Utiliser l’arbre EDK II PIO de référence construit selon la recette 5J, le modèle QEMU 5J, les sources correspondantes de QEMU 11.1.3, l’environnement Python de laboratoire 5E, LLVM/Clang, ACPICA, GLib, pkg-config et Rust stable avec sa cible x86_64-unknown-none. Cargo utilise le verrou livré et le cache hors ligne. EDK II et ses sous-modules correspondent à la révision `fbe0805b2091393406952e84724188f8c1941837` et aux adaptations PIO 5J. [Recette de construction 5J](../diagnostic-5j/README.md)

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/acpica/20260408/bin:$PATH"
d=novaOS/boot/diagnostic-5q
j=../diagnostic-uefi-phase5j/novaOS/build
bash "$d/construire.sh" "$j/ovmf5j-b" "$j/modele5j/qemu-11.1.3" rejeu-5q
```

Le constructeur exige un répertoire de sortie neuf sous `novaOS/build`. Il contrôle les empreintes des fichiers d’entrée modifiés, construit le vérificateur Rust, l’intègre au BDS et compile le témoin TCG pour l’hôte Darwin. Le GenFw de la copie privée réserve une longueur stable pour le nom de débogage avant sa suppression dans le PE de livraison. Le contrôle final exige le SHA-256 du volume qualifié.

## Campagne Q35

Le propriétaire du banc fournit localement `CLE_RECUS_LABO`, chemin de la clé privée de reçus 5M. Les magasins de variables publiés en 5O servent directement aux deux modes. La preuve hors VM ci-dessus reste accessible avec la seule clé publique.

```sh
set -eu
: "${CLE_RECUS_LABO:?chemin local du signataire de laboratoire 5M}"
b=novaOS/build/rejeu-5q
d=novaOS/boot/diagnostic-5q
j=../diagnostic-uefi-phase5j/novaOS/build
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
"$py" -B "$d/banc_usb.py" "$b/campagne" "$j" \
  "$b/edk/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd" \
  "$CLE_RECUS_LABO" --plugin "$b/controle.dylib"
python3 -B "$d/preuves.py" archiver "$b/campagne" "$b/observations.zip"
python3 -B "$d/preuves.py" verifier "$b/observations.zip"
```

Exécuter les blocs dans le même shell pour conserver les variables temporaires. Le banc prend lui-même le verrou QEMU partagé. L’option `--mode actif` ou `--mode inactif` choisit un magasin. L’option répétable `--cas` limite la campagne. Chaque exécution utilise une destination neuve, des images ordinaires synthétiques en lecture seule pour USB et une copie privée du canari NVMe. [Matrice et limites](../../docs/diagnostic-uefi-5q.md)
