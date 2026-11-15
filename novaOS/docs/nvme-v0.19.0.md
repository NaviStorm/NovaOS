# NovaOS v0.19.0 — Conception du premier pilote NVMe isolé

**Statut au 15 novembre 2026.** Proposition soumise à la MOA/MOE sur la base de NovaOS v0.18.0, merge `386e53da7e9abb7ce1ef5e32bd64f5c667c74bb7`. La version de NovaOS reste 0.18.0, celle de NovaFS 0.9.13 P9. Le premier jalon proposé est une lecture de données témoins depuis un namespace NVMe QEMU jetable, avec un pilote Rust en Ring 3. Les spécifications suivantes décrivent les étapes jusqu’à un montage P9, les critères d’arrêt et les observations qui permettraient d’autoriser une écriture ultérieure. [État de départ](fusion-materiel-v0.18.0.md)

| Livrable | Question traitée |
|---|---|
| [Contrôleur et files](nvme-v0.19.0-controleur.md) | Comment initialiser un contrôleur, identifier le support, exécuter une commande et déterminer son issue ? |
| [IPC, capacités et DMA](nvme-v0.19.0-contrats.md) | Qui peut autoriser une opération, publier un descripteur ou restituer un cadre ? |
| [Qualification future](nvme-v0.19.0-qualification.md) | Quelle observation indépendante autorisera le passage du disque témoin au montage P9 ? |

## Architecture proposée

```text
Applications → service de fichiers NovaFS P9 → service bloc
                       IPC de fichiers             │
                                                   │ requête bornée au volume
                                                   ▼
                                    pilote NVMe Rust en Ring 3
                                                   │ intention typée + ticket
                                                   ▼
                                  courtier matériel du superviseur
                                    │ files, traductions et MMIO
                                    ▼
                                  contrôleur PCIe NVMe → namespace
```

| Responsable | Responsabilité et frontière |
|---|---|
| Applications et service de fichiers | Les applications conservent leurs droits sur les noms et les objets de leur espace. Le service NovaFS traduit ses opérations P9 en accès bloc. Il reste propriétaire de l’ordre des écritures CoW, de F1, de l’ancre et de F2. |
| Service bloc de confiance | Il contrôle le volume ouvert, les permissions et les bornes de la partition. Il lie une autorisation privée à l’opération et au tampon demandés. Il rend les données ou l’erreur au titulaire. À la fermeture du canal, le service doit faire révoquer les baux et les tickets qui en dérivent. |
| Pilote NVMe utilisateur | Il orchestre l’identification, les files, la politique de progression, les délais et le traitement des statuts. Il demande une opération sous un ticket de portée limitée. Sa mort doit rester récupérable par le superviseur sans dépendre de sa coopération. |
| Courtier du superviseur | Il détient l’accès PCI, BME, les registres sensibles, les vraies files et les pages DMA. Il construit l’entrée matérielle à partir du ticket et du tampon contrôlé, publie la commande, relit la complétion puis applique le retrait. Le pilote ne peut ni modifier une commande validée ni substituer son NSID, ses LBA ou ses adresses. |
| Gestionnaire de périphériques | Il sélectionne le groupe admissible, crée l’instance de pilote, distribue les capacités minimales et surveille son cycle de vie. Une mise en quarantaine suspend les nouvelles attributions. |

Le modèle fait confiance au superviseur, à son allocateur, au service bloc pour la politique de volume et à l’IOMMU pour appliquer les traductions. Le pilote utilisateur peut être défaillant ou malveillant. La disponibilité des données et leur durabilité dépendent aussi du respect des commandes par le contrôleur et du contrat de stockage hôte. Un contrôleur qui falsifierait ses complétions ou les effets de FLUSH sort du modèle de qualification.

Cette répartition répond à deux risques distincts. Un domaine IOMMU peut circonscrire les écritures mémoire d’un contrôleur. Il laisse néanmoins ce contrôleur capable d’écrire sur son support si un pilote peut composer librement une commande NVMe. D’où l’association d’une portée de stockage vérifiée par le service bloc et de files matérielles publiées exclusivement par le courtier. Le premier profil autoriserait un seul contrôleur, un namespace NVM ordinaire, une paire de files d’E/S en plus de la paire d’administration, une commande d’E/S à la fois et deux pages de données privées alignées de 4 Kio, une par sens DMA. Chaque commande utilise au plus une de ces pages. Ces bornes sont des choix de qualification révisables par arbitrage. [Contrat proposé](nvme-v0.19.0-contrats.md)

## Séquence de démarrage sans dépendance circulaire

1. **Charger le minimum depuis un support indépendant.** Pour le premier essai Q35, un support UEFI en lecture seule apporte Limine, le noyau et les images minimales résidentes. Le lanceur doit pouvoir créer le gestionnaire, le pilote, le relais bloc et le service NovaFS depuis cette charge initiale. Les programmes, leur configuration initiale, l’allocateur, le minuteur et le canal série doivent être disponibles même si le namespace NVMe reste absent. Le stockage de NovaOS embarque déjà des exécutables au moyen du constructeur du noyau et du lanceur de bloc. [Constructeur](../kernel/build.rs) · [Lancement de la chaîne de stockage](../kernel/src/sched/process/storage.rs)
2. **Identifier le groupe avant sa prise en charge.** Le noyau recoupe ACPI/MCFG/DMAR, BDF, RID, classe PCI NVM Express, BAR et topologie avec le profil attendu. La présence d’un second contrôleur candidat ou d’une route DMA indéterminée empêche toute délégation. Un firmware peut avoir sondé un SSD même lorsqu’il a démarré sur CD. Si le contrôleur est actif ou les anciennes files inconnues, conserver les ressources et s’arrêter. Une reprise explicite et attestée du firmware serait un livrable séparé. [Limites de prise de contrôle v0.18.0](materiel-phase4-pci.md)
3. **Établir l’isolation et identifier le support.** Le courtier réserve les pages et prépare le domaine du seul groupe admis. Il confirme les permissions et invalidations avant d’ouvrir le bus mastering. Le pilote demande l’initialisation administrative et l’identification du contrôleur et des namespaces. En cas de refus ou d’échéance dépassée, le lanceur doit rester joignable sur la console indépendante. Le premier essai exploite une attente bornée de complétion, avec MSI/MSI-X et INTx neutralisées tant que leur routage ne dispose pas d’une qualification propre. [Séquence des registres et des files](nvme-v0.19.0-controleur.md)
4. **Publier une capacité de lecture vers la cible choisie.** Le manifeste du banc fixe le namespace et le volume attendus. Le service bloc vérifie identité, capacité, géométrie et, si le scénario utilise GPT, ses deux copies, leurs CRC, les bornes et l’identifiant de partition. Un volume P9 brut de référence peut être un autre scénario explicitement sélectionné. Un fichier sur le volume à monter ne peut pas choisir lui-même le disque racine. Aucune sélection du « premier disque disponible » et aucune réparation automatique.
5. **Monter P9 en lecture seule et ouvrir le service.** Le moteur identifie le volume attendu et applique ses règles ordinaires de montage. Les services supplémentaires et `nsh` peuvent être chargés ou lancés une fois la racine publiée. Le test de lecture utilise des octets connus et une relecture indépendante du média hors VM. Un montage ambigu, une identité inattendue ou une capacité retirée laisse la racine indisponible et la console de diagnostic active. [Contrat de récupération P9](../../novaFS/docs/novafs-recovery.md)
6. **Qualifier séparément l’écriture et le redémarrage.** Un ticket d’écriture viserait exclusivement une image jetable. FLUSH doit couvrir les écritures précédentes et se terminer avec succès pour que le service confirme la persistance promise. Un résultat indéterminé après soumission remonte à l’appelant. La fermeture, l’arrêt et le redémarrage vérifient les données dans une nouvelle VM. Un futur démarrage autonome depuis le SSD demanderait en plus une protection DMA et un passage de responsabilité démontrés dès le premier accès du firmware NVMe. Le démarrage VirtIO publié ne qualifie pas cette transition. [Démarrage VirtIO existant](installer-v0.15.0.md) · [Plan de coupures et de remontage](nvme-v0.19.0-qualification.md)

Un redémarrage du processus est envisageable seulement après drainage confirmé, retrait du domaine précédent et fermeture de tous les droits. Si une écriture a pu atteindre le support, conserver son issue indéterminée et demander une relecture selon les règles du volume. Si seul le support d’amorçage reste utilisable, la console de secours conserve le motif du refus, l’époque du bail et le statut du contrôleur. Elle ne publie aucune capacité d’écriture de secours.

## Prérequis, risques et arbitrages

| Décision MOA/MOE à prendre avant le développement concerné | Proposition et conséquence |
|---|---|
| Périmètre du premier contrôleur | Ouvrir un profil x86_64 Q35 fermé, un contrôleur QEMU virtuel et un namespace identifié. Le profil VT-d actuel admet l’EDU et ses deux pages à des fins de laboratoire. Étendre la politique de groupe et le modèle DMA dans un contrat distinct. Reprendre les refus du profil EDU lorsqu’une topologie reste inconnue. |
| Frontière entre Ring 3 et superviseur | Autoriser, après revue du contrat privé, un courtier de publication minimal et auditable. Une BAR entière en écriture ou des SQ modifiables après validation permettraient de contourner les droits du volume. Mesurer la surface du code privilégié et les cadres épinglés avant d’envisager davantage de parallélisme. |
| Complétions et MSI-X | Commencer en consultation bornée de la CQ, avec délais, cession du CPU et fermeture sur erreur. Pour une étape ultérieure, définir l’isolement de la table MSI-X et de son PBA, le contrôle des vecteurs, le remappage des interruptions et l’ordre masque/drainage/révocation. |
| Persistance et arrêt | Exiger une preuve distincte de succès de FLUSH, de cessation des DMA et d’achèvement des invalidations. Toute commande dont l’issue reste incertaine conserve les ressources concernées en quarantaine. Une perte d’alimentation physique requiert une qualification séparée. |
| Transition UEFI et indépendance de l’amorçage | Réserver l’usage de v0.19.0 à une source d’amorçage indépendante jusqu’à preuve de transfert d’autorité pour NVMe. Admettre un contrôleur hérité actif ou charger les premiers services depuis le volume principal nécessiterait un arbitrage supplémentaire. |
| Extension au matériel physique | La cible communiquée est le Dynabook Tecra A50-J, SSD Kioxia KBG40ZNV512G de 512 Go, PCI `1E0F:0001`. Ces identifiants sont déclaratifs. La fiche BG4 consultée décrit des références voisines. Le firmware installé, la géométrie du namespace, les capacités de reset et la topologie IOMMU du portable restent à inventorier dans une autre mission. Windows et BitLocker restent en dehors du banc virtuel. [Fiche constructeur BG4](https://americas.kioxia.com/content/dam/kioxia/shared/business/ssd/client-ssd/asset/productbrief/cSSD-BG4-product-brief.pdf) |

Les contrôleurs supplémentaires, les namespaces partagés, le hotplug, les transactions pair-à-pair, les files par CPU, PRP2/listes PRP, SGL, la mémoire tampon du contrôleur (CMB), les métadonnées de protection et les commandes administratives destructrices sont différés. Le port EL0 doit disposer d’un backend IOMMU et d’un routage PCIe/IRQ qualifiés pour sa plateforme. AMD-Vi, ARM SMMU et l’accès au SSD physique ne sont pas couverts par le profil x86_64 proposé. Les performances seront étudiées une fois le pilote fonctionnel et les critères d’intégrité satisfaits.

## Sources et méthode

Les références normatives sont les spécifications NVM Express Base, le transport PCIe et le jeu de commandes NVM. La révision annoncée par un contrôleur et sa réponse Identify déterminent les champs réellement utilisables. Pour le modèle émulé, comparer la documentation QEMU à la version binaire enregistrée dans le futur banc. Le contrat d’invalidation et les mécanismes de masquage PCIe s’appuient sur les documents de leurs organismes respectifs. [NVM Express](https://nvmexpress.org/specifications/) · [PCI-SIG](https://pcisig.com/specification-overview/pci-express-base) · [Intel VT-d](https://cdrdv2-public.intel.com/831418/vt-directed-io-spec.pdf) · [Modèle NVMe QEMU](https://www.qemu.org/docs/master/system/devices/nvme.html)

La lecture du dépôt Linux local, commit `97d73c8251226cf27cc6656b4c542869bb8c69fa`, a porté sur l’initialisation PCI, la prise en compte d’un contrôleur hérité, la création des files et la séparation entre Identify Controller et Identify Namespace. Les fichiers `drivers/nvme/host/pci.c` et `core.c` portent la licence GPL-2.0. Cette étude conserve uniquement les références et les exigences de conception. Une future réalisation Rust doit être écrite à partir des contrats approuvés et des spécifications, dans le périmètre de licence de NovaOS.

**Prochain arbitrage proposé.** Valider ensemble le contrat des tickets, les privilèges du courtier, les limites du premier domaine et la procédure de refus d’un contrôleur hérité. Autoriser ensuite, par une décision distincte, le premier jalon QEMU Identify et lecture seule. Le passage à WRITE/FLUSH et au montage inscriptible dépendra des preuves d’isolation, de drainage et de récupération sur copies jetables.
