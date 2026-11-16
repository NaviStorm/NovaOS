# ARC-027 · grep

**Référence fonctionnelle unique — GNU grep 3.12.** Cette branche livre un exécutable Darwin ARM64, le cœur de sélection compilé pour les deux architectures de NovaOS et le catalogue documentaire consultable dans les invités. La qualification de la recherche native reste ouverte. [Matrice différentielle](validation/matrice.md) · [Admission demandée à la MOA/MOE](admission-pipe-MOA-MOE.md)

## Provenance et construction

Le worktree part de `origin/main` vérifié à la révision `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`. L’implémentation dérive de uutils/grep 0.2.0, révision `617f541aa9f4db9d621db7867d7f598b6bc9f57e`, sous licence MIT. Le manuel et le programme GNU ont servi à spécifier et à comparer les comportements. La branche reprend le code Rust MIT et ses notices. [Source amont épinglée](https://github.com/uutils/grep/tree/617f541aa9f4db9d621db7867d7f598b6bc9f57e) · [Licence](LICENSE) · [Manuel GNU](https://www.gnu.org/software/grep/manual/grep.html)

Sur macOS, `-G` et `-E` utilisent Oniguruma 6.9.10 avec adaptation des syntaxes et du choix des correspondances. `-P` appelle PCRE2 10.49 8 bits. Les moteurs C sont liés statiquement. Le Mach-O ARM64 final importe libSystem et libiconv fournies par macOS. Le répertoire `notices` de l’archive contient les textes de licence des dépendances utilisées sur Darwin, des moteurs C, de Rust et des images QEMU. Le verrou couvre également les plateformes conditionnelles. [Inventaire des licences et versions](validation/licences.md) · [Arbre Cargo sur Darwin](validation/arbre-darwin.txt)

## Comparaison macOS du 16 novembre 2026

Le binaire testé vient de `ffe6b9f6004c8e8674ca75e353e356a634819fc7` (Rust Homebrew 1.100.0). L’oracle est le GNU grep 3.12 installé séparément dans `/opt/homebrew/opt/grep/libexec/gnubin/grep`. Chaque campagne enregistre la bannière complète, la locale et le résultat de `-Po '(?<=a)b'` sur `ab`. Dans les deux locales, GNU rend `b\n` et le statut 0.

| LC_ALL | Comparaisons | Statut, stdout et stderr identiques | Écarts |
|---|---:|---:|---|
| C | 1 663 | 1 659 | Texte sur stdout pour quatre invocations |
| en_US.UTF-8 | 1 663 | 1 659 | Mêmes quatre invocations |

`--help`, `-V`, `--version` et `--color=foo` (qui affiche l’aide) présentent les textes ARC-026. Leurs statuts et stderr coïncident avec GNU. La comparaison stricte conserve le statut 1 pour chacune des deux campagnes dès qu’un octet diffère. Chaque locale inclut 1 200 cas pseudoaléatoires de motifs/options sur des entrées ASCII, 300 sur des séquences d’octets comprenant NUL et UTF-8 malformé, et 163 témoins dirigés. La graine est 270312. La matrice recoupe les options avec le manuel Info livré par GNU grep 3.12 et associe chaque graphie à un résultat exécuté. [Résultats C](validation/C.json) · [Résultats UTF-8](validation/en_US.UTF-8.json)

Les témoins exercent notamment la priorité de `-q` en présence d’erreurs, les diagnostics de regex et d’opérandes invalides, les accès refusés, la récursivité et les liens, les fichiers binaires, stdin, FIFO, SIGPIPE, le vidage avant EOF et la position de stdin après `-m`. Le traitement de `-f` lisait tout le fichier et refusait une taille cumulée supérieure à 8 Mio. Il lit désormais les motifs ligne par ligne et conserve chaque motif de fichier distinct une fois, dans son ordre d’apparition. Un fichier de plus de 9 Mio avec un dernier motif sans LF donne le même résultat que GNU. Les 13 tests unitaires du workspace hôte passent.

## Longues lignes, mémoire et temps

Le critère retenu est le comportement GNU sur les longues lignes, avec mesure du maximum résident. Les dix essais de volume comparent la longueur et le SHA-256 de stdout et vérifient le statut attendu des deux processus. Ils couvrent notamment BRE, ERE, PCRE, `-z`, une absence, une ligne complète émise et la relecture du contexte après rotation du fichier temporaire. Les dix résultats coïncident. Les temps ci-dessous sont des mesures de processus, sur ce Mac, avec des entrées fictives et un cache chaud.

| Entrée | Option et motif | RSS maximale observée du candidat |
|---|---|---:|
| Ligne de 8 Mio suivie de Z | `-Ec 'x.*Z'` | 21,1 Mio |
| Ligne de 64 Mio suivie de Z | `-Ec 'x.*Z'` | 77,1 Mio |
| Ligne de 64 Mio suivie de Z | `-Fc Z` | 2,9 Mio |
| 64 Mio de lignes de 4 Kio puis FIN | `-B2300 FIN` | 11,1 Mio |

Sur 64 Mio de lignes courtes, les temps médians de trois essais de comptage sont respectivement de 0,07 s (candidat) et 0,02 s (GNU) en recherche fixe, 0,27 s et 0,05 s en ERE, 0,02 s et 0,02 s en PCRE. Les temps bruts portent la résolution de `/usr/bin/time -lp`. [Mesures détaillées](validation/performances.json) · [Longues lignes et contextes](validation/volumes.json)

Le comptage littéral applicable parcourt l’entrée par blocs de 64 Kio. Au-delà de 8 Mio, le chemin général transfère les données de ligne vers un fichier temporaire privé, puis donne aux moteurs la vue contiguë qu’ils exigent. Le contexte précédent dispose aussi d’une file temporaire privée. Le RSS regex peut donc augmenter avec une seule ligne, et l’espace temporaire avec les données à conserver. `-o` et la couleur peuvent conserver des positions proportionnelles au nombre de correspondances. La mémoire nécessaire à la compilation dépend du nombre et de la longueur des motifs distincts. Une borne RSS universelle et l’équivalence pour les autres locales ou tous les dialectes de regex restent à établir.

## NovaOS et arbitrage

Le paquet `grep-engine` utilise `core` et `alloc`. Il isole la recherche littérale en flux, les règles de sélection, la gestion des frontières et la fusion des résultats des motifs. Les fichiers objets ELF64 relogeables et les rlib de `aarch64-unknown-none-softfloat` et `x86_64-unknown-none` sont dans l’archive, avec les en-têtes inspectés et les journaux de construction. Le moteur requiert un allocateur à l’édition des liens. Les objets seuls n’ont pas de point d’entrée de processus.

La compilation de la commande complète échoue sur les deux cibles (E0463, `std` absent). L’adaptateur Darwin utilise `std::fs/io`, des parcours de répertoires, des fichiers temporaires privés et `mmap`. Les moteurs C dépendent de fonctions d’allocation et de libc. Le profil PCRE2 avec JIT dépend aussi de la gestion de mémoire exécutable. Il faut porter et qualifier les moteurs, notamment étudier PCRE2 sans JIT, et implémenter lecteur, émetteur et gestion des droits avec les interfaces utilisateur de NovaOS. Les journaux du contrôle de la pile complète et les tables de symboles des bibliothèques C figurent dans `construction/` de l’archive.

Le lanceur lie le rôle de pipe à une image admise. Les rôles présents sont Cat, Wc et Head. L’admission de grep, l’accès aux opérandes et le rôle de lecteur de pipe font l’objet d’une demande séparée. Les lectures P9, le passage natif de stdin à stdout, les contrôles de capacité du futur processus et ses statuts restent à qualifier sur l’ELF exécutable. [Demande de décision MOA/MOE](admission-pipe-MOA-MOE.md)

**Campagne initiale à la base 816843f.**

| Recette en invité | AArch64 | x86_64 |
|---|---|---|
| Documentation sur P9, secteurs de 512 octets | 9/9 | 9/9 |
| Documentation sur P9, secteurs de 4 096 octets | 9/9 | 9/9 |
| Recherche dans un ELF grep lancé sur P9 et via pipe | Admission et portage ouverts | Admission et portage ouverts |
| Régression générale des scripts sur P9, 512 | Échec sur le témoin de glob | Même échec |

Les 36 consultations ont été exécutées avec les images de `fa9a141ba3a0c5e201f8d6f730632233deb225f4`, 2 vCPU et 256 Mio. Le catalogue public de `nsh` fournit `grep --help`, `grep --version` et `man grep` en anglais pour `en-US`, en français pour `fr-CA` et en anglais pour la balise invalide `fr--CA`. Le statut vaut 0 pour chaque consultation. Après stabilisation du disque, l’empreinte du P9 jetable et les compteurs QMP d’octets écrits, d’écritures et de flush restent identiques entre les deux observations.

La suite `nsh-scripts-test` s’arrête sur `**/*.txt` avec capacité révoquée. L’échec est reproduit sur les deux architectures à la base de départ, avant les modifications de grep. La suite générale s’est arrêtée avant les secteurs de 4 096 octets. Au cours de la recette, le lanceur historique `cli-p9-test` a également démarré un invité sur un P8 jetable avant sa phase P9. C’est un écart à la consigne P9 exclusivement. La phase P9 de ce lanceur échoue sur `ls -latr --color /` en AArch64. L’attribution de cet échec reste ouverte. Les journaux sont archivés. Tous les disques utilisés étaient des témoins fictifs isolés.

## Requalification sur NovaOS v0.23.0 publié

La révision publiée de `origin/main` est `e5d7124a53e9b0d6a896b7001f717f655c371367`, arbre `e73ce9e2209449bda3e1e1475ac15053ac3e312e`. La branche ARC-027 a repris cette base au commit `42e413a7cd711549ff168dc58f65b8fbb77a2054`. Son diff par rapport à cette base est limité aux composants utilisateur. Le code du Mach-O et du moteur portable est identique à celui de la campagne macOS archivée. Le changement amont concerne la portée du jeton d’énumération et la validation de la révision d’un fichier en lecture groupée. Ces deux interfaces seront utilisées pour la récursivité et la lecture de gros fichiers. [Rapport de stabilisation publié](../../novaOS/docs/stabilisation-v0.23.x.md)

| Résultat après stabilisation | AArch64 / 512 | AArch64 / 4 096 | x86_64 / 512 | x86_64 / 4 096 |
|---|---:|---:|---:|---:|
| Consultations ARC-026, statuts et absence d’écriture P9 | 9/9 | 9/9 | 9/9 | 9/9 |
| Commandes du banc `nsh-scripts-test` et reprise après redémarrage | 31/31 | 31/31 | 31/31 | 31/31 |
| Liste paginée, lecture de 8 193 octets, pipe existant | 3/3 | 3/3 | 3/3 | 3/3 |

Les quatre profils passent le glob `**/*.txt` qui échouait à la base initiale. Le témoin `ls -latr --color /` rend 0, retrouve les 18 noms contrôlés et ordonne les deux dates UTC à 200000123 et 800000001 nanosecondes dans la même seconde. `cat grand` rend exactement 8 193 octets de contenu sans LF final. Le pipe admis `cat commun.nsh | wc -l` rend 26 et le statut 0. Les traces détaillent les neuf consultations documentaires par profil, soit trois commandes pour chacune des trois balises. Ces témoins couvrent les dépendances de grep avec des clients admis. La recherche dans un processus grep sur P9 attend encore son portage et son admission.

Le premier essai de lecture sans LF a révélé une erreur du banc. Celui-ci attendait un marqueur précédé d’un saut de ligne alors que l’octet 8 194 appartenait déjà au marqueur. Le banc corrigé vérifie les 8 193 octets attendus et le statut, puis le scénario a été rejoué dans les quatre profils. Les observations, consoles, empreintes du disque, manifestes des VM et contrôles de réception figurent dans la seconde archive. Les VM de cette requalification ont toutes monté des P9 jetables. [Banc ciblé](validation/services_p9.py) · [Preuves v0.23.0](validation/arc027-v0230.tar.xz) · [SHA-256](validation/arc027-v0230.sha256)

La demande séparée d’admission du lecteur de pipe a été transmise à la coordination MOA/MOE le 16 novembre 2026, avec le dossier publié à la révision `42e413a7cd71`. La décision d’intégration reste ouverte.

## Reproduction

Depuis la racine du worktree, avec la chaîne Rust du projet, PCRE2 8 bits et le GNU grep Homebrew indiqués ci-dessus, exécuter séparément les commandes suivantes. Les deux campagnes strictes écrivent leur rapport avant de rendre leur statut.

```sh
export NOVAOS_TMPDIR="/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/arc027-grep-target"
cd components/grep
cargo build --release --locked --bin grep
cargo test --locked --workspace
exe="$CARGO_TARGET_DIR/release/grep"
python3 -B validation/campagne.py "$exe" validation/C.json --locale C --fuzz 1200
python3 -B validation/campagne.py "$exe" validation/en_US.UTF-8.json --locale en_US.UTF-8 --fuzz 1200
python3 -B validation/matrice.py
python3 -B validation/mesures.py "$exe" validation/performances.json
python3 -B validation/volumes.py "$exe" validation/volumes.json
for cible in aarch64-unknown-none-softfloat x86_64-unknown-none
do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked -p grep-engine --target "$cible"
done
```

Pour les invités, utiliser le générateur P9 et le profil documentaire du dépôt dans le worktree isolé. Les répertoires de résultat doivent être neufs. Le test refuse un répertoire de résultat existant.

```sh
cd ../..
unset CARGO_TARGET_DIR
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
preuves="$(mktemp -d "$NOVAOS_TMPDIR/arc027-doc.XXXXXX")"
for arch in aarch64 x86_64
do
    make -C novaOS ARCH="$arch" kernel || exit 1
    for secteur in 512 4096
    do
        python3 -B components/grep/validation/documentation_p9.py "$arch" "$secteur" "$preuves/doc-$arch-$secteur" || exit 1
        python3 -B components/grep/validation/services_p9.py "$arch" "$secteur" "$preuves/lecture-$arch-$secteur" || exit 1
    done
done
```

L’archive contient le Mach-O exécutable `arc027/grep-darwin-arm64`, les objets et journaux de construction, les notices, les résultats différentiels et les images et traces des quatre invités. Son manifeste interne `arc027/SHA256SUMS` couvre chaque fichier de preuve. Depuis `components/grep/validation`, la commande `shasum -a 256 -c arc027-preuves.sha256` contrôle l’archive avant extraction. [Archive](validation/arc027-preuves.tar.xz) · [Empreinte SHA-256](validation/arc027-preuves.sha256)
