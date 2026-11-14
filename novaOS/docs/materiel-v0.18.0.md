# NovaOS v0.18.0 — Fondations matérielles, phase 1

## État de départ et responsabilités

Branche de travail issue de v0.17.0, `010ae3998aa158811b8a05c6d47753cd4589ad9a`. Le choix de cette phase et les identifiants du Tecra proviennent du mandat MOA. [Inventaire de référence au SHA d’audit](https://github.com/NaviStorm/NovaOS/blob/a602051c9f3f1119552dd51a6b58ffc64285c556/novaOS/docs/audit-v0.30.0/inventaire-materiel-confiance.md)

| Fonction | Existant vérifié dans le code v0.17.0 | Manquant | Action de phase 1 |
|---|---|---|---|
| ACPI | Limine fournit carte mémoire, HHDM, CPU et framebuffer. Le noyau sélectionne la base 3 et reconstruit ses propres tables. [Passage de relais](../kernel/src/boot.rs) · [MMU](../kernel/src/memory/mod.rs) | Lecture bornée du RSDP et des tables | Copier les données firmware autorisées avant la restitution du chargeur. |
| PCIe | Le backend bloc accède aux fonctions du profil par ports sur x86, ECAM fixe sur ARM. Sa découverte VirtIO peut préparer des écritures spécifiques au pilote. [Configuration et transport](../kernel/src/hardware/block/pci.rs) | Inventaire générique des fonctions et des ponts | Séparer la lecture de configuration de l’admission d’un pilote. |
| MMIO | Fenêtre superviseur Device/UC et sous-régions propres au bloc. [Accès matériels](../kernel/src/hardware/block/io.rs) | Attribution générique bornée et cycle de retrait | Médiation par ressource autorisée, sans exporter de pointeur physique vers les clients. |
| MSI/MSI-X | Routage MSI-X du profil Q35, INTx du profil ARM retenu et traitement des IRQ du transport. [Pilote](../kernel/src/hardware/block/mod.rs) · [Q35](../kernel/src/hardware/block/vtd.rs) | Gestion générique de route et d’acquittement | Inventorier les capacités et exercer le cycle masque/notification/retrait sur backend de qualification. |
| DMA/IOMMU | Bail du pilote, commandes en vol, retrait des traductions et quarantaine existants. [Propriété](../kernel/src/hardware/block/ownership.rs) · [Campagne](reservations-dma-v0.17.0.md) | Domaines physiques génériques Intel, AMD et ARM | Garder le défaut fermé. Identifier séparément la description firmware et la preuve d’isolation. |
| Pilotes utilisateur et ABI | Processus isolés, service, relais, pilote bloc, amorçage par le lanceur de confiance. [Services](../user/storage/) · [Publication](../kernel/src/sched/process/storage.rs) · [ABI](../abi/native.rs) | Consommateur d’inventaire et association des identités | Remettre des descriptions bornées à un processus sans droit de programmation du matériel. |

## Du firmware au processus utilisateur

La nouvelle découverte s’exécute à la demande. Le profil `materiel-inventaire` reçoit les descriptions ACPI que la carte mémoire permet de lire. Le profil de laboratoire `materiel-inventaire-qemu` ajoute le droit de lecture de configuration sur la plateforme prévue. Sur x86_64 il emploie le mécanisme PCI n° 1 de Q35. Sur AArch64 il impose la fenêtre ECAM du modèle `virt`. L’opérateur choisit le profil dans la ligne de commande de l’ISO. Ce choix est une décision de confiance du banc. Le démarrage ordinaire conserve le chemin de stockage classique et le polling. [Entrée noyau](../kernel/src/main.rs) · [Capture](../kernel/src/hardware/discovery.rs) · [Construction des ISO](../scripts/iso.sh)

1. Le processeur d’amorçage lit le RSDP physique fourni par Limine en révision de base 3. La lecture précède le lancement des processeurs secondaires et la restitution des pages du chargeur. Le noyau utilise une seule page virtuelle temporaire, superviseur, RO/NX. Il vérifie que chaque page ACPI est entièrement couverte par une entrée ACPI reclaimable ou NVS et refuse toute entrée contradictoire. Pour le DTB, chaque page doit provenir du chargeur et posséder un alias HHDM vérifié. La suppression d’une traduction temporaire invalide le TLB avant restitution des tables. [Lecteur physique](../kernel/src/memory/firmware.rs) · [Protocole Limine](https://github.com/Limine-Bootloader/limine-protocol/blob/trunk/PROTOCOL.md#base-revision-3)
2. Le lecteur ACPI valide les checksums du RSDP et des tables, le type de la racine RSDT/XSDT, les additions d’adresses et les recouvrements. Il exige une FADT et vérifie sa taille selon la révision. Il contrôle les longueurs des structures MADT/SRAT, la matrice SLIT, les fenêtres MCFG, les structures et portées DMAR et les renvois et cycles IORT. Les tables inconnues conservent seulement leur signature après contrôle de l’enveloppe. L’inventaire conserve la révision, les signatures et les fenêtres MCFG. Le budget est de 128 tables enfants, 128 Kio par table, 512 Kio de tables racine comprise, 32 fenêtres MCFG et 256 nœuds IORT. Un dépassement rend le diagnostic incomplet et empêche sa remise comme inventaire valide. AML, DSDT, NUMA actif et configuration des contrôleurs restent hors de cette phase. [Lecteur et contrôles](../kernel/src/hardware/acpi.rs) · [ACPI 6.6, chapitre 5](https://uefi.org/specs/ACPI/6.6/05_ACPI_Software_Programming_Model.html) · [Arm IORT](https://developer.arm.com/documentation/den0049/latest)
3. En l’absence de RSDP sur ARM, le profil QEMU reconnaît le DTB de `virt` et un unique contrôleur actif `pci-host-ecam-generic` directement sous sa racine. Il exige le domaine zéro, le bus initial zéro et une taille cohérente avec le dernier bus à la base `0x4010000000`. Le blob est limité à 2 Mio et 32 niveaux. Chaque lecture ECAM se fait avec l’attribut Device et exclut les plages RAM, firmware et framebuffer. Un RSDP présent mais invalide ferme la découverte. Une plateforme physique inconnue ne reçoit aucune autorisation PCI par ce mécanisme. [Lecteur DTB](../kernel/src/hardware/virt_dtb.rs) · [Machine virt de QEMU](https://www.qemu.org/docs/master/system/arm/virt.html) · [Description du contrôleur ECAM](https://www.kernel.org/doc/Documentation/devicetree/bindings/pci/host-generic-pci.yaml)
4. L’énumérateur prend un segment et une plage de bus, visite les fonctions présentes et suit les ponts configurés. L’adaptateur invité qualifié sélectionne uniquement le segment zéro, bus racine zéro. Il limite chaque relevé à 131 072 lectures de dword et lit les 256 premiers octets de configuration. Les en-têtes de type 0 et 1, les fonctions multiples, l’identifiant, la classe, l’interface, les BAR, les capacités MSI/MSI-X/PCIe et les indications INTx alimentent le diagnostic. Un pont refusé interrompt le parcours de sa plage. Une fonction instable, invalide ou un inventaire de plus de 128 fonctions empêche la remise du résultat au service. Les BAR non nulles donnent le type et la base. La détermination destructive de leur taille, l’accès aux registres du périphérique, les capacités PCIe étendues, l’activation du bus mastering et le hotplug ne font pas partie du relevé. [Énumération](../kernel/src/hardware/pci_inventory.rs) · [PCI Express Base](https://pcisig.com/specification-overview/pci-express-base)
5. Le noyau copie l’inventaire dans une page privée utilisateur RO/NX. Le contrat privé `NDI1` contient un en-tête de 8 octets, les signatures ACPI et des fiches de 16 octets. Chaque fiche contient BDF, identité, classe, interface, masque des BAR, indications IRQ et type d’en-tête. Le journal superviseur fournit le détail des bases BAR et des fenêtres MCFG. Le processus `devices` lit le cliché, consulte le catalogue, signale les identités connues et le nombre de fonctions reçues. En Ring 3 ou EL0, il vérifie lui-même le refus de l’amorçage IPC privilégié et des deux portails du pilote bloc. Il termine après le diagnostic. Aucun numéro d’appel système, droit d’un client historique ou structure de l’ABI publique n’a changé. [Lanceur](../kernel/src/sched/process.rs) · [Programme utilisateur](../user/devices.rs) · [Format privé](../user/device_inventory.rs)

## Contrat des futurs pilotes

| Ressource | Décision et contrôles | Portée de la preuve |
|---|---|---|
| MMIO | Le backend privilégié sélectionne une sous-région exclusive, une largeur, un attribut UC x86 ou Device ARM, un propriétaire et des droits. Le courtier contrôle l’époque, les débordements, les bornes, l’alignement et le sens à chaque opération. Le processus conserve un handle opaque. | Contrat Rust et backend de test. Pour créer un backend physique, vérifier indépendamment les limites et les effets de chaque registre. Une base BAR seule ne fournit pas de taille ni de permission. |
| Interruptions | Attribution initialement masquée, vérification de la source, une notification en attente, acquittement par le propriétaire et démasquage explicite. Les notifications tardives, parasites ou d’une ancienne époque sont écartées. | Testé sur backend simulé. INTx/MSI/MSI-X génériques restent à router et à qualifier sur le contrôleur réel. La détection d’une capacité PCI n’arme aucun vecteur. |
| Arrêt et disparition | Fermer immédiatement l’admission. Le retrait privilégié doit arrêter et purger les sources, attendre la fin des accès DMA et retirer les traductions. En cas de doute, conserver la propriété en quarantaine. Rendre la ressource réattribuable après confirmation du retrait. | Mort du pilote, échec partiel, reprise et refus de réemploi testés sur le courtier. Les essais du transport VirtIO existant exercent aussi un retrait réel dans QEMU. |
| Supervision | Une correspondance constructeur/modèle unique et une autorisation liée à la fonction, à l’incarnation, à l’image et à ses droits minimaux sont requises. États observé, démarrage, actif, retrait et quarantaine. Une relance consomme une nouvelle génération. | Automate testé. Le catalogue de pilotes physiques est vide. Le lancement, la surveillance, la révocation après panne du gestionnaire, et le chargement dynamique restent à raccorder au superviseur. |

Les pilotes nécessaires au démarrage conservent le chemin de lancement de confiance existant. Un futur catalogue devra distinguer ces pilotes résidents des pilotes optionnels. Le courtier générique n’expose actuellement aucune porte utilisateur vers le matériel. Son intégration future devra sérialiser les accès, la réception d’IRQ et le retrait sur plusieurs CPU. [Courtier de ressources](../kernel/src/hardware/driver_resources.rs) · [Supervision utilisateur](../user/device_lifecycle.rs)

### DMA et IOMMU

Le pilote bloc existant conserve le propriétaire, l’époque, les cadres privés épinglés et le sens de chaque transfert. Lors du retrait il attend l’achèvement en vol, arrête le périphérique, vérifie la désactivation du bus mastering, retire la route, purge les IRQ et retire chaque traduction puis le domaine. Toute confirmation manquante garde les cadres en quarantaine. Les preuves de cette branche portent sur les profils virtuels existants. [Chemin de retrait](../kernel/src/hardware/block/ownership.rs) · [Qualification de référence](reservations-dma-v0.17.0.md)

Pour un futur pilote physique, le superviseur devra identifier l’unité de remappage et le groupe d’isolation, attacher le requester ID ou stream ID à un domaine fermé avant le premier DMA, puis épingler et mapper uniquement les cadres et sens nécessaires. Les buffers resteront propriété de l’instance jusqu’au retrait. Il faudra traiter la cohérence cache appareil/CPU lorsque la plateforme l’exige. Au retrait, la fin d’un transfert, l’invalidation de traduction et le drainage des transactions déjà émises exigent chacun une preuve appropriée. Si ATS est actif, les traductions de l’appareil entrent également dans ce contrat.

| Famille | Donnée de découverte | Point à qualifier avant admission |
|---|---|---|
| Intel VT-d | La présence de DMAR et ses portées identifient des unités candidates. Le backend actuel vise Q35. | Domaine par source et alias, invalidations contexte/IOTLB, drainage approprié et achèvement de la file avant restitution. [Spécification Intel VT-d](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf) |
| AMD-Vi | Une table IVRS éventuelle reste opaque dans cette phase. | Interpréter IVRS, les alias et plages réservées, programmer les permissions, invalider DTE/IOTLB et attendre la complétion requise. [AMD IOMMU](https://www.amd.com/content/dam/amd/en/documents/processor-tech-docs/specifications/48882_IOMMU.pdf) |
| Arm SMMU | IORT décrit des relations et identifiants. Le transport VirtIO-IOMMU déjà qualifié dans QEMU a un autre backend. | Résoudre le StreamID, installer les traductions, synchroniser les commandes d’invalidation et drainer les accès avant désépinglage. [Arm SMMUv3](https://documentation-service.arm.com/static/63d7a2d5e4378a55c5e045b9) |

Aucun de ces trois contrats physiques n’est qualifié sur le Tecra ou une carte ARM. Le gestionnaire ne peut attribuer de DMA physique. Certaines fonctions virtuelles apparaissent avec le bus mastering déjà activé par le firmware. L’inventaire lit cet état sans le modifier. Aucun essai de cette phase ne démontre l’isolation DMA d’une plateforme pendant son firmware.

## Dynabook Tecra A50-J

| Identifiant | Diagnostic prévu | Pilote NovaOS sur ce matériel |
|---|---|---|
| `8086:9A49` | Intel Iris Xe | Absent |
| `8086:15FC` | Intel Ethernet I219-V | Absent |
| `8086:A0F0` | Intel Wi-Fi AX201 | Absent |
| `8086:A0ED` | Intel USB xHCI | Absent |
| `10EC:522A` | Realtek SD | Absent |
| `1E0F:0001` | Kioxia NVMe | Absent |

Le test du catalogue exerce ces six paires et un identifiant inconnu. Le relevé en VM associe à chaque fonction son BDF, sa classe, ses ressources descriptives, l’état d’inventaire et l’absence d’attribution. Les identifiants annoncés pour le portable proviennent de la MOA. Aucun relevé physique, changement de partition ou accès à son SSD n’a été effectué. Les implémentations sont en Rust natif. La consultation des sources de référence Linux n’a donné lieu à aucun import de code GPL.

## Qualification du 14 novembre 2026

Le code et ses tests sont au commit `8a6ce9720eb72c871001bd7e69fd556d6cfb7041`. Hôte macOS 27.2 (26B123) arm64, rustc stable 1.99.1. Les invités utilisent QEMU 11.1.3, TCG, 256 Mio, le firmware EDK II du paquet QEMU et les ISO de diagnostic. Le banc ajoute un pont PCIe et une fonction VirtIO RNG en aval. Il interroge indépendamment `query-pci`, compare chaque BDF, constructeur, modèle, classe et sous-classe, puis vérifie le nombre de fonctions effectivement reçu par le processus et sa sortie en EL0 ou Ring 3. L’interface de programmation (PI) figure dans la série. QMP ne la fournit pas dans le champ de classe comparé. [Banc QEMU](../tests/materiel.py) · [Bilans, empreintes et traces série/QMP](validation/materiel-v0.18.0.json)

| Démarrage d’inventaire | Tables ACPI enfants | Fonctions QMP | Fonctions reçues par le gestionnaire | Résultat |
|---|---:|---:|---:|---|
| x86_64 Q35, accès explicite | 6 | 7 | 7 | Concordance, enfant du pont inclus, Ring 3 |
| x86_64 Q35, profil générique | 6 | 7 | 0 | Autorité PCI refusée, Ring 3 |
| AArch64 virt, GICv2, ACPI | 9, dont IORT | 4 | 4 | Concordance, enfant du pont inclus, EL0 |
| AArch64 virt, GICv2, ACPI désactivé | 0 | 4 | 4 | Fenêtre issue du DTB vérifiée, concordance, EL0 |

| Qualification ciblée | AArch64 | x86_64 | Ce que contrôle le banc |
|---|---:|---:|---|
| Modèles Rust | 46/46 natifs | 46/46 sous Rosetta | 3 contrôles des pages firmware, 13 ACPI, 5 DTB, 12 PCI, 8 MMIO/IRQ, 1 catalogue de six identités, 4 cycles de pilote |
| DMA du transport existant dans QEMU | 8/8 | 8/8 | Quatre scénarios × secteurs 512 et 4 096. Motif lu, fautes de permissions, IRQ pendante, ancien bail, panne en vol et quarantaine |
| Écriture, FLUSH puis relecture dans une nouvelle VM | 2/2 | 2/2 | Images fictives entières vérifiées, deux tailles de secteur |
| Ordonnancement à 4 vCPU | 1/1 | 1/1 | Timers par CPU, huit clients, préemption, migration et invalidations TLB distantes |
| FPU/SIMD à 4 vCPU | 1/1 | 1/1 | Deux cycles par architecture, huit clients et huit migrations vérifiées à chaque cycle |

Pendant la revue, deux cas ont conduit à durcir l’implémentation. Un pont invalide pouvait laisser un pont voisin faire explorer le même bus secondaire. Le parcours s’arrête désormais avant cette lecture, dans les deux ordres de présentation. Une entrée de carte mémoire favorable pouvait aussi couvrir une page du DTB partiellement déclarée d’un autre type. Le lecteur vérifie désormais toutes les entrées qui touchent la page, même sur un octet. Les deux refus figurent dans les tests des modèles.

### Reproduire

Depuis la racine du dépôt, avec les cibles Rust bare-metal, Limine, QEMU et les firmwares du projet installés, lancer les commandes suivantes. Le premier banc construit et exécute les deux binaires Darwin sur Apple Silicon équipé de Rosetta. Sur un autre système il identifie la cible native exercée. Les bilans QEMU sont déposés dans des sous-répertoires distincts de `novaOS/build/`.

```sh
make -C novaOS materiel-modeles-test
make -C novaOS ARCH=aarch64 materiel-test
make -C novaOS ARCH=x86_64 materiel-test
```

Les commandes suivantes reproduisent les régressions ciblées depuis `novaOS/`. Les images du banc bloc sont temporaires. Le binaire FPU est construit séparément pour chaque cible.

```sh
set -e
for isa in aarch64 x86_64
do
    (
        export NOVA_ARCH="$isa"
        make ARCH="$isa" kernel
        python3 -B tests/bloc.py --only qualify
        python3 -B tests/bloc.py --only durable
        export PYTHONPATH=tests
        python3 -B -c 'import os, smp; smp.run(os.environ["NOVA_ARCH"], 4)'
        . scripts/target.sh
        export CARGO_TARGET_DIR="$PWD/build/fp"
        export RUSTC="$(rustup which --toolchain stable rustc)"
        rustup run stable cargo build --release --locked --features user-fpu --target "$rust_target"
        export NOVA_KERNEL_ELF="$CARGO_TARGET_DIR/$rust_target/release/novaos"
        python3 -B -c 'import os, smp_phase2; smp_phase2.run(os.environ["NOVA_ARCH"], "smp-fp", 4)'
    )
done
```

### Couverture et suite de v0.18.x

Le code SMP, la sauvegarde/restauration FPU, l’ABI publique, l’arbre NovaFS v0.9.13 P9 et les archives de distribution sont identiques à v0.17.0. La branche ajoute un appel conditionnel au lanceur de processus et partage la paire d’accès CF8/CFC sous sa garde existante. Les essais ciblés couvrent ces dépendances. Les campagnes d’endurance IPC, le million d’opérations différentielles NovaFS, les coupures de publication et les benchmarks de stockage n’ont pas été rejoués. Les résultats antérieurs restent rattachés à leurs profils et à leur révision. Aucun essai UTM ou sur ordinateur physique dans cette campagne. Les manifestes et bannières gardent la version stable livrée v0.17.0. [Qualification de la version de base](fusion-smp-v0.17.0.md)

1. **Qualifier la découverte physique sans pilote.** Recueillir sur un support de laboratoire autorisé les tables et les identités de la machine cible, sans monter son SSD. Confronter MCFG aux ressources racine avant d’autoriser une lecture de configuration. Traiter les segments supplémentaires, les fenêtres d’hôtes traduites et les conflits de ressources. La lecture physique, la configuration du firmware et la présence effective des six composants restent à établir.
2. **Qualifier un seul backend de ressources isolé.** Établir la source de confiance des sous-régions BAR, le propriétaire du requester, les alias et groupes d’isolation. Sur émulateur puis sur matériel dédié, éprouver le routage, les IRQ tardives, le retrait en vol et la conservation des cadres sous contention SMP. Une panne de lecture MMIO sur une plateforme inattendue reste un risque pour le noyau. Maintenir les adaptateurs QEMU derrière leur profil explicite.
3. **Raccorder la supervision à une instance autorisée.** Définir le canal privé d’autorisation, le catalogue d’images de confiance, le redémarrage du gestionnaire et le traitement de la disparition du matériel. Qualifier le retrait réel avant d’introduire un pilote optionnel. La production d’un pilote NVMe, USB, Ethernet, Wi-Fi ou GPU et une décision de fusion restent à arbitrer.
