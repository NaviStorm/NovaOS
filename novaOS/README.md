# NovaOS v0.16.0

L’installateur AArch64 prépare un disque VirtIO vierge et démarre ensuite `nsh` depuis ce disque. Le profil UTM utilise le firmware EDK II expérimental de la VM, un TPM CRB et GICv3 sans ITS. [Image installable](../distribution/images/novaos-0.16.0-utm-aarch64.tar.gz) · [Installation et qualification](docs/nsh-scripts-v0.16.0.md#installation-dans-utm)

Depuis la racine du monorepo : `make -C novaOS ARCH=x86_64 block-test` ou `make -C novaOS ARCH=aarch64 block-test`. Pour un démarrage graphique avec une image raw existante : `make -C novaOS ARCH=x86_64 block-run IMAGE=/chemin/absolu/support.raw SECTOR=512` (AArch64 : `ARCH=aarch64`, secteur 512 ou 4096).

Un noyau Rust `no_std` commun démarre depuis un CD UEFI avec Limine. En mode diagnostic sans disque, la cible x86_64 affiche `NovaOS v0.16.0`, écrit ses diagnostics sur COM1 et présente le prompt `nova>`. Cliquer dans la fenêtre QEMU pour lui donner le clavier. La cible AArch64 affiche la même bannière puis entre en veille. Les deux noyaux activent leurs propres tables de pages avant l’affichage.

Le tas superviseur fournit un allocateur global Rust commun à x86_64 et AArch64. Il engage ses premières pages à la demande. L’éditeur de ligne utilise désormais un `Vec<char>` réservé de manière faillible et conserve la limite de 128 caractères. [Fonctionnement et tests du tas](docs/tas.md)

Le noyau ordonnance ses threads en tourniquet sur CPU0 avec un timer matériel à 250 Hz. Le thread de saisie x86_64 se bloque en attente du clavier. Sur les deux cibles, le thread idle prend la main quand la file des prêts est vide. [Ordonnancement, synchronisation et validation](docs/ordonnancement.md)

Deux instances indépendantes du programme de démonstration démarrent en Ring 3 sur x86_64 ou en EL0 sur AArch64. Elles calculent sous préemption matérielle puis terminent avec les codes 41 et 82. Le terminal QEMU reçoit les PID et les résultats sur le port série. [Architecture des processus et validation de la v0.6.0](docs/processus.md)

Les six appels natifs historiques permettent de lire les identifiants du processus et du thread, de céder le CPU, de terminer, d’attendre un processus autorisé et d’écrire un diagnostic borné. [ABI native v1 et sécurité](docs/abi.md) · [Mesures de RAM et validation QEMU](docs/mesures-v0.7.0.md)

Dix appels IPC permettent à des processus isolés de communiquer par endpoints unicast. Le noyau vérifie les capacités locales, livre les messages en FIFO et engage les transferts de droits à la réception. Un client peut envoyer jusqu’à 128 octets ou fusionner des notifications sur 64 bits. [Contrat IPC](docs/ipc-architecture.md) · [ABI binaire](docs/ipc-abi.md) · [Résultats et limites de la v0.8.0](docs/mesures-v0.8.0.md)

## Stockage et portabilité de NovaFS

NovaFS v0.9.13 monte des volumes autonomes historiques et P9 depuis le service Ring 3 / EL0. Le client utilise les opérations de fichiers et de répertoires par IPC. Chaque session possède son répertoire courant. [Espace de noms et chemins](docs/namespace-v0.12.0.md)

Le démonstrateur écrit sur des images jetables. Pour monter votre image en lecture seule, utiliser `make ARCH=x86_64 file-info IMAGE=/chemin/volume.raw SECTOR=512`. Le démarrage sans disque conserve la session de diagnostic. [Interface et contrats](docs/novafs-integration-v0.11.0.md) · [Résultats QEMU](docs/validation/novafs-natif-v0.11.0.md)

## Shell utilisateur

Depuis la racine du dépôt, démarrer la console série interactive de l’architecture choisie.

```sh
make -C novaOS ARCH=x86_64 cli
make -C novaOS ARCH=aarch64 cli
```

Depuis le terminal, `nsh fichier.nsh` lit un fichier du volume et `nsh -c 'echo "$$"'` affiche le PID du shell. Le disque installé contient `/verification.nsh`. [Grammaire, exemples et qualification](docs/nsh-scripts-v0.16.0.md)

Un nouveau volume de démonstration utilise le profil P9 et contient `/documents/exemple.txt`. Les commandes externes s’exécutent en Ring 3 ou EL0. `exit` ferme la session. Dans le terminal QEMU, Ctrl-A puis X quitte l’émulateur. [Commandes, droits et validation](docs/cli-v0.14.0.md)

## Démarrage manuel dans QEMU

Depuis le répertoire `novaOS/`, choisir une commande. Chacune compile le noyau et ouvre la fenêtre QEMU correspondante.

```sh
make ARCH=x86_64 run   # Q35, affichage et clavier PS/2 AZERTY au prompt nova>
make ARCH=aarch64 run  # virt, bannière sur ramfb et diagnostics PL011 dans le terminal
```

Sur x86_64, cliquer dans la fenêtre pour saisir du texte. Sur AArch64, le noyau entre en veille après la bannière. Fermer la fenêtre pour quitter la VM.

## Saisie dans le framebuffer de diagnostic

La disposition est **AZERTY français PC**. Les deux touches Maj permettent de saisir des majuscules et les chiffres de la rangée supérieure. Les accents directs, AltGr et les touches mortes circonflexe et tréma sont pris en charge. Par exemple, AltGr + E donne `€` et les touches `^` puis `e` donnent `ê`.

**Retour arrière** efface un caractère de la ligne en cours. **Entrée** valide la ligne et présente un nouveau prompt. Une ligne peut contenir 128 caractères Unicode. Au-delà de la largeur de la fenêtre, sa fin reste visible pendant la frappe. Quand la zone de texte est pleine, les lignes défilent sous la bannière. Ce mode affiche la ligne validée. La cible `cli` donne accès à l’interpréteur sur le terminal série.

## Compiler sur un Mac Apple Silicon

Avec les outils en ligne de commande Xcode et Homebrew installés, exécuter depuis le répertoire `novaOS/`

```sh
brew install rustup qemu xorriso
rustup toolchain install stable --profile minimal
rustup target add --toolchain stable x86_64-unknown-none
make build
```

L’ISO amorçable est produite dans `build/novaos.iso` et le noyau ELF dans `target/x86_64-unknown-none/release/novaos`. Cargo récupère les dépendances au premier build. Le chargeur Limine est inclus dans le dépôt.

| Commande | Résultat |
| --- | --- |
| `make build` | Compile le noyau et crée l’ISO UEFI. |
| `make run` | Ouvre l’écran QEMU et affiche COM1 dans le terminal. Fermer la fenêtre pour quitter. |
| `make nsh-scripts-test` | Exécute les scripts, les limites, la révocation et la reprise après interruption sur des secteurs de 512 et 4 096 octets. |
| `make cli-test` | Exerce la session série et ses programmes sur des secteurs de 512 et 4 096 octets avec redémarrages. |
| `make test` | Exécute les scénarios QEMU de démarrage, de mémoire virtuelle, de tas de multitâche, de processus, d’ABI et d’IPC pour la cible choisie. Nécessite Python 3. |
| `make ARCH=x86_64 endurance DURATION=7200` | Exécute la campagne de contention et de restitution pendant au moins deux heures réelles. Accepte aussi `ARCH=aarch64`. |
| `make ARCH=aarch64 ram-test` | Mesure la RAM physique avec 64, 128 et 256 Mio. Accepte également `ARCH=x86_64`. |
| `make ARCH=x86_64 storage-test` | Qualifie le service résident, ses clients, la reprise, la mémoire et les latences. Accepte aussi `ARCH=aarch64`. |
| `make ARCH=x86_64 ipc-test` | Exécute les sondes IPC courtes, dont le retrait d’une page pendant une réception. |
| `make ARCH=aarch64 ipc-races` | Force cinq ordres de réveil 10 000 fois chacun. |
| `make ARCH=x86_64 ipc-perf` | Mesure six formes d’échange, au repos et sous contention. |
| `make ARCH=aarch64 ipc-endurance DURATION=7200` | Contrôle chaque cycle d’IPC et mesure deux heures sur l’horloge hôte. Ces quatre cibles acceptent les deux architectures. |
| `make ARCH=x86_64 elf-test` | Teste dans QEMU le chargeur ELF, le tas privé, les protections et la collecte. Accepte aussi `ARCH=aarch64`. |
| `make clean` | Supprime les produits de compilation et de test dans `build/` et `target/`. |

### Compiler pour AArch64

Par défaut, les commandes ciblent x86_64. Pour QEMU `virt`, installer la cible AArch64 bare-metal et choisir explicitement l’architecture. La variante Rust `softfloat` convient à l’entrée du noyau, où les registres SIMD et flottants peuvent être désactivés. [Cibles AArch64 bare-metal de Rust](https://doc.rust-lang.org/rustc/platform-support/aarch64-unknown-none.html)

```sh
rustup target add --toolchain stable aarch64-unknown-none-softfloat
make ARCH=aarch64 kernel
make ARCH=aarch64 test
make ARCH=aarch64 run
```

L’ELF AArch64 se trouve dans `target/aarch64-unknown-none-softfloat/release/novaos` et son ISO dans `build/novaos-aarch64.iso`. Les deux images peuvent coexister.

## Machines virtuelles

Les deux configurations utilisent l’émulation TCG, un processeur, 256 Mio de RAM et une surcouche temporaire pour les variables UEFI. Le firmware EDK II est choisi selon l’architecture. Les variables d’environnement `OVMF_CODE` et `OVMF_VARS` permettent de fournir une autre paire de fichiers de firmware.

- **x86_64** utilise Q35, une carte VGA standard, le contrôleur PS/2 i8042, les PIC 8259 et le timer PIT.
- **AArch64** utilise la machine `virt`, un Cortex-A72, un GICv2, le timer physique générique et un lecteur CD VirtIO SCSI et `ramfb`. Le firmware configure l’affichage et Limine transmet le framebuffer au noyau. Le noyau écrit ses diagnostics mémoire sur le PL011. Le clavier interactif reste réservé à x86_64. [Machine virt de QEMU](https://www.qemu.org/docs/master/system/arm/virt.html)

Dans UTM sur Apple Silicon, créer une VM en **émulation**, sélectionner **x86_64** et **Q35**, activer le démarrage UEFI et utiliser un périphérique d’affichage VGA standard. Insérer `build/novaos.iso` dans le lecteur CD/DVD et le placer en tête de l’ordre de démarrage. [Réglages système UTM](https://docs.getutm.app/settings-qemu/system/) · [Réglages QEMU dans UTM](https://docs.getutm.app/settings-qemu/qemu/)

## Organisation du noyau

| Emplacement | Responsabilité |
| --- | --- |
| `boot/limine.conf` | Sélection du noyau et demande du mode 800 × 600 × 32. |
| `kernel/linker.ld` | Point d’entrée, ordre des requêtes Limine et permissions des segments ELF. |
| `kernel/src/boot.rs` | Carte mémoire, HHDM, emplacement ELF, mode de pagination, framebuffer et arguments Limine. |
| `kernel/src/main.rs` | Initialisation, bannière dérivée de la version Cargo et traitement des paniques. |
| `kernel/src/hardware/framebuffer.rs` | Framebuffer Limine et dessin de la bannière, communs aux deux cibles. |
| `kernel/src/memory/` | Cadres physiques de 4 Kio, espaces d’adressage, mappage, démappage et vérification des protections. |
| `kernel/src/sched/` | Threads, piles gardées, file des prêts, sommeil, événements et mutex bloquant. |
| `kernel/src/sched/process.rs`, `kernel/src/memory/user.rs` | Cycle de vie des processus, espaces privés, protections et restitution. |
| `kernel/src/sched/process/ipc/` | Endpoints, arbres de capacités, files, attentes et transactions de réception. |
| `user/` | Images utilisateur autonomes de démonstration et d’essai IPC, compilées pour la cible. |
| `kernel/src/heap/` | Allocateur global, arbres de blocs, extension transactionnelle et sondes du tas. |
| `kernel/src/hal/` | Interfaces communes du processeur, du masquage des interruptions et de la mémoire virtuelle. |
| `kernel/src/arch/x86_64/` | Pagination IA-32e, #PF, instructions CPU, COM1, IDT, PIC 8259 et clavier PS/2. |
| `kernel/src/arch/aarch64/` | MMU EL1, vecteurs d’exception, UART PL011, masquage via DAIF et veille via WFI. |
| `kernel/src/console.rs` | Affichage du prompt, du curseur et des lignes dans le framebuffer. |
| `kernel/src/input.rs` | Édition et remise d’une ligne Unicode à l’appelant via `read_line` et `Line::as_chars`. |
| `scripts/`, `tests/` | Assemblage de l’ISO, configuration de la VM et tests de démarrage. |

Rust stable sélectionne le module d’architecture à la compilation. La HAL expose `cpu::halt` et `interrupts::mask` sur les deux plateformes. Le gestionnaire commun implémente le trait `VirtualMemory`. Il remplace les tables de Limine. L’ordonnanceur quitte ensuite la pile de démarrage pour la pile privée du thread principal. [Architecture mémoire et scénarios de validation](docs/memoire.md)

Le script Cargo `kernel/build.rs` s’exécute sur le Mac. Les deux cibles utilisent le même agencement de sections ELF et l’éditeur de liens sélectionne la machine selon la cible. [Cible bare-metal x86_64 de Rust](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)

Le pilote vidéo utilise les décalages RVB et le pas de ligne indiqués par Limine pour les pixels de 32 bits. Sur x86_64, une erreur d’initialisation ou une panique Rust envoie un diagnostic sur COM1 puis arrête le processeur avec `cli` et `hlt`.

Sur x86_64, la GDT du noyau reprend les sélecteurs de Limine et ajoute un TSS avec piles d’urgence pour les fautes de page et doubles fautes. L’IDT reçoit IRQ0 pour le PIT et IRQ1 pour l’i8042. Quand le tampon du clavier est vide, le thread de saisie s’inscrit sur un canal d’attente sous masque IRQ. L’interruption le replace dans la file des prêts. Le décodage AZERTY reste en contexte de thread. [Contrôleur i8042 de QEMU](https://github.com/qemu/qemu/blob/master/hw/input/pckbd.c) · [Disposition AZERTY de pc-keyboard](https://github.com/rust-embedded-community/pc-keyboard/blob/main/src/layouts/azerty.rs)

L’éditeur de ligne du shell série se trouve dans `components/shell/`. Le client natif communique avec le service NovaFS par IPC.

## Vérification

`make test` injecte les appuis et les relâchements via QMP. Il compare les captures complètes de démarrage, de composition (`êï`), de frappe (`aAzé2€x`) et d’édition après Retour arrière et Entrée. Il vérifie également la protection du prompt, la limite de 128 caractères et le défilement. Les captures PNG et le journal `serie-std.log` sont conservés dans `build/`.

Une seconde VM démarre sans carte graphique. Le test attend le diagnostic de framebuffer absent dans `build/serie-none.log`. Les deux tests contrôlent l’état de veille du processeur.

`make ARCH=aarch64 test` démarre QEMU virt, compare la capture complète de la bannière à sa référence 800 × 600 et contrôle l’instruction de veille `wfi`. La capture est disponible dans `build/demarrage-aarch64.png`.

Six VM supplémentaires par architecture exercent l’allocateur, les espaces indépendants et cinq fautes provoquées. Les journaux portent le nom du scénario, par exemple `build/serie-x86_64-mem-nx.log` et `build/serie-aarch64-mem-absent.log`. Les ISO de test sont temporaires.

Sept VM par architecture testent le tas, la pénurie physique et la récupération, les sentinelles, la cohérence de l’arbre, la réentrée du verrou, la page de garde et le traitement d’une allocation infaillible refusée. Les journaux se trouvent dans `build/serie-<architecture>-heap-*.log`.

Quatre scénarios d’ordonnancement par architecture vérifient la préemption matérielle, la contention, la terminaison, le débordement de pile, l’interdiction du SIMD et la stabilité sur au moins 15 000 ticks. Un cinquième scénario combine registres, fragmentation, contention, pénurie et collecte des threads pendant au moins 15 secondes réelles. Les deux suites effectuent **23 démarrages x86_64 et 23 AArch64**, dont deux scénarios de processus par cible. La sonde ABI tourne sur Q35 et virt, avec une exécution ARM supplémentaire sur Cortex-A76 pour PAN. Le même ELF sert aux tests et à l’image normale. Les diagnostics se trouvent dans `build/serie-<architecture>-sched-*.log`.

Les campagnes longues réutilisent ce cinquième scénario sur une VM continue par cible. [Protocole et mesures de la v0.5.1](docs/durcissement.md)

Les journaux et bilans de la matrice RAM sont conservés dans `build/`. Le test relève la phase exacte lorsqu’un firmware ou Limine interrompt un démarrage avant le noyau.

## Provenance de Limine

Les fichiers `BOOTX64.EFI`, `BOOTAA64.EFI` et `limine-uefi-cd.bin` proviennent de la distribution binaire officielle **v10.8.5**, commit `7a9013f305de0bea1f6310e76e7baba30499fef0`. Leurs empreintes SHA-256 sont contrôlées à chaque création d’ISO. Le noyau utilise la révision de base 3 du protocole via la crate `limine` 0.5.0. Les licences des composants tiers sont incluses dans `licenses/` et sur l’ISO. [Distribution Limine épinglée](https://github.com/Limine-Bootloader/Limine/tree/7a9013f305de0bea1f6310e76e7baba30499fef0) · [Liaisons Rust 0.5.0](https://docs.rs/limine/0.5.0/limine/)
