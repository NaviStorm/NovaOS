# VirtIO-net — négociation EVENT_IDX et limites des essais

**Candidat invité** `e81e3bbbf5b310c7a3dfac370ee326cc31b7f0b5`, branche `codex/pilote-virtio-net`. La négociation des quatre bits du profil privé et son refus en l’absence d’EVENT_IDX ont été exercés sur les deux architectures. Chaque essai démarre une VM distincte sous verrou commun. Le NIC reste sans bail de données. La vérification préalable a aussi mis en évidence une incompatibilité RX des deux sources QEMU dans le profil moderne sans fusion. [Analyse normative et arbitrage](virtio-net-revue-format-rx.md) · [Manifeste](validation/virtio-net/ethernet-20261125/SHA256SUMS)

## Résultats dans les invités

| Observation | Q35 x86_64, QEMU 11.1.3 | virt AArch64, QEMU UTM 10.0.12 |
|---|---|---|
| EVENT_IDX offert | Deux sélections relues à `0x320000020`, statut 11, BME=0. RX=16 et TX=16 configurés dans deux cycles. | Deux sélections relues au même masque et même statut. RX=16 et TX=16 configurés dans le premier cycle. |
| EVENT_IDX retiré de l’offre | `Feature`, statut 0 relu après reset, BME=0. Aucune file activée. Retrait du domaine et confirmation VT-d cookie 3. | `Feature`, statut 0, BME=0. Aucune file activée. Zéro cadre DMA final et volume P9/512 initial intact. |
| Contrôles supplémentaires | Firmware expérimental Q35, démarrage signé du noyau de test, domaine NIC 3, racine terminale vide et QI drainée. | Contrôle indépendant des affectations et retraits disque/NIC du reset à l’arrêt. En positif, fichier de 4 231 octets écrit, synchronisé, relu et contrôlé hors ligne, génération 7. |
| Temps du banc, positif / négatif | 1,920 / 1,874 s | 6,798 / 5,801 s |
| Données, interruption et arrêt en charge | Aucune trame ni IRQ de données exercée | Aucune trame ni IRQ de données exercée |

Le bit 29 est exigé en plus des bits MAC, VERSION_1 et ACCESS_PLATFORM. Les deux banques des fonctionnalités du pilote sont relues, les autres bits doivent être nuls et le statut doit être exactement `ACKNOWLEDGE | DRIVER | FEATURES_OK`. Le probe n’active jamais DRIVER_OK. Il vérifie le masquage PCI INTx et BME, les emplacements des anneaux et les tailles réellement acceptées. Les résultats finaux portent le préfixe `e81e3bb-`. Les quatre parcours `f46a652-` et les quatre essais de mise au point sont conservés avec leurs propres empreintes. [Campagnes, ELF et verrou](validation/virtio-net/ethernet-20261125/campagnes.json) · [Traces du candidat final](validation/virtio-net/ethernet-20261125/traces-candidat.tar.gz) · [Traces intermédiaires](validation/virtio-net/ethernet-20261125/traces-event.tar.gz)

Les quatre essais finaux cumulent 16,393 s d’exécution et 0,940 ms d’attente du verrou. Les vingt démarrages du mandat, dont les huit mesures P9, cumulent 100,009 s de banc et 5,080 ms d’attente. Les acquisitions et libérations ne se chevauchent pas.

## Inventaire des accès du modèle

Topologie examinée : split virtqueues RX/TX de 16, un seul descripteur par trame, quatre fonctionnalités négociées, transport PCI moderne, backend émulé logiciel et hub interne. L’offset est relatif à la page concernée. Les droits sont ceux du **NIC**. [Initialisation et notification QEMU 10.0.12](https://github.com/qemu/qemu/blob/v10.0.12/hw/virtio/virtio.c) · [QEMU 11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/virtio/virtio.c) · [Section 2.7 de VirtIO 1.3](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)

| Région | Droit | Chemin normal examiné |
|---|---|---|
| Descripteurs RX/TX | Lecture | `virtqueue_split_pop` et calcul de l’espace disponible. 16 descripteurs de 16 octets. Le profil exclut les chaînes indirectes et les anneaux packed. |
| Anneau disponible RX/TX | Lecture | Index à 2, entrées de 4 à 35, seuil `used_event` à 36 pour décider de notifier le pilote. |
| Anneau utilisé RX/TX | Écriture | Écriture des id et longueurs de complétion, puis de l’index à 2. Avec EVENT_IDX, `avail_event` à 132. Le modèle maintient l’index courant dans son propre état. Le chemin sans EVENT_IDX relit les drapeaux à 0 avant écriture et reste refusé. |
| Tampons RX | Écriture | 16 emplacements de 2 048 octets. Copie de l’en-tête puis de la trame dans le vecteur entrant. La conformité du champ RX est l’objet de l’arbitrage. |
| Tampons TX | Lecture | 16 emplacements de 2 048 octets. Lecture du vecteur sortant, puis publication de la complétion dans l’anneau utilisé. |
| Notification du pilote au NIC | Écriture CPU dans le BAR | La découverte vérifie le BAR et la plage de notification. Aucun kick de données dans les invités de ce lot. |
| Signalement du NIC | Confrontation de l’index local et de `used_event` | Le banc hôte vérifie la décision d’interrompre. L’acheminement d’une véritable IRQ à la suite d’une trame reste à exercer. |

Le banc hôte compile sept fonctions amont intactes et fournit des accesseurs instrumentés à la mémoire des anneaux. Pour **chaque** tag, 21 168 128 comparaisons couvrent tous les anciens indices 16 bits, des lots de 0 à 16 et 19 positions de seuil. 2 584 cas franchissent le retour à zéro. Chaque lecture observée de ces fonctions est à 2 ou 36 dans la page disponible, chaque écriture à 132 dans la page utilisée. Le chemin de notification sans EVENT_IDX ferait échouer ce banc. L’activation et la désactivation des notifications sont exercées au niveau des fonctions C. Cette preuve couvre leur calcul et leurs adresses dans le modèle hôte. Les files pleines, les IRQ réelles, les queues en vol au reset et la restitution de buffers ayant porté des trames restent à qualifier en invité. [Résultats des deux modèles](validation/virtio-net/ethernet-20261125/notifications-hote.json) · [Extraits des exécutables installés](validation/virtio-net/ethernet-20261125/modeles.json)

La revue inclut la préparation des caches de régions, les fonctions de lecture et d’écriture lentes utilisées derrière IOMMU, les opérations de pop, fill, flush, la branche TX différée et le reset du périphérique. Les opérations de chargement d’état VM, certaines restaurations de queues et l’introspection QMP d’un élément de virtqueue comportent des lectures de l’anneau utilisé. La migration, vhost et cette introspection restent hors du profil qualifié. Le QMP du banc relève les états PCI, IOMMU et les journaux. Le reset à vide a été exercé dans les invités. L’absence d’accès hors permission pendant du trafic et le drainage d’une transaction en vol exigent leurs propres traces une fois le modèle RX arbitré. [Sources épinglées et SHA-256](validation/virtio-net/ethernet-20261125/sources-qemu.json)

## Stockage et ressources

Huit démarrages additionnels P9/4096 avec les ELF de stockage historiques, quatre par configuration en ordre contrebalancé. Chaque démarrage exécute 64 lectures de bloc de 4 Kio, 16 FLUSH et la transaction avec relecture intégrale du fichier. Horloge invitée à 62,5 MHz. Les débits logiques ci-dessous incluent le traitement du service et du client. Le débit d’écriture inclut SYNC. Le contrôleur réseau est préparé dans la configuration à deux domaines et ne transporte aucune trame. [Séries brutes et statistiques](validation/virtio-net/ethernet-20261125/mesures-p9.json) · [Archive P9](validation/virtio-net/ethernet-20261125/traces-p9.tar.gz)

| P9/4096, 4 démarrages par ligne | Lecture bloc Mio/s | Lecture fichier Mio/s | Écriture fichier + SYNC Mio/s | FLUSH p50 / p95 / p99, µs |
|---|---:|---:|---:|---:|
| Gestionnaire historique | 51,361 | 0,0524 | 0,0470 | 67,008 / 158,992 / 202,000 |
| Deux domaines | 51,401 | 0,0546 | 0,0495 | 67,504 / 148,000 / 192,992 |

La hausse historique de p95 de 146 à 174 µs demeure dans le dossier. Les deux premiers FLUSH mesurés par configuration étaient respectivement 146 et 212 µs, puis 174 et 194 µs. Sur la nouvelle série de 64 FLUSH par configuration, le p95 après exclusion explicite du premier FLUSH de chaque démarrage vaut 106 et 82 µs. Les séries complètes restent la référence. Ces échantillons ne permettent pas d’attribuer l’écart au superviseur, de conclure à une accélération, ou de chiffrer la coexistence sous trafic. L’échec historique `BUSY (24)` en mode 11 sur les deux géométries reste ouvert. Il est reproduit avec les deux gestionnaires, lors de la lecture ordinaire sous écriture en attente dans le cas P9/512 instrumenté. L’oracle P9 reste inchangé. [Rapport d’isolation et de pannes](virtio-net-isolation-dma-20261125.md)

- **AArch64.** Dans l’essai positif final, 38 cadres DMA de 4 Kio au pic, dont 9 pour le contrôleur, 7 pour le disque et 22 pour le NIC. Ces 152 Kio incluent les six pages d’anneaux et les seize pages RX/TX. Après la fermeture du disque, 31 restent épinglés, puis zéro après retrait complet. Une racine IPC pour l’instance admise en préparation, zéro pour le témoin sans droit et zéro après collecte. Pendant la transaction P9, 1 521 cadres résidents pour tout l’invité, neuf cadres IPC, quatre processus. La réservation logicielle historique de 92 Kio recouvre les 88 Kio du NIC. L’addition de 92 et de 152 compterait ces 88 Kio deux fois.
- **Q35.** 30 cadres DMA comptés à la préparation, 27 rendus. Les trois restant affectés au contrôleur actif sont la racine vide de traduction, la file d’invalidation QI et la page où WAIT dépose le reçu. La lecture QMP confirme une racine vide, le reçu 3 et l’égalité tête/queue QI. Ces 12 Kio restent affectés tant que le contrôleur les référence. La mémoire totale d’un pilote en charge, ses piles, ses pics IPC et la restitution après circulation restent à mesurer.
- **ABI et noyaux ordinaires.** `make abi-check` réussi. Huit tests logiciels du pilote réussis en Darwin ARM64 et huit sous Rosetta x86_64. La vérification des exécutables ordinaires construits avant et après retrouve les mêmes entrées et les mêmes segments ELF PT_LOAD (adresses, tailles, permissions et octets) sur les deux architectures. Les deux SHA de fichiers entiers diffèrent par les tables de symboles et de noms non chargées. Les fichiers ABI et le moteur NovaFS ne changent pas depuis la référence réseau. [Empreintes et segments ordinaires](validation/virtio-net/ethernet-20261125/elf-ordinaires.json)

## Périmètre restant

Le réseau QEMU isolé devra encore livrer une trame à un pair, produire une IRQ du NIC, traverser la saturation et le bouclage en DMA réel, drainer les opérations lors d’un redémarrage ou d’une réponse durablement absente, puis restituer ses ressources. Le débit, le RTT, les plafonds IPC de tous les acteurs en charge et l’impact sur NovaFS sous trafic sont encore à mesurer. Les objectifs de 1 Mbit/s par sens et de RTT p95 inférieur à 50 ms restent ceux du mandat initial. Les garanties Q35 dépendent du firmware expérimental dans ce laboratoire. Les réserves de confinement, de NovaFS et de portabilité sur matériel physique sont conservées. [Objectifs initiaux](virtio-net-qemu.md) · [Chaîne Q35](virtio-net-ovmf-q35.md)

## Reproduction

À la racine du worktree réseau :

```sh
ev=novaOS/docs/validation/virtio-net/ethernet-20261125
amont=../archives-virtio-net/ethernet-20261125/amont
python3 -B novaOS/tests/virtio_net_rx_modele.py "$amont" "$ev/sources-qemu.json"
python3 -B novaOS/tests/virtio_net_notifications_modele.py "$amont" "$ev/sources-qemu.json"
(cd "$ev" && shasum -a 256 -c SHA256SUMS)
```

Le premier programme retourne 2 quand la réception moderne sans fusion échoue au contrôle normatif. Les sources amont sont rangées par tag dans `amont/v10.0.12` et `amont/v11.1.3`, avec les noms de base des URL du manifeste. Les ELF, les images de test et les volumes relus sont conservés en dehors des temporaires dans `OS-worktrees/archives-virtio-net/ethernet-20261125`. Les archives publiées rassemblent les journaux et mesures du banc. Les images, variables UEFI et états TPM restent locaux.
