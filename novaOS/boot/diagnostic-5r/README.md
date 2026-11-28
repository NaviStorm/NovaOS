# Reproduire le diagnostic USB 5R

Les commandes s’exécutent depuis la racine du worktree 5R. La cible du PE est x86_64 UEFI. Les deux passages hôtes Darwin utilisent le même PE et la même archive. Le compilateur qualifié est Rust 1.99.1, révision `58cf8f9c35b355199727627d07ae34a4e2846d67`, avec la cible `x86_64-unknown-uefi` et les dépendances du verrou en cache.

## Construction et contre-lecture publique

```sh
set -eu
export PYTHONDONTWRITEBYTECODE=1
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
d=novaOS/boot/diagnostic-5r
v=novaOS/docs/validation/diagnostic-uefi-5r
b=novaOS/build/rejeu-5r
sh "$d/construire.sh" "$b/rust"
pe="$b/rust/x86_64-unknown-uefi/release/diagnostic-5r.efi"
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  for phase in i j k l m n o p q r; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "novaOS/boot/diagnostic-5$phase"
  done
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/qualification.py" verifier "$v/observations.zip" "$pe"
done
make -C novaOS abi-check
python3 -B "$d/rapports.py" "$v/observations.zip" "$pe" "$b/rapports.zip"
cmp "$b/rapports.zip" "$v/rapports.zip"
```

Le juge utilise OpenSSL 3, la clé publique 5M, les octets du PE reconstruit, la capture des pixels et les tables VT-d. Le répertoire de destination des rapports doit être libre. Les archives excluent les images disques complètes, la mémoire générale de l’invité et les clés privées.

## Nouveau démarrage en laboratoire

Reconstituer le banc x86_64 des phases 5J et 5Q. Le QEMU 11.1.3 qualifié, le volume OVMF et le module TCG Darwin sont identifiés par le banc ou la recette correspondante. Le propriétaire fournit localement la clé de reçus autorisée. Le magasin 5O original provient des preuves publiées. [Construction du firmware et du témoin TCG](../diagnostic-5q/README.md)

```sh
: "${CLE_RECUS_LABO:?chemin local de la clé de reçus du banc 5M}"
: "${BUILD_5Q:?racine de la construction EDK II et du témoin controle.dylib de 5Q}"
j=../diagnostic-uefi-phase5j/novaOS/build
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
"$py" -B "$d/banc.py" magasin "$pe" "$b/magasin"
"$py" -B "$d/banc.py" qualifier "$b/campagne" "$j" "$BUILD_5Q" "$pe" "$b/magasin" "$CLE_RECUS_LABO"
python3 -B "$d/qualification.py" archiver "$b/campagne" "$b/campagne" "$b/observations.zip"
python3 -B "$d/qualification.py" verifier "$b/observations.zip" "$pe"
```

Le constructeur du magasin vérifie le SHA-256 complet du seul PE autorisé avant d’ajouter son empreinte Authenticode à une copie jetable. Il compare toutes les variables, leurs métadonnées et les entrées de db. L’option `--mode` du banc permet de scinder la campagne. Dans ce cas, passer les répertoires inactif puis actif à `archiver`. Le cas `reutilise` exige que le cas `nominal` du même mode ait déjà produit son reçu dans le même répertoire. Les époques, signatures de reçus et durées varient à chaque exécution.

Le banc prend le verrou QEMU partagé. Il travaille sur des fichiers ordinaires dans une destination neuve, ouvre les médias USB en lecture seule et conserve les compteurs du canari NVMe. La production de `rapports.zip` travaille hors VM. L’exporteur NOVALOGS 5N conserve sa propre politique d’admission. [Matrice de qualification et limites](../../docs/diagnostic-uefi-5r.md)
