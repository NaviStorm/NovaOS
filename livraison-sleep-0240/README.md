# sleep — ARC-027

Branche `codex/sleep-natif-0240`, établie le 17 novembre 2026 depuis `origin/main` à `425e4e7cc5c03e246efb64d3638376906344bf47`. Le dépôt principal à `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e` en est un ancêtre. Le composant CLI annonce `0.20.0`. La recette vise l’intégration ultérieure en v0.30.0.

| Exécutable | Plateforme | SHA-256 |
|---|---|---|
| [sleep](bin/sleep) | Mach-O macOS ARM64 | `0ad7f7e566144470c7b441012d4a4ed7bef0781491bdf813ae385c3e7d90a518` |
| [sleep-aarch64.elf](bin/sleep-aarch64.elf) | ELF natif NovaOS AArch64 | `a5af95cfb577db9bdfc668eea3905f291c3954b509fa3365c3d6c76d13716506` |
| [sleep-x86_64.elf](bin/sleep-x86_64.elf) | ELF natif NovaOS x86_64 | `8ffb444415bcdfac6ed3c096351c8eb1b4f4ccca3f4e6aefc79e3896493d01de` |

## Comportement et intégration

Le calcul est partagé entre macOS et NovaOS. L’analyseur choisit l’action documentaire avant la conversion des durées. Il additionne les valeurs dans l’ordre des opérandes, effectue les arrondis vers le haut et traite séparément le dépassement de la plage temporelle. L’adaptateur Darwin emprunte le séparateur décimal de la libc et attend dans le thread. L’adaptateur NovaOS crée un endpoint privé sans délégué, bloque dans `IPC_RECEIVE`, puis le ferme. L’ABI limite le délai d’une réception à 15 000 ticks. Chaque tranche réserve un tick pour la phase entre deux interruptions. Une durée nulle ne crée aucun endpoint. La mort du processus fait retirer le canal et l’attente par le noyau.

Le vrai `nsh` rend l’aide, la version et le manuel dans la langue demandée. Sur la base distante du 17 novembre, la branche de raccord `codex/lancement-generique-elf` est à `f8966d38fa5ef021e4b2354780e0a0c4dd69f946`. La commande ordinaire par nom ou chemin rend 127 et le pipeline `sleep 0 | head -n 1` rend 3 sur cette base. Le prérequis est le raccord entre la résolution de nsh, l’installation d’un ELF en P9 et le service public d’admission, avec remise des arguments et du droit de collecte. Au démarrage de ce mandat, la branche indépendante du lanceur disposait de mécanismes internes et d’un contrat SPAWN étendu. Ces travaux sont absents de l’ascendance de la base vérifiée. L’ABI v1 conserve ses 48 sélecteurs, NovaFS P9 son format et les services leur code. Les essais de durées en invité utilisent le profil SDK avec le véritable ELF de livraison et des volumes P9 exclusivement jetables.

Les trois binaires reconstruits dans un répertoire de compilation neuf sont identiques octet pour octet. Le moteur est à `9a4096a1347376c8c4c3bb0669336d5e73b46234`, la correction du diagnostic macOS à `c7a53a90ee0af998ace6ef794c7a78a873b64605` et le diagnostic du shell à `5a4864727badcb897397de17e3c701baf7f60cfb`. Le banc initial est à `96dbbd476e3364b6e0847ac21c96939967084b2d`.

## Résultats fonctionnels

Sur macOS ARM64, les 157 cas différentiels, les cinq contrôles de flux par exécutable et le script amont passent. Le banc de précision couvre les fractions de nanoseconde, un intervalle de 365 jours plus une heure, les dépassements et le sous-flux négatif. Les quatre campagnes ELF réussissent chacune 167 cas. Le vrai nsh passe 76 contrôles par profil, y compris les refus attendus, soit 304 contrôles. Les démarrages AArch64 ont ensuite été rejoués sur le noyau final. Les onze groupes de régression du monorepo passent sur chacune des deux architectures. La [liste des retours des 22 suites](preuves/regressions.tsv) et les [journaux détaillés](preuves/journaux.tar.gz) conservent aussi les commandes et les observations de récupération.

Deux défauts ont été corrigés pendant la qualification. Sur Darwin, la première version masquait errno après un refus d’écriture sur stdout. Le diagnostic conserve désormais la cause système. Le prétraitement documentaire de nsh utilisait initialement la première chaîne commençant par un tiret pour rendre une erreur générique de statut 3. Il utilise désormais le diagnostic de l’option réellement rejetée et rend 1, y compris pour `sleep - -X --h`. Le premier essai de compilation du contrôleur avait aussi restreint par erreur la visibilité d’un module dans la copie instrumentée. Le banc final utilise la visibilité existante.

## Référence et licences

GNU Coreutils 9.12, tag v9.12 au commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`. L’oracle Homebrew est `/opt/homebrew/Cellar/coreutils/9.12_1/bin/gsleep`. Le code de référence comprend `src/sleep.c`, `gl/lib/cl-strtod.c`, `gl/lib/dtimespec-bound.h`, le manuel `doc/coreutils.texi` et le test `tests/misc/sleep.sh`. Le SHA du sous-module gnulib est `106e9b2384d08a1696fcbd40cbab52237943f208` pour `lib/long-options.c`, `lib/dtotimespec.c`, `lib/xnanosleep.c` et `tests/test-nanosleep.c`. L’arrondi, les valeurs minuscules, le jour en suffixe ou chiffre hexadécimal, l’infini et la reprise après EINTR ont été examinés. Le script amont de sleep a été exécuté sur les deux binaires macOS. L’empreinte des fichiers consultés figure dans [references.json](preuves/references.json). [GNU sleep](https://github.com/coreutils/coreutils/blob/v9.12/src/sleep.c) · [Manuel GNU](https://www.gnu.org/software/coreutils/manual/html_node/sleep-invocation.html)

La référence GNU est sous GPL-3.0-or-later. L’implémentation produite est sous MIT. Le décodage hexadécimal et l’échappement des diagnostics reprennent les fonctions MIT du candidat Nova seq à `54895020035b7a082e20b9b602cc5682d42bd41b`. uutils 0.13.0, `4735bbe40d764a523c5ece37d47465b32452d1a2`, est sous MIT. Son sleep repose sur clap, uucore, la localisation et std::thread. NovaOS utilise le moteur autonome et son runtime no_std. Les licences de Rust, de nova-cli, dlmalloc, cfg-if, spin et unicode-width sont jointes. [uutils sleep](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/sleep/src/sleep.rs) · [Notices de la livraison](licences/composants.txt)

## Mesures

Le banc Darwin ARM64 alterne l’ordre GNU/Nova, avec un échauffement puis quinze passages par outil et par durée, en locale C sur le même Apple M4 Max. Le délai observé couvre la création et la collecte. `time.monotonic_ns` mesure le temps écoulé, `wait4` donne le CPU utilisateur + système et le pic RSS individuel. Le retard est le temps observé moins la durée demandée. Un second banc relève les compteurs CPU Darwin du PID pendant une attente infinie, après 50 ms de stabilisation, puis interrompt et collecte l’enfant. Les sept fenêtres par binaire durent entre 100,28 et 110,09 ms. Le delta utilisateur + système observé est de 0 ns dans les quatorze fenêtres. Les médianes, le retard au 95e centile, les charges moyennes avant/après et les valeurs individuelles sont disponibles. La tolérance prédéfinie sur une durée courte est de zéro avance et 250 ms de retard, lancement compris. Les délais infinis sont interrompus après 65 ms d’observation côté hôte. Un verrou coordonne les campagnes participantes. [Tableau Nova et GNU](preuves/tableau.txt) · [Mesures brutes](preuves/mesures.json) · [Environnement](preuves/environnement.txt)

| Durée demandée | Temps Nova / GNU (ms) | CPU Nova / GNU (ms) | RSS Nova / GNU (Kio) |
|---:|---:|---:|---:|
| 0 ms | 2.652 / 2.673 | 1.361 / 1.305 | 1968.000 / 1840.000 |
| 1 ms | 4.362 / 4.291 | 1.544 / 1.439 | 1968.000 / 1840.000 |
| 5 ms | 10.402 / 10.218 | 1.609 / 1.558 | 1968.000 / 1840.000 |
| 20 ms | 28.019 / 33.175 | 1.741 / 1.670 | 1968.000 / 1840.000 |
| 100 ms | 111.023 / 107.699 | 1.767 / 1.729 | 1968.000 / 1840.000 |
| 250 ms | 261.606 / 262.906 | 1.862 / 1.847 | 1968.000 / 1840.000 |

Les invités utilisent QEMU 11.1.3 TCG, une vCPU et 256 Mio, machine virt/GICv2/Cortex-A72 ou Q35/CPU max. Le témoin est un fichier de huit octets sur P9 de 2 Mio. L’observateur utilise le compteur matériel virtuel de la HAL, indépendant du nombre de ticks fourni à IPC. Le chronométrage commence avant le lancement et s’arrête après la collecte. Sept passages suivent le premier échauffement. La charge du processus invité est échantillonnée aux interruptions de 250 Hz. Un résultat de zéro tick signifie qu’aucun échantillon n’a rencontré l’enfant en cours d’exécution. Les brèves phases avant et après blocage peuvent échapper à l’échantillonnage. GNU ne dispose pas d’un ELF NovaOS dans ce profil. [Mesures des quatre invités](preuves/performances-qemu.tsv) · [Matrice et écarts](MATRICE.md)

| QEMU | Secteur P9 | Médiane sleep 0 (ms) | Médiane sleep 100 ms (ms) | Retard p95 à 100 ms (ms) |
|---|---:|---:|---:|---:|
| aarch64 | 4096 | 5.862 | 159.548 | 64.577 |
| aarch64 | 512 | 5.879 | 156.349 | 59.661 |
| x86_64 | 4096 | 4.326 | 110.796 | 16.467 |
| x86_64 | 512 | 4.276 | 114.141 | 18.000 |

## Rejouer

Depuis le worktree et sur macOS ARM64 avec Rust stable et ses deux cibles nues, GNU coreutils, QEMU, xorriso et les dépendances du dépôt installés. Les deux scripts acceptent `--help`, `--rapide` et `--complet`. La cible macOS est le défaut. Les temporaires se trouvent sous le répertoire frère `OS-worktrees/tmp`. L’option `--out` désigne un fichier JSON sur macOS ou un répertoire de preuves sur NovaOS. Le rejeu QEMU utilise les vecteurs différentiels inclus dans le paquet.

```sh
cd livraison-sleep-0240
export NOVAOS_TMPDIR="$(cd ../.. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
./reconstruire.sh macOS
./tester-options-sleep.sh --complet
./mesurer-performances-sleep.sh --complet
MANPATH="$PWD/man" man sleep
./reconstruire.sh aarch64
./reconstruire.sh x86_64
./tester-options-sleep.sh --cible novaos --complet
./mesurer-performances-sleep.sh --cible novaos --rapide --arch aarch64
python3 -B lab/bilan.py --out "$TMPDIR/sleep-tableaux"
python3 -B lab/archive.py
```

Pour rejouer le script GNU, définir `GNU_COREUTILS_SOURCE` vers le dépôt de la révision indiquée avec son sous-module gnulib disponible et lancer `sh lab/amont.sh "$TMPDIR/sleep-amont-rejeu"`. Pour rejouer les contrôles numériques et les régressions, exécuter les commandes suivantes.

```sh
rustup run stable rustc --edition 2024 --test lab/precision.rs -o "$TMPDIR/sleep-precision"
"$TMPDIR/sleep-precision"
python3 -B lab/regressions.py --out "$TMPDIR/sleep-regressions"
```

[Archive de livraison](sleep-0240.tar.gz) · [Manifeste des membres](SHA256SUMS) · [SHA-256 de l’archive](SHA256-archive)
