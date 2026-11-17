# Rejouer dd pour la livraison v0.30.0

Depuis la racine du worktree sur un Mac ARM64, utiliser Rust stable avec les cibles aarch64-apple-darwin, aarch64-unknown-none-softfloat et x86_64-unknown-none. Pour les invités, installer les outils de construction de NovaOS, QEMU, xorriso et les firmwares utilisés par le dépôt. Les scripts utilisent Python 3.11 ou supérieur. L’environnement exact du passage publié figure dans [environnement.txt](preuves/environnement.txt).

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/dd-arc027/"
mkdir -p "$TMPDIR"
L="$PWD/livraison-dd-0240"
preuve=$(mktemp -d "$(cd .. && pwd)/preuves-dd-rejeu.XXXXXXXX")
for cible in macOS aarch64 x86_64; do
    "$L/reconstruire.sh" "$cible" || exit 1
done
"$L/tester-options-dd.sh" --rapide "$preuve/options-rapides.json.gz"
"$L/tester-options-dd.sh" --complet "$preuve/options.json.gz"
"$L/mesurer-performances-dd.sh" --rapide "$preuve/mesures-rapides.json"
"$L/mesurer-performances-dd.sh" --complet "$preuve/mesures.json"
```

Le mode rapide parcourt les options simples et mesure 4 Mio avec un échauffement et trois passages. Le mode complet ajoute les paires de conversions et les épreuves de flux, mesure 32 Mio avec un échauffement et cinq passages, puis 100 Mio en un seul bloc par passage. Pour l’oracle ou le binaire Darwin d’une nouvelle livraison, fournir les chemins absolus dans `GNU_DD` ou `NOVA_DD`. Le deuxième argument des deux scripts hôtes désigne le rapport à créer. Le résultat Darwin propre à un candidat peut être comparé au paquet de référence sans changer les octets de celui-ci. [Options](tester-options-dd.sh) · [Performances](mesurer-performances-dd.sh)

```sh
for arch in aarch64 x86_64; do
    for secteur in 512 4096; do
        "$L/tester-options-dd.sh" --invite "$arch" "$secteur" "$preuve/$arch-$secteur" || exit 1
    done
done
python3 -B "$L/matrice.py" "$preuve"
python3 -B "$L/regressions.py" "$preuve/regressions"
```

L’entrée `--invite` des deux scripts rejoue la même campagne et conserve ses temps bruts avec les contrôles d’intégrité. Chaque profil utilise une nouvelle racine de preuves, un volume P9 de fichiers synthétiques et une copie du système réservée à la construction de recette. Les validations du moteur utilisent la matrice Darwin **archivée** dans le paquet pour que les mêmes vecteurs restent comparables. Le banc interroge GNU installé pour les scénarios SDK et le témoin de conversion P9. Les commandes du lanceur ordinaire, les trames SDK et le moteur sous sélection P9 figurent dans des journaux distincts. La recette écrit uniquement dans ses images de test.

```sh
"$L/amont.sh" "$NOVAOS_TMPDIR/sort-arc027-gnu" "$preuve/amont"
(cd "$L" && shasum -a 256 -c SHA256SUMS)
python3 -B "$L/archiver.py" "$L/preuves-dd.tar.xz"
```

Le checkout Coreutils doit correspondre au tag v9.12 et son dépôt gnulib doit contenir le gitlink indiqué dans [les références](REFERENCES.md). Le script crée un chemin de recherche isolé où dd désigne successivement le Mach-O Nova et le binaire GNU. Une sortie 77 est conservée avec le motif donné par le test. La vérification d’archive relit et décompresse chaque membre, refuse les doublons, puis compare les SHA-256 au manifeste inclus.
