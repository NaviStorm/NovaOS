# diff natif 0.24.0 — ARC-027

Le binaire Darwin et les ELF autonomes NovaOS ont été construits depuis les sources qualifiées `ba13f796a80f8afdca0293d4de8112ba7b501f95`, avec la recette de construction reproductible `2c30c06b46e6342e1b66bc539fb93a1bcb21b612`, sur la base `origin/main` vérifiée au démarrage `425e4e7cc5c03e246efb64d3638376906344bf47`. Branche de livraison `codex/diff-natif-0240`. La commande ordinaire reste bloquée dans cette base.

| Exécutable | Format |
|---|---|
| [diff](bin/diff) | Mach-O arm64, Apple Silicon |
| [diff-aarch64.elf](bin/diff-aarch64.elf) | ELF utilisateur AArch64, bloc de démarrage NXEXEC01 |
| [diff-x86_64.elf](bin/diff-x86_64.elf) | ELF utilisateur x86_64, bloc de démarrage NXEXEC01 |

Les empreintes des trois exécutables, de l’oracle et des modules étudiés, ainsi que les versions de Rust, du Mac et de QEMU, figurent dans la [provenance](preuves/provenance.json). L’ABI native est v1 à 48 sélecteurs, le noyau 0.20.0 et NovaFS 0.9.13 P9.

## Compatibilité observée

| Entrée | Couverture exercée | Résultat |
|---|---|---|
| Darwin ARM64 | 61 noms longs GNU, 46 caractères d’option courte avec les chiffres, arguments séparés/accolés, abréviations, `--`, erreurs, combinaisons et formats | 775/775, dont 762 contre GNU et 13 contre ARC-026 |
| ELF de recette, AArch64 et x86_64, P9/512 et P9/4096 | Même série, chargée depuis un fichier P9. Les fichiers, répertoires, liens, refus de lecture et stdin de cette série sont des objets en mémoire | 775/775 dans chacun des quatre invités |
| ELF de recette, objets P9 réels | Deux fichiers égaux, différences au début, à la fin ou dispersées, récursion sur deux arbres. Les flux bruts et le statut sont comparés à GNU | 5/5 pour chaque profil, à chacun des quatre passages |
| Flux Darwin | Pipeline `cat → diff → cat` sur données identiques, différentes et opérande absent. Lecteur fermé avec SIGPIPE par défaut ou ignoré | 5/5, dont signal 13 ou erreur 2 suivant le parent |
| Documentation du shell ordinaire | `diff --help`, `diff --version`, `man diff` avec `en`, `fr-CA` et l’étiquette invalide `fr--CA` | 9/9 par profil, SHA-256 du volume et compteurs QMP d’écriture et de flush identiques avant/après |
| Comparaison dans le shell ordinaire | `diff base diff-equal` et `cat base \| diff - diff-equal` | 127 pour la commande. Le shell refuse la syntaxe du pipe avant lancement sur les quatre profils |

La [matrice par option et par cible](preuves/MATRICE.md) renvoie aux [arguments, octets et statuts individuels](preuves/complet/options.json). Voir aussi les [tests des pipes Darwin](preuves/flux-darwin.json) et les [journaux du vrai shell et de l’ELF de recette](preuves/qemu). Le statut 0 signifie identiques, 1 différents, 2 erreur. L’outil lit ses opérandes et écrit sur stdout et stderr. La documentation du shell utilise son catalogue et son pager sans délégation de fichier, ni création de tmp. Une étiquette BCP 47 validée de langue principale `fr` active le français. Le repli est l’anglais.

### Oracle et limites

GNU Diffutils 3.12 provient du tag Savannah `v3.12`, commit `16681a3cbcea47e82683c713b0dac7d59d85a6fa`. Le SHA-256 de la distribution officielle est `7c8b7f9fc8609141fdea9cece85249d308624391ff61dedaf528fcb337727dfd`. Le binaire explicite a été compilé sur Darwin pour la campagne CMP. Le parseur de `src/diff.c`, le manuel, l’aide de l’oracle, le moteur et les modules de rendu, de parcours et les tests amont ont servi de références comportementales. Leurs chemins et empreintes sont consignés dans la provenance. GNU est sous GPL-3.0-or-later. L’implémentation Rust est MIT. Les notices des six dépendances compilées sont dans [licences](licences). Les deux paquets Windows du verrou Cargo sont inactifs sur les trois cibles. Le client de lancement dérive des modules MIT de `a94b4a6e869122edde227ebdd293aedbcf5dc43f`.

Le candidat uutils diffutils 0.6.0, MIT ou Apache-2.0, révision `cdbcb882760971ff5684518f1c15ef21d148a315`, reproduit les statuts et les deux flux de 292 des 762 lignes GNU. Les autres lignes incluent des différences de formats, de prise en charge d’options et de diagnostics. Les [observations du candidat](preuves/uutils.json) sont associées aux identifiants de la matrice. Son code n’entre pas dans l’exécutable Nova.

Les 31 scripts amont applicables ont été exécutés sur Darwin. GNU a 28 réussites, un échec attendu (`large-subopt`) et deux sauts. Nova a 27 réussites, le même échec attendu, deux sauts et un écart exigé par ARC-026 (`help-version` attend la version GNU 3.12). Les deux sauts requièrent respectivement le fichier Linux `/proc/cmdline` et Valgrind. Le corpus local teste CRLF. Sur les deux grands fichiers de `large-subopt`, les octets de sortie de Nova coïncident avec GNU en unifié, en unifié minimal et avec `-H`. Les régressions corrigées concernaient le placement des blocs de lignes répétées, le seuil de recherche des gros fichiers et la gestion de SIGPIPE héritée du parent. [Résultats amont et empreintes des trois sorties](preuves/amont)

Les essais fixent `LC_ALL=C`, `TZ=UTC0`, les noms, les données et les mtime à 1700000000 s. Les en-têtes sensibles aux noms utilisent aussi des libellés explicites. Darwin utilise `/usr/bin/pr` pour la pagination. La seule substitution d’horloge des vecteurs invités concerne le champ de date de l’en-tête de pagination, fixé au 14 novembre 2023 à 22 h 13 UTC. Le reste des deux flux est comparé en octets. Les diagnostics d’E/S réels du futur canal de session, les liens symboliques natifs, le terminal interactif pour `--color=auto`, les locales hors C pour les comparaisons de texte et toutes les combinaisons possibles de valeurs restent hors de la qualification complète.

### Raccord de production

L’adaptateur ordinaire attend la page NXEXEC01, les flux et le canal de fichiers de la session authentifiée. La branche commune du lanceur, examinée à `a94b4a6`, est hors de l’ascendance de la base. Le shell de cette base ne résout ni ne lance cet ELF depuis le volume. Le prérequis est le parcours commun de résolution de l’exécutable depuis `nsh`, de création de ticket pour la session NovaAuth et de délégation du répertoire courant et des flux. La validation de production devra faire ouvrir les opérandes par le processus `diff` et exercer le pipeline dans ce parcours. La recette emprunte le point `@probe` déjà réservé au profil `cli-test`, dans une extraction jetable, avec sélection en lecture des fichiers nommés. Son succès qualifie le moteur et ces lectures. L’ELF de production livré reste à valider en session ordinaire. Aucun rôle de pipe dédié ni changement de service, de sélecteur ou de format disque n’est livré.

## Performances

Les [mesures Darwin](preuves/perf-complet/MESURES.md) donnent les deux médianes, débits, volumes des différences et pics RSS. Sur Apple M4 Max, cinq passages alternent l’ordre GNU/Nova avec environ 4,85 Mo d’entrée cumulée par charge. Les sorties sont vérifiées avant et après chaque mesure. Le temps mural inclut la création du processus. Le pic RSS vient de `wait4`. Le verrou commun `cut-performances.lock` sérialise les campagnes participantes. La [charge de l’hôte](preuves/perf-complet/charge-avant.json) est consignée pour interpréter les petites différences.

Les [mesures QEMU](preuves/QEMU.md) portent sur des jeux beaucoup plus petits, de 4 352 à 4 384 octets cumulés. QEMU 11.1.3 utilise TCG, 2 vCPU, 256 Mio de RAM, machine virt/Cortex-A72 ou Q35/CPU max, sans réseau. Les volumes P9 sont des images jetables de 2 Mio. Chaque charge a un passage de vérification suivi de trois passages chronométrés et vérifiés. Le pic de 64 Kio mesure les pages de tas réservées par le processus de recette. Le temps inclut lancement, E/S P9 et transmission des résultats sur UART. Les [relevés individuels](preuves/qemu) contiennent les temps, le débit et les empreintes des différences. Aucun temps GNU natif sous NovaOS ni essai UTM ou physique n’est revendiqué.

## Rejeu

Depuis la racine du worktree sur un Mac ARM64, utiliser Python 3.11 ou ultérieur, la chaîne Rust stable avec les trois cibles, QEMU, xorriso et les prérequis ISO du projet. Les constructions Cargo utilisent le verrou et le cache local. Si nécessaire, remplir ce cache avec `cargo fetch --locked` depuis les workspaces concernés. Les scripts placent les essais privés dans le dossier `tmp` frère du worktree. Les permissions du témoin d’accès exigent un compte ordinaire.

```sh
cd livraison-diff-0240
./preparer-gnu.sh                      # si GNU 3.12 est absent
./reconstruire.sh
./tester-options-diff.sh --rapide
./tester-options-diff.sh --complet
python3 -B vecteurs.py
python3 -B flux.py
python3 -B amont.py
./mesurer-performances-diff.sh --rapide
./mesurer-performances-diff.sh --complet
./tester-options-diff.sh --novaos --complet
```

Le dernier appel affiche le dossier neuf des images, ELF de recette, captures et mesures. Les deux scripts acceptent `--help`. Le mode invité `--rapide` sélectionne AArch64/P9-512. Le mode `--complet` exerce les quatre profils, leurs performances et le shell ordinaire. L’alias `./mesurer-performances-diff.sh --novaos --complet` exécute le même parcours. Pour régénérer les tableaux invités, lancer `python3 -B synthese.py <dossier-du-rejeu>`. Les modes Darwin acceptent `--sortie <dossier>`. Les variables optionnelles `GNU_DIFF` et `GNU_DIFF_SRC` désignent respectivement le binaire 3.12 et son arborescence configurée. La comparaison uutils se rejoue par `python3 -B candidat.py /chemin/vers/diffutils`.

Les cinq tests du catalogue passent sur Darwin ARM64 et x86_64 sous Rosetta. Les suites de régression `nsh-test`, `nsh-scripts-test` et `nsh-terminal-test` ont réussi dans les deux géométries sur les deux ISA. Les essais de démarrage général, DMA, installation et endurance ne font pas partie de cette qualification de commande utilisateur. Les journaux sont conservés dans [les preuves](preuves).

Deux constructions isolées à des chemins différents reproduisent à l’octet près le Mach-O et les deux ELF. Le Mach-O possède un UUID dérivé de son contenu et une signature ad hoc vérifiée par macOS. Les empreintes des deux constructions figurent dans [le contrôle de reconstruction](preuves/reconstruction.json).

Le manifeste de livraison se vérifie dans ce répertoire avec `shasum -a 256 -c SHA256SUMS`. L’archive intégrale locale ajoute les ISO, les volumes de genèse, les états initiaux et finaux, les ELF de recette et les noyaux utilisés, avec un second manifeste à sa racine.

Pour réarchiver un rejeu complet depuis le worktree, utiliser `python3 -B archiver.py <dossier-du-rejeu> <nouvelle-archive.tar.gz>`. L’outil vérifie les empreintes des images, noyaux et disques, décompresse l’archive dans un nouveau dossier puis contrôle chaque fichier au moyen du manifeste interne.
