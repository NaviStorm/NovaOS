# NovaOS v0.25.0 — Phase 3E — critères et contrats de cycle de vie

La référence de livraison 3D est `4c3bd8896c496636118a7f4751b6947fba72d1e3`. La MOA a validé le reset privé des virtqueues sur ARM et demandé de conserver la garde de réadmission du Q35. Une tentative de nouvelle admission NIC pendant le même bail NVMe doit donc échouer, même après restitution complète du NIC.

## Critères de qualification

- Sur ARM, l’offre, l’acceptation du bit 40 et la longueur du champ PCI précèdent l’usage de `queue_reset`. La fermeture du canal du pilote précède les commandes du courtier. Chaque file requiert sa sélection, sa désactivation et son acquittement. Le titulaire, son incarnation et la génération matérielle sont ceux de la réservation interne.
- Les bits de propriété des descripteurs et les reçus Ethernet gardent leur sémantique. Seule la conjonction de deux resets vérifiés, du contrôle de clôture existant, de la purge IRQ, des retraits de traductions et du DETACH autorise la restitution intégrale d’un anneau RX partiel. L’oracle indépendant reconstruit les IOVA, les permissions et les rattachements depuis la trace VirtIO-IOMMU.
- Avec une seule trame consommée, un accusé de reset manquant ou attribué à une autre génération conserve les six pages de files et les huit pages RX. Un reçu IOMMU non livré conserve les 22 cadres du NIC. Les participants, les racines IPC et les cadres CPU du groupe sont contrôlés à sa clôture. Les lectures et les FLUSH du disque utilisent son propre domaine.
- Sur Q35, la sonde appelle la garde existante avec une nouvelle incarnation, après collecte du reçu de retrait, pendant qu’un CID NVMe est en attente de collecte. Le refus doit laisser le CID, le titulaire et le bail inchangés. L’oracle retrouve sa CQE, les commandes de stockage, les deux ancres du volume et le contenu publié par NovaFS. La file QI et les transitions BME révèlent toute nouvelle publication matérielle du domaine NIC.
- La comparaison Ethernet porte sur l’écho de trames de 1 514 octets. La cellule principale comporte huit trames par lot, 1 024 lots et trois démarrages indépendants par variante et architecture. L’ordre des variantes alterne. Les lots de un et quatre du même démarrage fournissent le contexte. Les mesures sont descriptives. La réserve de débit et de p99 de la phase 3D reste ouverte.

## Contrats pour un gestionnaire ultérieur

| Objet | Condition à établir et à conserver |
|---|---|
| Groupe isolable | Enregistrer segment, topologie, requester IDs et fonctions sœurs. Établir la granularité d’isolation et du reset, les éventuels alias DMA et la portée des IRQ. Invalider cette identité après redécouverte ou changement de topologie. |
| Contrôleur IOMMU | Un propriétaire sérialise les modifications de contexte, d’IOTLB et de la file commune. Un retrait individuel nomme les traductions et les ressources du seul bail concerné. Les pages du contrôleur ont leur propre durée de vie. |
| Reçus | Conserver l’identité du démarrage, le contrôleur, le groupe, le titulaire, son incarnation, la génération matérielle, la nature et le numéro de l’opération. Enregistrer séparément sa soumission, sa confirmation et sa consommation unique. Les reçus applicatifs demeurent indépendants. |
| Disparition du superviseur | Placer les preuves et l’inventaire sous une autorité survivante avant de permettre le redémarrage. Si la reprise ne peut authentifier l’état du contrôleur et des pages, garder les cadres épinglés et les admissions fermées. Le banc actuel conserve ses reçus en mémoire volatile du superviseur vivant. |
| Concurrence | Borner les opérations en attente, sérialiser les publications par contrôleur, associer chaque réponse à une seule demande. Les deux emplacements du Q35 de laboratoire ne couvrent pas une topologie arbitraire. |
| Générations | Allouer une génération strictement croissante par instance. Refuser les doublons et l’épuisement avant toute délégation. Une nouvelle époque de démarrage exige sa propre preuve de nettoyage matériel. |
| Panne commune | Geler les nouvelles admissions de tous les groupes dont les garanties dépendent du contrôleur atteint. Distinguer arrêt de la source, retrait des traductions et retour de propriété. Un délai expiré clôt l’attente et maintient la quarantaine. |
| Réadmission | Exiger une origine authentifiée, le retrait réconcilié, une nouvelle incarnation et des capacités fraîches. Sur Q35, la garde actuelle exige un NVMe en préparation avec BME désarmé. Son ouverture pendant le bail actif reste soumise à arbitrage. |
| Quarantaine persistante | Conserver les cadres, tables, droits, baux et reçus liés sans les rendre à l’allocateur. Prévoir l’observation des ressources retenues et un protocole de reprise démontré pour chaque matériel. |

Les mécanismes décrits pour la reprise après perte du superviseur et la gestion de groupes arbitraires sont des prérequis du futur gestionnaire. Les profils du banc couvrent un contrôleur et une topologie fixes.
