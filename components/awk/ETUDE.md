# ARC-027 · Choix du moteur awk, 16 novembre 2026

Base vérifiée sur le serveur `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`. L’adaptation porte sur **awk-rs 0.2.0**, au commit étudié ci-dessous. Il fournit un lexer, un parseur, un AST, des fonctions utilisateur, des tableaux et un interpréteur complet à étendre. Ses six charges à état borné et l’agrégation à 16 384 clés produisent les résultats attendus. Son implémentation de BEGINFILE, ENDFILE, FPAT, FIELDWIDTHS, patsplit et gensub réduit le travail de compatibilité GNU. Le choix engage un port de son entrée/sortie, de sa représentation des chaînes et de ses dépendances vers le SDK. La qualification est ouverte.

| Candidat | Référence examinée et licence | Constat et coût du port |
|---|---|---|
| [GNU awk](https://ftp.gnu.org/gnu/gawk/gawk-5.4.1.tar.xz) | 5.4.1, archive officielle SHA-256 `07f6f7342b7febe4313fc2c2542ad93d64fe20ad8717200109f105a826f5fd37`, identique à la formule Homebrew. La requête Git Savannah a rendu HTTP 502. GPL-3.0-or-later. | Oracle externe ARM64, MPFR 4.2.2, GMP 6.3.0. Implémentation GNU de référence en C et interfaces hôte, libc, locale et extensions dynamiques. La distribution étudiée reste extérieure au dépôt. |
| [One True Awk](https://github.com/onetrueawk/awk/tree/72e9b56caddb4a6f8d1405315a5183c26b8650a4) | 20261106, `72e9b56caddb4a6f8d1405315a5183c26b8650a4`. Licence permissive Lucent de 1997 avec clause de publicité. | Moteur historique actif. Grammaire Bison, regex interne, libc/libm. Il faudrait remplacer les FILE*, signaux, environnement, pipes et appels système, puis étendre le modèle POSIX. |
| [GoAWK](https://github.com/benhoyt/goawk/tree/ed9c27fd8523fa917a4c69f381ab471dd83f8c22) | v1.32.0, `ed9c27fd8523fa917a4c69f381ab471dd83f8c22`. MIT, bibliothèque standard Go sous licence BSD avec notices tierces, aucun module Go externe dans go.mod. | VM POSIX maintenue, CSV, E/S configurables via fs.FS. Excellent débit. Port du runtime Go, de la mémoire et de os/exec nécessaire avant la qualification native. |
| [uutils awk](https://github.com/uutils/awk/tree/1634d71d6fd2e1c30c0927732e48d10f03026da3) | 0.1.0, `1634d71d6fd2e1c30c0927732e48d10f03026da3`. MIT ou Apache-2.0, MinRX BSD-2-Clause. | VM GNU en développement. Lecture séquentielle rapide. La somme avec -F et l’agrégation échouent au contrôle. read_record_regex et read_record_blank_lines sont inachevées. Dépendances std, chemins natifs et entrées/sorties à isoler. |
| [eawk](https://github.com/awklib/eawk/tree/5f28fc86b3ce398a0a70d07c3c77b84ed4600ba5) | 0.1.1, `5f28fc86b3ce398a0a70d07c3c77b84ed4600ba5` inscrit dans .cargo_vcs_info.json. Apache-2.0. | Jeune bibliothèque embarquable, regex POSIX interne et aucune dépendance Cargo. Les fichiers, commandes et chaînes UTF-8 passent par std. Filtre rare à 6,84 Mio/s sur ce corpus. |
| [awk-rs](https://github.com/quinnjr/rawk/tree/5834140f2941ef7f565e6f342bd0be8c265b8207) | 0.2.0, `5834140f2941ef7f565e6f342bd0be8c265b8207`. MIT ou Apache-2.0. | Petit moteur Rust maintenu en 2026. Deux dépendances directes, regex et thiserror. Les lectures par String, l’ordre de préférence des alternatives regex et les appels directs au shell requièrent des essais distincts. |

Les inventaires Cargo dans [lab/results](lab/results) donnent les versions et licences déclarées des dépendances transitives normales et de construction, toutes plateformes du verrou. Les sondes `cargo check --lib --target` du moteur sélectionné échouent pour `aarch64-unknown-none-softfloat` et `x86_64-unknown-none` dès les dépendances configurées pour std. Le SDK fournit core et alloc. Le lanceur de cette base n’admet comme lecteurs de pipe que les commandes qualifiées wc et head. Une admission de awk doit faire l’objet d’un arbitrage distinct. [SDK](../../novaOS/docs/sdk-rust-v0.20.0.md) · [Contrat du pipe](../../novaOS/docs/pipe-head-v0.23.x.md)

## Première comparaison

Apple M4 Max, Darwin ARM64 27.2.0, locale C, cache non purgé, fichiers lus immédiatement après génération et hachage. Apple clang 21.0.0 -O2, Go 1.27.4 go build, Rust 1.100.0 cargo build --locked --release, eawk --offline. Le binaire de l’oracle est celui de Homebrew. Chaque charge comporte un échauffement et trois passages avec stdout vers un fichier temporaire APFS. Le banc compare chaque sortie au générateur, accepte les permutations des tableaux et rejette doublons ou omissions. wait4 donne les temps CPU et le pic résident du seul processus fils en octets. Le SHA de chaque exécutable, celui du corpus, les débits en enregistrements/s et les temps réels figurent dans les JSONL. [Médianes, étendues et RSS des six exécutables](lab/results/selection.tsv) · [Banc et générateur](lab/bench.py)

| Charge | gawk, médiane Mio/s | awk-rs, médiane Mio/s | Plancher proposé Mio/s |
|---|---:|---:|---:|
| Comptage | 263,91 | 287,63 | 250 |
| Somme | 225,65 | 286,53 | 250 |
| Regex rare | 153,21 | 272,26 | 230 |
| Regex fréquente | 142,47 | 257,68 | 220 |
| 16 clés fixes | 211,43 | 173,68 | 150 |
| 16 384 clés | 133,92 | 47,79 | 40 |
| Sortie | 214,99 | 62,72 | 55 |

Proposition MOA/MOE pour le même profil de sélection à 2 Mio, environ 10 à 17 % sous le minimum mesuré pour absorber la dispersion et le coût du lancement. Étendre ces seuils à d’autres tailles demande de mesurer leur profil. La correction des sorties et le plateau mémoire à état fixe conditionnent toute qualification de débit.

Depuis le worktree, après compilation des références ci-dessus et définition de NOVAOS_TMPDIR/TMPDIR suivant AGENTS.md, lancer `python3 -B components/awk/lab/bench.py --mib 2 --results selection.jsonl gawk CHEMIN_BWK CHEMIN_GOAWK CHEMIN_AWK_RS CHEMIN_UUTILS CHEMIN_EAWK`. Rejouer avec `--unique` et un nouveau fichier de résultats. Les grands paliers s’obtiennent avec `--mib 64`, `--mib 1024` et `--mib 4096 --count-only`. Le dernier ajoute un témoin après les premiers 4 Gio.
