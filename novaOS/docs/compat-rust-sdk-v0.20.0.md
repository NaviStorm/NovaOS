# Compatibilité binaire du SDK Rust · phase 2 de v0.20.0

Le 15 novembre 2026, deux binaires clients par ISA ont exercé le protocole fichier v2, les droits IPC et le décodage des appels natifs v1 dans QEMU 11.1.3 TCG. Un client a été lié avec la crate `nova-system` 0.1.0 de la phase 1 et l’autre avec la crate 0.2.0 de la phase 2. Les mêmes ELF ont servi sur les deux générations de noyau. Le profil comporte un vCPU, 256 Mio et une image jetable NovaFS P9 avec secteurs de 512 octets. [Relevés et SHA-256 des 4 journaux, 4 noyaux, 4 services et 4 clients](validation/compat-rust-sdk-v0.20.0.json)

| Client | Noyau et service NovaFS embarqué | AArch64 EL0 et x86_64 Ring 3 |
|---|---|---|
| Ancien SDK 0.1.0 | v0.19.0 avant l’extension threads | Fichier accessible, ouverture hors délégation refusée. `THREAD_CREATE` et `THREAD_JOIN` rendent `UNKNOWN_CALL`. |
| Ancien SDK 0.1.0 | v0.20.0 avec extension de phase 1 | Fichier accessible, ouverture hors délégation refusée. Collecte de 77, refus d’un second appel sur le droit consommé. |
| Nouveau SDK 0.2.0 | v0.19.0 avant l’extension threads | Même lecture et même refus sur le fichier. Le SDK remonte `UNKNOWN_CALL` pour la création. Le témoin vérifie aussi ce statut pour la jonction brute. |
| Nouveau SDK 0.2.0 | v0.20.0 avec extension de phase 1 | Même lecture et même refus sur le fichier. Collecte de 77. Une seconde tentative par le wrapper ou par la porte brute rend `DENIED`. |

**Étendue de la matrice.** La révision ancienne est `2bcb64575e396790089e3078d443b63d69ac05d7`. La révision avec threads est `90de918fa6ca090d731500127cf400c4836a0fec`. Les sources du service NovaFS, du moteur P9, des trames et de la jonction du noyau ordinaire sont communes aux phases 1 et 2. Le banc les compare aux sources de phase 1. L’ABI v1 antérieure conserve toutes ses déclarations et ajoute exactement neuf sélecteurs de `0x10060` à `0x10068`. Les deux noyaux avec l’extension, un par ISA, ont renvoyé `BAD_ARGUMENT` pour un argument réservé non nul de `THREAD_JOIN` sur un droit vivant. Le service de fichiers conserve son protocole v2 dans toutes les cellules. Le refus d’une fonction absente est exercé auprès du noyau v0.19.0.

**Admission et données.** Le banc exporte les sources Git épinglées et assemble chaque image dans un répertoire jetable. Dans les catalogues de test, `head` désigne le témoin ancien et `tail` le nouveau. Le lanceur standard de `nsh` donne à chaque fils les deux canaux de la seule vue fichier sélectionnée. Les témoins vérifient les octets `NovaOS\n`, la sentinelle située au-delà de cette lecture courte, puis une lecture de zéro octet à la fin du fichier. Une requête pour ouvrir un autre nom depuis la racine sans délégation doit rendre `DENIED`. Le `cat` ordinaire de chaque révision lit aussi le fichier dans chaque invité. Les quatre sessions se terminent normalement. [Client des deux SDK](../tests/compat-rust-0200/client.rs) · [Banc de construction, d’admission et d’exécution](../tests/compat_rust.py)

Les codes de sortie attendus par le banc sont 19 après vérification du refus de l’extension et 0 après une collecte réussie. Les mécanismes de concurrence, de révocation et de restitution sont exercés dans le banc threads distinct. Les clients de cette matrice utilisent les sources de la crate correspondant à leur révision. La construction depuis l’archive du SDK fait l’objet de la qualification de distribution. [Contrat de phase 2](rust-sdk-phase2-v0.20.0.md) · [SDK distribuable](sdk-rust-v0.20.0.md)

## Reproduction

Depuis la racine du monorepo, sur un hôte Darwin avec les deux cibles Rust nues et les outils QEMU de développement :

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
python3 -B novaOS/tests/compat_rust.py aarch64
python3 -B novaOS/tests/compat_rust.py x86_64
```

L’option `--host` accepte un autre chemin vers le même oracle hôte NovaFS v0.9.13. Chaque invocation génère un relevé JSON sous `novaOS/build/matrice-rust/`. Les journaux UART sont conservés sous `novaOS/build/cli-<isa>-512-sdk-<révision>.serial`.
