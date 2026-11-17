# seq natif — ARC-027

**17 novembre 2026.** Le code du composant est au commit `54895020035b7a082e20b9b602cc5682d42bd41b`, sur la branche `codex/seq-natif-0240` créée depuis `origin/main` à `425e4e7cc5c03e246efb64d3638376906344bf47`. Le numéro `0.20.0` affiché par `seq --version` provient du paquet `nova-cli`. Le chantier relève de NovaOS v0.24.0.

| Binaire | Format | SHA-256 |
|---|---|---|
| [seq](bin/seq) | Mach-O ARM64, macOS | `774faf53af2a78eef57605c5d28963300d976a587afcb011227cdc7a49ee1790` |
| [seq-aarch64.elf](bin/seq-aarch64.elf) | ELF64 AArch64, NovaOS natif | `cfed7c3ae6bd6225d4e6807480b9c9045881da0f96e8d86ff995fa9d6932fec6` |
| [seq-x86_64.elf](bin/seq-x86_64.elf) | ELF64 x86_64, NovaOS natif | `e0f66cc39675aafc6ae0dddfcee658246ae552f26c527c02b39ce7d3b8d94cee` |

## Reproduire

Sur un Mac Apple Silicon équipé des outils du dépôt, de Rust stable 1.99.1, des deux cibles nues, de QEMU, de xorriso et de GNU coreutils 9.12, lancer depuis le worktree dédié. Les scripts utilisent par défaut `OS-worktrees/tmp/seq-arc027/` pour leurs temporaires et résultats de rejeu. `--out DOSSIER` sélectionne une autre destination pour les résultats.

```sh
cd livraison-seq-0240
./reconstruire.sh macOS
./tester-options-seq.sh
MANPATH="$PWD/man" man seq
./tester-options-seq.sh --amont
./reconstruire.sh aarch64
./reconstruire.sh x86_64
./tester-options-seq.sh --qemu aarch64
./tester-options-seq.sh --qemu x86_64
./mesurer-performances-seq.sh
./mesurer-performances-seq.sh --complet
shasum -a 256 -c SHA256SUMS
```

Le banc QEMU prend par défaut les vecteurs archivés. Pour utiliser un autre journal différentiel, passer `--vecteurs DOSSIER/options.jsonl.gz`. La recette shell démarre le noyau ordinaire du worktree. La recette ELF crée une copie du noyau dans le TMPDIR privé et y adapte exclusivement le scénario SDK et l’observation des écritures console. Chaque ISO est relue par extraction du noyau. Le script vérifie les octets de l’ELF dans le noyau de recette et relit chacun des médias compressés. La branche de livraison conserve l’ABI v1, P9 et le lanceur partagé.

## Références et implémentation

La référence fonctionnelle est GNU coreutils `c0f8514d989184921d9b12a4d103a7b23abc5af8`. Le parseur, les deux chemins de génération et le traitement de la borne ont été examinés dans `src/seq.c`, avec la section `seq invocation` du manuel, `man/seq.x` et les sept fichiers `tests/seq/`. La conversion en locale courante puis C, la détection du dépassement, les écritures partielles et la fermeture ont été vérifiées dans `gl/lib/cl-strto{d,ld}.c` et dans gnulib au gitlink `106e9b2384d08a1696fcbd40cbab52237943f208`. Ces objets gnulib ont été lus par SHA, indépendamment de la révision extraite localement. L’oracle est `/opt/homebrew/bin/gseq`, SHA-256 `9c1006d0b9cab9e6ceb3963e977f62b3f64dd5d464af7abe970052637288f2c7`. [Source officielle épinglée](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/seq.c) · [Inventaire des sources et tests](preuves/sources-gnu.txt) · [Aide de l’oracle](preuves/gseq-help.txt) · [Environnement](preuves/environnement.txt)

Le moteur Rust MIT utilise `core` et `alloc`. Il partage le parseur, l’arithmétique et le formateur entre Mach-O et ELF. Les grands entiers positifs progressent directement dans leurs chiffres décimaux avec un tampon de sortie de 32 Kio. La mémoire dépend du nombre de chiffres, de la largeur d’une valeur et de la taille du tampon. Le reste des séquences utilise binary64 avec arrondi à l’affichage. Le chemin Darwin utilise les conventions numériques de la libc et le descripteur 1. L’ELF écrit au terminal ou sur l’extrémité d’écriture IPC lorsqu’elle lui est remise.

Le code `uu_seq` examiné emploie les E/S de `std`, Clap, Fluent, BigDecimal et les services de `uucore`. La cible nue et sa comparaison au GNU de Darwin ont motivé un moteur indépendant. Le port n’incorpore aucune source de GNU ni de uutils. Les tests amont sont exécutés depuis la copie locale en lecture seule. Le script `--amont` reconstitue l’initialisation des tests depuis le gitlink gnulib et compile un petit auxiliaire qui rapporte les limites du compilateur hôte. [Dépendances uutils examinées](preuves/uutils.txt) · [Notices](licences/composants.txt)

## Mesures

Sur le M4 Max, après un échauffement, les cinq passages alternent l’ordre Nova/GNU. Ils vont tous vers `/dev/null`. Des passages distincts calculent la taille et SHA-256 en flux pour les deux programmes. Les temps incluent le démarrage du processus. Le RSS est celui du fils donné par `wait4`. Le Mac était partagé avec d’autres activités, dont les campagnes QEMU de régression. Les moyennes de charge avant et après sont archivées.

| Taille ou nombre | Traitement | Nova seq | GNU seq |
|---:|---|---:|---:|
| 1 000 000 | Génération | 0,008470 s | 0,012516 s |
| 10 000 000 | Génération | 0,058469 s | 0,108970 s |
| 65 000 000 | Génération | 0,401402 s | 0,780519 s |

La sortie de 65 millions contient 573 888 897 octets, SHA-256 `d757dc25fc05c9629c9b277fd0e05a0efd123ef7a2250cfb2c88aacafa7ef0d8`. Les mesures brutes donnent le débit et le RSS de chaque passage. Le banc invité mesure aussi l’ELF livré vers la console série QEMU, pour 1 000 et 10 000 valeurs, avec un échauffement et trois passages. Le compteur matériel encadre lancement, génération et collecte. L’observateur par trame est désactivé pendant le chronométrage. La relecture de la série compare chaque sortie au GNU de Darwin après normalisation du LF rendu en CRLF. Les deux destinations et les environnements d’exécution diffèrent entre le tableau Mac et ces mesures invitées. [Profil court](preuves/mesures-court.json) · [Profil complet](preuves/mesures-complet.json) · [Matrice et limites d’intégration](MATRICE.md)

## Vérifier la livraison

L’archive `seq-0240.tar.gz` contient les fichiers modifiés du port, les binaires, les scripts, les résultats et les huit médias de recette. Depuis le dossier de livraison dans le worktree, `python3 -B lab/archiver.py` compare chaque membre décompressé aux fichiers présents, vérifie l’identité des ISO et des volumes, puis contrôle le manifeste SHA-256. La reconstruction utilise le monorepo à la base indiquée. Une extraction propre du commit de code, avec un cache Cargo neuf, a reproduit les trois binaires octet par octet. [Reconstruction indépendante](preuves/reconstruction.txt)

La mise au point a corrigé deux erreurs détectées par comparaison. L’adaptateur de sortie standard Rust traitait un descripteur fermé comme une sortie vide, ce qui faisait réussir `seq 1`. L’écriture Darwin propage désormais EBADF et rend 1. Le formateur validait trop tôt la largeur maximale, même si la plage était vide. Le contrôle de taille intervient maintenant à la première valeur à imprimer.

## Régressions du dépôt

Les onze cibles suivantes ont réussi sur chacune des deux architectures au commit de code `54895020035b7a082e20b9b602cc5682d42bd41b`. La campagne a exécuté les cibles successivement. Le tableau conserve le statut de chaque invocation et l’archive contient sa sortie ainsi que les traces du répertoire de construction. [22 statuts](preuves/regressions.tsv) · [Journaux et traces](preuves/regressions.tar.xz)

```sh
for arch in aarch64 x86_64; do
  for cible in test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test; do
    make -C novaOS ARCH="$arch" "$cible" || exit 1
  done
done
```
