# dd 0.24.0 pour macOS ARM64 et NovaOS

Recette ARC-027 du 17 novembre 2026. Sur macOS ARM64, les 512 invocations de la matrice passent avec GNU Coreutils 9.12 comme référence pour la copie et le catalogue Nova comme référence pour la documentation. Les ELF NovaOS copient et convertissent sur flux IPC délégués. Le lancement ordinaire de la copie depuis le shell et la sortie P9 nommée restent à raccorder.

## Exécutables et révisions

| Cible | Fichier | SHA-256 |
|---|---|---|
| macOS ARM64 | [dd](bin/dd) | `11da30dc82bd2a3aad490a7389018a0cc36321f04d238f83a147fc2c0b4586ac` |
| NovaOS AArch64 | [dd-aarch64.elf](bin/dd-aarch64.elf) | `f9d4c0798b18da3f941a3de306fcd4c31299f7edcda4ad1cd849fb105dc5b0b8` |
| NovaOS x86_64 | [dd-x86_64.elf](bin/dd-x86_64.elf) | `1cffd7a449ba6e5e87d3ad24a68c42adbb387bfce65aeaed49d07f2aa31a4a59` |

Base `origin/main` au démarrage du mandat `425e4e7cc5c03e246efb64d3638376906344bf47`. Sources des ELF `35eba5f687b495dd16fff7171e30efc11cc3b0c2`. Sources du Mach-O `022c9fa4221d7179d97206488aa0edbd160a1162`. Code de copie sous MIT, tables de conversion uutils sous MIT. ABI NovaOS v1, 48 sélecteurs, NovaFS P9. [Sources et licences](REFERENCES.md) · [Environnement](preuves/environnement.txt)

## macOS ARM64

Apple M4 Max, macOS 27.2.1, Coreutils 9.12, locale C. Une copie vers un nouveau fichier ordinaire par processus. Médiane de cinq passages après échauffement, ordre des exécutables alterné, SHA-256 et longueur vérifiés après chaque copie. La durée inclut le lancement et la fermeture du processus. Le pic de mémoire résidente provient de wait4. Ces copies utilisent le cache de macOS. [Mesures brutes](preuves/performances.json)

| Source | Quantité | bs | Nova (ms) | GNU (ms) | Nova (Mio/s) | GNU (Mio/s) | RSS Nova (Mio) | RSS GNU (Mio) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Fichier | 32 Mio | 512 o | 143,56 | 143,15 | 223 | 224 | 2,00 | 1,84 |
| Fichier | 32 Mio | 64 Kio | 9,09 | 8,79 | 3520 | 3641 | 2,08 | 1,89 |
| Fichier | 32 Mio | 1 Mio | 6,91 | 6,98 | 4631 | 4584 | 3,02 | 2,83 |
| `/dev/zero` | 32 Mio | 512 o | 138,60 | 137,74 | 231 | 232 | 2,00 | 1,84 |
| `/dev/zero` | 32 Mio | 64 Kio | 7,85 | 7,59 | 4074 | 4218 | 2,08 | 1,89 |
| `/dev/zero` | 32 Mio | 1 Mio | 6,21 | 6,28 | 5152 | 5096 | 3,02 | 2,83 |
| `/dev/zero` | 100 Mio | 100 Mio | 21,28 | 19,44 | 4700 | 5144 | 102,02 | 101,83 |

`bs=100m` renvoie 1. Avec `bs=100M count=1`, les deux programmes ont écrit 104 857 600 octets de zéros dans le fichier jetable. Les sondes complémentaires couvrent EAGAIN, une écriture partielle, SIGINFO, SIGINT, la progression et les blocs alloués au fichier creux. Treize suites amont réussissent pour chaque programme. Le test direct.sh rend 77 pour chacun sur Darwin. [Différentiel brut](preuves/options.json.gz) · [Flux Darwin](preuves/options.flux.json) · [Suites GNU](preuves/gnu-livraison/resultats.txt)

## NovaOS sous QEMU

QEMU 11.1.3, 256 Mio invités, deux vCPU pour le shell et le moteur, un vCPU pour l’admission SDK de l’ELF. Chaque profil utilise un volume P9 jetable. La mesure chronomètre, depuis l’hôte, la lecture et la publication de 4 096 octets de zéros entre deux marqueurs série du témoin. Les trois passages après échauffement alternent les tailles de bloc. Le témoin utilise la délégation existante de cp. Le temps comprend la synchronisation. La mémoire par processus, UTM et une exécution GNU en invité restent hors mesure.

| Architecture | Secteur P9 | Vecteurs moteur | ELF conformes à GNU | Refus spécifiques ELF | bs=512 (ms) | bs=4096 (ms) | bs=512 (Kio/s) | bs=4096 (Kio/s) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| aarch64 | 512 o | 347 | 80 | 24 | 69,66 | 28,78 | 57,4 | 139,0 |
| aarch64 | 4096 o | 347 | 80 | 24 | 62,81 | 26,13 | 63,7 | 153,1 |
| x86_64 | 512 o | 347 | 80 | 24 | 61,86 | 24,46 | 64,7 | 163,6 |
| x86_64 | 4096 o | 347 | 80 | 24 | 61,23 | 24,17 | 65,3 | 165,5 |

Sur les quatre profils, les quinze consultations de documentation réussissent dans nsh, avec les SHA disque et compteurs WRITE/FLUSH inchangés. Le shell renvoie 127 pour `dd if=entree of=sortie-dd status=none` et 3 pour le pipeline de copie testé. Les 104 scénarios de flux SDK par profil sont complétés par sept admissions ou refus et trois transferts dédiés avec vérification des octets. Le périphérique invité `/dev/zero` est absent. Les copies du témoin de zéros ont été relues dans les images P9. [Matrice Darwin et moteur](preuves/matrice-macos.tsv) · [Matrice des ELF](preuves/matrice-elf.tsv) · [Portée par option](MATRICE.md)

Le raccord ordinaire demande au lanceur commun de résoudre le nom dd et de remettre les ports de flux au processus. Pour `of=`, il faut également un contrat de sortie nommée couvrant la création, le repositionnement et la publication sur P9. Les 24 refus propres à l’ELF sont détaillés dans la matrice. Les options conditionnelles de GNU dépendantes du noyau Linux restent non qualifiées sur cette cible.

## Régressions et rejeu

22/22 cibles de régression réussies sur les deux architectures. Les cinq tests du catalogue documentaire et les neuf tests du registre ABI réussissent. [Régressions](preuves/regressions/resultats.tsv) · [Catalogue](preuves/catalogue.txt) · [ABI](preuves/registre-abi.txt)

Le moteur traitait une seconde demande de lecture après la dernière trame courte comme une attente de trame. En mode fullblock, le processus pouvait rester bloqué malgré EOF. L’adaptateur mémorise désormais EOF. En cas d’écriture courte suivie d’une erreur, la comptabilité inclut également le préfixe déjà écrit. Sous Darwin, Rust rouvre les descripteurs fermés au démarrage. L’adaptateur relève désormais la fermeture héritée de stderr et rend 1 lorsqu’une statistique devait y être écrite.

Les deux scripts proposent des modes rapide, complet et invité. Les journaux série, images de recette et volumes bruts réutilisables figurent dans l’archive avec leur manifeste SHA-256. [Rejouer pour v0.30.0](REJOUER.md) · [Tests fonctionnels](tester-options-dd.sh) · [Mesures](mesurer-performances-dd.sh) · [Archive](preuves-dd.tar.xz) · [Empreintes](SHA256SUMS)
