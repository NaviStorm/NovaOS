# sort 0.1.0 · NovaOS ARC-027

La livraison contient un Mach-O ARM64 pour macOS et deux ELF natifs NovaOS. Les trois exécutables utilisent le même moteur de comparaison et de fusion. Le référentiel est GNU coreutils 9.12. Le tri externe est qualifié au-delà de la RAM sur macOS. Sur NovaOS, le calcul, les petits fichiers P9, la récupération des temporaires et les limites du lanceur sont documentés séparément. [Mesures et décision](RECETTE.md) · [Matrice des options](MATRICE.md) · [Étude des sources](ETUDE.md) · [Admission dans nsh](ADMISSION.md)

| Système | Exécutable | Profil de validation |
|---|---|---|
| macOS ARM64 | [sort](artifacts/darwin-arm64/sort) | Darwin natif, GNU 9.12, APFS, C et locales UTF-8 |
| NovaOS AArch64 | [sort](artifacts/aarch64/sort) | QEMU, EL0, 2 vCPU, 256 Mio, P9, secteurs 512 et 4 096 |
| NovaOS x86_64 | [sort](artifacts/x86_64/sort) | QEMU, Ring 3, 2 vCPU, 256 Mio, P9, secteurs 512 et 4 096 |

## Rejouer sur Mac

Depuis la racine du monorepo, avec un répertoire de travail existant. Les deux scripts acceptent `--help`. Le comparatif utilise `/opt/homebrew/bin/gsort`. Le vérificateur Python demande Python 3.11 ou ultérieur. Le manuel local se consulte avec `man -M components/sort/doc/man sort` ou `man -M components/sort/doc/man/fr sort`.

```sh
export TMPDIR="/chemin/vers/espace-de-travail"
export NOVAOS_TMPDIR="$TMPDIR"
cd components/sort
./tester-options-sort.sh --engine both --work "$TMPDIR" --output "$TMPDIR/options-sort.json"
./mesurer-performances-sort.sh --engine both --work "$TMPDIR"
./mesurer-performances-sort.sh --engine both --mib 1024 --budget 8M --batch 8 --repeat 3 --work "$TMPDIR"
./mesurer-performances-sort.sh --engine both --mib 163840 --budget 2G --batch 128 --workers 1 --repeat 1 --stream --above-ram --work "$TMPDIR" --output "$TMPDIR/grand-sort.json"
```

Le dernier essai correspond à 160 Gio pour 128 Gio de RAM physique. Le script vérifie la RAM et l’espace réellement disponible avant création. Le générateur demande trois fois la taille du fichier plus 16 Gio de marge. Avec un corpus existant, `--manifest` vérifie sa taille et son empreinte avant mesure. Le chemin relatif du manifeste historique se résout depuis la racine du worktree d’origine. Pour régénérer ailleurs, utiliser `--mib` et la graine 27 intégrée. Options de charge supplémentaires : `--distribution random|sorted|reverse|duplicates`, `--engine gnu|nova|both`, `--candidate CHEMIN`. Le comportement par défaut est de 8 Mio et trois passages.

## Reconstruire et vérifier

Depuis la racine du monorepo, avec les trois cibles Rust installées, les dépendances du verrou Cargo et les outils de construction NovaOS. Les ELF utilisent le script de liens du projet. `build.sh` imprime le chemin du résultat. Le répertoire d’artefacts présente les exécutables effectivement testés.

```sh
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
components/sort/build.sh macos
components/sort/build.sh aarch64
components/sort/build.sh x86_64
(cd components/sort && shasum -a 256 -c SHA256SUMS)
components/sort/lab/amont.sh "$SOURCE_GNU_912" components/sort/artifacts/darwin-arm64/sort
python3 -B components/sort/lab/suites-shell.py "$SOURCE_GNU_912" --output "$TMPDIR/gnu-shell.json"
```

Le dernier banc lit le commit gnulib indiqué dans l’étude depuis le sous-dépôt `$SOURCE_GNU_912/gnulib`. Les suites GNU restent dans leur checkout GPL. Les adaptations uutils conservent la licence MIT. L’archive des licences Cargo contient les avis des dépendances des trois cibles et leur provenance. [Licence du composant](LICENSE) · [Avis uutils](LICENSE-uutils) · [Avis Cargo](artifacts/licences-cargo.tar.gz)

## Rejouer dans QEMU

La construction du frontal hôte NovaFS doit précéder la préparation des fixtures. Une invocation crée un dossier de preuves neuf. Le témoin SDK embarqué exécute les vecteurs GNU dans un processus utilisateur. Le banc de volume crée un clone APFS jetable par combinaison de secteur et d’ISA.

```sh
export NOVAOS_TMPDIR="$TMPDIR"
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --manifest-path novaFS/Cargo.toml --release --locked --target aarch64-apple-darwin
for arch in aarch64 x86_64; do
    python3 -B components/sort/lab/guest.py "$arch" "$TMPDIR/sdk-sort-$arch"
done
components/sort/lab/preparer-p9.sh "$TMPDIR/corpus-320m.raw" 512
for arch in aarch64 x86_64; do
    for secteur in 512 4096; do
        python3 -B components/sort/lab/guest_volume.py "$TMPDIR/corpus-320m.raw" "$TMPDIR/grand-sort-$arch-$secteur" "$arch" "$secteur"
    done
done
```

Le contrôle de bout en bout du producteur de pipe utilise un **checkout isolé** du catalogue récepteur. Les exécutables épinglés dans ce checkout sont remplacés exclusivement pour la recette. Les résultats sous `results/qemu` contiennent les SHA des ISO, les lignes de commande QEMU et les transcriptions UART.

```sh
recv="$TMPDIR/recepteur-sort"
git worktree add --detach "$recv" 6b94f2f6349add9fc6ac0bc8276ab1b8747a5e36
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --manifest-path "$recv/novaFS/Cargo.toml" --release --locked --target aarch64-apple-darwin
for arch in aarch64 x86_64; do
    cp "components/sort/artifacts/$arch/sort" "$recv/livraison-uniq-0240/bin/sort-$arch.elf"
    make -C "$recv/novaOS" ARCH="$arch" kernel
    for secteur in 512 4096; do
        python3 -B components/sort/lab/pipe_recepteur.py "$recv" "$arch" "$secteur" "$TMPDIR/pipe-sort-$arch-$secteur" --rows 8192
    done
done
```

Dans `nsh`, `sort --version`, `sort --help` et `man sort` fonctionnent sur la branche dédiée. Dans le récepteur, `sort donnees | uniq -c` lit le fichier P9 sélectionné. La [fiche d’admission](ADMISSION.md) précise le statut des commandes directes et des deux autres paires de pipe.

Pour rejouer les pannes sur une image neuve du catalogue récepteur, depuis la racine du monorepo :

```sh
outil=$(components/sort/lab/outil-p9.sh)
arch=aarch64
secteur=512
python3 -B components/sort/lab/espace_invite.py "$recv" "$arch" "$secteur" "$TMPDIR/sort-espace" "$outil" --blocs 40
python3 -B components/sort/lab/coupure_invite.py "$recv" "$arch" "$secteur" "$TMPDIR/sort-coupure" "$outil" --parcours 45
python3 -B components/sort/lab/fusion_interrompue.py "$recv" "$arch" "$secteur" "$TMPDIR/sort-fusion"
python3 -B components/sort/lab/reprise_p9.py "$recv" "$TMPDIR/sort-fusion" "$outil"
```

La copie locale de livraison comprend aussi `images-qemu.tar.gz`. Les empreintes individuelles des médias permettent de contrôler les images et ELF témoins après extraction. [Inventaire des médias QEMU](results/qemu/IMAGES-SHA256)
