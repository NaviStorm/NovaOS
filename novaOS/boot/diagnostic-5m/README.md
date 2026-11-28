# Reproduire et vérifier la phase 5M

Le premier EFI NovaOS est l’unique exécutable de diagnostic. Le laboratoire réutilise OVMF 5J et les domaines Q35 de 5K. Le rapport décrit la racine de confiance avant l’entrée et les limites sur machine physique. [Matrice et résultats](../../docs/diagnostic-uefi-5m.md)

## Contre-lecture publique

Depuis la racine du worktree, avec Python 3.9 ou ultérieur et OpenSSL 3 sur le PATH, exécuter les commandes ci-dessous. La contre-lecture reconstruit la copie de la flash octet par octet depuis l’archive originale 5J, les supports RAM de chaque démarrage et les sections PE relocalisées. Elle vérifie les signatures Ed25519, le reçu terminal, les compteurs QMP, les tables VT-d, les exceptions APIC et les traces d’accès. Les seules clés nécessaires sont publiques.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5m
v=novaOS/docs/validation/diagnostic-uefi-5m
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  for phase in m l k j i; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "novaOS/boot/diagnostic-5$phase" -v
  done
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/verifier.py"
done
make -C novaOS abi-check
```

## Nouvelle campagne avec une époque fraîche

Cette recette concerne le Mac de laboratoire qui possède le signataire privé correspondant à `autorite-laboratoire.pub`. Le fichier de signature doit appartenir au laboratoire, être un fichier ordinaire privé et rester dans son espace de travail exclu de Git. Il sert uniquement aux reçus de ce prototype. Le code contrôle la correspondance de la clé publique. La reconstruction du PE et des magasins ne nécessite aucun secret. Les trois empreintes admises et l’identifiant du PE nominal figurent dans le manifeste suivi. [Images de référence](images-laboratoire.json) · [Prérequis du modèle et du firmware](../diagnostic-5j/README.md)

```sh
p=novaOS/build/rejeu-5m
j=../diagnostic-uefi-phase5j/novaOS/build
py=../diagnostic-uefi-phase5e/novaOS/build/phase5e-tools/venv/bin/python
mkdir -p novaOS/build
mkdir "$p"
sh "$d/construire.sh"
pe=novaOS/build/target5m/x86_64-unknown-uefi/release/diagnostic-5m.efi
"$py" -B "$d/supports.py" "$j" "$pe" "$p/supports"
cmp "$d/images-laboratoire.json" "$p/supports/images.json"
"$py" -B "$d/banc5m.py" "$j" "$p/supports" "$p/observations" --cle-recu "$CLE_RECUS_LABO"
"$py" -B "$d/archiver.py" "$p/supports" "$p/observations" "$p/preuves"
python3 -B "$d/verifier.py" "$p/preuves"
```

`CLE_RECUS_LABO` désigne un chemin absolu fourni par le dépositaire de la clé. Les outils `pyfatfs`, `virt-firmware` et `cryptography` proviennent de l’environnement historique 5E. Le modèle QEMU 11.1.3 modifié pour le PIO, OVMF et le module minuterie sont contrôlés par empreinte. Le banc prend le verrou QEMU commun pendant les captures. Il exige une destination neuve. Chaque scénario possède un namespace synthétique et une VM froide. Les deux premiers lancements de calibration, un par mode, localisent l’image et le module minuterie. Leurs coordonnées sont vérifiées de nouveau au véritable point d’entrée de chaque scénario admis.

Le constructeur prend le magasin 5J en lecture seule et fabrique une copie régulière pour OVMF. Trois images sont nécessaires pour atteindre séparément les contrôles du manifeste et de l’ELF après admission de l’EFI. Le contre-exemple hors de `db` utilise une quatrième image dont l’empreinte est exclue. Les supports publiés contiennent les PE et les images FAT de laboratoire. Les archives d’observations retiennent les reçus signés, les engagements, les compteurs et les traces. L’archiveur utilise une liste fermée de fichiers. Une nouvelle autorité de reçus constitue une autre identité expérimentale et demande sa propre qualification.
