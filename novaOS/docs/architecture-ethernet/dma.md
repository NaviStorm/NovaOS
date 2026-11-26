# Permissions matérielles et retrait

## Contrat d’autorité

Dans la base, `physical_domain` renvoie `Denied`. `physical_dma::Direction` a deux valeurs, `ReadByDevice` et `WrittenByDevice`. `Domain::map` refuse une seconde IOVA identique ou le même cadre déjà mappé. `Unit` distingue épinglage, préparation, publication, retrait confirmé et désépinglage. Les backends Q35 et VirtIO-IOMMU du laboratoire ont leurs propres chemins. Une proposition de droits supplémentaires doit passer par une revue du contrat, de ses appelants et des permissions effectivement appliquées par chaque backend. [Source de la base](../../kernel/src/hardware/physical_dma.rs) · [Contrat VirtIO de laboratoire](https://github.com/NaviStorm/NovaOS/blob/56db7d5c0d04490cd284b91567cbbd4736534266/novaOS/docs/virtio-net-bail-dma.md)

L’objet de bail proposé contient démarrage, titulaire et incarnation, unité de traduction, groupe complet et alias, domaine, génération, profil de famille signé et révision de politique. Chaque allocation ajoute rôle, file, géométrie, pages, plage IOVA, direction **vue par le périphérique**, propriété CPU et état. Seul le superviseur émet les jetons. La requête porte un handle local atténué et une identité de ressource opaque. Le superviseur résout le véritable appelant, vérifie l’égalité du bail, l’état et les droits, puis valide toutes les plages et tous les alignements avant tout effet.

| Ressource | VirtIO-net | Killer E2400 | Intel I219-V |
|---|---|---|---|
| Données TX / RX | Lecture / écriture | Lecture / écriture proposées | Lecture / écriture proposées |
| Soumission | Table des descripteurs et anneau disponible en lecture | TPD et RFD en lecture proposée | Anneaux de descripteurs TX et RX sur pages dédiées en lecture **et** écriture |
| Retour matériel | Anneau utilisé en écriture | RRD en écriture proposée, à établir sur silicium | Écriture de retour dans le descripteur déjà lu. Vérifier résultat et propriété avant consommation |
| Pages du port, code, piles, registres, tables IOMMU | Absents du domaine du NIC | Idem | Idem |
| Niveau de preuve | Deux topologies QEMU identifiées | Étude. Vérifier notamment que le RRD fonctionne sans lecture DMA | Exigence de conception. Aucun bail physique admis dans la base |

La ligne « données TX / RX » signifie **TX lisible seulement**, **RX inscriptible seulement** dans les trois colonnes. L’extension proposée serait un type distinct `AnneauDescripteursRW` créé exclusivement par le superviseur pour un rôle et un profil autorisés. Refuser son emploi pour une page de données, une famille non habilitée, des usages mixtes ou un alias. Les tests négatifs doivent vérifier le refus par la politique *et* les droits effectifs de la traduction. Une unité incapable d’imposer le sens demandé rend le profil indisponible. Aucune émulation de RW par deux mappings. L’accusé du périphérique et les champs réécrits sont non fiables. Le courtier compare à sa copie CPU d’autorité les identités, limites, étapes et crédits. Il ne suit jamais une adresse renvoyée par l’appareil.

## Plateforme et propriétaire

| Plateforme | Preuves nécessaires avant attribution |
|---|---|
| x86_64 avec VT-d | Résoudre les portées DMAR, DRHD, requester IDs/alias, ponts, remises à zéro partagées, RMRR et chemins pair-à-pair. Vérifier confinement depuis le firmware, traduction effective, absence de bypass, isolation et attribution de l’IRQ. Les emplacements PCI et modes d’IRQ Q35 sont des données de profil |
| AArch64 avec SMMU | Résoudre la topologie depuis IORT ou DT et les correspondances de StreamID, domaine, flux réservés et cohérence. Vérifier refus par défaut, portée des invalidations et reçu de synchronisation, isolation des IRQ GIC et absence de voie de contournement. La preuve AArch64 VirtIO-IOMMU ne qualifie pas un SMMU physique |
| Périphérique USB | Le contrôleur hôte est le maître DMA. Le courtier USB possède son domaine et ses anneaux. Le pilote réseau reçoit seulement des transferts bornés sur les endpoints de son interface. Preuve de l’isolation logique des buffers par fonction et de l’annulation à la déconnexion. Si la politique exige un domaine matériel indépendant par NIC, refuser le port USB partagé ou réserver un contrôleur isolable dédié |
| Interface logicielle | Attacher au fournisseur autorisé deux files CPU privées, appliquer les mêmes crédits/tickets. Aucun bail DMA. Une fonction virtuelle matérielle s’évalue suivant ses vrais alias et sa dépendance au reset de la fonction physique |

Exiger isolation par groupe pour les maîtres DMA indépendants et fermeture avant transfert de propriété. Si deux fonctions ne peuvent être séparées, refuser deux instances prétendument indépendantes. Le contrôle du PHY, du firmware, des IRQ et du périmètre de reset relève du profil. N’autoriser un changement de propriétaire que lorsque les anciens participants ne peuvent plus agir. Le moteur de qualification mesure également les effets d’une panne d’unité partagée sur les autres domaines.

## Retrait transactionnel

1. Sous autorité sérialisée, marquer le bail en fermeture. Bloquer tout nouvel envoi, mappage, prêt et réarmement, révoquer les descendants, réveiller les attentes et figer la liste des travaux admis. Garder un reçu privé et consultable après la mort du propriétaire.
2. Masquer la source IRQ, synchroniser les handlers en cours, arrêter la production des queues. Réconcilier ce qui a été publié avec les complétions vérifiables. Drainer les transactions et arrêter le DMA selon la séquence de la famille. Un bit Bus Master à zéro, un timeout, une IRQ absente ou un simple reset ne prouvent pas à eux seuls la fin de tous les accès.
3. Retirer les traductions et le rattachement dans l’ordre prescrit par le backend. Attendre les invalidations IOTLB et, si applicable, celles des caches du périphérique. Attester leur portée et la synchronisation finale. Si plusieurs demandes partagent une file de commande, sérialiser la publication et corréler chaque reçu au bon titulaire, à son incarnation, à l’unité et à la génération. Un reçu tardif ne solde pas un autre retrait.
4. Après preuves d’arrêt, de drainage, de retrait des traductions, de détachement et de cohérence CPU, nettoyer les cadres et rendre pages, fenêtres, handles, attentes et réservations. Conserver un résumé de fin par génération. La répétition du retrait renvoie le même bilan.
5. Pour un échec partiel (attachement, mappage, publication ou retrait), garder l’inventaire des opérations confirmées et possibles. Quarantaine des pages, tables, descripteurs et reçus dont le réemploi est incertain. Une récupération ultérieure exige une procédure de preuve qualifiée pour l’unité et la topologie, jamais l’expiration d’un délai. Une faute locale laisse progresser les autres périphériques sous leurs traductions établies si l’unité est saine. Si l’unité commune est incertaine, bloquer les reconfigurations dépendantes et borner le service résiduel au périmètre encore démontrable.

Les écritures dans les registres de contrôle, les barrières d’ordre des descripteurs et la maintenance des caches pour les DMA non cohérents sont des responsabilités de la médiation. Les registres à lecture destructive ont un seul lecteur de confiance. Les compteurs et l’état du lien remontent au port avec une révision. Un changement d’autorité du firmware ou une reprise après veille impose une nouvelle vérification du bail avant ouverture.
