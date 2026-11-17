# dirname 0.24.0 · ARC-027

Livraison du 17 novembre 2026. Le Mach-O ARM64 et le moteur commun passent la recette GNU. Sur la base retenue, le vrai `nsh` fournit l’aide, la version et le manuel. Un appel métier comme `dirname a/b` renvoie **127**. La sortie binaire d’un ELF installé attend le raccord du lanceur générique et des flux bruts.

| Référence | Valeur |
|---|---|
| Base `origin/main` relevée par `git ls-remote` au départ | `425e4e7cc5c03e246efb64d3638376906344bf47` |
| Branche dédiée | `codex/dirname-natif-0240` |
| Port et bancs | `46d0495a2f3003af10d3b143de3ca84cb9cd91be` |
| Sources finales et trois binaires | `8196315ae2639b39bdd0fc8710ea382f3a6ea514` |
| ABI et disque exercés | Native v1, NovaFS P9, secteurs de 512 et 4 096 octets |

### Exécutables

| Fichier | Format | SHA-256 |
|---|---|---|
| [bin/dirname](bin/dirname) | Mach-O ARM64 | `1711ec9181434dee6e799a9c68591c9f437f05509b8ab04eed9009ed00846ad0` |
| [bin/dirname-aarch64.elf](bin/dirname-aarch64.elf) | ELF64 AArch64 statique | `2b628482bfe70f4617cf29dde12794dd8aa0645c9f69c1538b2a5d400155cdc5` |
| [bin/dirname-x86_64.elf](bin/dirname-x86_64.elf) | ELF64 x86_64 statique | `f138db5138100735cb5f2e4935873b7755dc301769786fad673a620499403d20` |

### Reconstruction et recette

Sur macOS ARM64, depuis ce dossier, avec la chaîne Rust stable de `rustup`, les trois cibles installées et les dépendances Cargo du dépôt en cache. Les essais QEMU emploient les outils habituels du dépôt, Python avec `hashlib.file_digest`, QEMU système, xorriso, les firmwares EDK II et les outils de construction du relais AArch64. Les versions relevées et les en-têtes ELF sont consignés. [Références](preuves/references.txt)

```sh
./reconstruire.sh mac
./tester-options-dirname.sh
./tester-options-dirname.sh --amont
./mesurer-performances-dirname.sh court
./reconstruire.sh elf
./tester-options-dirname.sh --qemu
```

Le profil complet de performance s’exécute avec `./mesurer-performances-dirname.sh complet` et réalise 31 passages par traitement. Un dernier argument désigne le dossier des mesures. Le profil court archivé effectue neuf passages. Le banc d’options accepte un chemin de résultat JSON. Par défaut, les résultats de rejeu restent sous `OS-worktrees/tmp/dirname-arc027/`. La campagne QEMU crée un répertoire neuf et en affiche le chemin. Pour la suite Perl, `GNU_SOURCE` peut désigner un autre checkout de la même révision.

### Résultats

| Exécution | AArch64 P9/512 | AArch64 P9/4096 | x86_64 P9/512 | x86_64 P9/4096 |
|---|---:|---:|---:|---:|
| Sonde SDK, cas conformes avec flux en mémoire | 187 | 187 | 187 | 187 |
| ELF **livré**, essais sans port, erreur d’écriture attendue | 8 | 8 | 8 | 8 |
| Vrai shell, commandes et statuts vérifiés | 14 | 14 | 14 | 14 |
| Écritures / octets écrits / flush P9 pendant les consultations | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 |
| Régression `nsh`, commandes contrôlées | 113 | 113 | 113 | 113 |
| Régression des scripts, commandes contrôlées | 31 | 31 | 31 | 31 |

- **Darwin ARM64**. 233/233 cas vérifient les sorties, les diagnostics et les statuts face à `/opt/homebrew/bin/gdirname` 9.12. Les textes documentaires suivent ARC-026 et la version du composant. Les 22 cas de `tests/misc/dirname.pl` réussissent sur le Mach-O. Les 5 tests du catalogue passent en ARM64 natif et sous Rosetta x86_64. Les 9 contrôles du registre ABI passent.
- **Sonde QEMU**. Le parseur, le calcul lexical, le rendu des citations et le catalogue sont ceux du port. Les octets des deux flux sont comparés à l’oracle. Les exemples de NUL et d’octets non UTF-8 s’exécutent avec des tranches d’octets en mémoire. La sonde ne possède aucun canal métier. Une image P9 synthétique, attachée en lecture seule, conserve son empreinte.
- **ELF QEMU**. Le noyau extrait de l’ISO contient les octets exacts de l’ELF livré. Dans une copie privée de la base, le contrôleur du profil SDK prépare huit blocs argv et utilise le lanceur de témoin existant. L’instrumentation observe le port, la taille et le retour de la vraie porte console après sa décision. Le témoin SDK a zéro port. Les écritures reçoivent `DENIED=5`, chaque ELF quitte avec 1 et le noyau confirme la restitution. Les octets refusés ne sont jamais lus par la sonde noyau. Le SHA du volume est identique avant et après.
- **Vrai shell QEMU**. La consultation ARC-026 précède la résolution métier. Les tests observent dans le lecteur natif `man dirname` en français avec `fr-CA` et en anglais avec `fr--CA`, puis le retour à l’invite. Les statuts de l’aide et de la version valent 0. Les compteurs QMP et l’empreinte du disque entier concordent entre les deux bornes. Le catalogue d’installation de la base ne contient aucun indice pour dirname. `dirname /objet/absent`, `dirname -z a/b` et `nsh -c 'dirname a/b'` rendent 127.

Les 12 démarrages regroupent trois campagnes distinctes par profil. Les séries UART, les ISO, les noyaux relus, les disques et les décisions par cas figurent dans l’archive. Les pièces directement consultables comprennent la [matrice fonctionnelle](MATRICE.md), les [résultats Darwin](preuves/options.json) et les [résultats par invité](preuves/qemu.json).

### Performances

Sur le Mach-O livré, la médiane d’un chemin de 8 octets avec LF est de **1,400 ms**, contre **1,391 ms** pour GNU. Pour 512 chemins de 512 octets avec NUL, les médianes sont **1,558 ms** et **1,587 ms**. Le chronométrage inclut la création du processus et l’attente de sa fin. Chaque paire utilise le même argv et le même descripteur vers `/dev/null`. L’ordre alterne. Les sorties et les statuts sont vérifiés avant et après les passages. Le pic RSS maximal relevé par `wait4` est de 2 588 672 octets pour Nova et de 2 228 224 octets pour GNU. La machine était partagée, avec une charge moyenne sur une minute de 8,89 au début de la série. Les campagnes QEMU du chantier avaient terminé avant ce profil court. [Tableau complet](preuves/darwin/tableau.txt) · [Passages, CPU, débits et RSS](preuves/darwin/mesures.json)

| Coût médian du moteur QEMU par appel | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---:|---:|---:|---:|
| 1 chemin de 8 octets | 212 ns | 208 ns | 342 ns | 341 ns |
| 32 chemins de 64 octets | 3 972 ns | 3 925 ns | 3 375 ns | 3 349 ns |
| 4 chemins de 512 octets | 1 464 ns | 1 462 ns | 1 035 ns | 1 028 ns |

Ces mesures portent sur le parsing et la production de LF en mémoire, sous TCG avec 2 vCPU et 256 Mio. Trois médianes par profil sont calculées sur trois passages après échauffement. L’horloge monotone de l’hôte mesure des lots entre marqueurs série. Les sorties sont vérifiées hors chronométrage. Cette méthode inclut l’incertitude de réception des marqueurs. Le profil SDK ne délègue pas d’observation du RSS. L’ABI autorise au plus 32 mots et 2 560 octets encodés. Les temps de démarrage d’un dirname installé restent à mesurer via le lanceur générique.

### Dépendance d’intégration

La base comporte un catalogue fixe, des mots argv UTF-8 et une console texte qui refuse NUL. L’ELF utilise le contrat de cette base. L’ABI, le lanceur partagé et les rôles de pipe gardent leur définition d’origine. À la disponibilité d’une base autorisée munie du lancement générique, raccorder les descripteurs de sortie du runtime, installer les deux ELF, puis rejouer la matrice des flux depuis le terminal, un fichier script et `nsh -c`. Vérifier les sorties brutes séparées, le statut, la locale, la fermeture du consommateur, les bornes argv et l’absence de capacité fichier. Les octets non UTF-8 sur argv dépendent du contrat futur.

Les régressions exécutées ciblent le catalogue, l’ABI et les deux parcours shell touchés. La campagne n’a pas reconstruit d’installation UTM ni rejoué les suites de pilotes, de pannes de stockage ou de l’éditeur. Les essais se déroulent sur des volumes synthétiques.

### Intégrité

```sh
shasum -a 256 -c SHA256SUMS
python3 lab/verifier.py
```

Le premier contrôle relit la livraison. Le second relit chaque membre de [l’archive](qualification.tar.xz) et vérifie son nom et son contenu avec [le manifeste interne des fichiers](qualification.sha256).
