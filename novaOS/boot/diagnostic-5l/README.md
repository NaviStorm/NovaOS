# Reproduire la qualification 5L

Le lot publié permet une contre-lecture hors VM avec Python 3.9 ou ultérieur. Les nouvelles captures utilisent le banc Darwin, QEMU x86_64 TCG, le firmware et les supports fictifs épinglés. [Résultats et matrice d’acceptation](../../docs/diagnostic-uefi-5l.md)

## Contrôler les preuves

Depuis la racine du worktree, avec le répertoire temporaire partagé déjà préparé et le lecteur NovaFS P9 natif de qualification disponible, exécuter les commandes suivantes. L’option `--lecteur-p9` remonte dix images et contrôle les ancres, le contenu et la garde. La retirer permet la seule vérification des pièces et de leurs empreintes.

```sh
set -eu
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)" TMPDIR="$(cd ../tmp && pwd)/" PYTHONDONTWRITEBYTECODE=1
d=novaOS/boot/diagnostic-5l
k=novaOS/boot/diagnostic-5k
v=novaOS/docs/validation/diagnostic-uefi-5l
lecteur=../pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs
(cd "$v" && shasum -a 256 -c SHA256SUMS)
for isa in arm64 x86_64; do
  for phase in l k j i; do
    /usr/bin/arch "-$isa" /usr/bin/python3 -B -m unittest discover -s "novaOS/boot/diagnostic-5$phase" -v
  done
  /usr/bin/arch "-$isa" /usr/bin/python3 -B "$d/verifier.py" --lecteur-p9 "$lecteur"
done
make -C novaOS abi-check
```

## Rejouer les VM

Utiliser un dossier neuf et le répertoire de compilation 5J contenant le modèle PIO épinglé. Les scripts prennent le verrou QEMU commun. Le banc de stockage réutilise le worktree du pilote VirtIO-net, son lecteur P9, les archives de qualification et l’outil de signature de laboratoire déjà provisionné. Les variantes de panne sont compilées dans des copies privées des sources historiques. Le contrôle des empreintes interrompt le banc si les entrées divergent. [Construction du modèle QEMU et d’OVMF](../diagnostic-5j/README.md) · [Dépendances du banc de stockage](../diagnostic-5k/README.md)

```sh
j=../diagnostic-uefi-phase5j/novaOS/build
p=novaOS/build/rejeu-5l
mkdir "$p"
python3 -B "$d/transition.py" "$j" "$p/efi"
python3 -B "$d/transition.py" "$j" "$p/minuterie" --chercher-minuterie 18
bash "$d/rejouer-stockage.sh" "$p/disques"
python3 -B "$k/banc.py" "$j" "$p/regression-edu"
bash "$k/rejouer-stockage.sh" "$p/regression-duo"
python3 -B "$k/archiver.py" "$p/regression-edu" "$p/regression-duo" "$p/regression"
retenu=$(python3 -B -c 'import json,sys;print(json.load(open(sys.argv[1]))["dossier"])' "$p/minuterie/selection.json")
python3 -B "$d/pieces.py" "$p/efi" "$p/disques" "$p/minuterie/$retenu" "$p/regression" "$j" "$p/preuves"
python3 -B "$d/verifier.py" --dossier "$p/preuves" --lecteur-p9 "$lecteur"
```

La recherche de concurrence s’arrête au premier vecteur de minuterie naturel au retour de SMM dans le même appel EFI. En l’absence d’observation au terme du nombre autorisé de démarrages, elle signale un échec. Les époques, adresses de processus hôte, durées et positions des interruptions varient. Les cas restent reproductibles par leurs invariants.

Les modules de comparaison et les supports déjà signés peuvent être reconstitués sans signature supplémentaire. Extraire `diagnostic-uefi-5j/laboratoire.tar.gz`, `diagnostic-uefi-5k/modules.tar.gz` et `diagnostic-uefi-5l/modules.tar.gz` depuis les dossiers de validation après vérification de leurs manifestes. Dans le répertoire neuf de la recette 5K, compléter aussi la copie de `supports5j-b/paquet.img` à 34 Mio dans `qualification5j/paquet.img`, comme pour `diagnostic.img`. Le collecteur contrôle l’empreinte de chaque image avant exécution. Les modules EDK II et leurs mentions de licence publiques accompagnent les preuves. Les instantanés de travail de RAM restent locaux. L’archive conserve les empreintes page par page, les tables DMA du banc et les images de stockage fictives.
