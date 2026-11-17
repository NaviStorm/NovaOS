# Reconstruire et vérifier tee

Depuis la racine du worktree, sur macOS ARM64, avec Apple clang, Rust stable et Python 3.11 ou supérieur. La campagne du 17 novembre 2026 emploie rustc 1.99.1. Les deux cibles Rust nues, les dépendances Cargo verrouillées, QEMU, son firmware EDK II, xorriso et les outils de construction habituels de NovaOS doivent être disponibles. Le binaire de référence est `/opt/homebrew/bin/gtee` 9.12.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/tee-arc027/"
mkdir -p "$TMPDIR"
L="$PWD/livraison-tee-0240"
"$L/reconstruire.sh" macOS
"$L/tester-options-tee.sh" macOS "${TMPDIR}macOS.json"
"$L/reconstruire.sh" aarch64
"$L/reconstruire.sh" x86_64
"$L/mesurer-performances-tee.sh" court "${TMPDIR}mesures.json"
```

Le profil complet utilise 1, 16, 64 et 256 Mio, une sortie puis quatre, un échauffement et sept mesures pour chaque outil. Prévoir environ 1,3 Gio de fichiers de travail pour la plus grande série.

```sh
"$L/mesurer-performances-tee.sh" complet "${TMPDIR}complet.json"
```

Pour les recettes invitées, fournir un dossier neuf par profil. L’entrée de test construit le noyau ordinaire, le témoin P9 et la recette SDK dans leurs contextes respectifs. Elle utilise la matrice hôte de la livraison comme référence pour la sélection des arguments du SDK. Les images temporaires sont des fichiers privés. Le chemin de sortie absolu conserve les journaux série, les ISO, l’image finale et les observations JSON.

```sh
serie=$(mktemp -d "${TMPDIR}campagne.XXXXXXXX")
for arch in aarch64 x86_64; do
    for secteur in 512 4096; do
        "$L/tester-options-tee.sh" "$arch" "$secteur" "$serie/$arch-$secteur" || exit 1
    done
done
```

Pour rejouer les suites officielles, utiliser le checkout et les objets Git gnulib indiqués dans les références. Les deux chemins ci-dessous doivent être absolus.

```sh
"$L/outils/amont.sh" "$NOVAOS_TMPDIR/sort-arc027-gnu" "$serie/amont"
```

Vérification des fichiers publiés, puis des octets de chaque membre de l’archive. La seconde lecture peut se faire sans extraire les images.

```sh
(cd "$L" && shasum -a 256 -c SHA256SUMS)
python3 - "$L/preuves-tee.tar.xz" <<'PY'
import hashlib, json, sys, tarfile
with tarfile.open(sys.argv[1], "r:xz") as archive:
    observed = {}
    manifest = None
    for entry in archive:
        with archive.extractfile(entry) as stream:
            if entry.name == "MANIFESTE.json":
                manifest = json.load(stream)
            else:
                assert entry.name not in observed
                observed[entry.name] = hashlib.file_digest(stream, "sha256").hexdigest()
    assert manifest == observed
    print(len(observed), "fichiers vérifiés")
PY
```
