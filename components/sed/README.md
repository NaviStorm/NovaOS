# NovaOS sed 0.1.0 · ARC-027

L’exécutable Darwin ARM64 et les deux ELF de production utilisent le même analyseur, les mêmes expressions régulières et le même cycle de lecture. Les ELF sont livrés séparément pour NovaOS AArch64 et x86_64. Le catalogue de la base ne permet pas encore de lancer leur traitement depuis `nsh`. [Binaires et licences](../../distribution/sed-arc027/) · [Matrice par cible](validation/matrice.md) · [Qualification](validation/qualification.md)

## Choix du moteur au 16 novembre 2026

| Candidat | Couverture et dépendances | Coût de portage observé |
|---|---|---|
| uutils/sed 0.3.0, MIT, révision `5170719b6de95b118f5f7a1dc806fffa66940a40`, mise à jour le 16 novembre 2026 | Programme Rust activement développé. BRE/ERE, traitement par octets, captures et chemins de repli pour références arrière. Dépend notamment de `regex`, `fancy-regex`, `clap`, `uucore`, `memmap2`, `rustix` et `std::process`. Binaire et tests Unix, banc externe GNU et cibles de fuzzing disponibles. La partie multioctet et les replis demanderaient une nouvelle qualification sur données invalides. | Les essais `cargo check --lib --locked` échouent avec E0463 sur les deux cibles sans OS. Il faudrait extraire le cœur des dépendances à `std`, isoler fichiers/mappage/processus et vérifier les deux moteurs de regex. Aucune dépendance de ce candidat distribuée au titre de son adaptation. |
| uutils/red 1.0.2, MIT, révision `f18ab518e41e98ab0d2ad72e54a342ddb7b1ca43`, 17 janvier 2026 | Implémentation expérimentale avec BRE/ERE, DFA/NFA, captures et références arrière, tests Rust et scripts de comparaison GNU. Conserve les octets bruts et un cache texte susceptible de conversion avec pertes. `encoding_rs`, `lexopt`, `anyhow`, tables allouées, libc multioctet et signaux Unix, commandes externes via shell. | E0463 pour les deux cibles, notamment dans les dépendances qui importent `std`. La séparation de la représentation texte, des signaux, des entrées/sorties et de l’exécution externe toucherait plusieurs couches. Aucune dépendance de ce candidat distribuée au titre de son adaptation. |
| QSE 0.9.1, révision `19ef5cc9e9149368666ae92ca2187400d03b6e67`, 20 décembre 2024 | Source C avec suite historique `regress/sed`. QSE et TRE BSD-2-Clause, tri d’origine Berkeley BSD-3-Clause. BRE/ERE, captures, références arrière, mémoire allouée par callbacks, tampons avec longueurs. | Sous-ensemble compilé avec Clang dès l’étude pour Darwin ARM64 et les deux cibles sans OS. Adaptation locale de la syntaxe GNU, de la sélection des octets et des effets, plus une interface Rust pour les flux et l’allocateur. Risques mesurés sur ancres, locale et ressources dans le différentiel. |

Révisions étudiées et références externes. [uutils/sed](https://github.com/uutils/sed/tree/5170719b6de95b118f5f7a1dc806fffa66940a40) · [uutils/red](https://github.com/uutils/red/tree/f18ab518e41e98ab0d2ad72e54a342ddb7b1ca43) · [QSE](https://github.com/hyung-hwan/qse/tree/19ef5cc9e9149368666ae92ca2187400d03b6e67) · [GNU sed 4.10](https://ftp.gnu.org/gnu/sed/sed-4.10.tar.xz) · [Manuel de GNU sed](https://www.gnu.org/software/sed/manual/sed.html)

Le code de liaison local est MIT. Les sources QSE/TRE conservent leurs notices. Les versions et licences des dépendances Rust utilisées figurent dans la livraison. Darwin fait appel aux bibliothèques système du Mac. Les ELF lient statiquement `nova-cli`, l’allocateur et le moteur. GNU sed est exécuté séparément comme oracle. [Verrou Cargo](Cargo.lock) · [Attributions](../../distribution/sed-arc027/LICENCES.txt)

## Reconstruction

Depuis la racine du dépôt, utiliser la chaîne Rust stable avec les trois cibles installées, Clang, LLVM et les dépendances du verrou présentes dans le cache Cargo. Le script imprime le chemin du binaire en dernière ligne. Le SDK et la HAL fournissent le démarrage et le script d’édition de liens des ELF. Darwin ne demande aucune installation de QSE.

```sh
export NOVAOS_TMPDIR="/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
mkdir -p "$NOVAOS_TMPDIR"
components/sed/build.sh macos
components/sed/build.sh aarch64
components/sed/build.sh x86_64
distribution/sed-arc027/sed-macos-arm64 --version
(cd distribution/sed-arc027 && shasum -a 256 -c SHA256SUMS)
```

Le moteur lit des octets, même si Darwin annonce une locale UTF-8. Un point peut remplacer plusieurs octets d’un seul caractère, un par correspondance. L’aide ARC-026 consulte indépendamment le tag `LANG` de l’invocation. Une langue principale française reconnue sélectionne le français. Les autres tags, y compris les tags malformés, sélectionnent l’anglais. Dans NovaOS, `sed --help`, `sed -i --help`, `sed --version` et `man sed` passent avant toute résolution métier.

## Architecture et décision demandée à la MOA/MOE

Les sources dans `qse/` portent l’analyse, les adresses, les regex, les deux espaces et le cycle. `engine.c` et `src/engine.rs` fournissent les allocations, les flux et les erreurs. Darwin fournit les fichiers, le shell pour `e` et la publication d’un candidat. L’adaptateur NovaOS utilise la sélection fichier déléguée, stdout, stderr et les ports privés du lanceur. Une sortie terminal représente les octets non affichables. La sortie d’un pipe conserve les octets. L’entrée terminal, les canaux de fichier, les erreurs de révocation et les deux rôles restent à qualifier après admission. [Adaptateur Darwin](src/host.rs) · [Adaptateur NovaOS](src/native.rs)

1. **Admission conjointe avec grep.** Attribuer un identifiant d’installation et un ELF à chaque commande dans le manifeste et le sélecteur du lanceur. Spécifier séparément pour chaque rôle les droits sur l’opérande sélectionné, le terminal et l’extrémité du pipe. La fonction `grant` du témoin SDK accepte zéro port terminal et zéro paire fichier. La sélection actuelle de `pipe_spawn` et celle du parseur/exécuteur permettent uniquement `cat` vers `wc -l` ou `head -n 1`. Le lecteur reçoit le mot de configuration 2, le producteur le mot 1. Le lot commun couvre aussi les identités, l’encodage des arguments, les deux statuts et la fermeture après échec partiel. La proposition du chantier grep « admission-pipe-MOA-MOE.md » décrit la même dépendance. [Manifeste](../../novaOS/abi/installation.rs) · [Lanceur](../../novaOS/kernel/src/sched/process/ipc/spawn.rs) · [Bibliothèque processus](../libraries/process.rs) · [Exécution](../shell/executor.rs) · [Sélection du shell](../shell/script.rs)
2. **Recette après admission.** Qualifier la lecture jusqu’à EOF, puis `1q` avec une ligne courte et un fichier qui sature réellement le tampon. Observer la suspension effective, le réveil et le statut `CLOSED=11` de cat, `sed=0`, la collecte et la restitution. Vérifier séparément le petit fichier déjà acquitté avec deux statuts 0. Comme producteur, vérifier trois lignes terminées et `wc -l = 3`. Le refus du lanceur observé aujourd’hui précède la création de ces enfants. [Contrat du pipe](../../novaOS/docs/contrat-flux-arc027-v0.23.x.md)
3. **Fichiers et effets.** Le lecteur vise uniquement la poignée sélectionnée. Plusieurs fichiers ou `-f` exigent des délégations supplémentaires. Le profil NovaOS refuse les commandes nommées `r R w W s///w` et l’exécution `e s///e` au moment de la compilation. Leur qualification sur des droits réellement accordés puis révoqués dépend du service et de l’admission. Une exécution externe exigerait un contrat de lancement distinct.
4. **Remplacement P9.** La publication disponible exige un nom libre. Soumettre une opération de remplacement conditionnée à l’identité et aux droits de la source et de la destination, avec révocation et récupération par remontage. Les ACL de comptes et la surprotection persistante restent des dépendances séparées. Le shell refuse `-i` avec le statut 4. [Délégations CLI](../../novaOS/docs/cli-v0.15.1.md) · [Arbitrage PC1](../../novaFS/docs/novafs-publication-conditionnelle.md)
