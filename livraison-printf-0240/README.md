# Livraison ARC-027 · printf 0.24.0

Le 17 novembre 2026, le Mach-O ARM64 a satisfait les 1 698 cas de la recette macOS. Les deux ELF ont exécuté les 13 scénarios de processus sur chaque géométrie P9. Dans le `nsh` livré, `printf --help`, `printf --version` et `man printf` fonctionnent. Un appel ordinaire comme `printf %s Nova` rend encore **127**. Son exécution depuis le shell dépend du lanceur générique.

Branche `codex/printf-natif-0240`, issue de `origin/main` à `425e4e7cc5c03e246efb64d3638376906344bf47`. Le port et ses bancs sont au commit `4e6b0df`, le contrôle documentaire et P9 au commit `7e05799` et la reconstruction des trois exécutables au commit `bc41a369fb6251aeaf358f1aff6747550d1b070a`.

| Exécutable | Octets | SHA-256 |
|---|---:|---|
| [macOS ARM64, Mach-O](bin/printf) | 430 608 | `5b5136beb5e1507e6489a20fd87793bd6443f0b9bb449b3722d013fd378ccdb7` |
| [NovaOS AArch64, ELF statique](bin/printf-aarch64.elf) | 91 464 | `76d872b69ff7b57f6b5116a14c5dd0becf0368acef1414a392e796e35c2ac844` |
| [NovaOS x86_64, ELF statique](bin/printf-x86_64.elf) | 95 560 | `f472ba377470ed36914d27f3125c01260244c7723263de7df6df5e740c4695d2` |

## Contrat et références

Le moteur travaille sur des octets et envoie séparément sortie et diagnostics à l’adaptateur système. Il interprète les positions, les étoiles, les répétitions du format, les conversions, les citations shell et les échappements. Les conversions flottantes emploient des entiers de précision arbitraire pour arrondir en binaire64, comme l’oracle Darwin ARM64. Elles fonctionnent sur la cible AArch64 `softfloat`. Darwin fournit l’encodage et les conventions de la locale du processus. Les ELF utilisent les arguments de l’ABI v1 et les ports de console délégués. [Matrice commentée](MATRICE.md) · [Résultat de chaque cas par cible](matrice.tsv) · [Sources du composant](../components/printf)

La référence est GNU coreutils v9.12, commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`. L’étude a couvert `src/printf.c`, les conversions de nombres, les auxiliaires de citation, d’Unicode et de sortie, le manuel et les sept tests de `tests/printf/`. Le harnais et les auxiliaires gnulib ont été lus dans les objets du gitlink `106e9b2384d08a1696fcbd40cbab52237943f208`. L’oracle est l’exécutable externe `/opt/homebrew/bin/gprintf` 9.12, SHA-256 `772f2ba2bd1ce63e621c472171e87edd843d7782a218cb09f4c63cdb80277c68`. [Chemins, versions et empreintes des sources](preuves/provenance.json)

L’examen de `uu_printf` et de `uucore` à `4735bbe40d764a523c5ece37d47465b32452d1a2` a identifié des dépendances à la bibliothèque standard, à clap et à l’infrastructure de localisation, ainsi qu’une interprétation différente de `--help` accompagné d’un opérande. Le moteur est une implémentation Rust MIT indépendante. Il réutilise `num-bigint` en `no_std` et la bibliothèque `nova-cli`. GNU sert d’oracle et fournit les tests amont. [Notices des dépendances et de la référence](licences)

## Reconstruction et recette

Depuis le worktree dédié, utiliser un Mac ARM64 avec les outils de ligne de commande Apple, Python 3.11 ou supérieur, Perl, GNU coreutils 9.12 et les prérequis QEMU du projet (dont `xorriso` et les firmwares des deux machines). La recette publiée utilisait macOS 27.2.1 (26B131), rustc et Cargo 1.99.1, QEMU 11.1.3. Rust stable doit disposer des cibles `aarch64-apple-darwin`, `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Les builds utilisent les dépendances Cargo verrouillées en cache. Sur une nouvelle machine, les préparer avec `cargo fetch --locked --manifest-path components/printf/Cargo.toml` et préparer les dépendances NovaOS et NovaFS pour les essais en invité. [Prérequis NovaOS](../novaOS/README.md)

```sh
cd /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/printf-natif-0240
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/printf-0240/"
export GNU_COREUTILS_SRC="$NOVAOS_TMPDIR/sort-arc027-gnu"
mkdir -p "$TMPDIR"
liv=./livraison-printf-0240
"$liv/reconstruire.sh" macos
"$liv/tester-options-printf.sh" --sortie "$(mktemp -d "${TMPDIR}macos.XXXXXXXX")"
"$liv/reconstruire.sh"
campagne=$(mktemp -d "${TMPDIR}campagne.XXXXXXXX")
"$liv/tester-options-printf.sh" --toutes-cibles --sortie "$campagne/recette"
"$liv/mesurer-performances-printf.sh" --sortie "$campagne/court"
# Profil complet explicite, 25 passages pour 1, 4, 12, 24 et 30 arguments.
"$liv/mesurer-performances-printf.sh" --complet --sortie "$campagne/complet"
python3 -B "$liv/verifier-preuves.py"
```

Les trois exécutables ont été reconstruits avec un nouveau répertoire de compilation et comparés octet par octet. Lors des premiers essais, l’UUID Mach-O variait avec l’édition de liens et sa suppression empêchait le démarrage sur ce Mac. La reconstruction finale attribue un UUID dérivé du contenu, signe en ad hoc, vérifie la signature et exécute `--version` avant d’installer les fichiers construits. [Journal de reconstruction et contrôles](preuves/reconstruction.tar.xz)

## Résultats du 17 novembre 2026

| Profil | Essais et observations |
|---|---|
| Darwin ARM64 natif | 1 698 cas admis. Les deux flux et le statut sont capturés. Le nom d’invocation est harmonisé dans les diagnostics comparés. Aide et version sont contrôlées selon ARC-026. [Captures comparatives](preuves/differentiel.tar.xz) |
| Tests GNU, Darwin ARM64 | Six scripts réussis pour chacun des deux exécutables. Le septième, `printf-surprise.sh`, rend 77 pour les deux car le shell macOS ne fournit pas la limite de mémoire virtuelle exigée. EBADF, EPIPE et SIGPIPE sont exercés dans la recette différentielle. [Journaux amont](preuves/amont.tar.xz) |
| QEMU AArch64, P9/512 et P9/4096 | Pour chaque géométrie, 1 674 cas du moteur en EL0, 13 cas avec l’ELF livré et 13 observations dans le vrai `nsh`. [Images et traces AArch64](preuves/qemu-aarch64.tar.xz) |
| QEMU x86_64, P9/512 et P9/4096 | Pour chaque géométrie, 1 674 cas du moteur en Ring 3, 13 cas avec l’ELF livré et 13 observations dans le vrai `nsh`. [Images et traces x86_64](preuves/qemu-x86_64.tar.xz) |
| Régressions | `cli-test`, `cli-p9-test`, `nsh-test`, `nsh-scripts-test`, `nsh-terminal-test` réussis sur les deux ISA (10/10). Six tests du catalogue documentaire réussis en Darwin ARM64 natif et six en Darwin x86_64 sous Rosetta. [Journaux et médias](preuves/regressions.tar.xz) |

Le témoin SDK compare en mémoire les deux flux et le statut de chaque cas, y compris les octets NUL et les grands champs. Le contrôleur d’essai démarre aussi l’ELF de livraison dans un processus distinct, avec une trame argv et des ports de terminal attribués explicitement. Les cas réels couvrent entre autres un port absent et des octets refusés. La console de cette base transforme LF en CRLF et refuse NUL, TAB et un premier octet UTF-8 isolé. L’ELF rend alors 1 et signale l’erreur si stderr est disponible.

Dans une session ordinaire sur volume P9 synthétique, l’aide, la version et le manuel réussissent en français avec `LANG=fr-CA` et en anglais de repli avec `LANG=fr-zzzzz`. Le chemin documentaire lit le catalogue embarqué sans solliciter de capacité métier. L’empreinte du disque et les compteurs QMP d’écriture et de FLUSH sont identiques avant et après ces commandes. Les commandes ordinaires et l’appel par `/system/bin/printf` rendent 127. [Observations du shell et compteurs par profil](preuves/cibles.json)

La base lance une liste fermée de commandes. L’ELF reçoit des arguments UTF-8, utilise le point décimal et n’hérite pas de la locale de `nsh` par cette interface. Le témoin exerce les conventions C, anglaises et françaises par injection. À la livraison du lanceur générique, reprendre les appels par nom, chemin absolu et chemin relatif, la transmission de la locale, les statuts, les tubes et la redirection vers un flux d’octets avec NUL. La qualification présente couvre les invités QEMU et la console texte. UTM et le matériel physique restent hors de cette campagne. [Catalogue des commandes](../novaOS/abi/installation.rs) · [Lancement sur la base examinée](../novaOS/kernel/src/sched/process/ipc/spawn.rs)

## Performances

Médianes Darwin ARM64 du profil court, lancement du processus inclus. Les deux exécutables reçoivent les mêmes arguments et écrivent vers `/dev/null`. La recette valide préalablement les octets et le statut, effectue trois échauffements par exécutable, puis sept passages en alternant l’ordre. Les charges moyennes du Mac partagé étaient de 3,24 / 6,04 / 8,36 avant et après. Aucun processus QEMU, rustc, cargo ou xorriso détecté à ces deux relevés. [Mesures brutes, débits et pics RSS](preuves/performances.json) · [Tableau terminal](preuves/tableau.txt)

| Taille ou nombre | Traitement | Nova printf | GNU printf |
|---:|---|---:|---:|
| 1 argument, 1 octet | Appel court | 2,195 ms | 2,152 ms |
| 30 arguments, 1 470 octets | Texte | 2,186 ms | 2,167 ms |
| 30 arguments, 540 octets | Binaire | 2,068 ms | 2,130 ms |
| 30 arguments, 330 octets | Décimal | 2,166 ms | 2,177 ms |

Dans QEMU, chaque échantillon démarre un témoin qui effectue 128 appels du moteur sur un puits mémoire. Les valeurs sont les médianes de cinq échantillons après un échauffement, en µs par appel, avec démarrage du témoin amorti. GNU a été chronométré sur Darwin uniquement. Le banc invité relève cycles, fréquence et débits. Le pic RSS du processus invité y est indisponible. [Mesures par profil](preuves/cibles.json)

| Profil | Court | Texte | Binaire | Décimal |
|---|---:|---:|---:|---:|
| AArch64 P9/512 | 50,02 | 75,95 | 77,18 | 289,17 |
| AArch64 P9/4096 | 71,68 | 76,90 | 77,30 | 282,53 |
| x86_64 P9/512 | 55,67 | 65,65 | 90,84 | 287,00 |
| x86_64 P9/4096 | 59,25 | 69,68 | 87,62 | 280,18 |

Le [manifeste SHA-256](SHA256SUMS) couvre les fichiers de livraison. Chaque archive de preuves contient aussi son inventaire et les empreintes de ses originaux. [Script de vérification par relecture](verifier-preuves.py)
