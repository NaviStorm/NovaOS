# Phase 2 — Ressources PCIe et contrat DMA

La découverte en lecture seule de la phase 1 fournit l’identité, la classe, les capacités et la base des BAR. La phase 2 ajoute les fenêtres des ponts et une politique d’attribution pour la carte EDU d’un banc QEMU fermé. Cette politique exige à la fois la construction `materiel-lab` et le scénario `materiel-pilote-qemu`. [Base de qualification](materiel-v0.18.0.md) · [Découverte et adoption](../kernel/src/hardware/discovery.rs)

## Découverte et attribution des BAR

L’énumérateur lit les fenêtres des ponts PCI de type 1, y compris les moitiés hautes des fenêtres I/O 32 bits et mémoire préfetchable 64 bits. Les limites publiées sont inclusives. Une base supérieure à la limite indique une fenêtre fermée. Le lecteur refuse les types réservés et les paires de types incohérentes avant d’explorer le bus aval. Les granularités standard prises en charge sont de 4 Kio pour les ports et de 1 Mio pour la mémoire. Les ponts à granularité I/O spécifique au constructeur, la reprogrammation des fenêtres et l’acheminement des ressources d’un pilote à travers plusieurs ponts restent à qualifier. [Lecteur des fenêtres](../kernel/src/hardware/pci_inventory/resources.rs) · [PCI Express Base Specification](https://pcisig.com/specification-overview/pci-express-base)

Le profil générique lit au maximum 131 072 dwords de configuration et livre au plus 128 fonctions par inventaire. La lecture d’une BAR à base nulle ne distingue pas une BAR absente d’une BAR non attribuée. Dans les deux cas elle ne fournit aucune ressource au pilote. La taille d’une BAR non documentée reste inconnue, même lorsque son adresse est très alignée. Aucun profil de phase 2 n’écrit de masque de taille ni ne déplace une BAR. [Énumérateur](../kernel/src/hardware/pci_inventory.rs)

Le profil EDU exige une VM neuve, la topologie attendue sans hotplug, l’absence de ROM d’extension active et la carte `1234:11e8` directement au BDF `0000:00:06.0`. Q35 utilise cinq fonctions racines sans VGA, et `virt` en utilise trois avec le contrôleur SCSI du CD. Le noyau attribue une géométrie aux seules BAR non nulles qui correspondent à un emplacement, une identité et un type de ce profil. Il vérifie les tailles en puissance de deux, l’alignement sur la taille entière, l’adresse de fin sans débordement et la disjonction des BAR dans chaque espace ports ou mémoire. Le tableau de contrôle possède au plus six entrées par fonction. Une géométrie de BAR inconnue ou une fonction ajoutée ferme l’admission. Aucun pilote invité des fonctions voisines ne reçoit de droit supplémentaire. [Politique du laboratoire](../kernel/src/hardware/pci_inventory/lab_resources.rs)

| Fonction dans le profil | BAR | Type | Taille du modèle |
|---|---:|---|---:|
| EDU, deux architectures | 0 | mémoire 32 bits | 1 Mio |
| AHCI Q35 | 4 et 5 | ports, mémoire 32 bits | 32 octets, 4 Kio |
| SMBus Q35 | 4 | ports | 64 octets |
| VirtIO SCSI ARM | 0, 1 et 4 | ports, mémoire 32 bits, mémoire 64 bits préfetchable | 64 octets si assignée, 4 Kio, 16 Kio |

La taille de l’EDU provient de son contrat QEMU. Les autres géométries décrivent les instances du banc. Le contrôle hôte interroge `query-pci` pour la taille réelle et le type de chaque région, puis relit les bases dans l’ECAM. QMP peut renvoyer `address=-1` lorsque le décodage est désactivé alors que la base reste programmée. La double lecture permet de confronter le relevé invité au modèle et de détecter qu’une option QEMU a changé. [QEMU EDU](https://www.qemu.org/docs/master/specs/edu.html) · [Oracle du banc](../tests/materiel_pilote.py)

## Adoption et retrait de l’EDU

Le noyau contrôle la classe `00ff00`, INTA, MSI désarmé, l’absence de MSI-X et la BAR0 mémoire 32 bits non préfetchable. Cette BAR doit être intégralement hors de toutes les régions Limine, RAM et réservations comprises. L’ouverture autorisée est `[0x80000000, 0xb0000000[` sur Q35 et `[0x10000000, 0x30000000[` sur ARM. L’opération a lieu sur le BSP, après le firmware et avant les autres CPU et processus. Après une dernière lecture de l’identité, de la commande et de la base, le noyau peut écrire le seul mot `PCI_COMMAND`, puis le relire. La fonction n’accepte ni adresse de configuration ni valeurs de BAR fournies par l’espace utilisateur. [Contrôle firmware](../kernel/src/memory/firmware.rs) · [Adoption de la fonction](../kernel/src/hardware/discovery.rs)

| Observation QEMU 11.1.3 sur VM neuve | Base EDU | Commande au passage de relais | Commande après adoption |
|---|---:|---:|---:|
| x86_64, Q35 sans VGA | `0x80000000` | `0x0007` | `0x0002` |
| AArch64, virt/GICv2 | `0x10000000` | `0x0000` | `0x0002` |

Sur Q35, OVMF avait activé le bus mastering. NovaOS le coupe avant attribution. Sur ARM, NovaOS ouvre le décodage mémoire du témoin. Dans les deux cas l’état adopté est MSE=1, I/O=0, BME=0. Le code n’émet aucune commande DMA sur cette carte. La lecture du registre de statut et des registres DMA vérifie que le témoin est au repos. Ces mesures commencent au passage de relais du firmware. Elles n’établissent aucune isolation avant ce point. [Backend EDU](../kernel/src/hardware/lab_irq.rs)

La fin d’un bail de pilote exige le retrait de la PTE utilisateur et des notifications IRQ. Le décodage PCI de la fonction et l’autorisation du processus sont deux états indépendants. La fermeture finale coupe le décodage et le bus mastering. Les six relectures indépendantes de `PCI_COMMAND` dans QMP donnent zéro. [Ticket de fermeture](../kernel/src/hardware/lab_irq/pci.rs) La preuve de restitution des droits et des cadres repose sur les contrôles de retrait du noyau, les fautes matérielles du processus et les bilans de ressources. [Oracle par scénario](../tests/materiel_pilote.py) · [Retrait de la page utilisateur](../kernel/src/memory/user.rs)

## Domaine DMA physique préparé

L’entrée commune `physical_domain` refuse toute attribution de DMA physique. Aucun backend VT-d physique, AMD-Vi ou SMMU n’est installé derrière elle. EDU fonctionne avec le bus mastering coupé. Le chemin du pilote bloc dispose toujours de son mécanisme antérieur de bail et de retrait. [Contrat préparatoire](../kernel/src/hardware/physical_dma.rs) · [Retrait du transport bloc](../kernel/src/hardware/block/ownership.rs)

| Étape | Obligation du futur backend et du superviseur |
|---|---|
| Rattachement | Résoudre l’unité, le groupe entier et ses alias. Maintenir une instance exclusive du groupe. Fermer le DMA hérité avant de remettre un jeton lié au groupe, au propriétaire et à une époque monotone. |
| Préparation | Vérifier le propriétaire de chaque cadre RAM, l’IOVA, les limites et le sens vu de l’appareil. Épingler avant de configurer la traduction. Le budget de la table est fixé par le superviseur. Refuser un cadre ou une IOVA en double. |
| Publication | Terminer la maintenance de cohérence nécessaire, publier et synchroniser les traductions, puis autoriser la source. Fermer l’ajout de pages à cette instance dès sa publication. |
| Révocation | Fermer immédiatement les admissions. Drainer ou arrêter les commandes, retirer les traductions, invalider l’IOTLB et, si nécessaire, l’ATC des périphériques. Attendre l’acquittement et assurer la cohérence côté CPU. Restituer les cadres ensuite. |
| Retrait incertain | Conserver groupe et pages épinglées en quarantaine. Le superviseur peut reprendre le retrait après décès du client ou échec partiel du rattachement. Toute ancienne époque doit rester inutilisable. |

Les interfaces abstraites ne programment aucun registre IOMMU. Pour Intel VT-d il reste à qualifier les portées DMAR, les alias, les invalidations IOTLB et le drainage. Pour AMD-Vi il faut résoudre les identifiants et alias d’IVRS, invalider DTE/IOTLB et vérifier les complétions. Sur Arm il faut résoudre les StreamID, publier les entrées appropriées et observer la complétion des invalidations SMMU. Une barrière CPU seule ne démontre pas le retrait des accès d’un appareil. [Intel VT-d](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf) · [AMD IOMMU](https://www.amd.com/content/dam/amd/en/documents/processor-tech-docs/specifications/48882_IOMMU.pdf) · [Arm SMMUv3](https://documentation-service.arm.com/static/63d7a2d5e4378a55c5e045b9)

## Résultats ciblés

- Les **18 tests PCI** passent sur Darwin ARM64 natif et x86_64 sous Rosetta. Ils couvrent notamment les fenêtres des ponts, la mémoire 32 et 64 bits, les ports, les plafonds d’adresse, l’absence de fonction, les fonctions multiples, les chevauchements et le refus de BAR inconnue. [Tests PCI](../kernel/src/hardware/pci_inventory/tests.rs)
- Les **4 tests DMA préparatoires** passent sur chacune de ces architectures. Le backend simulé conserve ses compteurs de cadres épinglés lors d’échecs injectés au rattachement, au mapping, à la publication, au drainage, à l’invalidation ou à la confirmation. Les tests vérifient la conservation des ressources et leur restitution unique après confirmation, ainsi que le refus du jeton périmé. [Tests de propriété](../kernel/src/hardware/physical_dma/tests.rs)
- **Quatre démarrages QEMU** ont validé l’inventaire, dont les deux ARM avec et sans ACPI. Les trois relevés autorisés concordent avec les fenêtres de pont de QMP. Le quatrième vérifie le refus de PCI hors attribution. La matrice d’exécution du pilote utilise des VM distinctes, sans pont PCI-PCI, pour qualifier la route exclusive d’EDU. [Banc d’inventaire](../tests/materiel.py) · [Banc du pilote](../tests/materiel_pilote.py)

Aucun essai de cette phase ne qualifie un domaine DMA physique ni un périphérique du Dynabook. L’admission d’un pilote sur matériel réel nécessite notamment une politique de ressources de confiance propre à la plateforme et la preuve d’un retrait acquitté des transactions et traductions pour tout son groupe.
