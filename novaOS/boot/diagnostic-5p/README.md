# Reproduire la qualification USB xHCI 5P

La contre-lecture publique couvre les 26 observations archivées. Elle utilise Python 3.9 ou ultérieur et OpenSSL 3. Les binaires de diagnostic, l’image de 68 Mio et la clé publique de contrôle proviennent des livraisons 5M et 5O déjà présentes dans le dépôt. [Matrice et conditions avant A8](../../docs/diagnostic-uefi-5p.md)

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
d=novaOS/boot/diagnostic-5p
v=novaOS/docs/validation/diagnostic-uefi-5p
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "$d"
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/preuves_usb.py" verifier
done
make -C novaOS abi-check
```

## Recréation des observations

Utiliser le worktree 5P et le répertoire temporaire partagé prévus par le laboratoire. Chaque banc QEMU réserve lui-même le verrou des qualifications du projet. Les chemins de sortie doivent être neufs. Le disque interne de chaque VM est un fichier ordinaire synthétique. Le banc ouvre les images USB avec un descripteur hôte en lecture seule. Les deux copies de magasin OVMF ont les empreintes de 5O.

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
b=novaOS/build/rejeu-5p
d=novaOS/boot/diagnostic-5p
o=novaOS/boot/diagnostic-5o
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
j=../diagnostic-uefi-phase5j/novaOS/build
mkdir "$b"
tar -xOzf novaOS/docs/validation/diagnostic-uefi-5e/laboratoire.tar.gz ovmf/code.fd > "$b/firmware-usb.fd"
gzip -dc novaOS/docs/validation/diagnostic-uefi-5o/magasin-ovmf-test.fd.gz > "$b/variables-actif.fd"

python3 -B "$d/banc.py" "$b/usb" "$b/firmware-usb.fd"
"$py" -B "$d/exclusion.py" "$b/exclusion" "$j" "$b/firmware-usb.fd"
"$py" -B "$o/ecran_qemu.py" "$j" "$CLE_RECUS_LABO" "$b/gop-inactif"
"$py" -B "$o/ecran_qemu.py" "$j" "$CLE_RECUS_LABO" "$b/gop-actif" --mode actif --magasin "$b/variables-actif.fd"
python3 -B "$d/preuves_usb.py" archiver "$b" "$b/usb" "$b/usb" "$b/exclusion" "$b/observations.zip" --reconnexion "$b/usb"
python3 -B "$d/preuves_usb.py" verifier "$b/observations.zip"
```

Le premier banc requiert QEMU x86_64 11.1.3 avec les événements xHCI et AHCI activables. Le profil VT-d emploie le modèle QEMU 5J identifié par son SHA-256. Le rejeu GOP utilise en plus le firmware PIO 5J et le signataire privé de reçus de laboratoire 5M. Le propriétaire du banc fournit `CLE_RECUS_LABO` localement. La vérification de l’archive distribuée utilise uniquement la clé publique.

Les paramètres `--profil` et `--mode` de `banc.py` permettent de rejouer une cellule. Dans `preuves_usb.py archiver`, la troisième source après `build` et `usb` contient la capture en vol. L’option `--reconnexion` désigne la série contenant les limites QMP et la lecture du clone. Le vérificateur exige ces preuves pour les deux modes. Une coupure de trace, une identité ambiguë ou un intervalle incohérent fait échouer le contrôle.

L’archive contient les captures PPM avant et après, les tables VT-d du modèle, les verdicts et les traces nécessaires à la contre-lecture. Les pointeurs d’objets du processus QEMU des traces USB sont remplacés par des identifiants cohérents de même longueur. Les positions dans les traces restent vérifiables. Les adresses d’anneaux et de pages visibles sont celles de la VM. Les époques, les durées et les signatures de reçu varient à chaque exécution.
