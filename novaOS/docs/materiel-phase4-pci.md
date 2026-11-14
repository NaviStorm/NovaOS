# Phase 4 — Admission d'un groupe PCI sur Q35

## Reprise après ExitBootServices

Sur le profil x86_64 Q35 fermé, le firmware avait laissé PCI_COMMAND à `0x0007` sur les cinq fonctions du bus racine. En phase 3, la reprise de l'EDU coupait le bus mastering avant la première lecture du moteur. L'ancienne boucle AHCI pouvait aussi arrêter un port avant d'avoir examiné le suivant. La phase 4 introduit une inspection préalable, en lecture seule, de l'ensemble du groupe admis et des six ports AHCI. En cas de doute, le noyau arrête le scénario sur le BSP et conserve les ressources du chargeur. [Découverte](../kernel/src/hardware/discovery/vtd.rs) · [Transition de phase 3](materiel-phase3-dmar.md)

1. Recenser le bus racine par CF8/CFC. Vérifier les cinq fonctions attendues, la géométrie des BAR, leurs plages et les routes. Refuser un pont, une fonction en plus, une description incomplète et un EDU qui annonce un slot multifonction, même si seule la fonction zéro répond.
2. Contrôler l'état hérité de l'EDU dans sa BAR déjà décodée. Une BAR inaccessible, un RUN actif, un calcul en cours, une IRQ pendante ou des registres DMA déjà programmés empêchent la prise de possession. Vérifier l'absence de commande AHCI active sur chaque port (CI, SACT et CLO) avant de modifier un registre PCI ou d'arrêter un moteur.
3. Adopter l'EDU vierge sous le ticket superviseur, neutraliser BME et relire le mot de commande, l'identité et le moteur. Arrêter dans l'ordre les moteurs AHCI de commandes et de réception FIS avec échéance bornée, contrôle CR/FR, puis vérification CI/SACT. Fermer BME sur les pairs. Une défaillance survenue pendant cette seconde étape interrompt la transition, sans délégation. Aucune commande disque n'est envoyée. [Procédure de transition AHCI, §10.1.2](https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/serial-ata-ahci-spec-rev1-3-1.pdf)
4. Admettre la DMAR, vérifier le DRHD, activer et confirmer la traduction. Créer le bail privé, ses deux pages, son contexte et ses permissions. Le courtier peut alors ouvrir explicitement BME pour le seul EDU. Le ticket, le bail et les fenêtres MMIO restent sous contrôle du noyau. [Courtier EDU](../kernel/src/hardware/edu_dma.rs) · [Publication et retrait VT-d](../kernel/src/hardware/vtd_lab.rs)

Le point de départ est l'exécution du BSP après le firmware sur cette topologie QEMU fixe, sans hotplug. La procédure ne peut garantir l'intégrité des données antérieures à la prise de contrôle. Un matériel qui continue une ancienne opération et un coprocesseur physique qui continuerait à soumettre des commandes sortent de ce modèle.

## Frontière PCIe

Les cinq fonctions Q35 connues incluent un seul demandeur délégué, EDU au RID `00:06.0` (`0x0030`), directement sur le bus zéro. Le chipset et ses trois fonctions ICH9 restent propriétés du superviseur. Les bus en aval, les alias de RID, les accès pair-à-pair et le partage interne entre fonctions exigeraient une justification de routage propre à chaque groupe. La politique refuse leur délégation individuelle ou commune. Elle ne modifie aucun réglage ACS. L'oracle hôte parcourt en lecture seule les capacités étendues du port testé et enregistre à la fois ce que le port annonce et les bits de contrôle actifs. La présence d'une capacité ACS ne suffit pas à établir le chemin effectif d'une transaction. [Groupes d'isolation dans la documentation VFIO](https://www.kernel.org/doc/html/latest/driver-api/vfio.html#groups-devices-and-iommus) · [Profil et ressources](../kernel/src/hardware/pci_inventory/lab_resources.rs)

## Neuf témoins négatifs x86_64

Les essais s'exécutent sur QEMU 11.1.3 Q35 en TCG, un vCPU, CD en lecture seule et VM neuve par ligne. QMP relit la configuration PCI et, en présence d'un remappeur, TE et les deux moitiés de RTADDR. Les journaux, les topologies QMP, la fixture active et les empreintes du noyau et des ISO sont versionnés. [Preuves des neuf VM](validation/materiel-phase4-pci.json) · [Banc reproductible](../tests/vtd_dmar.py)

| Configuration | Observation dans l'invité et contrôle hôte |
|---|---|
| DRHD tronqué | Le groupe au repos est repris. DMAR échoue au contrôle de longueur. La demande du pilote sans capacité échoue. EDU termine à PCI_COMMAND zéro et les quatre pairs à 3. |
| Deux DMAR | Le même chemin ferme la carte. La table dupliquée est refusée. TE et RTADDR restent nuls. |
| Deux EDU à des slots différents | L'inventaire refuse la fonction surnuméraire. L'EDU d'origine et les quatre pairs conservent la commande firmware 7. |
| EDU déjà programmée, inactive | La longueur préchargée vaut un octet, RUN vaut zéro. Le noyau conserve la commande héritée 7 et refuse avant attribution. |
| Slot EDU multifonction, fonction zéro seule | QMP relit le bit multifonction. L'invité refuse même en l'absence de fonction sœur visible. |
| EDU aux fonctions zéro et un du même slot | QMP observe les deux RID distincts. L'attribution individuelle du groupe partagé est refusée, sans action sur BME. |
| Port racine PCIe, pont PCIe-vers-PCI et EDU supplémentaire en aval | QMP recense les trois étages. Le port racine expose ACS à l'offset `0x148`, capacités `0x005f`, contrôle `0x0000`. Le noyau refuse toute attribution. |
| EDU déjà en cours à l'entrée du noyau dans la procédure | Le banc arrête TCG à l'entrée de la reprise, vérifie l'identité, la BAR et BME, puis lance une commande de lecture de longueur **zéro**. QTest observe RUN=1, puis le BSP lit lui-même RUN=1 avant toute écriture PCI. L'EDU et les pairs gardent PCI_COMMAND à 7. Aucun octet de RAM n'est transféré par le témoin. TE et RTADDR restent nuls. |
| Tag SATA NCQ réservé sur le dernier port | Le banc initialise SACT sur le port 5 alors que tous les CI sont nuls. Il ne soumet aucune commande SATA. Le noyau refuse la reprise avant écriture PCI. QTest vérifie le maintien du tag et l’identité de chacun des six registres PxCMD avant et après le refus, y compris celui du CD du firmware. |

Dans les sept derniers cas l'EDU ne reçoit aucun domaine. PCI_COMMAND à 7 matérialise la limite de confiance envers le firmware. Le témoin RUN valide la décision en présence d'une commande en cours, indépendamment des essais distincts qui transfèrent des pages sous contrôle de VT-d. Les mécanismes de fin de commande d'un périphérique physique ne sont pas qualifiés.

Depuis `novaOS/`, reconstruire le noyau `vtd-lab` et fournir son chemin ainsi qu'un répertoire de preuves neuf. Le banc utilise `llvm-nm` pour localiser l'entrée de la reprise dans le binaire exact. QTest et le débogueur TCG communiquent par FIFO privées. Aucun service réseau n'est ouvert. [Spécification EDU](https://www.qemu.org/docs/master/specs/edu.html) · [Points d'arrêt TCG](https://www.qemu.org/docs/master/system/gdb.html#breakpoint-and-watchpoint-support)

```sh
python3 -B tests/vtd_dmar.py \
  --elf build/vtd-phase3/cargo/x86_64-unknown-none/release/novaos \
  --preuves "$(mktemp -d build/vtd-phase3/admission.XXXXXX)/resultats"
```
