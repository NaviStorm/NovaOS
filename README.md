# NovaOS v0.1.0

Premier jalon de NovaOS pour x86_64. Le firmware UEFI démarre Limine depuis le CD virtuel. Limine charge le noyau Rust, qui écrit `NovaOS v0.1.0` sur COM1 et dessine la même bannière au centre de l’écran. Le processeur entre ensuite en veille.

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
| `make test` | Démarre deux VM sans fenêtre et contrôle l’écran, COM1 et l’état du processeur. Nécessite Python 3. |
| `make clean` | Supprime les produits de compilation et de test dans `build/` et `target/`. |

## Machine virtuelle

Le lanceur utilise QEMU en émulation TCG, une machine Q35, un processeur x86_64, 256 Mio de RAM et une carte VGA standard. Il utilise le firmware EDK II fourni avec QEMU et une surcouche temporaire pour les variables UEFI. Les variables d’environnement `OVMF_CODE` et `OVMF_VARS` permettent de fournir une autre paire de fichiers de firmware.

Dans UTM sur Apple Silicon, créer une VM en **émulation**, sélectionner **x86_64** et **Q35**, activer le démarrage UEFI et utiliser un périphérique d’affichage VGA standard. Insérer `build/novaos.iso` dans le lecteur CD/DVD et le placer en tête de l’ordre de démarrage. [Réglages système UTM](https://docs.getutm.app/settings-qemu/system/) · [Réglages QEMU dans UTM](https://docs.getutm.app/settings-qemu/qemu/)

## Organisation du noyau

| Emplacement | Responsabilité |
| --- | --- |
| `boot/limine.conf` | Sélection du noyau et demande du mode 800 × 600 × 32. |
| `kernel/linker.ld` | Point d’entrée, ordre des requêtes Limine et permissions des segments ELF. |
| `kernel/src/boot.rs` | Vérification de la révision du protocole et récupération du framebuffer. |
| `kernel/src/main.rs` | Initialisation, bannière dérivée de la version Cargo et traitement des paniques. |
| `kernel/src/hardware/` | Écritures vidéo volatiles, glyphes de la bannière, pilote COM1 et mise en veille. |
| `scripts/`, `tests/` | Assemblage de l’ISO, configuration de la VM et tests de démarrage. |

Le noyau cible `x86_64-unknown-none` avec Rust stable et `#![no_std]`. Il utilise la pile et les tables de pages fournies par Limine. Le script Cargo `kernel/build.rs` s’exécute sur le Mac pour sélectionner le script de liaison. [Cible bare-metal x86_64 de Rust](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)

Le pilote vidéo accepte des pixels RVB de 32 bits avec trois canaux de 8 bits alignés sur un octet. Il utilise les décalages de couleur et le pas de ligne indiqués par Limine. En cas d’erreur détectée ou de panique Rust, le noyau envoie un diagnostic sur COM1 et exécute `cli` puis `hlt`. Les interruptions matérielles masquables restent désactivées à ce stade.

Les prochains jalons introduiront la gestion mémoire et les interruptions, puis le clavier et le shell. L’architecture prévue réserve le VFS à un service distinct du code matériel. Le RAMFS en sera le premier fournisseur. Le futur format de disque NovaOS pourra fournir les mêmes opérations.

## Vérification

`make test` démarre une VM avec VGA. Le test attend le diagnostic du noyau, compare la capture complète à l’empreinte de l’écran de référence et conserve `build/demarrage.png` et `build/serie-std.log`. Une seconde VM démarre sans carte graphique. Le test attend le diagnostic correspondant dans `build/serie-none.log`. Dans les deux cas, il vérifie que le processeur a exécuté `hlt`. Une modification volontaire de la bannière demande de vérifier la nouvelle capture avant d’actualiser l’empreinte dans `tests/demarrage.py`.

## Provenance de Limine

Les fichiers `BOOTX64.EFI` et `limine-uefi-cd.bin` proviennent de la distribution binaire officielle **v10.8.5**, commit `7a9013f305de0bea1f6310e76e7baba30499fef0`. Leurs empreintes SHA-256 sont contrôlées à chaque création d’ISO. Le noyau utilise la révision de base 3 du protocole via la crate `limine` 0.5.0. Les licences des composants tiers sont incluses dans `licenses/` et sur l’ISO. [Distribution Limine épinglée](https://github.com/Limine-Bootloader/Limine/tree/7a9013f305de0bea1f6310e76e7baba30499fef0) · [Liaisons Rust 0.5.0](https://docs.rs/limine/0.5.0/limine/)
