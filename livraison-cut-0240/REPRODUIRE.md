# Reconstruire et rejouer

La [qualification QEMU du candidat](preuves/qemu-candidat/RAPPORT.md) fournit les quatre exécutions de la sonde du prévol, leurs images et le vérificateur des empreintes.

La [campagne du créneau Darwin](preuves/creneau/RECETTE.md) contient les comparaisons publié, candidat et GNU, le contrôle indépendant par charge et le prévol des régressions invitées.

## Recette fonctionnelle et performance sur Mac ARM64

Dans le dossier `livraison-cut-0240`, chaque commande conserve le différentiel brut et les mesures dans un répertoire neuf. Les deux bancs utilisent par défaut le Mach-O Nova et `/opt/homebrew/bin/gcut` 9.12. Pour une future livraison, définir `CUT_NOVA_BIN` avec le chemin du Mach-O à tester et `CUT_NOVA_VERSION` avec la version attendue de ce **composant**. Chaque banc relève les identités réellement exécutées.

**Rapide** — 90 combinaisons générées par locale en plus des cas fixes, TSV de 10 000 lignes, ligne longue de 1 Mio.

```sh
p=$(mktemp -d ../../tmp/cut-rapide.XXXXXXXX) && ./tester-options-cut.sh --jsonl "$p/options.jsonl.gz" && ./mesurer-performances-cut.sh --json "$p/mesures.json"
```

**Complète** — 900 combinaisons générées par locale, TSV de 1, 10 et 65 millions de lignes, lignes longues de 1, 16 et 64 Mio.

```sh
p=$(mktemp -d ../../tmp/cut-complet.XXXXXXXX) && ./tester-options-cut.sh --large --jsonl "$p/options.jsonl.gz" && ./mesurer-performances-cut.sh --large --json "$p/mesures.json"
```

La suite commence par les trois modes simples, puis vérifie les huit octets de la projection sur fichier et sur stdin alimenté par un pipe Darwin. Sur l’entrée de 12 octets `41 09 42 09 43 0a 44 09 45 09 46 0a`, les deux programmes doivent rendre exactement `41 09 43 0a 44 09 46 0a`, stderr vide et le statut 0. Les cas différentiels suivants couvrent les plages, les délimiteurs, les combinaisons, les limites et les erreurs. La comparaison conserve stdout, stderr et le statut séparément. Le préfixe d’invocation des diagnostics peut différer. L’aide localisée, le nom et la version du composant sont comparés aux attentes de chaque produit. La numérotation de la matrice SDK historique se rapporte à sa capture d’origine.

Pour la performance, la projection de champs lit exactement le même TSV par opérande de fichier puis via le descripteur stdin redirigé depuis ce fichier. Chaque mesure inclut le lancement du processus et écrit vers `/dev/null`. Génération, calcul des oracles et contrôle des sorties précèdent le chronométrage. Le JSON donne la taille en octets, l’empreinte des entrées et sorties, la locale, l’échauffement, les trois passages alternés, la médiane, le débit en Mio/s et le pic RSS en octets pour chaque outil.

Le 17 novembre 2026, le Mach-O Nova 0.24.0 a réussi 383/383 cas sur Mac ARM64. Sur les 10 000 lignes, les médianes Nova/GNU sont de 3,133/3,533 ms par fichier et 3,133/3,506 ms via stdin. Le profil court inclut également le complément, la sélection UTF-8 et le premier délimiteur tardif. Les nouvelles mesures stdin aux trois grandes tailles restent à exécuter. Les campagnes archivées à `008d62b` et leurs sondes SDK restent disponibles. La qualification via le vrai `nsh` relève du lanceur commun. [Sorties fonctionnelles](preuves/recette-2026-11-17/options.jsonl.gz) · [Tableau court](preuves/recette-2026-11-17/tableau.txt) · [Mesures brutes](preuves/recette-2026-11-17/mesures.json)

## Reconstruction et essais historiques

Depuis la racine du worktree CUT sur Darwin ARM64, utiliser le toolchain Rust stable avec les cibles `aarch64-apple-darwin`, `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Les essais utilisent Python 3.11+, Git, Perl et GNU Coreutils 9.12. Les invités utilisent l’outillage QEMU, xorriso et le chargeur de démarrage de NovaOS. Les mesures lourdes sont explicites. Exécuter les profils séquentiellement et utiliser des répertoires neufs.

```sh
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
rejeu=$(mktemp -d "$TMPDIR/cut-rejeu.XXXXXXXX")
liv=./livraison-cut-0240
(cd "$liv" && shasum -a 256 -c SHA256SUMS)
"$liv/reconstruire.sh" macOS
"$liv/reconstruire.sh" aarch64
"$liv/reconstruire.sh" x86_64
"$liv/tester-options-cut.sh" --help
"$liv/tester-options-cut.sh" --macOS --large
"$liv/tester-options-cut.sh" --novaOS --large --jsonl "$rejeu/options.jsonl.gz"
export GNU_COREUTILS_SRC="$NOVAOS_TMPDIR/sort-arc027-gnu"
"$liv/tester-amont-cut.sh" --macOS
"$liv/tester-amont-cut.sh" --novaOS
"$liv/mesurer-performances-cut.sh" --help
"$liv/mesurer-performances-cut.sh" --macOS --novaOS --large --json "$rejeu/mesures.json"
```

La reconstruction remplace les fichiers du sous-dossier `bin` dans le worktree choisi. Pour reconstruire depuis l’archive autonome dans un dossier isolé avec un cache Cargo initialement vide, exécuter les commandes suivantes depuis la racine du dépôt. Le chemin du compilateur est fourni par rustup.

```sh
mkdir -p "$rejeu/source" "$rejeu/cargo" "$rejeu/tmp"
tar -xzf "$liv/sources-cut.tar.gz" -C "$rejeu/source"
(
  cd "$rejeu/source"
  export CARGO_HOME="$rejeu/cargo" NOVAOS_TMPDIR="$rejeu/tmp" TMPDIR="$rejeu/tmp/"
  for cible in macOS aarch64 x86_64; do ./livraison-cut-0240/reconstruire.sh "$cible"; done
)
```

Le script de mesures recherche les fichiers SORT/UNIQ par taille et vérifie leurs empreintes avant réutilisation. L’option `--corpus DOSSIER` impose un emplacement existant. Sans corpus disponible, le script reconstruit les données dans son dossier jetable à partir de la graine 27027 et vérifie les SHA des tailles de référence. Le choix `--large` exerce 1, 10 et 65 millions de lignes. Sans cette option, le jeu court permet un contrôle de routine.

## QEMU depuis le dépôt complet

Chaque première commande crée le volume P9 fictif et construit le profil `nsh` ainsi que l’outil de préparation NovaFS. Les quatre commandes suivantes utilisent la sonde et les vecteurs du différentiel archivés dans la livraison. Les noyaux SDK sont construits dans des répertoires cibles dédiés. Le profil de référence accepte le refus du lanceur comme témoin. Après admission de CUT, faire évoluer l’assertion de ce témoin et qualifier les deux ELF par le chemin métier.

```sh
"$liv/tester-invite-cut.sh" aarch64 512  "$rejeu/nsh-aarch64-512"
"$liv/tester-invite-cut.sh" aarch64 4096 "$rejeu/nsh-aarch64-4096"
"$liv/tester-invite-cut.sh" x86_64  512  "$rejeu/nsh-x86_64-512"
"$liv/tester-invite-cut.sh" x86_64  4096 "$rejeu/nsh-x86_64-4096"
python3 -B "$liv/lab/sonde.py" aarch64 512  "$rejeu/sdk-aarch64-512" --large
python3 -B "$liv/lab/sonde.py" aarch64 4096 "$rejeu/sdk-aarch64-4096" --large
python3 -B "$liv/lab/sonde.py" x86_64  512  "$rejeu/sdk-x86_64-512" --large
python3 -B "$liv/lab/sonde.py" x86_64  4096 "$rejeu/sdk-x86_64-4096" --large
```

## Relire les médias

Les médias archivés incluent ISO, noyaux extraits, volumes P9 de genèse et de session, ELF des sondes et vecteurs. Les membres identiques partagent leur contenu dans le tar. Pour relire et comparer chaque membre au manifeste depuis la racine du dépôt, utiliser ce contrôle après celui de SHA256SUMS.

```sh
python3 - <<'PY'
import hashlib, json, pathlib, tarfile
p = pathlib.Path('livraison-cut-0240/preuves')
manifest = json.loads((p / 'medias-manifeste.json').read_text())
with tarfile.open(p / 'medias-qemu.tar.gz') as archive:
    assert set(archive.getnames()) == set(manifest['membres_relus'])
    for name, entry in manifest['membres_relus'].items():
        with archive.extractfile(name) as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == entry['sha256']
print(len(manifest['membres_relus']), 'membres vérifiés')
PY
```

Pour une version ultérieure, le script de reconstruction lit la version de Cargo et la vérifie contre `components/cut/VERSION`. Le différentiel documentaire et le témoin invité lisent ce même fichier. Vérifier la version des sources GNU et la décision d’admission du nouveau catalogue lors de la recette v0.30.0.
