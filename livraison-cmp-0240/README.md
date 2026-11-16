# CMP natif 0.24.0 — livraison ARC-027 du 16 novembre 2026

Le Mach-O ARM64 et les deux ELF utilisent le même parseur et le même moteur Rust. Sur deux fichiers de 256 Kio dont seul le premier octet diffère, `cmp -l grand1 grand2` écrit exactement 15 octets sur stdout et zéro sur stderr. GNU Diffutils 3.12 termine avec 0. Le Mach-O Nova et le processus ELF Nova sur chacun des quatre profils de recette terminent avec **1**, conformément à l’arbitrage de Thierry. Les octets stdout sont `20 20 20 20 20 31 20 20 20 30 20 31 33 30 0a`. GNU initialise son indicateur de différence dans la boucle de lecture et consulte celui du dernier bloc au retour. Nova le conserve pendant toute la comparaison.

La qualification du chemin ordinaire reste ouverte. Le catalogue partagé de la base ne contient pas cmp. Sur les quatre profils, `cmp a.bin b.bin` rend 127 dans le shell et le pipe `cat a.bin | cmp -s b.bin -` est refusé à l’analyse. L’extension de recette permet au vrai `nsh` de lancer les ELF fournis avec deux objets P9 délégués en lecture. L’attribution définitive et la délégation simultanée du fichier et du pipe attendent un arbitrage commun. [Matrice par cas](MATRICE.md) · [Proposition d’admission](ADMISSION.md)

## Qualification

| Chemin | Résultat observé |
|---|---|
| Darwin ARM64, GNU et Nova | 467 cas par outil, dont 25 documentaires. Parmi les 442 autres, 441 correspondent octet par octet et statut compris. Le témoin arbitré diffère uniquement par le statut. |
| Darwin, processus et fichiers | Les deux PID du pipe sont attendus séparément. À égalité, cat et cmp terminent avec 0. À la première différence, cmp rend 1 et cat reçoit SIGPIPE. Une lecture de fd ouvert en écriture seule rend 2. L’aide, la version et l’option inconnue conservent la position de stdin. |
| SDK EL0 et Ring 3 | 455 vecteurs par ISA, avec contrôle des deux flux avant rendu et du statut du moteur. L’exécutable de test rend 0 après avoir vérifié toutes les attentes. |
| Quatre invités P9 ordinaires | 124 observations par profil. Aide, version, manuel et erreurs précoces testés dans le shell réel. Les compteurs QMP d’écriture et de FLUSH ainsi que l’empreinte du disque sont stables autour des appels documentaires et d’erreur. |
| Quatre invités de recette | 202 lancements réels par profil. Le banc corrèle numéro d’image, PID, octets sur les ports stdout et stderr avant rendu, statut du processus, collecte et résultat de PROCESS_WAIT. Les octets des deux fichiers du témoin sont relus indépendamment sur le volume initial et après la session. |
| Régressions système | Suites de référence sur les deux architectures terminées avec 0. Tests amont GNU exécutés sur chaque outil sur le Mac. Tests du catalogue documentaire sur Darwin ARM64 natif et x86_64 sous Rosetta. |

Les noms d’arguments, les entrées binaires, la graine 270240, les locales, les attentes et les sorties hexadécimales sont conservés. L’oracle utilise son propre texte pour l’aide et la version. La sortie documentaire Nova utilise le catalogue français pour la langue primaire `fr` d’une étiquette BCP 47 valide et l’anglais par défaut. Les deux noms d’invocation des tests sont `cmp`. [Résultats bruts](preuves/options.json) · [Flux Darwin](preuves/flux-darwin.json)

## Référence et sources

La branche part du SHA vérifié de `origin/main` `2e3188dee6eaef859becca3919bd0d53a20674bb`. Le commit `748d77f0a102e3d69a6349d581e9a70286e408ae` contient le moteur qualifié. Le composant porte la version 0.24.0 indépendamment du noyau de la base. Le protocole natif v1 compte 48 sélecteurs et les volumes utilisent P9.

Le véritable GNU cmp est fourni dans `reference-gnu/bin/cmp` avec son archive officielle et sa licence GPLv3+. Son SHA-256 est `e986a20d14dd933c057b9b80f7445ac9c23821812ff379258ce9304c021e01d8`. Les fichiers `src/cmp.c`, `tests/cmp` et `doc/diffutils.texi` correspondent au commit `16681a3cbcea47e82683c713b0dac7d59d85a6fa`. L’étude couvre le calcul des positions et des lignes après saut, la sortie EOF sur stderr, la taille de bloc, les raccourcis par identité et par longueur en mode silencieux, et la destination /dev/null. Les alias historiques `-c` et `--print-chars` sont reconnus. Les formes BSD `-h`, `-x` et `-z` sont rejetées par GNU. [Provenance et empreintes](preuves/provenance.json) · [Aide GNU capturée](preuves/aide-gnu.txt)

Le candidat uutils/diffutils 0.6.0 emploie std, les fichiers et les erreurs de l’hôte, avec des dépendances dont libc. Sur huit sondes discriminantes, six divergent de GNU, dont les alias, les abréviations et des arguments numériques. Le composant livré est une implémentation originale MIT. Le Mach-O utilise std et libSystem, les ELF utilisent alloc et le runtime natif de nova-cli. Les notices du runtime sont dans `licences/`. Les sources correspondantes de NovaFS embarquées dans les noyaux de recette sont dans `novaFS/` du monorepo, sous MPL-2.0. [Évaluation de la candidate](preuves/evaluation-uutils.json) · [Licences](licences/)

## Reproduire sur le Mac

Depuis la racine du worktree, avec Rust stable et les cibles aarch64-apple-darwin, aarch64-unknown-none-softfloat et x86_64-unknown-none installées, Python 3.11+ et le cache Cargo du projet disponible. Les archives QEMU nécessitent aussi les outils de démarrage habituels du dépôt.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
D=livraison-cmp-0240
"$D/reconstruire.sh"
"$D/tester-options-cmp.sh" --help
"$D/tester-options-cmp.sh" --large --novaOS --macOS --json "$TMPDIR/cmp-options.json"
python3 "$D/flux-darwin.py" --json "$TMPDIR/cmp-flux.json"
"$D/mesurer-performances-cmp.sh" --large --novaOS --macOS --json "$TMPDIR/cmp-performances.json"
```

L’option `--novaOS` sélectionne le Mach-O Nova et `--macOS` sélectionne GNU. Les deux sont exécutés par défaut. `--large` ajoute l’offset supérieur à 4 Gio aux tests d’options et les paliers de 100 Mio et 1 Gio aux mesures. Pour reconstruire le Mach-O de référence dans un préfixe privé, extraire `reference-gnu/diffutils-3.12.tar.xz`, puis exécuter `./configure --prefix="$TMPDIR/gnu-cmp-3.12" && make && make install` dans le répertoire extrait. `GNU_CMP` permet de sélectionner cette référence. Une extraction autonome de `sources-nova.tar.gz` permet de reconstruire les trois exécutables. Le Mach-O et l’ELF ARM64 sont identiques à la livraison à l’octet près. Une autre racine de construction donne un SHA différent pour l’ELF x86_64. Les essais directs utilisent l’empreinte du fichier effectivement livré. [Comparaison des reconstructions](preuves/reconstruction.json)

## Rejouer QEMU

Préparer l’outil NovaFS sur le Mac et les noyaux ordinaires. Chaque chemin de sortie doit être neuf. Les scripts conservent l’image réellement amorcée, le disque d’essai et la trace UART.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
make -C novaOS ARCH=aarch64 kernel
make -C novaOS ARCH=x86_64 kernel
mkdir -p novaOS/build
python3 "$D/invite.py" aarch64 512  "$TMPDIR/cmp-cli-a512"
python3 "$D/invite.py" aarch64 4096 "$TMPDIR/cmp-cli-a4096"
python3 "$D/invite.py" x86_64 512   "$TMPDIR/cmp-cli-x512"
python3 "$D/invite.py" x86_64 4096  "$TMPDIR/cmp-cli-x4096"
```

Les quatre commandes ci-dessous appliquent la proposition dans une extraction isolée. L’indice privé 240 exige le profil de qualification. Le processus reçoit uniquement deux ports de sortie et le groupe de fichiers sélectionné. Le banc relit la sortie complète de chaque processus par PID et enregistre séparément stdout et stderr.

```sh
"$D/recette.sh" aarch64 512  "$TMPDIR/cmp-direct-a512"
"$D/recette.sh" aarch64 4096 "$TMPDIR/cmp-direct-a4096"
"$D/recette.sh" x86_64 512   "$TMPDIR/cmp-direct-x512"
"$D/recette.sh" x86_64 4096  "$TMPDIR/cmp-direct-x4096"
```

Ajouter `--mesures` en dernier argument avec un répertoire neuf pour reproduire les mesures invitées. Pour le moteur seul et les grands volumes, utiliser les commandes suivantes.

```sh
python3 "$D/temoin-sdk.py" aarch64 "$TMPDIR/cmp-sdk-arm"
python3 "$D/temoin-sdk.py" x86_64 "$TMPDIR/cmp-sdk-x86"
python3 "$D/corpus-p9.py" 512  "$TMPDIR/cmp-grands-512" --large
python3 "$D/corpus-p9.py" 4096 "$TMPDIR/cmp-grands-4096" --large
for arch in aarch64 x86_64; do
  make -k -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test
done
```

## Mesures et limites

Sur le Mac, les deux fichiers sont écrits séparément avec la graine 270240. Les temps incluent la création et l’attente de cmp avec la même destination ordinaire, après échauffement. Les trois passages mesurés alternent l’ordre des outils. Le cache reste chaud. Le JSON conserve les valeurs individuelles, les pics RSS en octets, l’ordre des exécutions, le premier écart et la longueur du parcours auquel s’applique le débit. La comparaison d’un fichier avec lui-même et les différences précoces ont leur propre ligne de latence. [Tableau terminal des 30 mesures](preuves/tableau.txt) · [Mesures individuelles Darwin](preuves/performances.json)

Les invités ont 2 vCPU et 268 435 456 octets de RAM. Chaque paire P9 contient deux objets distincts. Les objets ont été intégralement vérifiés depuis le lecteur hôte jusqu’à 1 073 741 824 octets chacun. En recette, le parcours intégral chronométré atteint 1 Mio. Les profils ont également exercé deux objets de 100 Mio, de 256 Mio + 1 octet et de 1 Gio avec arrêt précoce, identité, limite et décalages. Le temps invité va de la soumission à nsh jusqu’au retour à l’invite, sur un passage instrumenté par cas, machines exécutées en parallèle. La lecture de fichiers de la base transfère de petits fragments par IPC. Les gros parcours intégraux invités et les performances du chemin ordinaire restent ouverts avec l’admission. Les archives `perf-*` contiennent les valeurs et les compteurs de pages. [Mesures des quatre profils](preuves/tableau-invites.txt)

```sh
python3 "$D/verifier-preuves.py"
python3 "$D/matrice.py"
```

Les archives `rejeu-*`, `statut-*`, `perf-*`, `qual-sdk-*` et `grands-*` ont chacune un manifeste par membre. `SHA256SUMS` couvre les exécutables, archives, scripts, licences et résultats de la livraison.
