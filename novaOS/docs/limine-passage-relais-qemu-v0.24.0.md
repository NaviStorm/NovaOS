# NovaOS v0.24.0 — Passage de relais Limine AArch64 sous QEMU

## Reproduction et cause

L’ISO de lancement authentifié s’arrêtait dès l’initialisation de la mémoire physique, avant NovaAuth et le premier pipe. Le diagnostic en série était `Initialisation mémoire : Missing`. Dans la réponse Limine, la dernière page de RAM, `[0x4ffff000, 0x50000000[`, appartient à une zone récupérable du chargeur. Au premier PC de NovaOS, sa traduction dans la fenêtre directe (HHDM) à `0xffff00004ffff000` est absente. La référence possède une entrée de page `0x4ffff703`.

Trois points d’arrêt matériels dans `build_pagemap`, posés dans le même PE Limine officiel, montrent la première divergence. Les mesures ont été prises avant le correctif.

| Instant | ISO de référence | ISO du raccord |
| --- | ---: | ---: |
| Entrées physiques avant l’allocation du tampon de copie | 122 | 122 |
| Entrées après cette allocation | 122 | 123 |
| Entrées parcourues par le chargeur | 122 | 122 |
| Dernière page couverte par le parcours | oui | non |

L’allocation du tampon scinde une région de la carte physique sur l’ISO du raccord. Limine conserve la longueur lue avant cette allocation. La copie perd alors la dernière entrée, bien que celle-ci figure dans la carte transmise au noyau. Le contrôle mémoire de NovaOS détecte correctement la traduction manquante. Les enregistrements des deux chargeurs figurent dans `mesure-hhdm-{reference,candidat}/instantanes.json`.

## Propriété et restitution du cadre

La carte UEFI indique un cadre de type `EfiBootServicesData`, cache WB, sans attribut de conservation pour les services d’exécution du firmware (`EFI_MEMORY_RUNTIME` absent). Son contenu présente 64 descripteurs de la carte mémoire interne DXE d’EDK II, de 64 octets chacun, identifiés par leur structure et leur signature. Les services de démarrage UEFI ont déjà pris fin à la construction de la fenêtre directe.

Au passage à EL1, les parcours des deux racines de traduction comptent 54 pages de tables actives. Le cadre ne sert ni de table ni de pile. Il est extérieur aux réponses utilisées par NovaOS, à la carte UEFI transmise et aux tables de configuration examinées. Avec le chargeur corrigé, son seul alias dans ces traductions est celui de la fenêtre directe. Les objets examinés et les limites de l’observation figurent dans `inventaire-efi-{candidat,corrige}-arm` et `vie-cadre-{candidat,corrige}-arm`.

Sur le noyau identique, un point de surveillance matériel établit que la première écriture observée dans le mot surveillé intervient pendant `memory::reclaim_loader`. À son entrée, les réponses Limine sont retirées, le masque des processeurs en ligne vaut 3 et les deux vCPU utilisent leurs piles privées. L’adresse de l’écriture se résout dans la fermeture de restitution des pages. Le compteur du chargeur revient à zéro. Les contrôles de restitution et de protections de l’allocateur complètent ce témoin ponctuel.

## Correction et périmètre

Le profil de qualification `--lancement` utilise Limine 10.8.5 modifié uniquement sur AArch64. La copie est dimensionnée d’après la borne déjà imposée à la carte Limine. Le chargeur relit le nombre d’entrées après son allocation, vérifie cette borne et copie la carte sans allocation intermédiaire. Cette mémoire est récupérable par le chemin normal. Le correctif fonctionne quelle que soit l’adresse ou la position de l’entrée concernée. [Source du correctif et reconstruction](../user/novaauth/service/limine_qemu/README.md)

L’expérience croisée remplace uniquement le chargeur AArch64 du candidat. Le noyau conserve le SHA-256 `1e6de87cb277057afb70886e16f0ed11b68d7544e4eb46cc9a2d82c9d891c5a7` et le relais BootID conserve `ef00e436f87bbcfd07292a74a34607a8b9e3fe18855933fdaaf00e2bd3c94480`. Le contrôle FAT de l’image réussit. Une seconde reconstruction depuis la source amont et le patch livré reproduit octet pour octet le PE qualifié, SHA-256 `7b9cae50fc4649db9fb1ed48102c2b90d5df0ae2ad1113149d1b7f8707349b7e`. Les ELF ordinaires reconstruits sont identiques aux références sur les deux architectures.

Le diagnostic porte sur QEMU et le firmware utilisé dans ces essais. La distribution ordinaire conserve son chargeur officiel, y compris le code amont sensible à cette variation de carte. Une éventuelle intervention sur le chargeur distribué relève d’un arbitrage distinct. Aucun essai UTM ou matériel physique ne découle de cette qualification.
