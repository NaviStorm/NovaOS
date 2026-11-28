# Reproduire la phase 5O

Depuis la racine du worktree, utiliser Python 3.9 ou ultérieur, OpenSSL 3 et un répertoire temporaire privé du projet. Les scripts d’image travaillent sur des fichiers réguliers. Les sorties de génération et de copie doivent être neuves. La taille exacte est donnée en octets. [Matrice, mesures, contrats A7 et portes A8](../../docs/diagnostic-uefi-5o.md)

## Génération et contre-lecture publique

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5o
v=novaOS/docs/validation/diagnostic-uefi-5o
b=novaOS/build/rejeu-5o
mkdir -p novaOS/build
mkdir "$b"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
python3 -B "$d/image5o.py" creer "$b/original.img" --octets 1073741824
python3 -B "$d/image5o.py" copier "$b/original.img" --sortie "$b/media-virtuel.img" --octets 1073741824
python3 -B "$d/image5o.py" verifier "$b/media-virtuel.img" --octets 1073741824
python3 -B "$d/contrelecture.py"
sh "$d/construire.sh" "$b/rust"
gzip -dc "$v/diagnostic-x86_64.efi.gz" | cmp - "$b/rust/x86_64-unknown-uefi/release/diagnostic-5m.efi"
```

Le générateur utilise le PE publié et reproductible sur le banc qualifié. Le SHA-256 des octets logiques est indépendant de la représentation des trous. La GPT secondaire est à la fin de l’image. Copier l’image de 68 Mio au début d’un fichier plus grand ne prépare pas une GPT à la géométrie de ce fichier. Pour reproduire les quatre géométries, réserver un système de fichiers prenant en charge les fichiers clairsemés ou prévoir l’espace logique complet. Le pic RSS est rapporté en octets selon la convention Darwin.

## Qualification des VM du laboratoire

Le QEMU standard du profil USB et le modèle Q35 PIO du profil GOP sont deux exécutables distincts. Le second et son firmware sont épinglés par SHA-256. Les modules utilisés pour attribuer le gestionnaire de minuterie et le framebuffer restent dans le build 5J. Le signataire de reçus de laboratoire 5M est fourni par son dépositaire et reste hors de la livraison. Les clés UEFI du laboratoire sont indépendantes de ce signataire. La génération de la copie de flash active utilise les outils historiques `virt-firmware`, `cryptography` et `pyfatfs`. [Construction du modèle et du firmware 5J](../diagnostic-5j/README.md) · [Racine de confiance du laboratoire](../diagnostic-5m/README.md)

```sh
set -eu
d=novaOS/boot/diagnostic-5o
b=novaOS/build/rejeu-5o
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
j=../diagnostic-uefi-phase5j/novaOS/build
tar -xOzf novaOS/docs/validation/diagnostic-uefi-5e/laboratoire.tar.gz ovmf/code.fd > "$b/firmware-usb.fd"
python3 -B "$d/capacites.py" "$b/geometries" > "$b/capacites.json"
"$py" -B "$d/magasin.py" "$b/magasin"
"$py" -B "$d/usb_qemu.py" "$b/usb-a" "$b/firmware-usb.fd" "$b/geometries" "$b/magasin/actif.fd"
"$py" -B "$d/journaux_qemu.py" "$b/anciens" "$b/firmware-usb.fd" "$b/magasin/actif.fd"
"$py" -B "$d/ecran_qemu.py" "$j" "$CLE_RECUS_LABO" "$b/garde-pci" --temoin-pci
"$py" -B "$d/ecran_qemu.py" "$j" "$CLE_RECUS_LABO" "$b/gop-final-inactif"
"$py" -B "$d/ecran_qemu.py" "$j" "$CLE_RECUS_LABO" "$b/gop-final-actif" --mode actif --magasin "$b/magasin/actif.fd"
"$py" -B "$d/controle_variables.py" "$b" > "$b/variables-apres-arret.json"
python3 -B "$d/archiver.py" "$b" "$b/archives"
```

Chaque banc QEMU prend le verrou partagé. Les valeurs d’époque, les signatures de reçu, les traces, les instants et les temps d’exécution évoluent entre campagnes. La comparaison des pixels porte sur les captures avant et après dans la même VM. Sur le profil USB, il s’agit de captures de la console VGA via QMP. Sur le profil PIO, le PNG provient des octets du GOP RAM alloué par OVMF, projetés en lecture seule par le banc. Les fichiers contenant la mémoire générale de calibration sont privés.

## Contrôles de non-régression

```sh
for isa in arm64 x86_64; do
  for phase in o n m l k j i; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "novaOS/boot/diagnostic-5$phase"
  done
  /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5o/contrelecture.py
  /usr/bin/arch "-$isa" /usr/bin/python3 -B novaOS/boot/diagnostic-5n/verifier5n.py
done
make -C novaOS abi-check
```

La [procédure macOS de copie brute](../diagnostic-5n/README.md) demeure soumise à l’autorisation distincte d’un support physique. L’archive de 68 Mio et les scripts de cette phase ont été qualifiés sur fichiers ordinaires.
