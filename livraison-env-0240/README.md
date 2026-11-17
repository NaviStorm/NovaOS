# env 0.24.0 · NovaOS ARC-027

| Cible | Exécutable | Témoin |
|---|---|---|
| macOS ARM64 natif | [env-darwin-arm64](bin/env-darwin-arm64) | [env-witness-darwin-arm64](bin/env-witness-darwin-arm64) |
| NovaOS AArch64, EL0 | [env-aarch64.elf](bin/env-aarch64.elf) | [env-witness-aarch64.elf](bin/env-witness-aarch64.elf) |
| NovaOS x86_64, Ring 3 | [env-x86_64.elf](bin/env-x86_64.elf) | [env-witness-x86_64.elf](bin/env-witness-x86_64.elf) |

Le développement part de `origin/main` vérifié à `425e4e7cc5c03e246efb64d3638376906344bf47`, descendant du dépôt principal local `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`. Le commit source est `34d0667f0405ab8ca222e6b8692e56e9cf5db1f8`. Les six binaires figurent dans `6e83495e0fa92828d268aaa880c768c1e9975a83`. La branche de livraison est `codex/env-natif-0240`. [Empreintes et versions](preuves/references.json)

## macOS ARM64

Les **156 cas** du mode complet passent sur macOS 27.2.1 ARM64. Le banc compare stdout et stderr en octets et le statut, avec un oracle propre à la documentation ARC-026. Les corpus incluent les trois manipulations de signaux du témoin, la conservation de son statut et de sa locale, les accès refusés, un PTY et un pipe entre deux processus. Un second banc exécute les **199 cas de `tests/env/env-S.pl`** du tag GNU sur GNU puis Nova. Les deux passages réussissent. Les cinq suites shell amont ont servi à construire les cas différentiels. [Résultats détaillés](preuves/options.json) · [Résultats Perl](preuves/amont.json) · [Matrice des options](MATRICE.md)

Sur Darwin, une nouvelle affectation sans `-i` est insérée en tête par le chemin `putenv` de l’oracle. Le premier prototype l’ajoutait en fin de liste. Le moteur reproduit désormais les deux ordres observés, avec une insertion en fin de liste lorsque GNU gère lui-même le vecteur après `-i` ou un import non vide. Le banc compare aussi l’ordre des doublons et des entrées sans égal.

Sur Apple M4 Max, le banc mesure le temps mur d’un processus complet. Chaque résultat est la médiane de sept passages après échauffement, avec douze lancements par passage et un ordre Nova/GNU alterné. La mémoire est le maximum résident rapporté par `wait4` pour chaque processus, agrégé par médiane. La sortie chronométrée va vers `/dev/null` après contrôle des octets. Un verrou partagé sérialise les campagnes participantes. La charge moyenne de l’hôte est enregistrée. [Temps et mémoire côte à côte](preuves/performances.md) · [Échantillons bruts](preuves/performances.json)

## NovaOS

Les quatre cellules ont validé 220 appels de env, huit appels indépendants du témoin et 48 consultations documentaires dans le shell. Chaque groupe documentaire a conservé l’empreinte du disque. Les deux appels métier ordinaires de chaque cellule ont rendu 127 et le pipe a rendu 3.

Le banc rejoue sur chaque ISA avec P9/512 et P9/4096. Il crée un volume fictif neuf pour chaque cellule. QEMU 11.1.3 utilise TCG, 256 Mio et un vCPU. La machine ARM est virt, Cortex-A72 avec GICv2. La machine x86 est q35 avec le CPU max. Le profil SDK lance le véritable ELF sous contrôle d’un banc temporaire. L’ELF témoin reçoit un lancement indépendant et vérifie aussi le moteur sur des entrées binaires en mémoire. Le profil ordinaire démarre ensuite avec son propre noyau et interroge le vrai `nsh`. Le banc extrait le noyau de chacune des trois ISO par cellule et contrôle son empreinte.

Les mesures invitées couvrent le chargement, la préparation, l’exécution et la collecte de **env** pour une et trente affectations. La préparation est également chronométrée séparément. Les compteurs sont divisés par la fréquence de la minuterie invitée. Le tableau présente la médiane de cinq appels après un échauffement. Les pages initiales proviennent de `UserSpace::page_counts()` avant la première instruction de l’enfant. Le pic du tas en cours d’exécution reste inconnu. Le statut et l’audit des ressources sont contrôlés après collecte. Le coût d’un enfant lancé *par env*, le transport public des variables exportées et le pipe binaire de bout en bout restent ouverts jusqu’au raccord du lanceur. GNU Coreutils ne fournit aucun ELF pour l’ABI NovaOS. [Mesures invitées](preuves/performances-qemu.md) · [Résultats par cellule](preuves/matrice-cibles.json)

## Référence et licences

Le tag GNU 9.12 correspond à `c0f8514d989184921d9b12a4d103a7b23abc5af8`. Le parseur, `-S`, l’import brut et l’appel de lancement ont été étudiés dans `src/env.c`, avec `src/operand2sig.c`, `src/printenv.h`, `src/system.h`, le chapitre *env invocation* de `doc/coreutils.texi` et les six fichiers de `tests/env/`. Le sous-module gnulib épinglé à `106e9b2384d08a1696fcbd40cbab52237943f208` fournit notamment `argz.c`, `read-file.c`, `execvp.c`, `putenv.c` et `unsetenv.c`. La révision Homebrew 1 applique le correctif GNU `782a1e5` qui réserve la mise entre guillemets au terminal. Le banc utilise explicitement `/opt/homebrew/opt/coreutils/libexec/gnubin/env`, lien vers `genv`. [Source GNU épinglée](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/env.c) · [Correctif de l’oracle](https://github.com/coreutils/coreutils/commit/782a1e5bc2090212273bb731dceee2cc2a071e54)

L’implémentation uutils étudiée à `4735bbe40d764a523c5ece37d47465b32452d1a2` est sous MIT et utilise `std`, `clap`, `fluent`, `uucore` et `nix`. Le port Nova emploie son propre moteur sur octets et deux adaptateurs, Darwin et NovaOS. Le rendu des octets et le contrôleur de recette réutilisent le travail MIT de `codex/basename-natif-0240` à `36f37751dc43928925ef221de572d6226e140fae`. GNU sous GPL-3.0-or-later sert de référence externe. Le script de rejeu amont lit les fichiers de son checkout. Les licences des bibliothèques liées accompagnent les binaires. [Source uutils](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/env/src/env.rs) · [MIT](LICENCE-MIT) · [Dépendances](NOTICES.txt.gz) · [Bibliothèques Rust](NOTICES-Rust.html.gz)

## Rejouer depuis le worktree

Les scripts exigent Python 3.11 ou ultérieur, Rust stable avec les trois cibles installées, les dépendances Cargo verrouillées disponibles hors ligne, les outils de construction NovaOS, QEMU et xorriso. Les recettes invitées acceptent un dossier de sortie neuf. Le mode rapide réduit le corpus hôte et le nombre de mesures. Les temporaires sont placés dans le répertoire frère `OS-worktrees/tmp/`. Réserver 2 Gio pour les constructions et médias de cette recette. Chaque cellule crée une image de genèse et un disque de session avec les données fictives de qualification.

```sh
cd livraison-env-0240
./reconstruire.sh macOS
./tester-options-env.sh --rapide
./tester-options-env.sh --complet ../novaOS/build/env-options
./mesurer-performances-env.sh --complet ../novaOS/build/env-perf
TMPDIR="$(cd ../../tmp && pwd)/" python3 -B lab/amont.py ../../tmp/sort-arc027-gnu ../novaOS/build/env-amont
./reconstruire.sh aarch64
./reconstruire.sh x86_64
./tester-options-env.sh --complet --novaos aarch64 ../novaOS/build/env-arm
./tester-options-env.sh --complet --novaos x86_64 ../novaOS/build/env-x86
python3 -B lab/archiver.py ../novaOS/build/env-arm ../novaOS/build/env-x86 ../novaOS/build/env-archives
shasum -a 256 -c SHA256SUMS
```

La forme `mesurer-performances-env.sh --complet --novaos ISA dossier-neuf` exécute la même campagne invitée avec ses contrôles fonctionnels. Chaque archive de preuves possède un manifeste interne. L’outil d’archivage relit les octets décompressés de chaque membre et vérifie également l’ensemble exact des noms.
