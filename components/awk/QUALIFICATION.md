# ARC-027 · Moteur AWK et qualification macOS ARM64

**Décision du 16 novembre 2026.** Le binaire Darwin et le cœur portable sont livrables pour essai. La concordance GNU reste partielle. L’admission du véritable ELF dans NovaOS et la validation des objectifs de performance restent ouvertes. [Étude de sélection](ETUDE.md) · [Proposition sur le pipe](PROPOSITION-PIPE.md)

## Construction et périmètre

Le moteur dérive de awk-rs 0.2.0, commit `5834140f2941ef7f565e6f342bd0be8c265b8207`, sous licence MIT ou Apache-2.0. Le paquet de développement NovaOS est en version 0.1.0. Le lexer, le parseur, l’AST, les valeurs, le moteur d’évaluation et le lecteur d’enregistrements utilisent `core` et `alloc`. L’adaptateur Darwin possède les fichiers, les flux, les processus, l’horloge et l’environnement. Le trait `Host` refuse par défaut les opérations qu’un adaptateur ne fournit pas. L’inventaire contient les versions exactes du verrou et les notices distribuées. GNU awk et ses programmes de régression restent des références externes. [Moteur](src/interpreter/mod.rs) · [Adaptateur](src/darwin.rs) · [Licences](artifacts/licences/inventaire.tsv)

Le binaire optimisé Mach-O ARM64 porte le SHA-256 `574e46d9198abc886169e964a4bb2666a1cbcb413be985a324d32012395fcbde`. Le cœur seul compile en mode optimisé sans fonctionnalités par défaut pour `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Ces constructions produisent des bibliothèques Rust. [Exécutable Darwin](artifacts/darwin-arm64/awk) · [Compilation AArch64](lab/results/coeur-aarch64-unknown-none-softfloat.txt) · [Compilation x86_64](lab/results/coeur-x86_64-unknown-none.txt)

## Résultats fonctionnels

| Campagne | Observation |
|---|---|
| Rust | 181 tests unitaires, deux tests d’intégration sur les pannes de flux et quatre exemples de documentation réussis. [Journal](lab/results/unitaires.txt) |
| Différentiel de développement | 61 cas nommés et 128 programmes synthétiques de graine 27. Les 189 cas ont les mêmes octets sur stdout, statut et empreintes de fichiers que GNU awk. 166 sont également identiques sur stderr. Les 23 autres ont un diagnostic différent. [Première campagne](lab/results/differentiel-initial.json) · [Détail final](lab/results/differentiel.json) |
| Suite indépendante GNU 5.4.1 | 13 programmes identifiés par empreinte. Neuf résultats identiques sur les quatre comparaisons. Trois diagnostics diffèrent sur stderr seulement. `gsubind` reste divergent sur le statut, stdout et stderr car la syntaxe des regex fortement typées et des appels indirects n’est pas implémentée. [Résultats](lab/results/suite-gnu.json) |
| Options | 97 invocations couvrent les 32 entrées du tableau d’options de la distribution de référence, leurs formes courtes/longues, les options conditionnelles et des formes supplémentaires. Les codes de retour et les fichiers sont enregistrés pour chaque invocation. [Matrice](OPTIONS.md) |
| Darwin, erreurs et autorités | Dix contrôles de processus réussis, dont le vrai `/bin/cat \| awk`, le lecteur de sortie fermé, SIGINT, un répertoire présenté en entrée, un fichier absent, le refus de taille de NF et le repli linguistique. [Journal](lab/results/es-darwin.json) |

Le différentiel exerce le pipeline d’expressions, les phases BEGIN, END, BEGINFILE et ENDFILE, les compteurs modifiables, les champs, les plages, la récursivité, les tableaux plats et imbriqués, les substitutions, les entrées binaires, les caractères UTF-8 et les séparateurs traversant une frontière de lecture. Les tableaux sans ordre de parcours contractuel sont aussi vérifiés par paires dans le banc. Le générateur borné couvre des expressions arithmétiques et relationnelles valides et des erreurs syntaxiques. Il ne couvre pas tout le langage.

La suite GNU a révélé que le parseur associait une affectation à toute une condition logique, que les noms de fonctions pouvaient être réemployés comme variables et que l’affectation d’un nombre à `$0` perdait son type. Le moteur valide désormais les cibles d’affectation, distingue les contextes tableau/scalaires exercés et conserve la valeur numérique pour appliquer OFMT à l’affichage et CONVFMT au découpage. Le cas `fmtmix` reproduit les changements de format en cours de programme.

## Sécurité et limites

Les tests Darwin exercent `getline` depuis un fichier et depuis une commande, les deux redirections de sortie, le pipe vers une commande, `close()` et `system()`. Le mode `-S` refuse les redirections et commandes externes et permet de lire le fichier d’entrée explicitement nommé. `--load` échoue avant exécution. Le cœur abandonne un enregistrement partiellement lu si le lecteur renvoie une erreur. Le test d’allocation force un refus d’au moins 1 Mio pendant l’agrandissement du tampon et vérifie le diagnostic ainsi que l’absence de sortie partielle. Une capacité de table de champs impossible renvoie aussi un échec explicite. [Essais du lecteur](tests/stream.rs)

Les limites restantes comprennent les syntaxes GNU indirectes, les profils traditionnels/POSIX demandés par option, le chargement dynamique et la couverture des calendriers et fuseaux. Aucune garantie globale d’épuisement contrôlé de toutes les allocations ni de budget d’un programme arbitraire n’est établie. La gestion des expressions régulières POSIX les plus longues est exercée pour `match` et `RS`. Les autres opérations regex nécessitent encore une campagne dédiée aux alternatives et aux captures. Les capacités NovaOS, leur révocation et les fichiers protégés restent à qualifier **avec awk**. La mesure de mémoire QEMU du futur processus awk et le maximum lisible par son chemin fichier restent inconnus.

## Mesures et reproduction

Apple M4 Max, Darwin ARM64, Rust 1.100.0, optimisation 3, LTO, une unité de génération, oracle GNU awk 5.4.1. Locale C, fichier temporaire APFS par charge, cache après génération et lecture SHA-256, un échauffement hors statistiques, trois mesures. Le débit de pipe inclut le producteur et l’attente des deux processus. Le RSS et les temps CPU de ce cas viennent de `wait4` sur le consommateur seul. Les seuils proposés à 2 Mio figurent dans l’étude et attendent une décision MOA/MOE. [Médianes, étendues, débits en enregistrements/s, CPU et RSS](lab/results/performances.tsv) · [Lignes longues, limites de bloc et pipe Darwin](lab/results/limites.json)

Le dernier corpus compte **4 294 967 306 octets et 33 554 433 enregistrements**. Le générateur place le premier octet de TEMOIN-4G à l’offset 4 294 967 296. Les deux exécutables l’ont retrouvé au dernier enregistrement sur les quatre passages. SHA-256 du corpus `f1bd9d0539b52b4eb29c31ed2175f5ca72d45c4341ed12fead09d15e89efe680`. Le comptage du candidat atteint 580,57 Mio/s (étendue 569,33–582,05), contre 1 694,05 pour l’oracle. Le pic du candidat reste à 2,52 Mio à l’arrondi entre 64 Mio, 1 Gio et le dernier palier. À 1 Gio, la somme atteint 478,0 Mio/s, le filtre rare 491,4, le filtre fréquent 448,3, les 16 clés fixes 224,6 et la sortie 356,8. Les sept médianes de sélection à 2 Mio dépassent les planchers proposés dans l’étude.

Sur 16 384, 524 288 puis 8 388 608 clés distinctes, le pic du candidat passe de 5,33 à 87,05 puis 1 352,97 Mio environ. Chaque résultat a été vérifié clé par clé. Une ligne de 1 Mio puis de 16 Mio porte le pic à 8,8 et 100,86 Mio. Le pipe Darwin de 64 Mio compte 524 288 enregistrements, avec un débit médian de 604 Mio/s de bout en bout. Ces mesures n’utilisent aucun disque NovaFS. [Mesures sur fichiers](lab/results/performances.tsv) · [Mesures de flux](lab/results/limites.json)

L’échantillonnage macOS du binaire livré montre que même le comptage passe par `split_fields` et l’allocation ou la libération de chaînes. L’agrégation ajoute l’évaluation des expressions et les opérations du BTreeMap. Le découpage systématique explique une partie de l’écart avec gawk sur END seul. Une future optimisation différée devra conserver les effets des affectations de FS, FPAT et FIELDWIDTHS. [Profil du comptage](lab/results/profil/comptage.txt) · [Profil de l’agrégation](lab/results/profil/tableau.txt)

Depuis la racine du worktree dédié, utiliser les variables temporaires définies dans AGENTS.md. Le premier argument du banc amont désigne le dossier `test` de l’archive officielle gawk 5.4.1. Le différentiel renvoie le statut 1 en présence de différences, y compris sur le seul texte de stderr. Le banc de performance crée exclusivement un nouveau fichier de résultats.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
cd components/awk
cargo test --locked --offline
cargo build --release --locked --offline
python3 -B lab/differential.py /opt/homebrew/bin/gawk artifacts/darwin-arm64/awk --report "$TMPDIR/differentiel.json"
python3 -B lab/amont.py "$TMPDIR/arc027-awk-study/gawk-5.4.1/test" /opt/homebrew/bin/gawk artifacts/darwin-arm64/awk "$TMPDIR/suite-gnu.json"
python3 -B lab/options.py /opt/homebrew/bin/gawk artifacts/darwin-arm64/awk "$TMPDIR/options.json"
python3 -B lab/darwin_io.py artifacts/darwin-arm64/awk "$TMPDIR/es.json"
python3 -B lab/profil.py artifacts/darwin-arm64/awk "$TMPDIR/profil-awk"
python3 -B lab/edges.py /opt/homebrew/bin/gawk artifacts/darwin-arm64/awk "$TMPDIR/limites.json"
for n in 2 64 1024 4096; do
    python3 -B lab/bench.py --mib "$n" --results "$TMPDIR/rejeu-$n.jsonl" gawk artifacts/darwin-arm64/awk
done
for n in 2 64 1024; do
    python3 -B lab/bench.py --unique --mib "$n" --results "$TMPDIR/rejeu-cles-$n.jsonl" gawk artifacts/darwin-arm64/awk
done
for t in aarch64-unknown-none-softfloat x86_64-unknown-none; do
    RUSTC="$(rustup which --toolchain stable rustc)" cargo build --locked --offline --release --lib --no-default-features --target "$t"
done
```

L’aide, la version et l’exemple `manpath` sélectionnent la langue avec le même code ARC-026. Depuis `components/awk`, une consultation du manuel avec la locale du processus se lance ainsi. Le MANPATH reste propre à cet appel. Pour le manuel seul, les répertoires `doc/man/fr` et `doc/man` sont aussi utilisables directement. [Manuel français vérifié](lab/results/man-fr.txt) · [Manuel anglais vérifié](lab/results/man-en.txt) · [Contrôle par étiquette](lab/results/man-locales.txt)

```sh
cargo build --locked --offline --example manpath
MANPATH="$(target/debug/examples/manpath)" man awk
```
