# readlink 0.24.0 · ARC-027

| Cible | Livrable | Portée |
|---|---|---|
| macOS ARM64 | [readlink](bin/readlink) | Lecture de cibles brutes et résolution physique avec toutes les options de GNU 9.12. |
| NovaOS AArch64 | [readlink-aarch64.elf](bin/readlink-aarch64.elf) | ELF de recette lancé en EL0. |
| NovaOS x86_64 | [readlink-x86_64.elf](bin/readlink-x86_64.elf) | ELF de recette lancé en Ring 3. |

La [matrice fonctionnelle](MATRICE.md) précise les limites de chaque cible. Les [mesures macOS](preuves/darwin/tableau.md) portent sur 4 096 chemins par lancement. Neuf passages alternent l’ordre des exécutables après contrôle des octets attendus. Le temps couvre posix_spawn à wait4, le RSS est le maximum résident du processus pour chaque passage, puis la médiane. Les deux sorties chronométrées vont vers /dev/null. Arborescence et cache chauds, données synthétiques, locale C et verrou commun arc027-mesures.lock. [Passages individuels](preuves/darwin/mesures.json)

## Référence et licences

GNU Coreutils v9.12, commit c0f8514d989184921d9b12a4d103a7b23abc5af8. Sources consultées sous GPL-3.0-or-later, dont src/readlink.c, doc/coreutils.texi, tests/readlink/*.sh. Le sous-module gnulib est épinglé à 106e9b2384d08a1696fcbd40cbab52237943f208, notamment lib/areadlink-with-size.c (LGPL-2.1-or-later) pour les octets et lib/canonicalize.c (GPL-3.0-or-later) pour les composantes et les cycles. Oracle installé /opt/homebrew/bin/greadlink, version 9.12. [Sources GNU](https://github.com/coreutils/coreutils/tree/c0f8514d989184921d9b12a4d103a7b23abc5af8) · [Gnulib](https://github.com/coreutils/gnulib/tree/106e9b2384d08a1696fcbd40cbab52237943f208)

Candidat uutils 0.13.0, commit 4735bbe40d764a523c5ece37d47465b32452d1a2, licence MIT. Son module src/uu/readlink/src/readlink.rs utilise std::fs, clap, fluent et uucore. L’ELF NovaOS utilise no_std. Le moteur, son interface Namespace en octets et le parseur sont implémentés en Rust sous MIT. L’adaptateur Darwin, le rendu des citations et le contrôleur SDK réemploient les travaux MIT des commandes dirname et basename. Les scripts GNU sont lus depuis un checkout externe pour le rejeu. [Module uutils](https://github.com/uutils/coreutils/tree/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/readlink) · [Licences et versions des dépendances Cargo](preuves/environnement.json)

## Espace de noms et lancement

La base distante vérifiée au départ est 425e4e7cc5c03e246efb64d3638376906344bf47, descendante du dépôt principal local 3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e. Les résultats portent sur cette branche de développement et ses trois binaires identifiés par SHA-256. [Environnement](preuves/environnement.json)

NovaFS P9 définit File et Directory. LOOKUP_AT, STAT, DIR_OPEN et GETCWD du protocole sont des opérations sur des sessions et des handles délégués. Une lecture du contenu d’un fichier ne fournit pas la cible brute d’un lien. L’ELF de recette n’a aucune délégation métier. Le raccord complet nécessite un type lien persistant, une lecture de sa cible en octets sans traversée finale et un parcours autorisé ancré à la racine et au répertoire courant de la session. Même -m doit consulter les composantes existantes. [Types P9](../novaFS/src/fs.rs) · [Protocole](../novaOS/abi/files.rs) · [Parcours par session](../novaOS/user/novafs/files/namespace.rs)

Sur cette base, le lanceur ordinaire ne distribue pas readlink. Une branche parallèle du lanceur a poursuivi ses essais de tickets, dont f8966d38fa5ef021e4b2354780e0a0c4dd69f946. Elle ne fournit pas de primitive de lecture de lien à readlink. Les travaux de ce lot portent sur la commande, le catalogue documentaire et le banc. Les 48 sélecteurs ABI v1, les services de stockage et le format P9 restent ceux de la base.

Le contrôleur SDK de test utilise une copie du noyau avec attribution du seul port console nécessaire. Il vérifie le contenu des sorties série et les statuts, la restitution des instances et le SHA du disque en lecture seule. Dans le noyau ordinaire, nsh fournit localement --help, --version et man readlink. Les contrôles comparent le SHA du volume stabilisé avant et après consultation. Les essais utilisent des volumes P9 jetables. L’ELF de sonde est un exécutable distinct qui appelle le moteur avec un espace de noms synthétique. [Contrôleur](lab/controleur.rs) · [Sonde](lab/sonde.rs)

## Rejouer depuis ce répertoire

```sh
./reconstruire.sh tout
./tester-options-readlink.sh --rapide
./tester-options-readlink.sh --complet
./mesurer-performances-readlink.sh --rapide
./mesurer-performances-readlink.sh --complet
./qualifier-qemu.sh
```

Les deux scripts de banc acceptent --help et une destination facultative après le mode. Le script QEMU affiche un répertoire neuf contenant les deux ISA et les deux géométries. Pour rejouer les huit suites amont, passer au script lab/amont.sh le chemin absolu du checkout GNU v9.12 et un dossier de journaux. Les temporaires utilisent OS-worktrees/tmp. Les cibles Rust, QEMU avec ses firmwares, xorriso, l’oracle Homebrew et les dépendances Cargo du monorepo doivent être disponibles sur l’hôte.
