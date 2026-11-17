# basename 0.24.0 · ARC-027

| Exécutable | Format et exécution vérifiés |
|---|---|
| [basename-darwin-arm64](bin/basename-darwin-arm64) | Mach-O ARM64, macOS 27.2.1 natif |
| [basename-aarch64.elf](bin/basename-aarch64.elf) | ELF statique, EL0, QEMU AArch64 |
| [basename-x86_64.elf](bin/basename-x86_64.elf) | ELF statique, Ring 3, QEMU x86_64 |

Branche `codex/basename-natif-0240`, créée depuis le SHA distant vérifié de `origin/main`, `425e4e7cc5c03e246efb64d3638376906344bf47`. Commit du code et des binaires `1c510225f6672090de90eb7adf23ae52281ee7d1`. Les vérifications datent du 17 novembre 2026, heure de Paris. [Révisions, outils, types et empreintes](preuves/references.json)

## Résultats

- Le Mach-O passe 351 comparaisons différentielles et les 35 cas de `tests/misc/basename.pl`. Les enregistrements conservent séparément arguments, stdout et stderr en hexadécimal, ainsi que le statut de chaque processus. Pour comparer les diagnostics, le banc harmonise uniquement le préfixe de programme `gbasename:`. Pour l’aide et la version, il contrôle le contrat NovaOS. [Résultats Darwin](preuves/options.json)
- Chaque combinaison QEMU AArch64 ou x86_64, P9/512 ou P9/4096, a exécuté 121 fois l’ELF livré dans le profil de qualification SDK. Le contrôleur a aussi vérifié 83 résultats binaires du moteur par cellule, avec les octets produits par GNU. Les 121 cas incluent l’échauffement et les répétitions de mesure. [Matrice détaillée](preuves/matrice-cibles.json)
- Dans le véritable `nsh`, les trois consultations publiques réussissent dans les quatre cellules et les quatre locales essayées. Le SHA-256 du disque reste identique pendant chaque groupe de consultations. Les deux appels métier exercés dans chaque shell rendent **127**. La base vérifiée ne possède pas le raccord public de lancement générique. [Matrice et recette à reprendre](MATRICE.md)
- Le lot de 512 noms sur macOS prend **1,579 ms** pour Nova et **1,623 ms** pour GNU, médianes incluant le lancement. La machine exécutait aussi des processus QEMU et de compilation. Les valeurs décrivent cette campagne. [Tableau](preuves/performances.txt) · [Temps bruts, débits et pics RSS](preuves/performances.json)

Les douze charges LF/NUL du créneau Mac dédié du 17 novembre respectent le seuil par charge de 1,10 × GNU. [Tableau des médianes, dispersion et rejeu](preuves/seuil-darwin-1/README.md)

## Référence et réalisation

La source GNU épinglée est `c0f8514d989184921d9b12a4d103a7b23abc5af8`. Ont servi à établir le contrat le parseur de `src/basename.c`, ses fonctions de retrait de suffixe et d’affichage, la sélection du style dans `src/system.h`, le chapitre « basename invocation » du manuel et les fichiers gnulib `basename.c`, `basename-lgpl.c`, `stripslash.c` et `quotearg.c` au commit du sous-module. Le banc Perl a été exécuté sur chaque programme. Le scénario terminal de `tests/misc/tty-quoting.sh` a été repris sur un vrai pseudo-terminal Darwin. Le banc Linux utilise une injection `strace` indisponible sur cet hôte. L’oracle Homebrew `/opt/homebrew/bin/gbasename` s’identifie comme GNU coreutils 9.12. [Source officielle](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/basename.c) · [SHA-256 des sources et du binaire](preuves/references.json)

L’examen de `uu_basename` au commit `4735bbe40d764a523c5ece37d47465b32452d1a2` a porté sur son manifeste, son implémentation, la licence MIT et le rendu de noms de `uucore`. Cette commande utilise `std::path`, `std::ffi`, `clap`, `fluent` et `uucore`. Le port MIT a son propre moteur sur tranches d’octets, partagé entre Darwin et la cible nue. La page et la validation de la langue réutilisent le catalogue NovaOS. GNU sert d’oracle externe. [Moteur](../components/basename/src/engine.rs) · [Parseur](../components/basename/src/options.rs) · [Implémentation uutils examinée](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/basename/src/basename.rs)

## Limites de la sortie native

L’argv v1 transporte au plus 32 chaînes UTF-8 et 2 560 octets de trame, avec un octet de compte et deux octets de longueur par chaîne. Le calcul lexical accepte toute séquence d’octets, y compris un nom vide. La console NovaOS valide les scalaires et convertit LF en CRLF sur l’UART. Un nom destiné au terminal utilise le style de protection `shell-escape`. Le processus enfant ne reçoit pas l’environnement Darwin `QUOTING_STYLE`. Pour `-z`, la seule destination publique disponible refuse le NUL. L’ELF renvoie **25** sur cette destination avant d’imprimer le premier nom. Le moteur et le Mach-O produisent les NUL demandés, y compris sur le pseudo-terminal macOS. Une sortie ELF binaire de bout en bout dépend d’une destination native compatible. [Interfaces](../novaOS/abi/terminal.rs) · [Adaptateur ELF](../components/basename/src/guest.rs)

## Reconstruction et rejeu

Depuis le worktree dédié, avec les cibles Rust `aarch64-apple-darwin`, `aarch64-unknown-none-softfloat` et `x86_64-unknown-none` installées. Les dépendances verrouillées doivent être présentes dans le cache Cargo pour les constructions hors ligne. Les scripts utilisent un temporaire privé dans `OS-worktrees/tmp/basename-arc027`. Le banc invité exige QEMU, ses firmwares EDK II, xorriso, l’outillage habituel de construction de NovaOS et un répertoire de résultats neuf par architecture. Conserver ce répertoire après chaque campagne.

```sh
cd livraison-basename-0240
./reconstruire.sh macOS
./tester-options-basename.sh ../novaOS/build/basename-macos \
  --amont ../../tmp/sort-arc027-gnu
./mesurer-performances-basename.sh court ../novaOS/build/basename-mesures
./reconstruire.sh aarch64
./reconstruire.sh x86_64
./tester-invite-basename.sh aarch64 ../novaOS/build/basename-qemu-arm
./tester-invite-basename.sh x86_64 ../novaOS/build/basename-qemu-x86
```

Le profil complet augmente la taille des lots jusqu’à 4 096 chemins et effectue neuf passages mesurés. Le profil court effectue cinq passages après trois lancements d’échauffement. Chaque passage alterne l’ordre Nova/GNU et contient 6 à 12 lancements dans le profil court. Les arguments, la locale C et la destination `/dev/null` sont identiques pour les deux programmes. Le script contrôle stdout hors chronométrage, vérifie les statuts, mesure le RSS maximal de chaque enfant avec `wait4` et affiche séparément le témoin de création et d’attente du processus `/usr/bin/true`.

```sh
./mesurer-performances-basename.sh complet ../novaOS/build/basename-mesures-completes
shasum -a 256 -c SHA256SUMS
```

## Régressions

Les cinq tests du catalogue passent en Darwin ARM64 natif et en Darwin x86_64 sous Rosetta. Les suites de régression `cli.py`, `cli_p9.py`, `nsh.py`, `nsh_scripts.py` et `nsh_terminal.py` passent sur les deux architectures QEMU, avec les deux géométries de leur banc. Chaque cellule de `nsh.py` a vérifié 113 commandes et chaque cellule de `nsh_scripts.py` en a vérifié 31 avec reprise. Les premiers lancements de `cli_p9.py` ont rencontré un chemin de bibliothèque d’oracle ou de noyau absent. Le rejeu a fourni les mêmes binaires aux chemins attendus et a terminé les contrôles de dates et de remontage. Les journaux des tentatives et des réussites figurent dans l’archive hôte. Les essais matériels DMA, NVMe, l’installateur et l’endurance de stockage ne relèvent pas des fichiers modifiés.

```sh
# Depuis la racine du worktree, avec TMPDIR privé configuré.
for arch in aarch64 x86_64; do
  CARGO_PKG_VERSION=0.20.0 rustup run stable rustc --edition 2024 \
    --target "$arch-apple-darwin" --test novaOS/tests/documentation_catalogue.rs \
    -o "$TMPDIR/catalogue-$arch"
  "$TMPDIR/catalogue-$arch"
  make -C novaOS ARCH="$arch" kernel cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test
done
```

## Conservation

Les archives de preuves ont été décompressées et chaque membre a été relu et comparé à son original. La livraison inclut les huit ISO effectivement démarrées par le banc ciblé, les disques préparés, les séries SDK et shell, les résultats, les vecteurs et les journaux de construction. Le banc a extrait le noyau de chaque ISO et comparé son empreinte au noyau attendu. [Archive QEMU](preuves/qemu.tar.gz) · [Empreintes des membres](preuves/qemu-membres.sha256) · [Recette hôte et régressions](preuves/hote-regressions.tar.gz)

Le worktree conserve également les passages intermédiaires du banc, 68 membres relus, dans `novaOS/build/basename-passages-intermediaires.tar.gz`, SHA-256 `a0cc6ae2eb883a554aee3c15929fab1858e143a220e169110eda3b835e7f45d8`. Les premiers essais ont permis de corriger la lecture d’un pseudo-terminal plein, la distinction entre la ligne de collecte du noyau et stderr, et l’attente de l’invite au retour du manuel.

La licence du port et les notices du code lié accompagnent les trois exécutables. [MIT](LICENCE-MIT) · [Dépendances](NOTICES.txt.gz) · [Bibliothèques Rust](NOTICES-Rust.html.gz)
