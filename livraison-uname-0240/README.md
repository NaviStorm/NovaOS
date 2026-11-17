# uname 0.24.0 · ARC-027

Les trois exécutables proviennent du commit `3e3c11d544e270b1f91e1f9d843e613c0d341219` de `codex/uname-natif-0240`. La base vérifiée avec `git ls-remote origin refs/heads/main` est `425e4e7cc5c03e246efb64d3638376906344bf47`. Le worktree principal était à `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`, ancêtre de cette base. Version de l’utilitaire, jalon v0.24.0 du monorepo et version 0.20.0 du noyau ont des sources distinctes.

| Exécutable | Usage |
|---|---|
| [bin/uname](bin/uname) | Mach-O macOS ARM64 |
| [bin/uname-aarch64.elf](bin/uname-aarch64.elf) | ELF statique NovaOS AArch64 |
| [bin/uname-x86_64.elf](bin/uname-x86_64.elf) | ELF statique NovaOS x86_64 |

## Référence et implémentation

GNU Coreutils 9.12, commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`, est l’oracle de comportement. L’étude a recoupé le point d’entrée, le parseur, le manuel Texinfo, l’aide de Homebrew et le test de `--all-labeled`. GNU est sous GPL-3.0-or-later. Le nouveau code du port est sous MIT. Le candidat uutils 0.13.0, MIT, dépend de `std`, `platform-info`, `clap`, `fluent` et `uucore`. Un parseur partagé sur tranches d’octets et deux fournisseurs d’identité permettent d’employer le runtime natif sans ces dépendances. [GNU uname.c](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/uname.c) · [Test GNU](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/tests/misc/uname-labeled.sh) · [uutils](https://github.com/uutils/coreutils/blob/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/uname/src/uname.rs) · [Versions, licences, chemins et empreintes](preuves/references.txt)

Sur Darwin, le fournisseur interroge `uname(3)` pour les cinq champs du noyau si la sélection le demande. Sur NovaOS, l’architecture découle de la cible ELF et la révision du même manifeste Cargo que la bannière du noyau. Le build relit ce manifeste lors d’une reconstruction. Le noyau d’essai doit démarrer avec les octets vérifiés dans son ISO. **La version du noyau est qualifiée pour cette paire ELF/noyau.** L’ABI v1 à 48 sélecteurs n’expose aucune requête d’identité permettant à un ELF déplacé sur un autre noyau d’en découvrir la révision. Une installation indépendante appelle un raccord d’identité au démarrage ou une requête publique. Le nom de nœud, le type de processeur et la plateforme n’ont pas de fournisseur sur cette base. [Manifeste du noyau](../novaOS/kernel/Cargo.toml) · [Bannière](../novaOS/kernel/src/main.rs) · [ABI](../novaOS/abi/syscalls.toml)

## Rejeu

Depuis ce dossier dans un worktree sous `OS-worktrees/`, avec la chaîne Rust stable du dépôt, les trois cibles installées, les dépendances Cargo en cache, QEMU, les firmwares, xorriso, clang et Python 3.11 ou ultérieur. Le banc GNU amont utilise le checkout de référence et son dépôt gnulib, y compris l’objet épinglé. Définir `GNU_UNAME_SOURCE` pour un autre emplacement et `GNU_UNAME` pour un autre exécutable GNU 9.12. Les tests fonctionnels et de performance proposent `--help`, `--rapide` et `--complet`.

```sh
./reconstruire.sh mac
./tester-options-uname.sh --complet
./tester-options-uname.sh --amont
./mesurer-performances-uname.sh --complet
./reconstruire.sh elf
./tester-options-uname.sh --qemu
```

Le rejeu QEMU exploite la matrice Darwin archivée et les deux ELF reconstruits. Pour mesurer uniquement le profil SDK sur les quatre cibles, utiliser `./mesurer-performances-uname.sh --qemu`. Le dernier argument facultatif des bancs sélectionne une destination de résultats. Choisir un dossier neuf pour conserver des campagnes distinctes. Le banc crée des répertoires temporaires privés sous `OS-worktrees/tmp/uname-arc027/`. Chaque disque de recette occupe 2 Mio. Les volumes exclusivement synthétiques et les ISO des démarrages de qualification restent dans les archives de médias.

## Qualification et limites

Les résultats par option, les abréviations, les erreurs et les règles de langue figurent dans la [matrice](MATRICE.md). Le test GNU amont s’exécute sur le Mach-O. Les catalogues ARM64 natif et x86_64 sous Rosetta et les neuf contrôles du registre ABI ont également été exécutés. [Test GNU](preuves/amont.txt) · [Catalogue ARM64](preuves/catalogue-aarch64-apple-darwin.txt) · [Catalogue x86_64](preuves/catalogue-x86_64-apple-darwin.txt) · [Registre ABI](preuves/abi.txt)

La première version du diagnostic Darwin classait les octets avec les règles Unicode de Rust. Elle échappait donc des caractères imprimables en ISO-8859-1 et Shift-JIS et laissait passer un caractère de largeur nulle en UTF-8. Le correctif utilise la conversion multioctet et la classification de la locale Darwin. Les 15 cas supplémentaires d’encodage passent. Lors du premier essai shell, le banc avait également imbriqué deux paires de guillemets simples autour de `nsh -c`, provoquant une erreur de syntaxe puis un délai d’attente. Le rejeu contrôle l’entrée directe et le parcours `nsh -c` séparément. [Trace initiale](preuves/diagnostic/nsh-essai-imbrique.serial.gz)

Le profil SDK lance les octets exacts de chaque ELF avec de vrais blocs argv, stdout et stderr. Le contrôleur de recette prépare uniquement les deux ports de sortie nécessaires. Le noyau d’essai consigne les trames après validation de la porte console. Les mesures désactivent cette trace. Une seconde VM démarre le noyau ordinaire et le véritable `nsh`. Le banc y interroge l’aide, la version et le lecteur `man`, puis teste le nom, le chemin absolu et le pipe. La documentation s’exécute localement avant toute sélection de capacité métier. L’empreinte du disque et les compteurs QMP établissent la stabilité du volume pendant ces consultations.

Le catalogue fixe de la base ne possède aucune installation `uname`. Le chantier parallèle du lanceur examiné au commit `f8966d38fa5ef021e4b2354780e0a0c4dd69f946` documente le bloc `NXEXEC01` et le ticket, et conserve le raccord de la session réelle de `nsh` à NovaAuth comme travail ouvert. À la livraison v0.30.0, raccorder l’ELF aux descripteurs stdin/stdout/stderr du runtime générique, installer l’image par ISA, puis rejouer les appels directs, les scripts et les fins de pipe. L’état fonctionnel actuel de l’entrée utilisateur ordinaire et la portabilité de la révision vers un autre noyau restent des limites de livraison. Les essais matériels et UTM sont hors de cette recette.

Les dépendances liées sont `libc` 0.2.191 sur Darwin, et le runtime Nova CLI MIT avec `dlmalloc` 0.2.14, `cfg-if` 1.0.5, `spin` 0.9.9 et `unicode-width` 0.2.2 selon la cible. Les notices MIT, Apache et Unicode des paquets et celles de la chaîne Rust sont livrées. [Licence du port](LICENCE-MIT) · [Notices des dépendances](NOTICES.txt.gz) · [Notices Rust](NOTICES-Rust.html.gz)

## Performances

macOS 27.2.1 ARM64, Apple M4 Max, locale C. Chaque paire utilise le même argv, les mêmes substitutions synthétiques, la même destination /dev/null, une création POSIX par invocation et wait4. La série complète alterne l’ordre, élimine un échauffement puis publie la médiane de neuf passages. Les temps incluent le lancement. Le RSS indiqué est la médiane du plus grand pic d’un processus par lot, en Kio. La charge moyenne sur une minute au début était de 23,84. Les résultats caractérisent ce passage sur machine partagée. La médiane de cinq lots de 20 000 appels directs à uname(3) vaut 1,181 µs par interrogation. [Tableau Nova et GNU](preuves/darwin.md) · [Passages bruts et environnement](preuves/mesures.json)

Les mesures QEMU chronomètrent un lancement puis huit lancements de chaque sélection vers la console série, sous TCG avec un vCPU et 256 Mio. Le compteur matériel invité encadre l’admission du processus et le trajet complet jusqu’à la collecte. Le reliquat inclut calcul, rendu, console, ordonnanceur et collecte. Le contrôleur vérifie la sortie intégrale de chaque lot. Il écarte un échauffement et prend la médiane de trois passages. Le profil SDK ne fournit pas le RSS par programme. GNU Coreutils est un exécutable Darwin et ne possède pas de binaire sur cette ABI. [Résultats et médianes par profil](RESULTATS.md)


## Régressions du shell

Chaque profil a passé les 113 commandes du banc nsh et les 31 du banc de scripts, avec relecture après remontage. Au total, huit démarrages dédiés à uname et seize démarrages de régression ont exercé les deux architectures. Les suites du pilote DMA, de l’installateur, de l’éditeur et les campagnes de pannes du moteur P9 concernent des composants que ce port ne modifie pas. [Résultats et journaux](preuves/regressions)

```sh
export NOVAOS_TMPDIR="$(cd ../.. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/uname-arc027/"
python3 -B lab/exclusif.py sh lab/regressions.sh "$TMPDIR/regressions"
```

## Vérification de l’archive

La vérification relit chaque fichier du tar décompressé, refuse un nom en double et compare l’inventaire et les octets au manifeste SHA-256 interne. Depuis le dossier de livraison du worktree, passer le chemin de l’archive remise avec ce rapport.

```sh
python3 -B lab/archiver.py verifier /chemin/qualification-uname.tar.gz
```
