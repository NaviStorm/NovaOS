# Rejouer la qualification UEFI 5F

## Relire la livraison

Depuis la racine du worktree dédié, Python 3.11 ou suivant peut vérifier les deux livraisons hors invité. Le lecteur 5F vérifie les empreintes puis recoupe le bilan avec les compteurs QMP, les journaux OVMF, les relevés PCI/VT-d, les rapports en RAM et les états CPU archivés. Le lecteur 5E vérifie que les sources binaires et les résultats historiques sont toujours lisibles.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
python3.11 -B novaOS/boot/diagnostic-5f/preuves.py
python3.11 -B novaOS/boot/diagnostic-5e/verifier_livraison.py
```

La première série indépendante est également conservée. Pour la vérifier avec le même lecteur, depuis la racine du worktree et avec les variables temporaires ci-dessus.

```sh
set -eu
premiere=novaOS/docs/validation/diagnostic-uefi-5f/premiere-serie
(cd "$premiere" && shasum -a 256 -c SHA256SUMS)
mkdir -p novaOS/build
lecture=$(mktemp -d "$PWD/novaOS/build/lecture-5f.XXXXXXXX")
tar -xzf "$premiere/serie.tar.gz" -C "$lecture"
python3.11 -B novaOS/boot/diagnostic-5f/preuves.py --dossier "$lecture"
```

## Refaire la mesure

Utiliser une machine de laboratoire avec QEMU x86_64 11.1.3 (TCG, SMM, trace) et la disposition des worktrees du projet. Les seuls supports exposés à la VM sont des fichiers réguliers. Le banc acquiert et libère le verrou QEMU partagé. Chaque cellule démarre avec un disque SATA jetable contenant le témoin signé. Le profil BootNext réutilise exclusivement le magasin **virtuel** issu du précédent démarrage. Le banc ne monte aucun volume hôte.

Le Python de laboratoire utilise `virt-firmware==26.10`, `cryptography==50.0.2`, `pyfatfs==1.1.0` et `setuptools==80.10.2`. Pour l’épreuve Authenticode, utiliser osslsigncode 2.10. Le certificat tiers public vient du paquet virt-firmware et son DER est contrôlé par empreinte. Le firmware, les clés **publiques** de laboratoire et tous les supports proviennent de l’archive 5E épinglée. Les commandes ci-dessous utilisent par défaut les outils existants du worktree 5E en lecture seule. Les variables `PY5F` et `SIGN5F` permettent de choisir un environnement équivalent. Les dates affichées par les outils hôtes et les durées QMP peuvent varier. La vérification Authenticode hors invité est datée du 27 novembre 2026 à 20 h UTC pour rendre le contrôle historique reproductible après expiration du certificat de laboratoire.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export PYTHONDONTWRITEBYTECODE=1
outils="$PWD/../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools"
py="${PY5F:-$outils/venv/bin/python}"
sign="${SIGN5F:-$outils/sign-build/osslsigncode}"
base=novaOS/docs/validation/diagnostic-uefi-5e
src=novaOS/boot/diagnostic-5f
mkdir -p novaOS/build
travail=$(mktemp -d "$PWD/novaOS/build/rejeu-5f.XXXXXXXX")
mkdir "$travail/lot"
(cd "$base" && shasum -a 256 -c SHA256SUMS)
tar -xzf "$base/laboratoire.tar.gz" -C "$travail/lot"
(cd "$travail/lot" && shasum -a 256 -c SHA256SUMS)
gzip -dc "$base/diagnostic-x86_64.img.gz" > "$travail/lot/supports/diagnostic.img"
"$py" -B "$src/confiance.py" "$travail/lot/supports" "$travail/confiance.json" --outil "$sign"
"$py" -B "$src/qualifier.py" "$travail/lot" "$travail/serie" --worktrees ..
python3.11 -B "$src/preuves.py" --emballer "$travail/serie" "$travail/confiance.json" --dossier "$travail/livraison"
```

Le bilan donne les entrées/sorties brutes par périphérique, les adresses RIP, les lectures de registres et les variables de démarrage. L’archive rassemble les journaux des mesures et des calibrations, les magasins avant/après construits par le banc et les rapports binaires. La FAT SATA initiale figure dans la livraison 5E sous le même SHA-256 que le support nominal. Le banc en contrôle les octets après chaque exécution. Les adresses, numéros Boot#### et lectures comptées appartiennent à ce firmware et à ce modèle de disque. [Matrice, propositions et décision A8](../../docs/diagnostic-uefi-5f.md)
