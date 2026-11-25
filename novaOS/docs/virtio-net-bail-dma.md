# Bail DMA privé VirtIO-net proposé pour arbitrage

**Décision sollicitée.** Autoriser dans un profil QEMU explicite un bail exclusif pour une carte Ethernet et une instance de pilote. L'inventaire reste disponible séparément. L'activation exige toutes les conditions ci-dessous sur chaque architecture.

### 1. Titulaire et opérations

Le superviseur lance l'image du pilote inscrite au catalogue de confiance, en Ring 3 ou EL0. Il associe la référence opaque et non transférable du bail à l'incarnation du processus, à la fonction PCI complète, au groupe d'isolation et à une génération non réutilisée. PID, BDF, nom et identifiants vendeur/modèle seuls sont insuffisants. Tout appel est contrôlé contre le véritable appelant, la phase du bail, la direction, le slot et la longueur. Une génération épuisée bloque l'attribution.

Le pilote peut proposer les fonctionnalités du profil, préparer RX, soumettre TX, retirer les complétions, attendre un événement et fermer son bail. Le superviseur valide chaque transition et conserve configuration PCI, bus mastering, BAR, tables IOMMU, descripteurs et routage IRQ. Les applications ne reçoivent aucun canal réseau au démarrage. Le contrat ne crée aucun sélecteur public et ne donne aucun droit sur le stockage.

### 2. Conditions DMA et démarrage

Une seule autorité supervise l'IOMMU de la machine et conserve les domaines existants. Elle vérifie le chemin PCI, le requester ID, les alias, le groupe d'isolation et les plages réservées. Elle refuse un groupe partagé, un chemin pair-à-pair non maîtrisé, un domaine en bypass ou un matériel encore attribué. Sur Q35, exiger VT-d actif et vérifier les invalidations et le confinement des interruptions. Sur virt AArch64, exiger virtio-IOMMU actif, boot-bypass désactivé, attachement explicite de l'endpoint et acquittement des commandes. Le domaine du NIC est distinct de ceux du disque et du firmware. ATS, PASID et caches de traduction périphériques sont exclus du premier lot.

Le profil actif doit établir le refus DMA depuis la remise par le firmware ou prouver un transfert de propriété protégé de bout en bout. Une capture PCI tardive ne démontre pas cette continuité. En cas de bus mastering initial actif sans preuve de confinement antérieur, refuser la prise en charge. Le profil d'inventaire a effectivement observé ce bit actif sur Q35. Sans IOMMU fonctionnel, seules découverte et opérations logicielles sans DMA sont admissibles. Les tampons intermédiaires protègent les copies CPU mais ne bornent pas les transactions émises sur le bus.

Masquer la route IRQ, contenir le périphérique, vérifier le reset et l'absence d'ancien transfert, puis configurer un domaine neuf. Négocier VERSION_1, ACCESS_PLATFORM et MAC, relire FEATURES_OK, valider la MAC sous génération stable. Installer les files RX 0 et TX 1, préposter RX et vérifier DRIVER_OK avant notification. Le superviseur active le bus mastering uniquement lorsque traductions, descripteurs et route d'interruption sûre sont en place. Une étape incertaine interdit le passage en service.

### 3. Mémoire et files

Par carte, deux split virtqueues de 16 descripteurs chacune, un tampon de 2 048 octets par descripteur. Borne Ethernet de 1 514 octets hors FCS, en-tête VirtIO moderne de 12 octets. Chaque direction a ses propres cadres. Du point de vue du NIC, données TX, tables de descripteurs et anneaux disponibles en lecture seule, données RX et anneaux utilisés en écriture seule. Les types d'anneaux occupent des pages disjointes. Si la direction requise ne peut pas être imposée, refuser le profil. Les cadres privés du noyau, du pilote, du disque et des applications sont absents de la carte IOVA.

Le pilote manipule des copies bornées dans sa mémoire utilisateur RW/NX. Les BAR, anneaux et tampons DMA restent hors de ses tables utilisateur. Une soumission TX copie puis publie exactement la longueur validée. Une livraison RX copie seulement la plage validée après complétion. Chaque slot passe de libre à réservé, soumis, terminé, puis libre après consommation. En cas de file pleine, exercer une contre-pression. Une soumission d'issue inconnue conserve son slot.

### 4. Interruptions

Le superviseur réserve une route exclusive ou refuse le démarrage. Proposition initiale INTx médiée, MSI et MSI-X désarmées, avec protection de la route démontrée sur chaque machine. Le gestionnaire identifie la source, masque et acquitte l'événement. Le pilote draine au plus 16 entrées par file et par passage. Vérifier index, delta modulo 16 bits, identité du descripteur, état, direction et longueur avant copie ou restitution. Une complétion impossible met la file en quarantaine. Réarmer puis revérifier les indices pour couvrir une arrivée concurrente. Une interruption seule ne restitue aucun tampon. Compter les IRQ sans travail et borner les réveils en cas de tempête.

### 5. Retrait, panne et reprise

Les phases sont libre, en préparation, actif, en retrait, puis libéré ou en quarantaine. À la fermeture, à la mort du pilote ou sur faute, révoquer les appels de l'ancienne génération et arrêter les soumissions. Masquer et synchroniser l'IRQ, contenir le DMA résiduel, confirmer le reset et l'arrêt des transactions, invalider les traductions et attendre leur acquittement. Vérifier le détachement du domaine et la purge des notifications avant d'effacer et rendre les pages. Une expiration, un processus disparu ou une IRQ silencieuse conserve les ressources en quarantaine. Le diagnostic garde la phase, la génération et les compteurs. Une relance exige un retrait confirmé, une nouvelle incarnation et de nouvelles références. Aucun TX d'issue inconnue n'est retransmis automatiquement.

### 6. Budgets proposés

La réservation DMA du NIC représente 16 pages de données et 6 pages d'anneaux, soit **22 pages de 4 Kio, 88 Kio**. Une page privée de 4 Kio pour deux copies temporaires porte ce sous-total à **92 Kio**. Les tables IOMMU, cadres du processus, pile, métadonnées du courtier et ressources de routage sont à chiffrer séparément sur chaque ISA avant admission. Toute allocation partielle doit suivre la même procédure de retrait. La qualification mesurera référence avant attribution, repos, pic de saturation et restitution, y compris en panne.

Prévoir une conversation de contrôle privée et une attente IRQ coalescée sur la même route, une attente au plus par pilote. Objectif d'une racine IPC supplémentaire au plus par acteur concerné, plafond total de quatre par acteur, y compris les racines déjà occupées. Messages de contrôle de 128 octets maximum. La copie d'une trame utilise une plage utilisateur bornée vérifiée par le courtier dans le portail privé à arbitrer. Aucun handle par trame. Vérifier les totaux et quotas réels aux phases d'attachement, de charge et de retrait avant activation.

### 7. Recette et portée

Sur Q35 x86_64 et virt AArch64, sans interface vers le réseau hôte, confronter la vue invitée à QMP et les octets transmis à un pair Ethernet privé. Exiger le refus pour un autre processus, un ancien bail, un slot hors plage, une direction erronée, un groupe ambigu et une IOMMU absente ou en bypass. Contrôler les limites des mappings avec des pages témoins synthétiques, leur intégrité et les confirmations IOMMU. Vérifier que le disque continue d'utiliser son propre domaine. Démontrer la fermeture et la relance avec RX/TX en vol, IRQ pendante et invalidation non acquittée. Exercer saturation, bouclage des index, interruptions coalescées, débit, latence et endurance selon les objectifs fixés avant campagne. Conserver séries, configuration, compteurs, mesures et SHA-256.

Ces essais portent sur des périphériques émulés et un hyperviseur de confiance. Sur matériel physique, il faudra reprendre la chaîne firmware, l'isolation réelle des groupes PCIe, les chemins pair-à-pair, la cohérence des caches, le reset et les garanties de drainage propres au contrôleur. La perte de disponibilité en quarantaine est un risque résiduel accepté uniquement pour la proposition de laboratoire.

**Arbitrage demandé** sur le titulaire privé, le domaine indépendant, la médiation IRQ et la copie bornée ci-dessus. La mise en œuvre du bail et les essais de trafic dépendent de cette décision.
