# Journal des versions

## NovaOS v0.17.0 — SMP et retrait des réservations DMA

- Le noyau démarre les processeurs secondaires sur AArch64 et x86_64. Les résultats couvrent 1, 2 et 4 vCPU. L’archive UTM AArch64 utilise un vCPU, le stockage classique, le polling VirtIO-blk et NovaFS v0.9.13 P9. [Qualification SMP et archive](novaOS/docs/fusion-smp-v0.17.0.md)
- Une expiration IPC fermait l’attente de l’appelant tout en laissant ouvertes les 256 pages réservées sur un autre canal. Le noyau ferme désormais l’admission avant le réveil et conserve les cadres d’une commande soumise jusqu’au retrait matériel confirmé. Le premier refus spontané AArch64/512 reste inexpliqué. [Cycle de vie et requalification](novaOS/docs/reservations-dma-v0.17.0.md) · [Diagnostic initial](novaOS/docs/qualification-stockage-v0.17.0.md)

## NovaOS v0.16.6

- Le stockage classique reste actif dans la distribution. Les pairs négocient la voie PAGE sur leur canal. Les composants de la v0.16.5 peuvent continuer à employer leurs interfaces publiées.
- VirtIO-blk conserve le propriétaire de chaque cadre DMA jusqu’au retrait confirmé et maintient en quarantaine les cadres encore accessibles. Le pilote utilise le polling par défaut.
- Les E/S groupées, le transfert multipage immuable et l’attente IRQ se qualifient dans des constructions expérimentales explicites. NovaFS reste en v0.9.13 P9. [Configuration, image et rapport de qualification](novaOS/docs/integration-v0.16.6.md)

## NovaOS v0.16.5

- Progression du montage P9 et copie par plage dans une destination sélectionnée par capacité. [Rapport de version](novaOS/docs/es-v0.16.5.md)
