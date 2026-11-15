# Phase 4 — Prise de contrôle PCIe et retrait DMA sous Intel VT-d

**Qualification du 14 novembre 2026.** Sur QEMU 11.1.3 Q35 x86_64, NovaOS protège les pages testées même lorsqu'une écriture interdite ne déclenche aucune interruption. Un périphérique ou un groupe dont l'état initial est incertain ne reçoit aucun droit. La restitution d'un cadre exige une fin de commande vérifiable puis le retrait confirmé des traductions. Ces conclusions couvrent le profil expérimental EDU de la branche `codex/materiel-0180`. [Qualification de phase 3](materiel-phase3-v0.18.0.md)

## Périmètre reproduit

Le noyau de qualification provient du commit `b140be8c83cb2ab49b05b1aeb3c26919fd67c81f`, issu de `342d561a9dfe4a41f250efe0a1774b62e84c4ffa`. Les 19 VM de cette campagne ont utilisé le même ELF, SHA-256 `3c473987ea872da33cefdef80cb956284f6950c8fe5ca79b1deeb5b135db880e`. Dix VM exercent le remappeur ou un refus d'initialisation, neuf la reprise PCI/DMAR. Une des dix redémarre dans le même processus QEMU, soit vingt démarrages au total. Les régressions MMIO/IRQ et SMP s'ajoutent à ce décompte. [Archives VT-d](validation/materiel-phase4-vtd.json) · [Archives PCI](validation/materiel-phase4-pci.json)

Le banc utilise TCG, 256 Mio, le CPU `max`, le firmware du profil de démarrage x86_64 et un CD en lecture seule. Le modèle nominal associe EDU directement au bus zéro, RID `00:06.0`, SID 48, un seul DID actif de valeur 1, quatre niveaux de traduction et une largeur de 48 bits. La compilation `vtd-lab` et le scénario de démarrage sélectionnent ce laboratoire. Les anciens clients, l'ABI, les chemins de stockage et le format NovaFS v0.9.13 P9 conservent leur contrat. Les E/S classiques et le polling VirtIO-blk restent les valeurs par défaut. [Configuration du banc](../tests/vtd_edu.py) · [Fonctionnalités du noyau](../kernel/Cargo.toml)

## Prévention, notification et acquittement

| Essai | Protection observée | Diagnostic matériel observé |
|---|---|---|
| Écriture sur une page RO dont la traduction vient de servir | Les 8 192 octets des deux pages conservent leurs motifs distincts. | 1 024 consultations de la traduction RO en cache, zéro FRCD valide, zéro MSI. Le masque était ouvert avant et après la commande. |
| Même opération sur une génération de mêmes droits, après invalidation | Les deux pages demeurent intactes. | Un FRCD de raison 5, une MSI, acquittement W1C vérifié. |
| Lecture d'une page WO | Le courtier interdit la publication du tampon interne comme source et refuse la commande de réexportation. | Un FRCD de raison 6 et une MSI sur le BSP. Le collecteur tourne sur le CPU 0, 1 ou 3 selon la taille de la VM. |
| Écriture hors plage ou à l'adresse témoin d'un autre processus | Les pages du titulaire restent intactes. L'autre processus vérifie lui-même ses 4 096 octets. | Un FRCD de raison 5 et une MSI par scénario. |
| Deux commandes successives vers la même ancienne IOVA, après retrait et réallocation des cadres | Le superviseur récupère les deux cadres exacts auprès de l'allocateur, vérifie leur remise à zéro et écrit deux nouveaux motifs. Les 8 192 octets restent intacts après les deux commandes. | Deux FRCD de raison 2 et deux MSI. L'acquittement du premier précède la deuxième commande. |

Sur le chemin chaud de QEMU, l'émulation VT-d retourne la traduction et ses permissions mémorisées. La couche mémoire de QEMU écarte ensuite l'écriture sans appeler le chemin d'enregistrement de faute du remappeur. À froid, le parcours de table de VT-d détecte le défaut, remplit FRCD et déclenche la MSI. Les six notifications froides de chaque campagne standard ont été individuellement recueillies, acquittées et réarmées. Le témoin de lecture interdite vérifie le refus de sortie des données par le courtier. Il ne lit pas le tampon interne de l'EDU. [Diagnostic et rejeu de l'archive](materiel-phase4-fautes.md) · [Traduction VT-d de QEMU v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c) · [Permissions de l'espace d'adressage QEMU](https://github.com/qemu/qemu/blob/v11.1.3/system/physmem.c)

## Prise de contrôle du groupe

Le BSP recense le groupe, contrôle les cinq fonctions attendues et leur configuration, lit les registres EDU et examine les six ports AHCI avant toute écriture dans la configuration PCI. Il accepte uniquement la carte EDU vierge directement raccordée au bus racine. Après neutralisation et relecture du bus mastering, il configure VT-d, publie les feuilles RO et WO, confirme les invalidations et peut alors ouvrir BME pour le pilote autorisé. Une transition qui échoue après le début de la fermeture s'arrête avec les ressources du chargeur réservées. Certains pairs peuvent alors avoir déjà leur BME fermé. [Procédure et neuf essais négatifs](materiel-phase4-pci.md)

La présence d'un périphérique supplémentaire, d'un slot multifonction, de fonctions sœurs ou d'un pont entraîne un refus. Le port racine testé expose ACS `0x005f`, contrôle `0x0000`. Une EDU dont RUN vaut 1 et un tag AHCI réservé sur le dernier port font également refuser la délégation avant toute écriture PCI. Les six registres PxCMD du témoin AHCI gardent leur valeur d'origine. Après un véritable reset système, la longueur EDU vaut encore 4 096 et sa direction vaut 2. Le second noyau refuse la reprise, l'hôte retrouve RTADDR et GSTS à zéro et la trace ne contient aucun nouveau DMA de l'EDU. Dans ces refus précoces, BME peut garder sa valeur firmware. La période antérieure à la prise de contrôle reste hors garantie. [Topologies et registres mesurés](validation/materiel-phase4-pci.json) · [Second démarrage, entrée normal à 1 vCPU](validation/materiel-phase4-vtd.json)

## Révocation, réemploi et quarantaine

| État à la fermeture de l'admission | Résultat pour le domaine et ses cadres |
|---|---|
| Réservation jamais soumise | Aucune programmation DMA. Le processus quitte, la demande tardive est refusée. Après retrait confirmé, le contexte est vide et les deux cadres sont restitués une fois. |
| Commande en vol, révocation ou destruction du pilote | L'EDU est observée RUN actif. Les pages restent épinglées jusqu'au contrôle du repos et de la commande exacte, à la fermeture de BME puis à la confirmation du retrait VT-d. Une seconde restitution échoue. |
| Reçu WAIT provisoirement masqué pour le vérificateur | Deux pages retenues et tout nouveau bail refusé. Quand le vérificateur retrouve le reçu attendu et la file consommée, il termine le retrait. |
| Drainage EDU confirmé, erreur de file IQE | Contexte retiré, QMP relève FSTS = 0x10, tête = 0x60 et queue = 0x90. Les deux cadres restent épinglés avec les huit cadres de service, même après nouvelle demande. |
| Registre de fin EDU incohérent | Le banc altère uniquement la longueur après un transfert réel autorisé. RUN et BME sont à zéro. La fermeture conserve le contexte, les deux feuilles aux permissions initiales et les dix cadres. QMP vérifie les pages octet par octet. Les tentatives répétées de drainage et de nouvelle attribution échouent. |

Le courtier vérifie le démarrage de la commande et recoupe ses adresses, sa longueur, son sens et son état final. La preuve de drainage et de fermeture de BME reste empruntée pendant le retrait des tables. L'IOMMU invalide les caches de contexte puis l'IOTLB avec drainage en lecture et écriture. Le noyau vérifie le numéro du reçu WAIT, la consommation de la file et l'état de l'unité avant de restituer les pages. Une commande dont le départ ou la fin reste incertain ne produit aucune preuve de drainage. [Courtier et contrôle hôte du retrait](materiel-phase4-retrait.md)

Le DID 1 est recréé à chaque nouveau bail. Le noyau vérifie les droits des feuilles courantes et l'absence des deux IOVA de l'époque précédente. Une tentative de soumission avec l'ancien bail est refusée même lorsque BME est ouvert pour le nouveau titulaire. L'adresse numérique d'une IOVA peut resservir dans le même bail. Entre deux époques, les fenêtres restent disjointes. Après 3 584 époques le courtier refuse toute nouvelle attribution. Le test unitaire couvre la dernière attribution, l'épuisement et le débordement. [Gestion des générations](../kernel/src/hardware/vtd_lab/format.rs) · [Scénarios de révocation](../kernel/src/sched/process/vtd.rs)

## Régressions et reproduction

| Validation exécutée | Résultat |
|---|---|
| Q35 VT-d standard à 1, 2 et 4 vCPU | Dix scénarios par profil. À 2 et 4 vCPU, le même pilote migre respectivement du CPU 1 ou 3 vers le BSP et émet sa commande suivante. Les retraits en vol, dont la mort du pilote, s'achèvent sur le CPU 1 ou 3. |
| Profils Q35 complémentaires | Les sept VM valident l'annulation avant soumission, la perte de preuve EDU, le reçu retardé, IQE, l'absence d'IOMMU, la largeur 39 bits et le refus de CAP.CM=0. |
| Modèles d'inventaire, PCI, DMA et VT-d | 65/65 dans dix suites sur Darwin ARM64 natif et 65/65 sous Rosetta x86_64. |
| Ancien pilote EDU MMIO/IRQ, 2 vCPU | 11/11 sur QEMU AArch64 et 11/11 sur QEMU x86_64. |
| Noyau x86_64 hors laboratoire et FPU/SIMD | Compilation réussie. Les deux essais QEMU à 4 vCPU, ordonnancement SMP et FPU/SIMD, ont réussi. |

Les huit pages de service occupent 32 Kio. Un bail peut ajouter 8 Kio de données. Les sentinelles de réemploi retiennent temporairement les deux cadres après leur remise à l'allocateur. Chaque scénario standard se termine avec zéro cadre client, zéro page IPC et les compteurs du tas revenus à leur état initial. En quarantaine permanente, les dix cadres occupent 40 Kio. Ces mesures portent sur le laboratoire de DMA. Les campagnes longues NovaFS, les coupures de stockage, l’endurance IPC, les benchmarks et l’interface graphique n’ont pas été rejoués pendant cette phase ciblée. Les résultats antérieurs gardent leur date et leur périmètre. [Relevé des régressions ciblées](validation/materiel-phase4-regressions.json) · [Contrôles antérieurs de stockage](materiel-phase3-v0.18.0.md#ressources-compatibilité-et-reproduction)

Depuis la racine du dépôt, chaque campagne VT-d crée de nouvelles preuves sous `novaOS/build/vtd-phase3/`. Les commandes suivantes reproduisent les modèles et les dix-neuf VM principales. Le rapport de fautes donne également la commande de vérification hors VM des captures à 1, 2 et 4 vCPU.

```sh
make -C novaOS materiel-modeles-test
make -C novaOS ARCH=x86_64 vtd-edu-test
```

**Décision proposée.** Soumettre la phase 4 à validation MOA/MOE pour le seul laboratoire Q35. Conserver le profil expérimental et la mise en quarantaine en cas d'incertitude. Une extension à un contrôleur ou un groupe supplémentaire exige une preuve propre de routage et de fin des transactions. La notification de chaque accès bloqué, le routage pair-à-pair général, le hotplug, le FLR individuel et le comportement d'une plateforme Intel physique restent hors qualification. Aucun essai Dynabook n'a été effectué.
