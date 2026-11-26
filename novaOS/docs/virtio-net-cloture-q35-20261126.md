# VirtIO-net — Audit de clôture fonctionnelle du laboratoire QEMU

Le 26 novembre 2026, la continuité après panne locale est démontrée dans les deux sens sur Q35, avec P9-512 et P9-4096. Les seize essais du superviseur double domaine ont produit les issues attendues. Deux essais d’endurance et un essai de reprise après saturation complètent l’audit des chemins Ethernet. La proposition de clôture porte sur le premier lot Ethernet et sur ces configurations de laboratoire.

## Révisions et portée des preuves

| Révision | Ce qu’elle identifie |
|---|---|
| `77657fb3f04b842b16211491d04357350a6e9cd4` | Publication précédente, avec les essais de performance et de coexistence du lot |
| `0dbbf90014e01cbb41bf73d7d5dbc8ffc69ac3d8` | Construction et sélection du QEMU de faute WAIT |
| `c4bf40ce468046585c462caf7085ca9564db7ef2` | Seize essais double domaine et confirmations d’endurance AArch64 et Q35 |
| `1863ff413db0c9c47a52d3e1a1592e718aae22e6` | Correction du rendez-vous de pression RX du banc Q35 et témoin invité réussi |

Les deux ELF double domaine reconstruits depuis `c4bf40c` retrouvent les SHA-256 des supports testés. Le noyau de réarmement a l’empreinte `635d60aca7cb7e46d116e947e12523531bc8bcda8fb59ce5544dd5094be270cc`. Celui des cycles a l’empreinte `26b94a2253b64ac49d75f673317da74197a7f6229f12fa0f7588fb48bb9a0c0a`. Chaque campagne conserve l’identité du firmware, du modèle, du support, du volume et des titulaires.

Sur `1863ff4`, la correction s’active à la compilation du seul profil de pression Q35 à un bail. Pour chacun des deux ELF double domaine, l’entrée et les quatre descripteurs de chargement sont identiques à ceux de `c4bf40c`. Les segments de code et de données modifiables sont identiques octet par octet. Dans chaque segment en lecture seule, les 19 octets différents correspondent entièrement à 19 numéros de ligne de diagnostic Rust. Pour chaque emplacement, le chemin source, le texte de la ligne et la colonne ont été rapprochés entre les commits. La même comparaison identifie seulement des déplacements de lignes pour les deux profils d’endurance. L’ELF du témoin de pression reconstruit après commit est identique en entier à l’ELF exécuté. Ces résultats établissent la portée de réutilisation des essais. [Comparaisons par segment et par octet](validation/virtio-net/cloture-20261126/applicabilite-candidat-final.json)

## Continuité après mort d’un pilote

| Pilote défaillant | Événement réellement exercé | P9-512 | P9-4096 |
|---|---|---|---|
| NIC, PID 7, SID 56, DID 3 | Sortie Ring 3 `Exit(9)` après le premier lot et la remise à disposition de buffers RX. Drainage du NIC en `Quarantine`, Bus Master relu désarmé | 21 READ, 7 WRITE, 3 FLUSH NVMe après désarmement. Tous les CQE valent 0 | Mêmes comptes et mêmes statuts |
| NVMe, PID 3, SID 48, DID 1 | Sortie Ring 3 `Exit(0)` après la soumission matérielle du premier WRITE de la transaction, avant collecte du résultat par le pilote | Après désarmement du disque, 32 TX et 32 RX supplémentaires, octets vérifiés dans l’invité et au pair. Drainage réseau `Ok(())` | Mêmes comptes et même drainage |

Le stockage survivant confirme la publication après les deux FLUSH de la transaction, CID 32 et 34. Le troisième FLUSH du parcours appartient au montage. Le lecteur P9 indépendant a retrouvé les deux ancres, la génération 4 et les 4 231 octets du fichier `/sortie`, d’empreinte `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`. Dans l’autre sens, le namespace conserve les ancres et le fichier de la génération 3. Les 4 080 octets modifiés se trouvent dans le premier bloc de copie sur écriture visé, hors des ancres. La sauvegarde interrompue ne reçoit aucune confirmation.

Avant armement, le banc inventorie les 40 cadres physiques et vérifie leur disjonction. Au point d’isolement, le superviseur relit les tables, les permissions, le Bus Master et l’état du contrôleur. Pendant la poursuite, le chemin du disque contrôle VT-d avant de livrer les complétions. Le chemin réseau refuse une nouvelle opération si les vérifications du contrôleur, du cookie, des indices de QI ou du désarmement du disque échouent. Le témoin recoupe les ordres matériels avec la trace PCI et les E/S. À la fin, une lecture indépendante de la racine, des contextes et des feuilles vérifie toutes les translations conservées. Le seul reçu de retrait, cookie 4, désigne le périphérique sain. Le bail du pilote mort conserve tous ses cadres. [Rapports individuels et inventaires physiques](validation/virtio-net/cloture-20261126/bilan.json) · [Traces de continuité](validation/virtio-net/cloture-20261126/continuites-locales.sha256)

## Matrice des seize essais Q35 à deux domaines

| Scénario | P9-512 | P9-4096 | Observation terminale |
|---|---|---|---|
| Panne du NIC | Réussi | Réussi | 26 cadres du NIC en quarantaine, 8 pour le contrôleur |
| Panne du NVMe | Réussi | Réussi | 6 cadres du disque en quarantaine, 8 pour le contrôleur |
| Deux demandes pendantes, priorité au NIC | Réussi | Réussi | DMA 40 → 14 → 8 cadres |
| Deux demandes pendantes, priorité au NVMe | Réussi | Réussi | DMA 40 → 34 → 8 cadres |
| WAIT court sur le premier retrait | Réussi, NIC premier | Réussi, NVMe premier | Cookies matériels 4 puis 5, tête et queue à 240, 8 cadres |
| WAIT suspendu sur le premier retrait | Refus attendu, NIC premier | Refus attendu, NVMe premier | Cookie 0, tête 176, queue 192, 40 cadres conservés |
| WAIT livré après expiration | Refus attendu, NIC premier | Refus attendu, NVMe premier | Cookie 4 et tête 192 après expiration. Les deux demandes restent bloquées et les 40 cadres restent épinglés |
| Quatre redémarrages des deux baux dans un même invité | Réussi | Réussi | 4 générations, 17 WAIT démarrage compris, 192 TX et 192 RX, 4 publications P9 |

Dans les quatre essais de contention nominaux, les deux tâches déposent des demandes au nom de titulaires distincts. Les deux emplacements sont occupés avant la première invalidation de retrait. Un doublon et une collecte avant WAIT sont refusés. Le superviseur vérifie séparément les preuves de drainage puis sérialise la file QI. Les tuples SID, DID, PID, génération, opération et cookie identifient chaque reçu. Le réseau emploie INTx sur GSI 23. Les IRQ ne décident pas de la restitution. La coordination occupe 48 octets pour les deux emplacements et 80 octets verrou inclus, sans racine IPC supplémentaire. [Demandes et acquittements](validation/virtio-net/cloture-20261126/retraits-concurrents.sha256) · [Redémarrages](validation/virtio-net/cloture-20261126/redemarrages.sha256)

Le modèle de faute QEMU 11.1.3 a l’empreinte `e19eefd4e4bd662fafb85a2093fdbb12723c1cdec4cb960bfe0b68723d6dac67`. Il suspend un seul descripteur WAIT valide. Les modes court et tardif reprennent le traitement après respectivement 5 ms et 4 s sur l’horloge virtuelle. Le mode absent conserve le descripteur suspendu jusqu’à la fin du témoin. Les sept précontrôles à maîtres fermés couvrent signature EFI refusée, signature admise avant dbx, refus après dbx, noyau altéré, absence d’IOMMU, publication des deux domaines et retrait du NIC. Les contrôles ont été effectués sur ce binaire exact avant les ouvertures expérimentales. Trois sondes fermées enregistrent respectivement 6 659 000 cycles TSC pour la confirmation courte, 1 074 535 000 pour le refus en l’absence du cookie et 1 069 967 000 pour le refus avant son retour tardif. Les six essais actifs ajoutent les constats matériels, les refus de collecte et de nouvelle reconfiguration. Dans les quatre essais incertains, `NVME bilan ok=false` est le statut invité attendu. Aucun retrait dépendant n’aboutit. Le modèle nominal double domaine garde l’empreinte `f46d87c83a72ddea961b2b02a357c66b34c8b424f5613658262567e516f79f99`. [Préconditions épinglées](validation/virtio-net/cloture-20261126/preconditions-wait.json) · [WAIT sous contention](validation/virtio-net/cloture-20261126/wait-concurrents.sha256)

## Mémoire et IPC

Cadres de 4 Kio mesurés sur le parcours Q35 double domaine. La colonne IPC compte des pages. Les racines sont comptées par titulaire séparément.

| Étape | DMA | IPC | Cadres résidents |
|---|---:|---:|---:|
| Deux services prêts, avant le trafic du témoin de panne | 14 | 18 | 2 394 |
| Files pleines, P9-512 / P9-4096 | 40 | 18 | 2 436 / 2 439 |
| NIC mort et désarmé, stockage encore présent | 40 | 17 | 2 395 / 2 398 |
| Stockage mort et désarmé, réseau encore présent | 40 | 15 | 2 344 |
| Fin après panne du NIC | 34 | 0 | 1 915 |
| Fin après panne du NVMe | 14 | 0 | 1 895 |
| Fin après incertitude de la file commune | 40 | 0 | 1 921 |
| Fin de chacun des quatre cycles nominaux | 8 | 0 | 1 889 |

Pendant toute la durée de vie du superviseur de cet invité, les huit cadres permanents du contrôleur totalisent 32 Kio. Ils contiennent une racine VT-d, une table de contextes partagée, quatre tables de traduction pour le disque, une file d’invalidation et une page de résultat WAIT. En activité s’ajoutent six cadres pour les files et données NVMe, quatre tables du NIC et vingt-deux cadres de files et buffers Ethernet. Le pic DMA mesuré atteint ainsi 160 Kio. Les pages DMA participent déjà à la mémoire résidente. Après le premier cycle, le cache conserve trois cadres. Chaque fin de cycle retrouve ensuite les mêmes compteurs, dont 25 343 cadres libres, zéro processus, zéro page utilisateur et zéro page IPC. Les cinq acteurs ont respectivement 3, 4, 4, 2 et 1 racines. L’absence de croissance cumulative est mesurée sur quatre cycles par géométrie.

L’endurance AArch64 du candidat monte à 38 cadres DMA, soit 152 Kio, puis revient à zéro DMA. L’endurance Q35 à un domaine revient aux trois cadres de son contrôleur. Le budget initial de 92 Kio était partiel. Les relevés par phase détaillent image, tables CPU, piles, tas, cache, DMA et IPC. L’attribution exclusive de chaque octet de code de l’image partagée à un pilote reste indéterminée. [Mesures brutes invitées](validation/virtio-net/cloture-20261126/bilan.json)

## Couverture Ethernet et stockage

| Critère | AArch64 QEMU | x86_64 QEMU |
|---|---|---|
| Endurance, unicité, contenu au pair et dans l’invité | 100 000 TX et 100 000 RX sur le candidat | 100 000 TX et 100 000 RX sur le candidat |
| Bouclage 16 bits, progression après réarmement | Indice 34 464, 6 249 réarmements | Indice 34 464, 6 249 réarmements |
| Saturation TX | 17e soumission refusée avec `WOULD_BLOCK (7)` | Même refus |
| Saturation RX, puis progression | Capture de 17 trames pour 16 buffers, puis 48/48 dans l’essai AArch64 archivé | Capture PCAP et état de file 17/16, puis 48/48, deux réarmements, `Exit(0)`, retrait confirmé en `1863ff4` |
| Copie RX en DMA avec perte d’une complétion | Modèle de faute dédié, 16 copies et 15 complétions, quarantaine, lecture P9 indépendante | Modèle de faute dédié, 16 copies et 15 complétions, quarantaine dans le profil NIC seul |
| Interruptions, retrait et redémarrage | Interruptions du NIC et quatre générations du pilote dans l’essai archivé | GSI 23 observé. Quatre générations des deux baux dans chacun des deux invités de cycles |
| NovaFS sous trafic | 256 transactions et relecture par parcours P9-512 et P9-4096 | 128 transactions par parcours, 257 FLUSH intégralement couverts par le trafic dans les campagnes M02 |

Le premier rapport annonçait à tort la réussite complète du témoin de saturation RX Q35. Le PCAP de `q35-pression-01` prouvait l’arrivée anticipée de la 17e trame. Le pair terminait sur une erreur et l’invité sur `Exit(8)` et `Quarantine`. Le défaut a été reproduit sur `c4bf40c`. Le superviseur dormait pendant la fenêtre d’observation alors que le pilote attendait déjà la réponse IPC au réarmement. Le témoin corrigé attend, sur sa page privée et avant le dépôt de cette demande, la fin de la fenêtre de capture pour sa génération. Le superviseur vérifie que la file IPC est vide avant de le réveiller. La limite IPC de la commande et les permissions DMA sont conservées. Les essais en échec et en réussite sont archivés séparément. [Capture, états de file et résultats du correctif](validation/virtio-net/cloture-20261126/saturation-et-comparaisons.sha256) · [Endurance du candidat](validation/virtio-net/cloture-20261126/endurance-sur-candidat.sha256)

## Performances et réserves

Les mesures soutenues acceptées sur les binaires de référence sont de 254,096 Mbit/s RX et 121,559 Mbit/s TX sur AArch64, 63,461 Mbit/s RX et 60,792 Mbit/s TX sur Q35. En duplex, les débits par sens sont de 93,627 et 49,901 Mbit/s respectivement. Les p95 RTT des trois fenêtres vont de 1,837 à 1,883 ms sur AArch64 et de 4,979 à 5,022 ms sur Q35. Les nouvelles confirmations d’endurance portent sur l’intégrité et la progression. Les distributions de débit, RTT et CPU restent celles des essais de performance identifiés dans le rapport précédent. [Campagnes et mesures de référence](virtio-net-finale-20261125.md)

Sous trafic, les débits AArch64 de lecture et d’écriture, rapportés au parcours complet, baissent respectivement de 58,2 % et 53,7 % en P9-512, de 59,7 % et 56,6 % en P9-4096. Sur le compteur monotone invité à 62,5 MHz, le parcours 512 passe de 42,22 à 104,41 s. Les fenêtres cumulées de lecture passent de 3,71 à 7,46 s. Le parcours 4096 passe de 27,55 à 69,94 s, ses lectures de 0,49 à 0,97 s. Les p99 FLUSH passent de 462 à 511 µs et de 464 à 529 µs. Une part importante de la hausse s’observe hors des fenêtres physiques READ/WRITE/FLUSH. L’invité utilise un vCPU. Les traces distinguent échanges IPC, réarmements et opérations disque. Elles ne ventilent pas séparément le temps CPU du pilote, de l’ordonnanceur, du pair, de la collecte et de l’émulation. Le maximum de 3 288,992 µs du parcours 512 chargé reste visible. Les réserves antérieures portent aussi sur `BUSY (24)`, le p95 de 146 à 174 µs en P9-4096 et le pic historique de 407,008 µs en P9-512. [Analyse des durées AArch64](validation/virtio-net/cloture-20261126/analyse-stockage-aarch64.json)

Les 48 entrées du registre de sélecteurs v1 et les sources NovaFS sont identiques entre les deux publications. Les neuf contrôles ABI ont réussi sur le candidat final. Les suites de modèles totalisent 67 réussites sur Darwin ARM64 et 67 sous Rosetta x86_64, auxquelles s’ajoutent trois tests des emplacements dans chaque environnement. Les deux noyaux ordinaires conservent l’entrée et les quatre segments PT_LOAD de la référence. L’empreinte de l’ELF entier diffère de la livraison précédente sur chaque architecture. Entre les deux commits fonctionnels de cette campagne, chacun des deux ELF ordinaires est identique en entier. La reproductibilité complète entre environnements reste ouverte. [Comparaison des noyaux ordinaires](validation/virtio-net/cloture-20261126/noyaux-ordinaires.json)

## Archives et recommandation

Les deux volumes de départ et les seize volumes terminaux synthétiques ont été relus après décompression avec le lecteur NovaFS. Les dix-huit relectures retrouvent les empreintes attendues. Pour chaque volume terminal, la génération, les deux ancres, le contenu, la garde et le résultat complet concordent avec le bilan invité et le contrôle initial du banc. [Résultats des dix-huit relectures indépendantes](validation/virtio-net/cloture-20261126/relecture-p9-archives.json) Les sept contrôles du nouveau binaire, les sources et le diff QEMU, les sorties des suites natives, les acquisitions et libérations du verrou ainsi que les essais intermédiaires sont archivés. La vérification du manifeste historique et de chacun des 1 034 fichiers de ses 34 archives a réussi. [Volumes de départ](validation/virtio-net/cloture-20261126/volumes-initiaux.sha256) · [Volumes terminaux](validation/virtio-net/cloture-20261126/volumes-synthetiques.sha256) · [Contrôle de l’historique](validation/virtio-net/cloture-20261126/verification-historique.json) · [Manifeste SHA-256](validation/virtio-net/cloture-20261126/SHA256SUMS)

Les 16 essais double domaine ont pris 75,641 s d’exécution cumulée et 0,0011 s d’attente cumulée du verrou. Toutes les heures UTC par essai figurent dans la chronologie. Les 59 exécutions instrumentées de ce mandat, développement inclus, totalisent 624,575 s d’essais et 196,630 s d’attente par demande. Certaines attentes se chevauchent avec un autre essai en cours. Les sept échecs conservés comprennent les lectures tardives des données de démarrage, la répétition d’un contenu de cycle identique, l’absence de recouvrement des E/S, trois refus pendant la mise au point de la panne disque et l’expiration du témoin de pression RX. Pour la panne disque, les traces distinguent un `TIMED_OUT (8)`, une calibration effectuée sous verrou de l’ordonnanceur, puis un contrôle hôte qui refusait le FLUSH initial du montage. Le contrôle corrigé exige un FLUSH de montage acquitté avec zéro écriture avant la transaction et limite les changements au premier WRITE non confirmé. Aucun de ces sept essais ne contribue au nombre de réussites. [Chronologie du verrou et résultats individuels](validation/virtio-net/cloture-20261126/chronologie.json) · [Traces de développement](validation/virtio-net/cloture-20261126/developpement-et-diagnostics.sha256)

### Commandes et identification du banc

Depuis le répertoire `novaOS`, les constructions invitées emploient `rustup run stable cargo build --offline --release --locked` avec les cibles `x86_64-unknown-none` ou `aarch64-unknown-none-softfloat`. Le double domaine sélectionne `net-q35-duo` et la série `rearmement` ou `redemarrage`. La pression utilise `net-q35-lab,net-ethernet-lab` et `NOVAOS_NET_SERIE=pression-rx`. Les tests de modèles et du registre utilisent `python -B tests/materiel_modeles.py` et `python -B tests/abi_registre.py`. Les trois tests des emplacements sont compilés par `rustc --edition 2024 --test` pour chacun des deux triplets Darwin. Les oracles matériels sont ceux de `virtio_net_pio_q35.py`, `virtio_net_relais_q35.py` et `virtio_net_disque_arm.py`. Pour les seize démarrages double domaine, le banc appelle le chemin de qualification sous le verrou commun avec le support, le scénario, la géométrie, le volume d’origine et le lecteur indiqués dans les pièces par essai. Les fichiers `arguments.json` conservent les arguments exacts de QEMU. [Journaux des contrôles sur le candidat final](validation/virtio-net/cloture-20261126/controles-natifs-candidat.sha256)

Depuis le répertoire du manifeste, la commande `shasum -a 256 -c SHA256SUMS` vérifie les pièces distribuées. Chaque archive `.tar.zst` possède son manifeste individuel `.sha256` avec les noms des membres et les empreintes de leur contenu décompressé.

**Recommandation MOA/MOE.** Prononcer la clôture fonctionnelle du premier lot Ethernet sur les combinaisons QEMU, firmwares et profils identifiés. Les deux pannes locales conservent les pages du pilote mort. La récupération de ces pages après une incertitude et une corruption arbitraire du contrôleur pendant des E/S ne sont pas qualifiées par ces essais. Les variantes OVMF et QEMU demeurent expérimentales. La restitution du support PIO de 34 Mio après le transfert définitif au noyau et la qualification sur matériel physique restent ouvertes.
