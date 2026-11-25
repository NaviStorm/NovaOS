# VirtIO-net — qualification Ethernet et coexistence Q35

Campagne du 25 au 26 novembre 2026 sur la branche `codex/pilote-virtio-net`. Le code du double domaine et du client de mesure privé est publié en `a2709fbf245c31340694039099f274d6c8151a69`. Les rapports par essai identifient les ELF, les modèles et les états effectivement mesurés. L’extension IR/EIM est qualifiée sur la topologie Q35 du laboratoire pour les états ci-dessous. Le premier lot reste ouvert sur les pannes et les reprises du superviseur partagé.

## Interruptions et traduction sur Q35

Le contrôle générique garde ses refus historiques. Le profil privé distingue la capacité annoncée par ECAP de l’état réel de GSTS et des 64 bits d’IRTA. Il conserve les conditions génériques de largeur, de drainage, de topologie et de file d’invalidation. L’identité du NIC et sa ligne INTx sont vérifiées. Le NVMe travaille par interrogation de sa file de complétion, avec INTx masquée et MSI-X désactivée. Les IRQ du NIC proviennent du GSI 23. Les commandes VT-d, le registre IRTA, les écritures PCI et les WAIT ont été enregistrés depuis le reset. Le garde relit l’état de livraison aux admissions, pendant les interruptions et les invalidations, puis au retrait.

| Témoin invité | Observation |
|---|---|
| IRES actif avant attribution | `Err(Feature)`, aucun WAIT de publication, aucun Bus Master armé |
| IRTA non nulle avant attribution | Même refus avec relecture matérielle de la valeur et racine vide |
| IRTA devenue non nulle après publication des deux contextes | Deux tentatives de retrait refusées en `Quarantine`, dont une après remise à zéro d’IRTA. Les deux maîtres étaient désarmés. Les 40 cadres restent épinglés et les contextes inchangés |
| Échanges simultanés | NVMe SID 48/DID 1 et NIC SID 56/DID 3, deux instances Ring 3, deux arbres disjoints. Chaque retrait correspond à son invalidation de contexte, à son drainage IOTLB et à son propre cookie WAIT |
| Ordres de retrait | NIC puis NVMe sur P9-512, NVMe puis NIC sur P9-4096. Les cinq WAIT, démarrage compris, ont été vérifiés pour chaque ordre |
| Refus de synchronisation | Dans le cas fermé de perte du résultat WAIT, QEMU écrit effectivement le cookie. Le superviseur privé conserve les cadres faute de confirmation. Ce cas couvre la perte de l’observation logicielle |

Le premier couplage actif a révélé un conflit entre les fenêtres virtuelles du BAR NVMe et de l’IOAPIC. Une deuxième exécution a identifié le contrôle de restitution encore limité à l’ancienne fenêtre. La paire privée 336–337 est maintenant liée aux deux pages physiques du BAR attendu. Les essais C01 et C02 sont archivés avec leurs traces. Les deux géométries ont ensuite effectué de véritables écritures, lectures et FLUSH.

Le démarrage de laboratoire utilise un support de 34 Mio fourni en PIO. La description ACPI conserve le comportement nominal lorsque le DMA fw_cfg est disponible. Le banc vérifie la propriété DMA désactivée, l’absence du registre DMA, la topologie PCI au reset, les écritures de Bus Master et la chaîne Secure Boot, Limine, noyau. Les témoins de mauvaise signature, de révocation dbx, de noyau altéré et d’absence de VT-d sont archivés. L’image OVMF et le QEMU PIO sont identifiés dans chaque campagne. La restitution de l’allocation du support au retour du relais EFI est distincte du transfert définitif au noyau, pour lequel sa restitution reste à établir.

## Réseau isolé

| Contrôle | AArch64 | x86_64 Q35 |
|---|---:|---:|
| Endurance TX et RX vérifiés au pair et dans l’invité | 100 000 par sens | 100 000 par sens |
| Indice final modulo 65 536, réarmements | 34 464, 6 249 | 34 464, 6 249 |
| Pression RX au-delà des 16 buffers, progression après réarmement | Réussie | Réussie |
| Incarnations du pilote dans le même invité | 4, PID et générations distincts | 4, PID et générations distincts |
| RX soutenu, Mbit/s et trames/s | 254,096 et 20 979 | 63,461 et 5 240 |
| TX soutenu, Mbit/s et trames/s | 121,559 et 10 036 | 60,792 et 5 019 |
| Duplex simultané, Mbit/s par sens | 93,627 | 49,901 |
| RTT p50 / p95 / p99, intervalles des trois fenêtres en ms | 1,670–1,684 / 1,837–1,883 / 1,958–2,030 | 3,267–3,373 / 4,979–5,022 / 6,434–6,673 |
| CPU hôte du processus QEMU, sur les quatre campagnes de performance | 91,9–93,8 % | 101,2–102,3 % |

Les moyennes utilisent trois fenêtres complètes de dix secondes après cinq secondes d’échauffement. Le RTT de 64 octets emploie l’horloge monotone du pair et le chemin en lots de 16. Le protocole dépasse les objectifs initiaux de 1 Mbit/s par sens et d’un p95 inférieur à 50 ms. Le coût CPU inclut les threads du modèle et la collecte des traces. Les mesures identifient les réarmements par lots et les échanges IPC du pilote. Leur contribution individuelle en cycles CPU reste à isoler.

Le binaire de faute distinct a copié la trame RX désignée en DMA. Quinze complétions RX ont été observées pour seize copies et seize TX. L’absence de complétion a entraîné le désarmement et la quarantaine. Sur AArch64 le disque a encore fourni une lecture P9 et rendu ses sept cadres. Sur Q35 le témoin de panne concernait le NIC isolé. Les binaires nominaux, le diff de faute et les requalifications du confinement sont identifiés dans les archives. Ces résultats portent sur les machines virtuelles et binaires testés.

## Stockage pendant le trafic

Temps FLUSH en microsecondes. Les débits physiques de lecture et d’écriture couvrent la durée totale du parcours invité, en Mio/s. Le JSON donne également les débits rapportés au seul temps de service. Les percentiles sont calculés sur les mesures individuelles.

| Machine / géométrie | Charge | Nombre de FLUSH | p50 | p95 | p99 | Lecture / écriture |
|---|---|---:|---:|---:|---:|---:|
| AArch64 / 512 | Repos | 599 | 136 | 432 | 462 | 0,171 / 0,158 |
| AArch64 / 512 | Trafic | 723 | 145 | 466 | 511 | 0,072 / 0,073 |
| AArch64 / 4096 | Repos | 569 | 131 | 440 | 464 | 0,260 / 0,234 |
| AArch64 / 4096 | Trafic | 653 | 151 | 446 | 529 | 0,105 / 0,101 |
| Q35 / 512 | Repos | 257 | 4 128 | 5 700 | 6 948 | 0,102 / 0,070 |
| Q35 / 512 | Trafic | 257 | 4 121 | 4 704 | 5 941 | 0,102 / 0,071 |
| Q35 / 4096 | Repos | 257 | 4 161 | 5 855 | 6 470 | 0,102 / 0,071 |
| Q35 / 4096 | Trafic | 257 | 4 055 | 4 640 | 5 383 | 0,102 / 0,071 |

Sur AArch64, 256 transactions WRITE/SYNC/READ et la relecture indépendante du contenu ont réussi par parcours. L’activité Ethernet recouvre les opérations P9. Pour Q35, les quatre mesures M02 utilisent le même ELF, le même firmware et le même volume initial pour chaque paire. Les 128 transactions ont produit 128 publications distinctes. Pour chaque géométrie les soumissions et CQE des 257 FLUSH sont entièrement comprises entre les premières et dernières complétions du million de trames par sens. Sur les mesures exploratoires à 100 000 trames, seulement 43 des 257 soumissions étaient couvertes. Deux remontages supplémentaires en lecture seule des volumes M01 après redémarrage ont retrouvé le fichier de 4 231 octets, avec conservation de l’image complète et contrôle des deux ancres. Le SHA-256 du fichier est `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`.

Sur AArch64, la lecture et l’écriture ralentissent sous trafic et les p99 augmentent dans les deux géométries. Le maximum observé sous trafic atteint 3 288,992 µs sur P9-512. Les réserves historiques restent distinctes pour le statut BUSY (24), le p95 de 146 à 174 µs sur P9-4096 et le pic de 407,008 µs sur P9-512. Les mesures Q35 sont des observations sur le parcours NVMe privé. Une paire de campagnes par géométrie ne permet pas d’attribuer une amélioration au réseau.

## Ressources et compatibilité

Les compteurs détaillés distinguent image résidente partagée, piles, tables CPU, IPC, données utilisateur, cache et DMA. Les colonnes ne sont pas toutes additives. Le budget de conception de 92 Kio est partiel. Les chiffres ci-dessous sont les relevés aux étapes instrumentées.

| Parcours | Avant | Files pleines | Après arrêt | Explication |
|---|---:|---:|---:|---|
| AArch64, endurance | 1 171 | 1 250 | 1 183 cadres résidents | 38 cadres DMA au pic, soit 152 Kio incluant 9 cadres de contrôleur, 7 disque et 22 NIC. Zéro DMA en fin de parcours, 12 cadres de cache supplémentaires |
| Q35, endurance NIC seul | 1 359 | 1 418 | 1 362 cadres résidents | 30 cadres DMA pendant l’échange. Racine VT-d vide, file QI et page WAIT restent publiées, soit 3 cadres |
| Q35, NVMe et NIC | 14 DMA avec disque préparé | 40 DMA | 8 DMA du contrôleur en place | 6 pages de données NVMe, 22 pages NIC, 4 tables supplémentaires pour le NIC et 8 pages du superviseur NVMe existant. Surcoût du deuxième arbre mesuré à 16 Kio |

Pour le parcours d’endurance, la différence d’activation du processus réseau est de 29 cadres. Le détail comprend les piles, les tables CPU, les pages utilisateur et les ressources IPC. Les profils à deux domaines mesurent respectivement 3, 4, 4, 2 et 1 racines pour les PID 3 à 7. Après restitution, la fenêtre mesurée revient à zéro page IPC et zéro processus. Le budget mémoire total sur toutes les configurations physiques et le coût de code imputable au seul réseau dans l’image partagée demeurent ouverts. Les neuf contrôles ABI ont réussi. La comparaison des noyaux ordinaires porte séparément sur l’entrée, les quatre PT_LOAD et l’ELF complet.

## Réserves de clôture

La stabilité d’IR/IRTA est établie sur les trajectoires enregistrées. Le témoin de divergence après publication a été mené avec les maîtres désarmés. Les deux ordres de retrait ont été exercés. Restent à qualifier une contention effective de deux demandes de reconfiguration, un WAIT matériel réellement tardif ou absent, les pannes locales symétriques avec le second périphérique actif et les redémarrages répétés de chaque bail dans ce profil double. Ces réserves empêchent la clôture du lot. Le banc OVMF et les modèles QEMU modifiés restent privés au laboratoire.

Les profils, binaires, acquisitions et libérations du verrou, échantillons bruts, états des registres et reçus se trouvent dans les pièces ci-dessous. Les captures DMA archivées sont celles des fixtures synthétiques. Les supports et clés de signature restent conservés localement.

[Mesures consolidées](validation/virtio-net/finale-20261125/bilan.json) · [Segments ordinaires](validation/virtio-net/finale-20261125/noyaux-ordinaires.json) · [Manifeste SHA-256](validation/virtio-net/finale-20261125/SHA256SUMS) · [Contrat des deux domaines](virtio-net-q35-stockage-contrat.md)

## Vérifications sur le code livré

Les deux noyaux ordinaires ont retrouvé, sur chaque architecture, l’entrée de référence et les octets, adresses, tailles, alignements et permissions des quatre segments PT_LOAD. L’empreinte de l’ELF entier diffère dans les deux cas. Le client de mesure est construit séparément pour le laboratoire. Les neuf tests du registre ABI ont été rejoués sur cette source. Les suites de modèles ont réussi 67 contrôles sur Darwin ARM64 et 67 sous Rosetta x86_64.

Sur le commit `a2709fb`, le parcours Q35/P9-512 F01 a vérifié 100 000 trames dans chaque sens et le bilan NVMe complet. Les valeurs de performance du tableau restent attachées aux campagnes M02 et AArch64 série 03, identifiées par leurs binaires. Les deux répétitions M03 sur Q35 et repos-04 sur AArch64/P9-4096 ont été arrêtées pendant l’attente du verrou. Leurs rapports partiels sont conservés. L’annexe identifie individuellement les témoins terminaux, leurs binaires, les statuts observés et leurs temps d’exécution. [Confirmations terminales](validation/virtio-net/finale-20261125/confirmations.json)
