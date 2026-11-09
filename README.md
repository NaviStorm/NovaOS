# NovaOS v0.2.0

Le firmware UEFI démarre Limine depuis le CD virtuel. Le noyau Rust affiche `NovaOS v0.2.0`, écrit ses diagnostics sur COM1 et présente le prompt `nova>`. Cliquer dans la fenêtre QEMU pour lui donner le clavier.

## Saisie interactive

La disposition est **AZERTY français PC**. Les deux touches Maj permettent de saisir des majuscules et les chiffres de la rangée supérieure. Les accents directs, AltGr et les touches mortes circonflexe et tréma sont pris en charge. Par exemple, AltGr + E donne `€` et les touches `^` puis `e` donnent `ê`.

**Retour arrière** efface un caractère de la ligne en cours. **Entrée** valide la ligne et présente un nouveau prompt. Une ligne peut contenir 128 caractères Unicode. Au-delà de la largeur de la fenêtre, sa fin reste visible pendant la frappe. Quand la zone de texte est pleine, les lignes défilent sous la bannière. Le jalon se termine à la remise de la ligne validée au noyau. L’interprétation des commandes est prévue pour le futur shell.

## Compiler sur un Mac Apple Silicon

Avec les outils en ligne de commande Xcode et Homebrew installés, exécuter depuis la racine du dépôt

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
| `make test` | Démarre deux VM, envoie des frappes au clavier PS/2 et vérifie les pixels affichés, COM1 et la veille. Nécessite Python 3. |
| `make clean` | Supprime les produits de compilation et de test dans `build/` et `target/`. |

## Machine virtuelle

Le lanceur utilise QEMU en émulation TCG, une machine Q35, un processeur x86_64, 256 Mio de RAM et une carte VGA standard. Il active le contrôleur PS/2 i8042 et les PIC 8259. Il utilise le firmware EDK II fourni avec QEMU et une surcouche temporaire pour les variables UEFI. Les variables d’environnement `OVMF_CODE` et `OVMF_VARS` permettent de fournir une autre paire de fichiers de firmware.

Dans UTM sur Apple Silicon, créer une VM en **émulation**, sélectionner **x86_64** et **Q35**, activer le démarrage UEFI et utiliser un périphérique d’affichage VGA standard. Insérer `build/novaos.iso` dans le lecteur CD/DVD et le placer en tête de l’ordre de démarrage. [Réglages système UTM](https://docs.getutm.app/settings-qemu/system/) · [Réglages QEMU dans UTM](https://docs.getutm.app/settings-qemu/qemu/)

## Organisation du noyau

| Emplacement | Responsabilité |
| --- | --- |
| `boot/limine.conf` | Sélection du noyau et demande du mode 800 × 600 × 32. |
| `kernel/linker.ld` | Point d’entrée, ordre des requêtes Limine et permissions des segments ELF. |
| `kernel/src/boot.rs` | Vérification de la révision du protocole et récupération du framebuffer. |
| `kernel/src/main.rs` | Initialisation, bannière dérivée de la version Cargo et traitement des paniques. |
| `kernel/src/hardware/` | Framebuffer, COM1, IDT, PIC 8259 et pilote clavier PS/2. |
| `kernel/src/console.rs` | Affichage du prompt, du curseur et des lignes dans le framebuffer. |
| `kernel/src/input.rs` | Édition et remise d’une ligne Unicode à l’appelant via `read_line` et `Line::as_chars`. |
| `scripts/`, `tests/` | Assemblage de l’ISO, configuration de la VM et tests de démarrage. |

Le noyau cible `x86_64-unknown-none` avec Rust stable et `#![no_std]`. Il utilise la pile et les tables de pages fournies par Limine. Le script Cargo `kernel/build.rs` s’exécute sur le Mac pour sélectionner le script de liaison. [Cible bare-metal x86_64 de Rust](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)

Le pilote vidéo utilise les décalages RVB et le pas de ligne indiqués par Limine pour les pixels de 32 bits. Une erreur d’initialisation ou une panique Rust envoie un diagnostic sur COM1 puis arrête le processeur avec `cli` et `hlt`.

Le noyau conserve la GDT de Limine et installe sa propre IDT. Il remappe les PIC sur les vecteurs `0x20` à `0x2f` et démasque uniquement IRQ1. L’i8042 utilise le premier port PS/2 et transmet les scancodes du jeu 2 avec la traduction matérielle désactivée. La routine IRQ1 acquitte le PIC maître. La boucle de saisie lit les octets du contrôleur, puis `pc-keyboard` décode les appuis et les relâchements. Lorsque le tampon est vide, la séquence `sti; hlt; cli` permet d’attendre la frappe suivante sans perdre une interruption entre la vérification et la veille. [Contrôleur i8042 de QEMU](https://github.com/qemu/qemu/blob/master/hw/input/pckbd.c) · [Disposition AZERTY de pc-keyboard](https://github.com/rust-embedded-community/pc-keyboard/blob/main/src/layouts/azerty.rs)

Le shell consommera les lignes remises par `read_line`. Le VFS reste prévu comme un service distinct du matériel, avec un RAMFS comme premier fournisseur.

## Vérification

`make test` injecte les appuis et les relâchements via QMP. Il compare les captures complètes de démarrage, de composition (`êï`), de frappe (`aAzé2€x`) et d’édition après Retour arrière et Entrée. Il vérifie également la protection du prompt, la limite de 128 caractères et le défilement. Les captures PNG et le journal `serie-std.log` sont conservés dans `build/`.

Une seconde VM démarre sans carte graphique. Le test attend le diagnostic de framebuffer absent dans `build/serie-none.log`. Les deux tests contrôlent l’état de veille du processeur.

## Provenance de Limine

Les fichiers `BOOTX64.EFI` et `limine-uefi-cd.bin` proviennent de la distribution binaire officielle **v10.8.5**, commit `7a9013f305de0bea1f6310e76e7baba30499fef0`. Leurs empreintes SHA-256 sont contrôlées à chaque création d’ISO. Le noyau utilise la révision de base 3 du protocole via la crate `limine` 0.5.0. Les licences des composants tiers sont incluses dans `licenses/` et sur l’ISO. [Distribution Limine épinglée](https://github.com/Limine-Bootloader/Limine/tree/7a9013f305de0bea1f6310e76e7baba30499fef0) · [Liaisons Rust 0.5.0](https://docs.rs/limine/0.5.0/limine/)
