# Journal des versions

## NovaOS v0.20.0 — Intégration source de la phase 2 et SDK Rust 0.2.0

- `JoinHandle::join(&self)` conserve le droit de collecte après une attente annulée. En 0.1.0, le passage du handle par valeur déclenchait son détachement à la sortie en erreur. Les deux invités QEMU ont vérifié le transfert du droit à un autre thread du processus et la collecte unique du résultat. [Contrat et qualification](novaOS/docs/rust-sdk-phase2-v0.20.0.md)
- Le SDK source permet la compilation croisée hors dépôt. L’archive contient la crate, les définitions ABI, le script de liaison et l’exemple à deux threads. La matrice des anciens et nouveaux clients couvre AArch64 et x86_64. [Archive et empreinte](distribution/README.md) · [Matrice de compatibilité](novaOS/docs/compat-rust-sdk-v0.20.0.md)
- Les deux phases sont intégrées dans `main`. NovaOS v0.20.0 reste ouverte, avec NovaFS v0.9.13 P9 et l’archive installable UTM v0.17.0. La clôture attend la validation finale MOA/MOE. [Dossier de clôture](novaOS/docs/cloture-rust-v0.20.0.md)

## NovaOS v0.20.0 — Intégration source de la phase 1 Rust et threads

- La bibliothèque Rust `nova-system` 0.1.0 expose les capacités IPC, le processus, le tas, l’horloge et les opérations de fichiers et de répertoires délégués. Les applications Rust `no_std` utilisent la porte native v1 et le chargeur ELF statique existant. [API Rust](novaOS/libs/nova-system/src/lib.rs) · [ABI](novaOS/docs/abi.md)
- Neuf appels ajoutent aux processus admis la création de threads dans le même espace mémoire, la terminaison, la jonction, le détachement, l’annulation coopérative, le TLS expérimental et l’attente/notification sur mot atomique. Le témoin de phase 1, compilé hors dépôt, a exercé ces interfaces sur QEMU AArch64 et x86_64 à 1, 2 et 4 vCPU. [Contrat et résultats de phase 1](novaOS/docs/rust-threads-v0.20.0.md)
- Le numéro 0.20.0 des manifestes et bannières identifie cette phase. Les travaux suivants et la clôture de la version requièrent une nouvelle décision MOA/MOE. [Qualification finale de la phase 1](novaOS/docs/qualification-rust-threads-v0.20.0.md)
- Les ISO de développement utilisent VirtIO-blk et le polling par défaut. L’archive installable UTM AArch64 porte la version v0.17.0 et le moteur NovaFS la version v0.9.13 au format P9. [Distribution](distribution/README.md)

## NovaOS v0.19.0 — Pilote NVMe isolé sur Q35

- Le pilote utilisateur NVMe identifié par le profil `nvme-lab` accède au namespace virtuel par des capacités de lecture, d’écriture et de synchronisation distinctes. Le courtier vérifie la complétion, les bornes du bail et l’état du domaine Intel VT-d. [Contrat matériel](novaOS/docs/nvme-phase2-contrats.md)
- NovaFS v0.9.13 P9 conserve les deux barrières de publication et rend l’état incertain lorsque F2 échoue. Le client peut alors lire le CID et le statut de la complétion en erreur. Les campagnes QEMU utilisent des disques jetables en secteurs de 512 et 4 096 octets. [Qualification NVMe](novaOS/docs/nvme-phase3-v0.19.0.md)
- Les ISO ordinaires AArch64 et x86_64 conservent VirtIO-blk et le polling. L’archive UTM distribuée reste celle de v0.17.0. [Versions et preuves de compatibilité](novaOS/docs/validation/nvme-phase3-compatibilite.json)

## NovaOS v0.18.0 — Découverte matérielle et isolation des pilotes

- Découverte ACPI et PCI bornée, validation des BAR et attribution de ressources MMIO et IRQ au pilote EDU isolé sous QEMU. Le laboratoire qualifie la révocation et la destruction du pilote sur un, deux et quatre vCPU. [Découverte matérielle](novaOS/docs/materiel-v0.18.0.md) · [Qualification MMIO/IRQ](novaOS/docs/materiel-phase2-v0.18.0.md)
- Le profil expérimental Intel VT-d contrôle les traductions DMA de l’EDU directement raccordée à la racine Q35 x86_64. Les cadres restent épinglés si la fin de commande ou l’invalidation ne peut être confirmée. Le refus d’une écriture est vérifié indépendamment de sa notification. [Qualification finale et limites matérielles](novaOS/docs/materiel-phase4-v0.18.0.md)
- La version des sources passe à v0.18.0. L’archive UTM AArch64 distribuée porte la version v0.17.0. NovaFS conserve la version v0.9.13 et le format P9. [Contrôles de fusion](novaOS/docs/fusion-materiel-v0.18.0.md)

## NovaOS v0.17.0 — SMP et retrait des réservations DMA

- Le noyau démarre les processeurs secondaires sur AArch64 et x86_64. Les résultats couvrent 1, 2 et 4 vCPU. L’archive UTM AArch64 utilise un vCPU, le stockage classique, le polling VirtIO-blk et NovaFS v0.9.13 P9. [Qualification SMP et archive](novaOS/docs/fusion-smp-v0.17.0.md)
- Une expiration IPC fermait l’attente de l’appelant tout en laissant ouvertes les 256 pages réservées sur un autre canal. Le noyau ferme désormais l’admission avant le réveil et conserve les cadres d’une commande soumise jusqu’au retrait matériel confirmé. Le premier refus spontané AArch64/512 reste inexpliqué. [Cycle de vie et requalification](novaOS/docs/reservations-dma-v0.17.0.md) · [Diagnostic initial](novaOS/docs/qualification-stockage-v0.17.0.md)

## NovaOS v0.16.6

- Le stockage classique reste actif dans la distribution. Les pairs négocient la voie PAGE sur leur canal. Les composants de la v0.16.5 peuvent continuer à employer leurs interfaces publiées.
- VirtIO-blk conserve le propriétaire de chaque cadre DMA jusqu’au retrait confirmé et maintient en quarantaine les cadres encore accessibles. Le pilote utilise le polling par défaut.
- Les E/S groupées, le transfert multipage immuable et l’attente IRQ se qualifient dans des constructions expérimentales explicites. NovaFS reste en v0.9.13 P9. [Configuration, image et rapport de qualification](novaOS/docs/integration-v0.16.6.md)

## NovaOS v0.16.5

- Progression du montage P9 et copie par plage dans une destination sélectionnée par capacité. [Rapport de version](novaOS/docs/es-v0.16.5.md)
