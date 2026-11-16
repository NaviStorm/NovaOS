# Reproduire ARC-027 TR

## Recette Darwin pour v0.30.0

Depuis le répertoire `livraison-tr-0240` du worktree de recette, chaque ligne exécute les deux bancs et conserve leurs rapports dans un nouveau dossier. La première lance le profil court. La seconde active les grands corpus.

~~~sh
r=$(mktemp -d ./tr-rapide.XXXXXX) && ./tester-options-tr.sh --rapport "$r/options.jsonl.gz" && ./mesurer-performances-tr.sh --rapport "$r/mesures.json"
r=$(mktemp -d ./tr-complet.XXXXXX) && ./tester-options-tr.sh --large --rapport "$r/options.jsonl.gz" && ./mesurer-performances-tr.sh --large --rapport "$r/mesures.json"
~~~

Le profil par défaut compare le Mach-O `bin/tr` au GNU `/opt/homebrew/bin/gtr`. Pour une nouvelle version, exporter `NOVA_TR_BIN` avec le chemin absolu du Mach-O à tester. `GNU_TR_BIN` permet de choisir un autre exécutable GNU de référence. Sa version figure dans les rapports. Les deux scripts acceptent `--help`.

Le différentiel vérifie la disponibilité de C et en_US.UTF-8. Il commence par vérifier que `abc-123` suivi de LF devient exactement `ABC-123` suivi de LF, avec un statut nul et stderr vide, pour les deux outils. Il enchaîne options simples, combinaisons, grammaire des ensembles, refus, flux binaires et cas à graine fixe. Chaque enregistrement conserve les deux sorties brutes en hexadécimal et les statuts. Pour l’aide et la version, le contenu attendu dépend du projet. Le seul préfixe de diagnostic neutralisé est le nom d’invocation GNU `gtr:`. Les valeurs brutes restent consultables.

Le banc court utilise 100 000 lignes de journal ASCII pour la traduction, 4 096 lignes répétées précédées de 1 Mio de `a` pour la suppression et la compression, puis 1 048 576 octets binaires pour les quatre traitements. Le profil complet reprend les quatre corpus historiques et ajoute la conversion de journaux ASCII à 1, 10 et 65 millions de lignes. Le plus grand journal utilise 3 380 000 000 octets. Les corpus sont générés un à un après vérification de l’espace disponible.

**Passage du 17 novembre 2026.** Le Mach-O de la livraison `61c8211` et GNU Coreutils 9.12 réussissent 1 014 comparaisons, 507 par locale. Huit mesures comparatives donnent 48 passages chronométrés après échauffement. Avec `-ds X a`, les deux outils ont produit 8 192 octets identiques pour 9 965 568 octets lus. Le tableau donne les médianes en secondes et le débit en Mio d’entrée par seconde. Le JSON conserve les empreintes, l’ordre des passages, les durées et le pic RSS en octets. [Résultats fonctionnels](preuves/recette-courte-2026-11-17/options.jsonl.gz) · [Tableau terminal](preuves/recette-courte-2026-11-17/tableau.txt) · [Mesures brutes](preuves/recette-courte-2026-11-17/mesures.json)

Le passage du 17 novembre porte sur le Mach-O Darwin ARM64. Les suites QEMU déjà archivées à `61c8211` couvrent 3 652 cas du moteur par profil dans la sonde SDK. Dans le vrai nsh, le lancement de production de `cat donnees.txt | tr 'a-z' 'A-Z'` attend le lanceur commun. Les journaux de grande taille ajoutés à la recette restent à chronométrer lors d’un passage complet.

## Reconstruction et suites amont

À la racine du worktree NovaOS, utiliser Rust stable avec les cibles `aarch64-apple-darwin`, `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Le comparatif Darwin nécessite la bouteille GNU indiquée dans [les références](REFERENCES.md). Le banc invité utilise Python 3, QEMU, le firmware EDK II du paquet QEMU, xorriso et les outils de construction NovaOS. Les noms de sorties de recette doivent être libres.

~~~sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
d="$PWD/livraison-tr-0240"
"$d/reconstruire.sh" macOS
"$d/tester-options-tr.sh" --macOS
"$d/tester-options-tr.sh" --novaOS --large --rapport "$TMPDIR/rejeu-tr-options.jsonl.gz"
"$d/reconstruire.sh" aarch64
"$d/reconstruire.sh" x86_64
"$d/mesurer-performances-tr.sh" --comparer --large --rapport "$TMPDIR/rejeu-tr-mesures.json"
~~~

Sur le Mac, `--macOS` sélectionne GNU et `--novaOS` sélectionne le Mach-O Nova. `--comparer` alterne les deux exécutables. Les trois passages suivent l’échauffement. Les flux synthétiques sont écrits une fois, puis relus sur le même APFS sans purge du cache. Les validations SHA-256 ont lieu avant le chronométrage, vers un lecteur séparé. Les mesures utilisent la même destination `/dev/null`. Le débit utilise les octets d’entrée. La sortie du filtre et le pic RSS du processus enfant sont consignés pour chaque mode.

Pour rejouer les tests amont, fournir le checkout Coreutils v9.12 avec la révision de gnulib référencée par ce tag et un dossier de résultats existant. Le lanceur exécute le Perl original, puis le script shell original avec son infrastructure gnulib. Sur ce Mac, le test shell saute son bloc conditionnel ISO-8859-1 après interrogation de `locale charmap`. Les locales du différentiel obligatoire sont C et en_US.UTF-8.

~~~sh
mkdir "$TMPDIR/rejeu-tr-amont"
"$d/outils/amont.sh" "$NOVAOS_TMPDIR/sort-arc027-gnu" "$TMPDIR/rejeu-tr-amont"
python3 -B "$d/outils/flux.py" "$TMPDIR/rejeu-tr-flux.json"
python3 -B "$d/outils/pipe_mac.py" "$TMPDIR/rejeu-tr-pipe-mac.json"
~~~

## Invités QEMU indépendants

Chaque invocation construit le noyau ordinaire, crée un nouveau volume P9 et démarre le vrai nsh. La sonde du SDK démarre ensuite dans sa propre ISO avec le volume attaché en lecture seule. `--large` exerce les quatre jeux de données, dont 521 048 576 octets, et les quatre traitements. Le chemin en mémoire du témoin utilise des blocs de 4 096 octets, un générateur déterministe et un consommateur qui compte les octets produits. Le cas multi-trame du fichier P9 destiné au pipeline contient 4 096 espaces consécutifs. Les résultats attendus des grands jeux proviennent du comptage hors chronométrage de GNU.

~~~sh
python3 -B "$d/outils/invite.py" aarch64 512  "$TMPDIR/rejeu-tr-aarch64-512"  --large
python3 -B "$d/outils/invite.py" aarch64 4096 "$TMPDIR/rejeu-tr-aarch64-4096" --large
python3 -B "$d/outils/invite.py" x86_64  512  "$TMPDIR/rejeu-tr-x86_64-512"   --large
python3 -B "$d/outils/invite.py" x86_64  4096 "$TMPDIR/rejeu-tr-x86_64-4096"  --large
~~~

Les configurations, ISO, noyaux extraits, empreintes des ELF retrouvés dans ces noyaux, disques, journaux série, statuts et mesures se trouvent dans les dossiers de sortie. Le contrôle d’admission attendu correspond à la base du mandat. Une évolution du catalogue en v0.30.0 doit remplacer l’attente du refus par la recette de production de l’[arbitrage du pipe](ARBITRAGE-PIPE.md). Les options, le moteur et les générateurs de corpus se rejouent indépendamment.

Depuis la racine de la livraison publiée, vérifier `shasum -a 256 -c SHA256SUMS`. Chaque archive de session porte son propre manifeste de membres. L’archive des sources contient les composants, leur licence, les fichiers de l’ABI et du runtime, ainsi que les outils de reproduction au commit de code. Le monorepo à ce même commit fournit les autres sources de la construction système.
