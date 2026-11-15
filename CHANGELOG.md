# Journal des versions

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
