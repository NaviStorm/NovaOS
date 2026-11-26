# VirtIO-net — Audit de clôture du premier lot Ethernet et préparation de l’intégration

**Décision du 26 novembre 2026.** La MOA/MOE a approuvé la clôture fonctionnelle du lot Ethernet dans les invités AArch64 virt et x86_64 Q35, avec les topologies, firmwares, modèles QEMU et profils identifiés dans les preuves. La branche réseau est intégrable sur la tête de `main` examinée sans réconciliation de sources. L’intégration, la distribution ordinaire et la qualification des cartes physiques relèvent de décisions distinctes.

## Candidat et pièces vérifiées

| Référence | Rôle dans la décision |
|---|---|
| `5c7f1bfa313a3859ccc0830660438e76d598acb1` | Tête de `main` relue directement sur `origin`, base de la branche réseau |
| `77657fb3f04b842b16211491d04357350a6e9cd4` | Publication des campagnes historiques de performances et de coexistence, avec leurs identités binaires |
| `c4bf40ce468046585c462caf7085ca9564db7ef2` | Seize essais Q35 à deux domaines et deux confirmations d’endurance, une par architecture |
| `1863ff413db0c9c47a52d3e1a1592e718aae22e6` | Dernière modification fonctionnelle, témoin de pression RX Q35 corrigé |
| `6783dc9c2f3d3bcc13f26281cab05cca5c2c5e36` | Publication auditée, arbre `0b58cb620877f7f7489fef15a077c35c1aaa5181`. Les 36 fichiers différents du candidat fonctionnel sont dans `novaOS/docs/` |

Les octets ont été relus pendant cet audit, sans démarrage de VM. Pour chacune des deux campagnes, la liste des fichiers distribués concorde exactement avec le manifeste extérieur. Chaque archive a été décompressée en flux. Chaque membre régulier a été identifié par son chemin et haché avant comparaison avec son manifeste propre. Le contrôle détecte également les chemins absents, supplémentaires ou dupliqués. Les entrées locales ont été vérifiées sur les artefacts durables et sont comptées séparément.

| Campagne | Entrées du manifeste extérieur | Archives | Fichiers internes vérifiés | Références locales vérifiées | SHA-256 du manifeste extérieur |
|---|---:|---:|---:|---:|---|
| Clôture du 26 novembre | 34 | 12 | **841/841** | 77/77 | `36cfb99ae328c4053b8fb638d15260dbdf387f3ab96b4fa31903ad4769adb4d0` |
| Finale du 25 novembre | 72 | 34 | **1 034/1 034** | 106/106 | `80af261be37c4c82643b671eff7fea1a77b690293a14d5640112581a7d1dbdc0` |

Les 841 pièces incluent 345 fichiers de développement et de diagnostic. Pour les 16 essais terminaux Q35, les 64 empreintes des rapports de campagne, du pair, des séries invitées et des traces QEMU concordent avec les fichiers archivés. Les sommes des 16 volumes terminaux et des deux volumes de départ ont été rapprochées des relectures P9 archivées. Génération, ancres et empreinte du fichier concordent pour chacun des 16 résultats. Les six autres manifestes extérieurs de l’inventaire, du firmware, de l’isolation, d’EVENT_IDX et de la correction RX ont également été relus, ainsi que les 427 membres du dossier RX corrigé. [Relevé de cet audit par archive et par ELF](validation/virtio-net/audit-cloture-20261126.json) · [Manifeste de clôture](validation/virtio-net/cloture-20261126/SHA256SUMS) · [Manifeste historique](validation/virtio-net/finale-20261125/SHA256SUMS) · [Relectures P9](validation/virtio-net/cloture-20261126/relecture-p9-archives.json)

### Applicabilité au candidat fonctionnel

- **Preuve directe.** L’ELF du témoin de saturation Q35 exécuté et l’ELF reconstruit après le commit `1863ff4` sont identiques, SHA-256 `15b6b529907e328e96522b26b60f52555ffb19a6ff77951ca399c0b1f4633af1`. L’essai initial et sa reproduction sur `c4bf40c` avaient capturé la 17e trame, puis terminé en échec. Le superviseur attendait alors que le pilote avait déjà soumis sa demande IPC de réarmement. Seul le témoin corrigé démontre la reprise Q35 après saturation. [Traces et comparaisons](validation/virtio-net/cloture-20261126/saturation-et-comparaisons.sha256)
- **Preuve réutilisable.** Les huit paires d’ELF du dossier d’applicabilité ont été relues. Pour les deux profils Q35 double domaine, l’entrée, les quatre descriptions de segments et tous les octets chargés concordent après rapprochement de 19 changements de numéro de ligne dans les données en lecture seule de chaque ELF. Même contrôle pour l’endurance Q35 (15 emplacements), l’endurance AArch64 (10) et le profil stockage AArch64 (20). Les lignes source avant/après ont été retrouvées dans Git. Les deux ELF ordinaires sont identiques en entier entre ces deux révisions fonctionnelles. [Comparaisons d’applicabilité](validation/virtio-net/cloture-20261126/applicabilite-candidat-final.json)
- **Mesures historiques.** Les débits, RTT, temps CPU et percentiles P9 ci-dessous restent attribués à leurs exécutables et campagnes d’origine. Les anciens profils de mesure et les reconstructions du candidat présentent des différences de segments. La confirmation ARM/P9-4096 en charge et le parcours Q35/P9-512 de confirmation ont des résultats propres. Les répétitions Q35 M03 et ARM/P9-4096 au repos interrompues avant démarrage sont exclues. Les sept échecs de développement de la dernière campagne restent au dossier. [Profils recompilés](validation/virtio-net/cloture-20261126/profils-recompiles.json) · [Confirmations et interruptions](validation/virtio-net/finale-20261125/confirmations.json) · [Chronologie](validation/virtio-net/cloture-20261126/chronologie.json)

## Matrice fonctionnelle et nominale

| Critère | AArch64 virt | x86_64 Q35 |
|---|---|---|
| Détection, négociation et attribution | Inventaire PCI confronté à QMP. VirtIO-IOMMU avec bypass désactivé dès le démarrage, NIC initialement détaché. Domaine 3/SID 56 distinct du disque 1/SID 48 | Inventaire PCI confronté à QMP. VT-d, domaine NIC 3/SID 56. Pour le double domaine, NVMe 1/SID 48, démarrage fw_cfg PIO, Secure Boot → Limine → NovaOS. Bus Master contrôlé depuis le reset |
| Permissions et accès | RX et anneaux utilisés en écriture pour le NIC, TX, descripteurs et anneaux disponibles en lecture. Deux files de 16, un tampon par trame, EVENT_IDX négocié, `num_buffers=1` contrôlé | Mêmes permissions et contrôles. Tables et cadres NIC/NVMe disjoints. Dans le profil double, remappage d’interruptions inactif et IRTA nulle |
| Trames et IRQ | Échanges réels avec le pair isolé, intégrité dans les deux sens, INTID 38 du NIC | Échanges réels, intégrité dans les deux sens, INTx du NIC sur GSI 23 |
| Endurance, unicité, bouclage | 100 000 TX et 100 000 RX, indice final 34 464 modulo 65 536, 6 249 réarmements | Mêmes comptes et même bouclage. Les essais logiciels d’inventaire sont comptés séparément |
| Pression et progression | 17e soumission TX en `WOULD_BLOCK (7)`. 17 trames RX présentées pour 16 tampons, puis progression jusqu’à 48/48 | Même refus TX. Après le rendez-vous corrigé, 17/16 en RX, 48/48 au terme, deux réarmements et `Exit(0)` |
| Arrêt et redémarrage | Quatre générations dans le même invité. Drainage, désarmement du NIC et restitution DMA sur le parcours nominal | Quatre générations des deux baux dans le même invité, pour chaque géométrie. 192 TX et 192 RX, 17 WAIT et quatre publications P9 par essai |
| NovaFS sous trafic | 256 transactions par parcours, données relues sur P9-512 et P9-4096 | 128 transactions par parcours M02. Les 257 FLUSH sont entièrement couverts par le trafic dans les deux géométries |
| Révocations et contention | Deux domaines, une commande de contrôle matérielle en vol. Attribution par titulaire, refus de la collecte par l’autre et lecture disque déjà admise achevée | Deux demandes réellement pendantes avant le premier retrait, deux ordres exercés sur chaque géométrie. Une commande matérielle en vol, cookies WAIT 4 et 5 individualisés, doublon et collecte prématurée refusés |

Les quatre fonctionnalités négociées par le NIC sont VERSION_1, ACCESS_PLATFORM, MAC et EVENT_IDX. Les IRQ de données et les complétions résultent du périphérique virtuel. Le pair utilise une liaison privée du banc. Les descripteurs, adresses DMA et BAR restent du côté superviseur. [Attribution et isolation](virtio-net-isolation-dma-20261125.md) · [Mesures des premiers échanges](virtio-net-qualification-rx-20261125.md) · [Matrice des seize essais Q35 et sources](virtio-net-cloture-q35-20261126.md)

### Refus, défaillances et disponibilité

| Cas exercé | Profils et résultat attendu observé | Limite de la preuve |
|---|---|---|
| Admission incompatible | EVENT_IDX absent sur les deux ISA, refus avant ouverture. Sur Q35 double, IRES actif ou IRTA non nulle entraîne `Err(Feature)` avant attribution. Signature invalide, clé dbx révoquée, noyau altéré et VT-d absent disposent de témoins Q35 | Les préconditions concernent les binaires et firmwares identifiés |
| Réponse Ethernet du pair perdue | Sur les deux ISA, 16 trames reçues au pair, 15 renvoyées, sortie 8 et quarantaine des DMA incertains | Le pair est la source de cette perte |
| Complétion RX perdue après copie DMA | Modèle de faute distinct sur les deux ISA. 16 copies, 15 complétions, désarmement et quarantaine. Sur AArch64, lecture P9 indépendante et restitution des sept cadres disque | Sur Q35 ce témoin exerce le NIC seul |
| Panne locale effective du pilote NIC | Q35/P9-512 et P9-4096. Après désarmement, 21 READ, 7 WRITE et 3 FLUSH NVMe, tous CQE à zéro. Relecture de 4 231 octets, génération 4. Les 26 cadres NIC restent en quarantaine | Le reçu du disque sain ne libère aucun cadre NIC |
| Panne locale effective du pilote NVMe | Sur les deux Q35, après soumission du premier WRITE, 32 TX et 32 RX supplémentaires vérifiés et drainage du NIC. Les deux ancres P9 conservent la situation antérieure, dernière génération publiée 3. Six cadres NVMe en quarantaine | L’écriture interrompue demeure sans confirmation de publication |
| WAIT court, absent ou tardif | Modèle expérimental Q35 distinct. NIC premier en 512, disque premier en 4096. Court accepté. Absent ou livré après expiration, retrait et nouvelle reconfiguration dépendante refusés. Les 40 cadres restent épinglés, y compris après retour du cookie tardif | Incertitude du contrôleur commun. La continuité du pair sain démontrée pour une panne locale ne s’étend pas à ce cas |
| Divergence IRTA après publication | Q35 double à maîtres désarmés, retrait en quarantaine, y compris après remise à zéro d’IRTA | Corruption arbitraire du contrôleur pendant le trafic et récupération ultérieure des pages ouvertes à qualification |

[Traces des pannes locales](validation/virtio-net/cloture-20261126/continuites-locales.sha256) · [Traces WAIT](validation/virtio-net/cloture-20261126/wait-concurrents.sha256) · [Sept précontrôles du modèle de faute](validation/virtio-net/cloture-20261126/preconditions-wait.json) · [Dossier IR/IRTA et pertes RX](virtio-net-finale-20261125.md)

## Performances et ressources

Mesures soutenues sur un vCPU TCG et 256 Mio, avec 5 s d’échauffement puis trois fenêtres de 10 s. Débits sur trames de 1 514 octets, RTT sur 64 octets mesuré sur l’horloge du pair. La charge CPU est celle du processus QEMU sur l’hôte.

| Profil | RX Mbit/s (trames/s) | TX Mbit/s (trames/s) | Duplex Mbit/s par sens | RTT p50 / p95 / p99, ms sur les trois fenêtres | CPU hôte |
|---|---:|---:|---:|---|---|
| AArch64 | 254,096 (20 979) | 121,559 (10 036) | 93,627 | 1,670–1,684 / 1,837–1,883 / 1,958–2,030 | 91,9–93,8 % |
| Q35 | 63,461 (5 240) | 60,792 (5 019) | 49,901 | 3,267–3,373 / 4,979–5,022 / 6,434–6,673 | 101,2–102,3 % |

Le coût propre du pilote, du pair, de la collecte, de l’ordonnanceur et de l’émulation reste à isoler. Sur AArch64, la lecture/écriture rapportée au parcours complet baisse de 58,2 % / 53,7 % sur P9-512 et de 59,7 % / 56,6 % sur P9-4096 sous trafic. Les p99 FLUSH passent respectivement de 462 à 511 µs et de 464 à 529 µs. Sur Q35, p99 passe de 6 948 à 5 941 µs en 512 et de 6 470 à 5 383 µs en 4096. Une paire de campagnes par géométrie ne démontre pas un gain apporté par le réseau. [Débits de stockage et distributions p50/p95/p99 au repos et en charge](virtio-net-finale-20261125.md) · [Analyse des fenêtres AArch64](validation/virtio-net/cloture-20261126/analyse-stockage-aarch64.json)

| Profil | Cadres résidents avant / files pleines / fin | Cadres DMA au pic → fin | Attribution et restitution |
|---|---|---|---|
| Endurance AArch64, confirmation | 1 160 / 1 239 / 1 172 | 38 → 0 | 152 Kio DMA, dont 9 cadres de contrôle, 7 disque, 22 NIC. Le cache passe de 1 à 13 cadres |
| Endurance Q35, NIC seul, confirmation | 1 352 / 1 411 / 1 355 | 30 → 3 | Restent la racine vide VT-d, la file QI et la page WAIT |
| Q35 double, P9-512 / P9-4096 | 2 394 avant trafic dans le témoin de panne, 2 436 / 2 439 files pleines. Dans les essais de cycles, 1 889 à chaque fin | 40 → 8 après cycle nominal | 160 Kio DMA au pic. Les 8 cadres permanents du contrôleur contiennent racine, contextes, quatre tables disque, file et page WAIT. Les 32 autres sont 6 cadres NVMe, 22 cadres NIC et 4 tables NIC |

Un cadre vaut 4 Kio. Les 22 cadres NIC comprennent 16 pages de données et six pages d’anneaux. Le sous-total prévisionnel de 92 Kio inclut déjà ces pages. Les cadres DMA font partie de la mémoire résidente. Les images contiennent du code partagé dont la part exclusive du réseau reste indéterminée. Le pilote privé utilise une racine SEND. Dans les 16 essais Q35 double domaine, les 110 relevés par PID donnent 3, 4, 4, 2 et 1 racines par génération des cinq acteurs. La coordination des deux retraits coûte 80 octets verrou compris et zéro racine supplémentaire. Fin des quatre cycles par géométrie après échauffement du cache, 25 343 cadres libres, zéro processus et zéro page IPC. Les fins en panne ont leurs propres comptes DMA, 34 après panne du NIC, 14 après panne du disque et 40 si le WAIT commun reste incertain. [Relevés invités par phase, acteurs et générations](validation/virtio-net/cloture-20261126/bilan.json)

## Proposition d’intégration

À la lecture directe des références distantes, `origin/main` est l’ancêtre commun. `git rev-list --left-right --count origin/main...6783dc9` donne **0 / 31**. La différence de la référence auditée représente 216 ajouts et 35 fichiers modifiés. Il n’existe donc aucun changement divergent de `main` à réconcilier à ces deux SHA. La proposition porte sur les sources et les preuves des profils privés.

| Surface à revoir à l’intégration | Dépendance et risque à contrôler |
|---|---|
| HAL, PCI, GICv3/INTx, ACPI/DMAR et VT-d | Le traitement des interruptions, les tables et le reconnaisseur de topologie sont partagés avec le stockage. Conserver le chemin historique et les contrôles génériques hors profils réseau |
| Bloc, NVMe, mémoire et IPC | Allocation des cadres, BAR NVMe/IOAPIC, SIDs, invalidations, collecte des CQE et retrait dépendent du profil choisi. Un retard de l’unité partagée peut bloquer les deux reconfigurations |
| Inventaire, construction et image de démarrage | Le gestionnaire reconnaît désormais le NIC moderne dans son inventaire. Les features Cargo, `build.rs`, le scénario EFI `net-arm-p9` et les bancs sélectionnent les parcours privés. Conserver le démarrage ordinaire et séparer les QEMU nominaux, les modèles de faute et l’OVMF expérimental |
| ABI, stockage portable et livraison | Identité des sous-arbres Git de `novaOS/abi`, `novaFS`, `components` et `distribution` entre les deux têtes. Les 48 appels v1 sont conservés. Neuf contrôles ABI archivés, 67 tests de modèles et 3 tests de contention par cible Darwin ARM64 / x86_64 sous Rosetta |

Le double profil Q35 sélectionne explicitement `net-q35-duo`, qui dépend de `net-q35-lab`, `net-ethernet-lab` et `nvme-lab`. Le témoin AArch64 utilise le superviseur VirtIO-IOMMU partagé. La compatibilité des deux noyaux ordinaires a pour preuve l’identité de l’entrée et des quatre PT_LOAD avec les binaires de référence de la publication `77657fb`, revérifiée sur les archives. Les ELF complets diffèrent entre ces environnements. Cette comparaison a pour référence binaire la livraison réseau précédente. Pour une intégration exigeant l’identité des noyaux avec la base de `main`, il faudra comparer les constructions correspondantes dans le même environnement. La branche R2 en pause (`1e560898ce42aa758f79bc4251b485b86c202c79`) n’est pas un prérequis du lot. Par rapport à la base commune, ses chemins modifiés ne recoupent aucun des 251 chemins du lot à la référence auditée. [Features de compilation](../kernel/Cargo.toml) · [Comparaison des noyaux ordinaires](validation/virtio-net/cloture-20261126/noyaux-ordinaires.json)

**Procédure proposée à la MOA/MOE.** Lors d’un mandat d’intégration, relire les deux têtes sur `origin`. Si l’ascendance est conservée, préparer une fusion `--no-ff` dans un worktree dédié, vérifier l’ordre des parents (ancienne tête de `main`, puis tête réseau approuvée) et exiger l’identité stricte de l’arbre de fusion avec l’arbre source incluant cet audit. Si `main` a divergé, examiner le diff des composants partagés sur l’arbre résultant. Requalifier uniquement les chemins affectés, notamment les deux retraits si le superviseur change, les refus avant ouverture si firmware ou modèle change, et les noyaux ordinaires si la construction change. Garder les variantes de laboratoire hors des profils de distribution. La publication sur `main` attend un mandat distinct.

**Jalon proposé — v0.25.0, lot 1 « Ethernet VirtIO-net sous QEMU ».** La feuille de route associe v0.24.0 aux identités et v0.25.0 au réseau virtuel, à la double pile et aux transports. Consigner la clôture de cette tranche Ethernet dans ce jalon. La version réseau complète reste ouverte, comme NovaAuth v0.24.0. [Découpage du jalon réseau](virtio-net-qemu.md) · [Feuille de route](reconciliation-v0300/proposition.md)

## Inventaire pour les deux pilotes physiques et la future pile

| Point de contact | Élément existant à réexaminer | Élément propre au laboratoire ou au matériel |
|---|---|---|
| Découverte et autorité | Inventaire PCI en lecture seule, correspondance constructeur/modèle/classe, bail lié au titulaire et à la génération, aucune attribution au seul vu d’un identifiant | Identifiant moderne VirtIO `1af4:1041`, BDF fixes et topologies QEMU. Chaque carte exige son inventaire réel, ses alias DMA et son propriétaire |
| Émission, réception et contre-pression | Transport de trames Ethernet sans FCS livré au consommateur, longueur validée, files bornées, propriété du tampon jusqu’à la complétion, refus explicite d’une file pleine | `encode_tx` et `decode_rx` traitent l’en-tête VirtIO de 12 octets. Le chemin réel `Port::demande` impose les MAC du banc, l’EtherType privé, des lots de 16 et une page de copie. Ces éléments ne forment pas une API de trames pour applications |
| Interruptions et progression | Médiation avec source et titulaire identifiés, masquage, drainage, réarmement et revérification de la progression. Le courtier possède les opérations `set_mask`, `interrupt` et `ack` | INTID 38, GSI 23, split rings, EVENT_IDX, indices 16 bits et `num_buffers` sont liés aux modèles qualifiés. Chaque pilote doit établir comment son matériel signale une complétion |
| DMA, panne et restitution | Domaine propriétaire, cadres épinglés avec direction, admission fermée au retrait, preuves de drainage et d’invalidation avant restitution, quarantaine en cas d’incertitude | Le contrat interne `physical_dma::Unit` existe. `physical_domain` renvoie `Denied` pour le matériel physique. Les backends Q35 et VirtIO-IOMMU ont leurs propres reçus et séquences d’arrêt |
| Future pile native | Candidats à une frontière de service : trame L2 et longueur, sens, propriété et restitution, pression, perte, état du lien et fermeture. La médiation maintient BAR et adresses DMA hors des applications | Le port actuel est un dialogue de qualification entre une instance et son superviseur. Le contrat client, ses capacités et l’arbitrage du partage entre consommateurs restent à concevoir |

[Inventaire PCI](../user/device_inventory.rs) · [Format de trame et modèle de file](../user/virtio_net/mod.rs) · [Chemin actif au superviseur](../kernel/src/hardware/block/net_trames.rs) · [Client privé du banc](../user/virtio_net/ethernet_lab.rs) · [Courtier matériel](../kernel/src/hardware/driver_resources.rs) · [Domaine physique préparatoire](../kernel/src/hardware/physical_dma.rs)

### Confrontation avec les dossiers de phase 0

Les deux chantiers physiques ont produit de la documentation pendant l’audit. Les identités ci-dessous sont déclarées dans leurs dossiers respectifs. Les essais Ethernet et les mesures précédentes appartiennent aux machines QEMU.

| Point | MSI, Killer E2400 | Lenovo, Intel I219-V (13) | Conséquence pour le futur port de trames |
|---|---|---|---|
| Identité et accès | `1969:e0a1`, sous-système `1462:115a`, révision `10h` selon la MOA | `8086:15fc`, sous-système `3100:0001`, révision `20h` selon le dossier Lenovo. Le catalogue du Tecra documente séparément le même couple vendeur/périphérique | L’admission doit être liée à l’instance réelle et au groupe d’isolation. La seule paire PCI ne désigne ni une machine ni un droit |
| Queues et permissions DMA | Le dossier propose TPD/RFD en lecture et RRD en écriture pour le NIC. Le sens effectivement requis doit être démontré sur le MSI | Le MAC lit puis réécrit les descripteurs Intel. Le dossier demande des pages d’anneau dédiées en lecture/écriture, des données TX en lecture et RX en écriture. La livraison RX suppose notamment DD, EOP, longueur et erreurs valides | Exposer aux clients sens, longueur, crédits, issue et restitution. Garder formats, taille des files et permissions dans chaque backend. Les anneaux RW proposés pour Intel n’élargissent pas les droits DMA de VirtIO |
| IRQ, lien et arrêt | MAC/PHY, MDIO, contraintes de reset, MSI remappé envisagé, INTx sur route prouvée. La proposition fait remonter l’état de lien et les changements de porteuse | MAC intégré au PCH et PHY cuivre. Le transfert de propriété firmware, les sémaphores, le périmètre de reset, la vidange avant reset/D3 et la route d’IRQ restent à vérifier sur le Lenovo | Une notification signale du travail à examiner. Une prise en charge TX, une complétion locale, une livraison au pair et un retrait DMA sont quatre faits distincts |
| Frontière Ring 3 et service de trames | Le dossier propose des opérations typées, des droits RX/TX/observation et une fragmentation IPC possible. Le superviseur conserve registres, descripteurs et tables | Le dossier propose une admission TX atomique par lot, un ticket par trame publiée et des fenêtres privées de copie. Une réponse perdue conduit à consulter le ticket avant toute décision. Une racine privée de commande est proposée pour le pilote | L’ELF VirtIO utilise une copie dans sa page privée. Le mode de copie client, les droits d’observation, l’interface de lien et les statistiques restent à arbitrer avant une API commune |

Le MSI et le Lenovo doivent chacun apporter une preuve de confiance depuis le firmware, d’isolation DMA et d’interruption attribuable, puis de retrait et de restitution sur la machine. L’inventaire fournit des exigences communes et des différences concrètes à arbitrer. Cette publication n’introduit ni sélecteur, ni droit réseau client, ni interface commune.

**Dossiers consultés.** Les fichiers de travail suivants appartiennent aux branches physiques parallèles. Les SHA-256 identifient les versions lues sans intervenir dans ces worktrees.

| Branche locale | Document dans `novaOS/docs/` | SHA-256 lu |
|---|---|---|
| `codex/pilote-killer-e2400` | `killer-e2400-phase0.md` | `5dedc8c0d276c2e2d34b59120c7ac089bc370bbdd18b4487286037749d05eafb` |
| `codex/pilote-killer-e2400` | `killer-e2400-architecture.md` | `fc9165a519feb4957d68cb2b51bd41cb06cba55d82746bb77b80480c418d5972` |
| `codex/pilote-intel-i219v` | `intel-i219v-phase0.md` | `23dbfcaa0b625b5dd15166519a662efe4785e79088e2c8de18b29185e464912d` |
| `codex/pilote-intel-i219v` | `intel-i219v-architecture-phase0.md` | `7791968d2a2d976facd0996aa8461dc553138272fd91bd09005e05dc4e149992` |

Le dossier MSI est identifié au commit `00335af793e456b6fa6546514f65305f9c114165`. [Architecture MSI de phase 0](https://github.com/NaviStorm/NovaOS/blob/00335af793e456b6fa6546514f65305f9c114165/novaOS/docs/killer-e2400-architecture.md)

L’inventaire historique du Tecra permet de recouper la reconnaissance de famille uniquement. [Catalogue matériel v0.18.0](materiel-v0.18.0.md)

## Réserves conservées

- **NovaFS.** Le parcours historique de fichiers en mode 11 rend `BUSY (24)` sur les deux géométries avec le gestionnaire historique et le candidat. Les publications et relectures P9 réussies ont leur propre parcours. Les réserves de latence incluent le passage historique de 146 à 174 µs au p95 FLUSH P9-4096, le pic historique de 407,008 µs sur P9-512 et le maximum ultérieur de 3 288,992 µs sous trafic. [Diagnostic du parcours historique](virtio-net-isolation-dma-20261125.md)
- **Mémoire et reprise.** La restitution des cadres reste conditionnée à une preuve de retrait. La récupération des quarantaines, la restitution finale du support de démarrage PIO de 34 Mio après transfert au noyau et la part exclusive du code résident partagé restent ouvertes. La réserve antérieure de 301 cadres AArch64 instrumentés appartient à son propre parcours.
- **Portabilité.** La variante OVMF Q35, le correctif RX de QEMU, la description ACPI de fw_cfg PIO et les modèles de faute font partie du laboratoire. L’identité des segments chargés ne résout pas la reproductibilité des ELF complets entre environnements. L’audit n’établit aucune qualification de production, de firmware constructeur, de matériel physique ni de pile IP.
