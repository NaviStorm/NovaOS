# Reproduire ARC-027 TR

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

Sur le Mac, `--macOS` sélectionne GNU et `--novaOS` sélectionne le Mach-O Nova. `--comparer` alterne les deux exécutables. Sans `--large`, le banc de performances utilise un million de lignes. Les trois passages suivent l’échauffement. Les flux synthétiques sont écrits une fois, puis relus sur le même APFS sans purge du cache. Les validations SHA-256 ont lieu avant le chronométrage, vers un lecteur séparé. Les mesures utilisent la même destination `/dev/null`. Le débit utilise les octets d’entrée. La sortie du filtre et le pic RSS du processus enfant sont consignés pour chaque mode.

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
