# stat 0.24.0 — ARC-027

Le Mach-O pour macOS ARM64 et les deux ELF NovaOS exécutent le même parseur et le même moteur de formatage. Sur P9, l’ELF lit les métadonnées des objets délégués. Dans le système ordinaire, `stat --help`, `stat --version` et `man stat` fonctionnent. Le lancement de `stat temoin` reste à raccorder au lanceur générique. [Matrice par option et directive](MATRICE.md)

| Cible | Exécutable | Qualification |
|---|---|---|
| Darwin ARM64 | [stat-darwin-arm64](bin/stat-darwin-arm64) | 366 contrôles, dont les sorties, diagnostics et statuts comparés à GNU, deux consultations documentaires Nova et sept lectures indépendantes de statvfs |
| NovaOS AArch64 | [stat-aarch64.elf](bin/stat-aarch64.elf) | 159 appels consignés par géométrie, dont 152 de l’ELF, sur P9/512 et P9/4096 |
| NovaOS x86_64 | [stat-x86_64.elf](bin/stat-x86_64.elf) | Même recette, même jeu de données synthétiques et mêmes contrôles |

Base distante vérifiée au démarrage, `425e4e7cc5c03e246efb64d3638376906344bf47`. Sources et trois exécutables au commit `c8a076ed2121b4c1d8b3a6c0b35d233ca28724fe`. La dépendance du constructeur de `nsh` sur le parseur a été déclarée au commit `8f6e377e3e81d143bd112d679af0c8652283c5a3`. Campagne du 17 novembre 2026. [Environnement, révisions et empreintes](preuves/references.json)

## Moteur indépendant

Cinq vecteurs à valeurs fictives vérifient le parseur et le formateur sans accès aux métadonnées. Ils exercent les octets NUL, les temps négatifs, le formatage des liens, la distinction fichier/volume et la sortie partielle après une erreur. Les cinq passent sur Darwin ARM64 natif et les cinq sur Darwin x86_64 sous Rosetta. Les exécutions ELF sous QEMU figurent séparément ci-dessous. [Tests](moteur.rs) · [ARM64](preuves/moteur-aarch64.txt) · [Rosetta](preuves/moteur-x86_64.txt)

## Comparaison macOS

Apple M4 Max, macOS 27.2.1, ARM64 natif. Les 2 048 requêtes d’un lot impriment l’identifiant, la taille et la date de modification. Les programmes écrivent vers /dev/null et reçoivent les mêmes inodes, la même locale C et le même fuseau UTC. Le premier lecteur alterne entre six jeux synthétiques. Les mesures du premier passage ont trois échantillons par programme. Le passage à chaud en a six. Chaque temps inclut le lancement et l’attente du processus. Le RSS maximal de chaque processus est relevé par wait4. Le premier passage suit la création des fichiers par le banc, sans purge du cache macOS.

| Charge | Passage | Nova ms | GNU ms | Nova objets/s | GNU objets/s | RSS Nova Mio | RSS GNU Mio |
|---|---|---:|---:|---:|---:|---:|---:|
| 2 048 fichiers | Premier | 7,34 | 7,06 | 278 899 | 289 914 | 2,67 | 2,14 |
| 2 048 fichiers | Chaud | 6,23 | 6,57 | 328 706 | 311 529 | 2,67 | 2,14 |
| Même fichier × 2 048 | Premier | 4,85 | 5,08 | 422 003 | 402 865 | 2,67 | 2,14 |
| Même fichier × 2 048 | Chaud | 4,86 | 5,17 | 421 625 | 395 765 | 2,67 | 2,14 |

Les 366 contrôles ont satisfait leurs oracles. Pour un compteur APFS variable, le banc conserve les lectures différentielles intermédiaires et accepte uniquement une paire identique, dans la limite de 16 essais. La lecture indépendante compare le nombre renvoyé à l’intervalle fermé entre les relevés statvfs immédiatement avant et après. Les valeurs stables doivent concorder exactement. Les sept scripts GNU ont réussi sur chacun des deux exécutables. Le sous-cas de montage privé avec unshare est conditionnel et ne s’exécute pas sur Darwin. [Données fonctionnelles](preuves/macos-options.json) · [Mesures brutes](preuves/macos-performance.json) · [Tests amont](preuves/amont.log)

## Mesures NovaOS

QEMU 11.1.3 TCG, un vCPU, 256 Mio, VirtIO-blk et volumes P9 jetables. Modèles virt/cortex-a72/GICv2 et q35/max. Chaque passage porte sur 64 requêtes, distribuées en huit lancements de huit opérandes. L’ELF affiche la taille. Le premier passage chronométré reste dans les fichiers bruts. Le tableau donne la médiane des quatre passages suivants, de l’envoi des commandes au retour de l’invite, hors commandes de vérification de statut. Les octets privés sont les pages de données et de pile comptabilisées à la sortie dans le noyau de recette. Les charges concurrentes des autres chantiers de ce Mac peuvent contribuer à la dispersion observée. Les temps incluent la délégation, les démarrages d’ELF et l’UART. GNU n’a pas de processus natif dans ces invités.

| Architecture | Secteur | 64 distincts ms | Objets/s | 64 répétés ms | Objets/s | Mémoire privée à la sortie |
|---|---:|---:|---:|---:|---:|---:|
| AArch64 | 512 | 465,87 | 137 | 343,22 | 186 | 408 Kio |
| AArch64 | 4 096 | 620,12 | 103 | 456,65 | 140 | 408 Kio |
| x86_64 | 512 | 310,47 | 206 | 342,59 | 187 | 412 Kio |
| x86_64 | 4 096 | 304,58 | 210 | 307,75 | 208 | 412 Kio |

Dans une copie de qualification uniquement, la commande privée `@stat` utilise le mécanisme de sélection existant de ls et lance les octets de l’ELF livré. Le noyau de cette copie mesure les pages à la sortie. Chaque ISO de recette et chaque ISO ordinaire ont été relus après décompression. Le noyau extrait contient l’ELF livré uniquement dans les quatre médias de recette. Les contrôles UART bruts comprennent aussi les messages du noyau. Les essais n’établissent pas des flux stdout et stderr binaires indépendants dans l’invité. [Contrôle des huit ISO](preuves/controle-iso.json)

Une seconde passe a rejoué onze cas d’admission par cellule sur une copie des mêmes images. La requête sans canaux métier a renvoyé 5. Une demande groupant un objet valide, un absent et un autre valide a renvoyé 20. Les noms avec espace, la priorité des formats, les options invalides et les préfixes longs ont également été contrôlés. [Exemple de résultats](preuves/QEMU-x86_64/4096/admission.json)

Les quatre sessions ordinaires ont testé les consultations en français et le repli en anglais, avec contrôle du SHA-256, des écritures et des FLUSH du disque. Elles ont constaté le statut 127 pour le lancement métier et le statut 3 pour son usage comme producteur d’un pipe. Le passage en production requiert le raccord générique des métadonnées par opérande et des flux autorisés. Le mandat conserve le contrat ABI v1, les capacités et le format P9. [Exemple de trace du système ordinaire](preuves/nsh-aarch64/512/nsh.json)

## Régressions du système ordinaire

Les onze cibles ci-dessous ont terminé avec succès sur AArch64 et sur x86_64. Elles couvrent notamment le démarrage, les ELF, le cycle des processus, les courses IPC, le bloc, les commandes, le shell et vi. Les tests CLI P9 et nsh ont exercé les deux géométries. [Journal AArch64](preuves/regression-aarch64.log.gz) · [Journal x86_64](preuves/regression-x86_64.log.gz)

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test
done
```

Le manifeste couvre le contenu de livraison, les sources de stat et les fichiers de raccord modifiés. Depuis la racine du dépôt ou de son archive extraite, vérifier les octets avec `shasum -a 256 -c livraison-stat-0240/SHA256SUMS`. [Manifeste](SHA256SUMS)

## Sources et licences

GNU Coreutils v9.12 au commit `c0f8514d989184921d9b12a4d103a7b23abc5af8` fournit l’oracle de comportement. Le binaire Homebrew provient de la formule 9.12_1. Les références consultées comprennent le parseur, la sélection des formats, les conversions, les règles de citation, la recherche du point de montage, le chapitre du manuel et les sept tests de tests/stat. GNU utilise GPL-3.0-or-later. Le code livré de la commande est en Rust sous licence MIT. [Source officielle](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/stat.c) · [Manuel de la révision](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/doc/coreutils.texi)

Le candidat uu_stat de uutils 0.13.0, sous MIT, dépend de clap, fluent, rustix et uucore. Son parseur à la révision examinée ne déclare pas --cached. Son adaptation aurait nécessité de remplacer les accès std et le traitement des arguments pour la cible nue. L’implémentation Nova réutilise la citation des noms et la conversion de locale Darwin du composant MIT nova-basename (`36f3775`), la validation de langue du catalogue Nova et le client de métadonnées existant. Les notices des dépendances verrouillées et de la bibliothèque Rust accompagnent les binaires. [Source uutils examinée](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/stat/src/stat.rs) · [Licences](LICENCES.txt.gz) · [Notices Rust](LICENCES-Rust.html.gz)

## Rejouer

Depuis la racine du worktree. Les deux scripts principaux proposent --help, --rapide, --complet, --sortie et --novaos. La recette QEMU rassemble fonctionnel et performance lors d’une même session et exige un répertoire de sortie neuf. Sur Darwin, --sortie désigne un fichier JSON. Rust stable doit disposer des trois cibles indiquées dans le script de reconstruction. Les dépendances Cargo verrouillées sont utilisées hors ligne. GNU stat, QEMU, les firmwares, xorriso et les outils du projet doivent être disponibles.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd -P)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
liv=livraison-stat-0240
"$liv/reconstruire.sh" macOS aarch64 x86_64
"$liv/tester-options-stat.sh" --complet --sortie "$TMPDIR/options-stat.json"
"$liv/mesurer-performances-stat.sh" --complet --sortie "$TMPDIR/mesures-stat.json"
for arch in aarch64 x86_64; do
    rustup run stable rustc --edition=2024 --test --target "$arch-apple-darwin" "$liv/moteur.rs" -o "$TMPDIR/stat-moteur-$arch"
    "$TMPDIR/stat-moteur-$arch"
    campagne=$(mktemp -d "$TMPDIR/stat-$arch.XXXXXXXX")
    "$liv/tester-options-stat.sh" --complet --novaos "$arch" --sortie "$campagne/recette"
    python3 -B "$liv/nsh.py" "$arch" "$campagne/ordinaire"
    for secteur in 512 4096; do
        cellule="$campagne/recette/$secteur"
        python3 -B "$liv/rejouer-admission.py" "$cellule" "$cellule/admission.json"
    done
done
```

Pour la suite amont, fournir le dossier Coreutils v9.12 dans GNU_SOURCE et son init.sh de tests généré dans GNU_INIT si nécessaire, puis exécuter `livraison-stat-0240/amont.sh`. La recette utilise exclusivement des objets synthétiques. Le support virtuel contient 512 blocs de 4 Kio. Les archives de la campagne comprennent les ISO compressées, les images P9 de recette avant et après, les traces, les configurations VM, les résultats et les empreintes.
