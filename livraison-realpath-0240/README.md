# Nova realpath 0.24.0 — recette ARC-027 pour v0.30.0

| Binaire | Format et point d’entrée |
|---|---|
| [realpath](bin/realpath) | Mach-O macOS ARM64 |
| [realpath-aarch64.elf](bin/realpath-aarch64.elf) | ELF statique, processus utilisateur NovaOS AArch64 |
| [realpath-x86_64.elf](bin/realpath-x86_64.elf) | ELF statique, processus utilisateur NovaOS x86_64 |

Base distante vérifiée au départ `425e4e7cc5c03e246efb64d3638376906344bf47`. Branche `codex/realpath-natif-0240`. Sources de production et correctifs jusqu’à `3c079270d64180eb57177df5122e09a4d60ad2c4`. Binaires et bancs au commit `29e5f66`. Les versions désignent le composant et la base de recette. La décision d’installation dans v0.30.0 reste ouverte.

## Références et réalisation

L’oracle est GNU Coreutils 9.12 (Homebrew). Le banc emploie le lien Homebrew nommé `realpath` afin que le préfixe des diagnostics corresponde au nom de Nova. Le banc transmet le même `argv[0]` aux deux exécutables. Les chemins, arguments non UTF-8, stdout, stderr et statuts figurent dans les mesures brutes. L’aide, le manuel et la version Nova suivent le catalogue ARC-026. Le code produit est sous MIT. [Matrice par option](MATRICE.md) · [Références, versions et SHA-256](preuves/references.txt)

- GNU Coreutils `c0f8514d989184921d9b12a4d103a7b23abc5af8`, GPL-3.0-or-later. Étude du parseur, des références relatives, du manuel et des tests. [realpath.c](https://github.com/coreutils/coreutils/blob/v9.12/src/realpath.c) · [relpath.c](https://github.com/coreutils/coreutils/blob/v9.12/src/relpath.c) · [Manuel](https://github.com/coreutils/coreutils/blob/v9.12/doc/coreutils.texi) · [Suite amont](https://github.com/coreutils/coreutils/blob/v9.12/tests/misc/realpath.sh)
- gnulib `106e9b2384d08a1696fcbd40cbab52237943f208`, GPL-3.0-or-later pour la routine étudiée. Vérification de la dernière composante, des barres finales et du traitement des cycles. [canonicalize.c](https://github.com/coreutils/gnulib/blob/106e9b2384d08a1696fcbd40cbab52237943f208/lib/canonicalize.c)
- uutils `4735bbe40d764a523c5ece37d47465b32452d1a2`, MIT. Son interface dépend de clap, de std et des interfaces hôte de uucore. Le port utilise un moteur Rust indépendant en `core` et `alloc` avec une interface pour lire le cwd, suivre un lien et vérifier un objet. Le rendu des citations et l’outillage de recette sont adaptés du chantier MIT dirname `8b35a61`. Les sources GNU restent des références de comportement. [realpath uutils](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/realpath/src/realpath.rs) · [Moteur Nova](../components/realpath/src/path.rs)
- Cargo verrouille libc 0.2.191, dlmalloc 0.2.14, cfg-if 1.0.5 et unicode-width 0.2.2 (MIT ou Apache-2.0), spin 0.9.9 (MIT), et nova-cli local (MIT). Le build est hors ligne. Les dépendances Windows du verrou ne sont pas liées aux trois cibles.

## Rejouer

Depuis ce dossier dans le worktree, avec Rust stable, les trois cibles Rust installées, Homebrew Coreutils, Python 3.11 ou ultérieur et la chaîne QEMU du dépôt. Les tests créent uniquement des fixtures sous `OS-worktrees/tmp/realpath-arc027` et les répertoires de construction du worktree. Chaque campagne invitée affiche un nouveau chemin de preuves. Le profil rapide invité utilise AArch64/P9-512, le profil complet les quatre combinaisons.

```sh
./reconstruire.sh mac
./tester-options-realpath.sh --macos --rapide
./tester-options-realpath.sh --macos --complet
python3 -B lab/flux.py
./mesurer-performances-realpath.sh --macos --complet
./reconstruire.sh elf
./tester-options-realpath.sh --novaos --complet
./mesurer-performances-realpath.sh --novaos --rapide
```

Pour la suite GNU, fournir un checkout de Coreutils 9.12 et un dossier de sortie absolu à `lab/amont.sh`. L’outil extrait le harnais gnulib épinglé ou le télécharge si l’objet manque. Pour les régressions, définir `NOVAOS_TMPDIR` selon le monorepo et `TMPDIR="$NOVAOS_TMPDIR/realpath-arc027/"`, puis lancer `python3 -B lab/regressions.py "$TMPDIR/regressions"`. Les deux lanceurs publics acceptent `--help`.

## Résultats fonctionnels

**Darwin ARM64.** 221/221 cas réussis. La suite officielle `tests/misc/realpath.sh` termine avec 0 pour les deux exécutables. Les sorties d’un pipe fermé concordent avec GNU pour SIGPIPE par défaut (signal 13) et ignoré (statut 1 et diagnostic). L’erreur d’un nom UTF-8 redirigé provenait d’une initialisation de locale limitée au terminal. Dans un cycle de trois liens, l’ancienne détection immédiate choisissait un autre composant en mode -m. Les deux cas ont un test différentiel. [Observations](preuves/options.json) · [Pipe fermé](preuves/pipe.json)

**NovaOS.** Les deux architectures utilisent 2 vCPU et 256 Mio sous QEMU TCG 11.1.3, AArch64 virt/Cortex-A72/GICv2 et x86_64 Q35/CPU max. Les secteurs logiques et physiques ont la même taille, 512 ou 4096 octets. Une genèse synthétique est migrée en P9 sur un disque jetable par essai.

- **Moteur en processus invité.** Les 57 jeux d’arguments comprennent les 42 combinaisons des modes d’existence et de liens, les modes silencieux, les références relatives, les erreurs et le séparateur. La sonde appelle le parseur, la résolution et le calcul relatif du port. Son espace de noms en mémoire représente les fichiers, répertoires, liens et refus de la fixture Darwin. Les flux en mémoire sont comparés aux octets de l’oracle. Les liens synthétiques mesurent le moteur. Le volume P9 est attaché en lecture seule et son SHA-256 est contrôlé après arrêt.
- **ELF livré.** Huit invocations empruntent le profil SDK dans une copie isolée du contrôleur de test, avec les vrais argv et zéro port. Le noyau démarré contient les octets exacts de l’ELF livré. Les sorties tentées sur stdout et stderr obtiennent DENIED, puis chaque enfant termine avec le statut 1. Le noyau observe le port et la taille après la décision de la porte. Le volume conserve son empreinte.
- **Entrée ordinaire.** Le vrai `nsh` répond localement à `--help`, `--version` et `man realpath`, en français pour `fr-CA` et en anglais pour `fr--CA`. Empreinte du disque, écritures et FLUSH sont identiques avant et après ces consultations. Le catalogue de la base ne sait pas lancer le métier realpath, statut 127, y compris dans `nsh -c`. Le pipe `nsh -c 'realpath / | wc -l'` rend 3. La grammaire du pipe exige actuellement cat à gauche puis wc ou head. Les résultats du profil SDK ne confèrent aucune capacité à l’utilisateur du shell.
- **Régressions.** Les cinq tests du catalogue et des étiquettes passent en ARM64 natif et x86_64 sous Rosetta. Les neuf tests du registre ABI passent. Les suites nsh et scripts sont rejouées sur les deux géométries de chaque ISA. Portée de la recette ciblée sur la CLI et ses dépendances de construction. Le noyau, les pilotes, l’éditeur et le moteur de stockage conservent leur logique de production. [Résultats des régressions](preuves/regressions)

## Raccord nécessaire

P9 expose `File` et `Directory`. Il manque un type de lien persistant et une opération pour en lire la cible. La délégation CWD_VIEW autorise GETCWD. Elle ne donne pas de résolution arbitraire LOOKUP_AT depuis la racine de session. L’ELF accepte le calcul lexical absolu -ms et demande le cwd pour un opérande relatif. Une résolution dépendant de métadonnées indisponibles rend une erreur, y compris en -m physique. Les remontées lexicales s’arrêtent à la racine. Les consultations documentaires précèdent toute demande métier. [Types P9](../novaFS/src/fs.rs) · [Contrôle des délégations](../novaOS/user/novafs/files/delegated.rs)

Pour qualifier le métier depuis le vrai nsh, il faut une base intégrée avec installation et admission générique, un adaptateur au bloc enfant versionné et aux flux bruts (dont NUL), et un contrat de résolution en lecture dans la racine et le cwd délégués. La branche expérimentale du lanceur a son client NXEXEC01 et des ELF de laboratoire. Elle ne fait pas partie de l’ascendance du main retenu. La qualification de l’accès aux liens exige aussi une sémantique de liens arbitrée. Ce mandat utilise l’ABI v1 à 48 sélecteurs et le P9 existants.

## Mesures et intégrité

Les médianes Darwin incluent posix_spawn, résolution, émission vers /dev/null et wait4. Les deux binaires traitent les mêmes 512 chemins par lot avec LC_ALL=C. Neuf passages après échauffement alternent l’ordre des exécutables. Le maximum RSS par processus est exprimé en Kio. L’oracle des deux flux et du statut est vérifié avant chronométrage. Les campagnes chronométrées finales partagent le verrou ARC-027 sur cette machine. [Darwin, Nova et GNU côte à côte](preuves/darwin/tableau.md) · [Passages et environnement](preuves/darwin/mesures.json)

Sous QEMU, les mesures isolent le moteur dans le processus déjà démarré. L’horloge monotone hôte date deux marqueurs série encadrant les lots. Trois passages après échauffement, résultat vérifié pour chaque nom. La mémoire indiquée est la croissance du tas privé entre l’entrée de la sonde et la fin du passage. Elle exclut code, pile et mémoire de QEMU. Le coût complet d’un lancement métier et le coût de résolution réelle sur P9 restent ouverts. [Mesures invitées](preuves/tableau-qemu.md) · [Détails par profil](preuves/qemu.json)

```sh
shasum -a 256 -c SHA256SUMS
python3 -B lab/verifier.py
```

L’archive contient les séries brutes, les ELF, les noyaux relus depuis les ISO, les volumes et les mesures. Son manifeste est contrôlé sur les octets décompressés. [Archive de qualification](qualification.tar.xz) · [Manifeste interne](qualification.sha256)
