# NovaOS v0.16.6 — Qualification VirtIO-blk multipage

Branche expérimentale `codex/es-groupees-0166`, base `8d941022daefb0e232f02f67e21da3a80f9e998c`. Le chemin mesuré est celui de `vi :w` vers un nom libre. La comparaison part du pilote physique de 4 Kio et du plan CoW déjà groupé au niveau des services. Les fichiers NovaFS, leurs métadonnées P9 et les capacités des clients suivent les mêmes contrats. [Qualification précédente](es-v0.16.6-plages.md) · [Relevés par exécution](validation/es-v0.16.6-dma.json)

## Requête disque et isolation

Le pilote utilisateur parcourt dans l'ordre les LBA reçus et propose une suite contiguë. Le composant noyau vérifie de nouveau **chaque LBA** dans la liste figée par le moteur, le rang courant, le nombre de pages restantes, le sens, l’identité du processus et les deux extrémités de la réservation. Il vérifie séparément le propriétaire et l'époque du bail du disque, ses droits et les bornes de la partition. Il traduit ensuite le début en secteurs VirtIO de 512 octets avec multiplication contrôlée. À la notification, il revalide le bail et la réservation sous masque sur le BSP. Une discontinuité, une répétition de LBA ou la fin du lot termine la chaîne. Le pilote conserve l'ordre des écritures.

Le transport négocie les options `VIRTIO_BLK_F_SEG_MAX` et `VIRTIO_BLK_F_SIZE_MAX` offertes par le disque. Il borne le nombre de pages au minimum de 64, du maximum négocié et du nombre de descripteurs disponibles moins deux. Sans `SEG_MAX`, il utilise un seul segment de données. Un périphérique annonçant des segments limités à moins de 4 Kio sort du périmètre de ce prototype. Une chaîne directe contient l’en-tête, un descripteur par page de données et le statut. Le noyau construit toutes les adresses. La file bloc retient au plus 128 entrées. Les files de commande et d’événements VirtIO-IOMMU gardent leurs huit entrées. [Spécification VirtIO 1.2, files divisées et périphérique bloc](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html) · [Fixture du pilote](../tests/fixtures/io-0166-virtio-multipage.patch)

Les cadres privés de 4 Kio sont alloués à la demande et initialisés avant leur traduction. Pour le périphérique, la banque IN autorise uniquement l’écriture et la banque OUT uniquement la lecture. Les slots IOVA de données couvrent `8..72` et `72..136`, à partir de `0x08000000`, bornes supérieures exclues. Le slot 7 reste une garde. L’arène peut épingler 64 cadres par sens, soit **512 Kio au total**, en plus des pages historiques de transport et d’IOMMU. Une requête peut décrire 256 Kio. Les trois pages existantes du vring suffisent. Aucune page utilisateur n’est mappée dans le domaine du disque.

OUT copie le texte du moteur vers l’arène avant la soumission. IN revalide toutes les pages de destination sous le même verrou mémoire avant la première copie. La copie IN commence après l’indice utilisé attendu, la barrière mémoire, l’interruption correspondante, le contrôle des fautes IOMMU, l'identifiant de tête, la longueur exacte et le statut de succès. Pour IN la longueur attendue comprend les données et le statut. Pour OUT elle vaut un octet. Le noyau libère les verrous du domaine IPC et de la mémoire pendant l'attente du disque. Le verrou du transport sérialise l'unique chaîne en vol.

Le retrait ferme l'admission. Une chaîne déjà publiée conserve ses pages. Une complétion incertaine ferme la session et empêche d'acquitter le retrait du lot. L'arrêt du transport attend la file, réinitialise le périphérique, désactive le bus mastering, purge la route IRQ, retire les traductions puis vérifie le détachement ou l'invalidation du domaine. Les cadres sont effacés avant leur restitution à l’allocateur de pages (Buddy). Un retrait matériel incertain les laisse épinglés. Les pages CoW déjà écrites peuvent rester sur disque après une erreur. Seule une génération intégralement validée est accessible à la relecture.

Cette synchronisation utilise le BSP de l’invité à un seul vCPU et les mappings DMA cohérents des profils testés. Une extension à plusieurs processeurs actifs ou à un périphérique non cohérent demanderait une qualification distincte.

Les deux baux ont des rôles distincts. Le descripteur de huit `u64` de l’appel privé `0x1003a` contient le tag, la capacité de file, l’époque du bail matériel, le LBA initial, le rang, le nombre total de pages réservées, le nombre demandé et le sens (0 pour IN, 1 pour OUT). La taille du message est 64 octets. La réponse indique le nombre effectivement traité. Le pilote peut poursuivre au rang suivant uniquement après la réussite. Les appels existants restent disponibles, notamment pour les usagers historiques et le FLUSH. Le véhicule de distribution est une fixture opt-in appliquée dans une archive privée.

## Publication et méthode de mesure

F1 suit la préparation CoW et les écritures dont dépendent les métadonnées. F2 suit l’écriture de l’ancre. Chaque FLUSH emprunte la voie déjà qualifiée, dans l’ordre de la réservation. Le service répond après l’acquittement de F2. La complétion vérifiée d’une requête OUT autorise le réemploi de son tampon dans la même arène. La persistance promise exige aussi le FLUSH de publication et les garanties du stockage sous-jacent. Les essais arrêtent la VM, exportent ou relisent son disque et vérifient les générations et le contenu. La coupure électrique du Mac et l'atomicité générale des secteurs restent hors de la preuve. [Contrats de publication P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

- **Temps utilisateur**. Horloge monotone du Mac entre l’envoi de `w destination` et le message de succès de l’éditeur. Le banc répond immédiatement à la demande de confirmation. Les temps avant et après « OUI » sont disponibles dans les relevés. Un second noyau instrumenté mesure la fenêtre de `SAVE start` à `SAVE end` et l’acquittement de F2 dans cette fenêtre.
- **IPC**. La sonde compte les deux sens de chaque liaison uniquement pendant la sauvegarde. Le total conservateur comprend les dons et les contrôles locaux des capacités et du prêt. Le JSON donne aussi le total en incluant les copies locales du prêt. Les notifications MMIO, interruptions et opérations locales de copie DMA ont leurs propres compteurs. [Vérificateur des métriques](../tests/vi_ipc_sauvegarde.py)
- **Latences physiques**. Les durées sont cumulées par sauvegarde. La soumission va de la préparation de l'anneau à l'écriture du doorbell. L'attente va du retour du doorbell à l'observation de `used` dans l'invité. L’acquittement va de cette observation aux contrôles IRQ, IOMMU, statut et longueur. La sonde mesure aussi le délai jusqu'à l'entrée IRQ et le temps du gestionnaire. Ces deux valeurs recouvrent une partie de l'attente. L'invité ne dispose pas d'une horloge isolant le temps CPU de QEMU, le traitement du fichier image, APFS et le SSD. [Sonde multipage](../tests/fixtures/io-0166-dma-sonde.patch) · [Témoin de 4 Kio](../tests/fixtures/io-0166-dma-reference.patch)
- **Topologie**. Mac Apple M4 Max, 16 cœurs, 128 Gio, APFS sur Apple Fabric (bloc hôte annoncé de 4 Kio), macOS 27.2 (26B123), Rust 1.99.1 et qemu-img 11.1.3. Les invités ont un vCPU TCG et 256 Mio. QEMU direct 11.1.3 utilise RAW en writeback, VirtIO-IOMMU et GICv2 sur AArch64, Q35 et VT-d sur x86_64. L'application UTM 5.0.8 exécute QEMU 10.0.12, Cortex-A72, `virt-10.0`, GICv3 sans ITS, INTx, TPM CRB, firmware expérimental et VirtIO-IOMMU avec refus par défaut. Chaque essai de l’application réimporte sa propre image. Le journal effectif confirme `tcg,tb-size=64`, `discard=unmap,detect-zeroes=unmap`, un port série PTY et SPICE. Les matrices 512 et 4096 configurent la taille logique et physique annoncée. Le profil et les SHA-256 des noyaux et du firmware figurent dans les relevés.

## Résultats hors sonde

Une source dense contient 4 194 304 octets. Une petite édition publie environ 4 Mio. La création publie précisément 4 Mio. L’ajout de 4 Mio à la source dense publie environ 8 Mio. Le terme « fragmentation » du tableau désigne les extents P9 de la source. Les durées regroupent les résultats distincts des géométries 512 et 4096. Chaque borne correspond à une exécution. Les temps utilisateurs proviennent des invités sans sonde, et les compteurs IPC d’un passage distinct qui publie les mêmes octets.

| Opération | QEMU AArch64 avant → après (ms) | QEMU x86_64 avant → après (ms) | UTM QCOW2 avant → après (ms) | IPC avec contrôles |
|---|---|---|---|---|
| Créer 4 Mio | 163,2–186,7 → 110,1–114,4 | 194,3–205,3 → 171,9–193,0 | 609,4–700,1 → 449,1–589,6 | 36–37 |
| Insérer en tête | 281,7–283,9 → 128,0–144,6 | 286,6–291,9 → 179,4–185,5 | 1222,7–1277,0 → 695,1–774,6 | 41 |
| Insérer au milieu | 293,6–321,4 → 119,2–133,8 | 271,5–305,7 → 188,6–191,5 | 1086,2–1220,0 → 508,1–902,2 | 43 |
| Insérer en fin | 255,1–390,9 → 127,5–158,3 | 287,1–295,6 → 182,0–188,1 | 1474,4–1635,4 → 632,2–828,9 | 43 |
| Insérer en trois endroits | 256,2–270,9 → 135,9–142,2 | 263,0–322,6 → 186,0–198,3 | 840,6–1049,8 → 486,0–525,9 | 48 |
| Supprimer en tête | 267,7–349,8 → 135,7–165,2 | 275,6–278,6 → 187,8–202,5 | 627,3–2237,9 → 499,1–536,6 | 39–40 |
| Supprimer au milieu | 235,0–336,0 → 137,2–137,6 | 256,1–304,0 → 177,3–202,2 | 1107,0–1787,4 → 582,6–593,1 | 41 |
| Supprimer en fin | 262,0–333,5 → 142,6–156,9 | 275,7–278,5 → 192,7–201,5 | 1002,1–1790,3 → 542,0–563,4 | 41 |
| Supprimer en trois endroits | 260,5–330,4 → 148,3–155,1 | 324,5–325,3 → 238,4–243,2 | 1185,9–1267,8 → 548,9–645,3 | 46–47 |
| Remplacer en tête | 255,4–412,0 → 144,8–152,8 | 278,9–285,0 → 178,3–181,8 | 1607,4–2107,7 → 450,4–730,5 | 41 |
| Remplacer au milieu | 254,2–306,2 → 135,7–142,2 | 278,8–288,9 → 181,0–194,0 | 1098,4–1599,9 → 556,4–633,8 | 43 |
| Remplacer en fin | 260,2–297,8 → 146,1–148,4 | 279,1–292,0 → 199,9–200,9 | 1048,8–1240,7 → 628,2–689,7 | 43 |
| Remplacer en trois endroits | 247,1–288,0 → 137,0–144,4 | 262,6–266,7 → 184,9–188,7 | 1232,2–1483,0 → 674,3–727,4 | 48 |
| Ajouter 4 Mio en tête | 397,6–433,7 → 225,5–240,8 | 496,5–533,3 → 352,0–355,2 | 1452,1–2423,4 → 956,6–1107,2 | 52 |
| Ajouter 4 Mio au milieu | 414,3–415,0 → 249,5–260,8 | 447,0–451,1 → 335,6–352,3 | 1709,8–3306,7 → 1144,8–1205,9 | 54–55 |
| Ajouter 4 Mio en fin | 357,4–376,7 → 191,3–205,3 | 414,4–415,8 → 299,0–307,6 | 1361,6–1767,7 → 744,5–790,2 | 54–55 |
| Ajouter 4 Mio et éditer ailleurs | 344,3–347,1 → 188,9–191,8 | 381,9–385,5 → 265,5–274,5 | 1675,7–2859,3 → 1019,2–1163,6 | 56–57 |
| Source fragmentée | 377,1–441,0 → 257,8–268,9 | 406,6–408,2 → 346,6–346,7 | — | 41 |
| Navigation puis insertion | 239,7–265,9 → 150,5–159,2 | 282,5–287,6 → 178,8–184,3 | — | 41 |

Les deux refus pour saturation par architecture sont exclus des délais de succès. Les 145 blocs libres sont insuffisants. L’ancienne source et le tiers ont été relus intégralement. La destination est absente et les générations correspondent au témoin d’ouverture seule. Les éditions sur la source à 983 extents ont réussi dans les deux géométries sous QEMU.

QEMU a publié et vérifié 76 fichiers dans la matrice hors sonde, avec 76 sauvegardes sous la seconde. Les 40 répétitions hors sonde ont pris 101,1–284,2 ms. Sur UTM QCOW2, 29 des 34 sauvegardes de la matrice hors sonde passent le seuil. Le JSON associe chaque durée à sa géométrie, son profil et son noyau.

Les 20 répétitions QEMU aarch64 des cinq références ont pris 101,1–178,4 ms.

Les 20 répétitions QEMU x86_64 des cinq références ont pris 158,0–284,2 ms.

Dans la matrice UTM, les créations et éditions localisées de 4 Mio comptent **26/26** passages sous la seconde (449,1–902,2 ms). Les grandes insertions portant la destination à 8 Mio en comptent **3/8** (744,5–1205,9 ms).

## Requêtes, IRQ, copie et acquittement

Les mesures suivantes portent sur les secteurs logiques de 4096 octets. Les écritures finales de métadonnées et les deux commandes FLUSH sont incluses. Le total des notifications physiques et celui des IRQ acceptées sont égaux pour chaque passage instrumenté. Les temps du pilote de 4 Kio sont issus des témoins reconstruits avec les mêmes points de mesure.

| Invité | Opération | IN/OUT/FLUSH | Kicks = IRQ | Copie DMA IN + OUT (ms) | Soumission (ms) | Doorbell → used observé (ms) | used → acquit (ms) | CPU IRQ (ms) |
|---|---|---|---|---|---|---|---|---|
| aarch64 | Créer 4 Mio | 19/1047/2 → 19/22/2 | 1068 → 43 | 4,3 → 3,3 | 2,6 → 0,4 | 81,6 → 5,6 | 0,33 → 0,04 | 0,78 → 0,08 |
| aarch64 | Insérer au milieu | 1038/1048/2 → 142/25/2 | 2088 → 169 | 8,2 → 6,5 | 5,0 → 0,8 | 157,6 → 15,8 | 0,58 → 0,08 | 1,58 → 0,18 |
| aarch64 | Ajouter 4 Mio et éditer ailleurs | 1039/2088/2 → 143/45/2 | 3129 → 190 | 12,4 → 6,1 | 6,3 → 0,9 | 148,3 → 24,0 | 0,83 → 0,09 | 2,03 → 0,22 |
| x86_64 | Créer 4 Mio | 19/1047/2 → 19/22/2 | 1068 → 43 | 2,3 → 4,1 | 2,6 → 1,3 | 31,5 → 11,2 | 0,71 → 0,10 | 0,50 → 0,14 |
| x86_64 | Insérer au milieu | 1038/1048/2 → 142/25/2 | 2088 → 169 | 4,3 → 8,0 | 5,5 → 0,7 | 71,4 → 7,7 | 1,76 → 0,23 | 1,49 → 0,17 |
| x86_64 | Ajouter 4 Mio et éditer ailleurs | 1039/2088/2 → 143/45/2 | 3129 → 190 | 6,4 → 9,7 | 6,0 → 0,8 | 84,5 → 14,6 | 2,38 → 0,22 | 1,78 → 0,21 |
| UTM | Créer 4 Mio | 19/1047/2 → 19/22/2 | 1068 → 43 | 20,4 → 11,4 | 10,2 → 1,3 | 161,5 → 17,8 | 1,02 → 0,12 | 3,03 → 0,27 |
| UTM | Insérer au milieu | 1038/1048/2 → 142/25/2 | 2088 → 169 | 24,6 → 21,5 | 13,8 → 3,1 | 245,7 → 35,2 | 1,40 → 0,26 | 4,41 → 0,69 |
| UTM | Ajouter 4 Mio et éditer ailleurs | 1039/2088/2 → 143/45/2 | 3129 → 190 | 58,0 → 25,1 | 26,0 → 3,1 | 405,1 → 60,4 | 3,22 → 0,30 | 8,41 → 0,83 |

### Publication dans UTM (ms)

| Opération | Préparation et données | Lot métadonnées + F1 | Lot ancre + F2 | Du début de publication à F2 | Deux FLUSH physiques cumulés |
|---|---|---|---|---|---|
| Créer 4 Mio | 282,5–325,5 | 8,3–9,6 | 2,3–2,8 | 294,9–337,2 | 3,1–4,3 |
| Insérer au milieu | 520,8–631,0 | 7,8–8,5 | 1,2 | 530,6–640,2 | 3,0–3,3 |
| Supprimer en trois endroits | 551,8–583,8 | 10,5–11,0 | 1,3–1,6 | 564,7–595,8 | 4,2–4,3 |
| Remplacer en trois endroits | 422,2–516,2 | 6,9–8,1 | 1,4–1,8 | 432,5–524,7 | 2,1–3,1 |
| Ajouter 4 Mio et éditer ailleurs | 638,1–893,3 | 6,9–9,0 | 1,1–1,2 | 648,6–901,7 | 3,2–4,0 |

Deux plages de LBA séparées exigent deux requêtes distinctes. En dense, les 128 extents de la source séparent une partie des lectures de données. Le regroupement des écritures suit exactement leur ordre dans la préparation CoW. La somme des durées de requêtes physiques inclut les FLUSH. La somme des durées de réservation du moteur les recouvre également.

## Ressources et seuil IPC

| Environnement | Ancien pic session (Kio) | Pic session multipage (Kio) | Arène observée (Kio) | IPC avec contrôles | En incluant aussi les copies locales du prêt |
|---|---|---|---|---|---|
| AArch64 | 4452–38692 | 4772–39012 | 260–320 | 36–56 | 39–64 |
| x86_64 | 4492–38732 | 4812–39052 | 260–320 | 36–57 | 39–65 |
| UTM AArch64 | 4376–38616 | 4696–38936 | 260–320 | 36–56 | 39–64 |

Les pics de session viennent du compteur de cadres du noyau. Les allocations de vi, du moteur et des autres processus y contribuent. Le delta du pic ne mesure donc pas uniquement l’arène. La borne indépendante de la taille du fichier est 512 Kio de données DMA, avec deux vecteurs de métadonnées limités à 64 descripteurs de cadres chacun. Les copies locales des pages scellées n’envoient pas de message à un autre service. Dans la cellule QEMU x86_64/512, la grande édition multiple a aussi compté trois fermetures locales de capacités. Son total conservateur est de 57, ou 65 si on ajoute les huit copies mémoire du prêt. Les deux comptes figurent dans les relevés. Le même vérificateur reproduit les comptes de 47 anciennes traces UTM groupées et des quatre témoins du transport historique. Les quatre anciennes traces d’une sonde préliminaire sans compteurs de lots restent rejetées, comme avant cette modification.

Les échantillons utilisateurs QEMU comptent les pages privées et la pile. Les cadres DMA résident au niveau superviseur. Les colonnes sont des maxima observés par PID, à des instants potentiellement différents.

| Architecture (secteurs de 4096 octets) | Édition | Pilote utilisateur (Kio) | Moteur P9 (Kio) | nsh (Kio) | vi (Kio) | Arène privée (Kio) |
|---|---|---|---|---|---|---|
| aarch64 | Créer 4 Mio | 36 | 1748 | 460 | 12872 | 260 |
| x86_64 | Créer 4 Mio | 36 | 1772 | 476 | 12872 | 260 |
| aarch64 | Insérer au milieu | 36 | 3796 | 460 | 6600 | 320 |
| x86_64 | Insérer au milieu | 36 | 3820 | 476 | 6600 | 320 |
| aarch64 | Ajouter 4 Mio et éditer ailleurs | 36 | 3796 | 460 | 33608 | 320 |
| x86_64 | Ajouter 4 Mio et éditer ailleurs | 36 | 3820 | 476 | 33608 | 320 |

## Stockage hôte et répétitions

| Édition | Secteur | Image | Préallocation | Traces | Octets hôte initiaux | Commande → succès (ms) | OUI → succès (ms) |
|---|---|---|---|---|---|---|---|
| Ajouter 4 Mio et éditer ailleurs | 4096 | UTM QCOW2 | par défaut | bloc désactivées | 7602176 | 955,3 | 904,7 |
| Ajouter 4 Mio et éditer ailleurs | 4096 | UTM RAW | par défaut | bloc désactivées | 22085632 | 1004,1 | 922,6 |
| Remplacer en trois endroits | 4096 | UTM QCOW2 | par défaut | bloc désactivées | 7602176 | 556,5 | 536,2 |
| Remplacer en trois endroits | 4096 | UTM RAW | par défaut | bloc désactivées | 22085632 | 681,8 | 651,9 |
| Ajouter 4 Mio et éditer ailleurs | 512 | UTM QCOW2 | par défaut | bloc désactivées | 7536640 | 926,2 | 853,5 |
| Ajouter 4 Mio et éditer ailleurs | 512 | UTM RAW | par défaut | bloc désactivées | 22364160 | 894,4 | 838,4 |
| Remplacer en trois endroits | 512 | UTM QCOW2 | par défaut | bloc désactivées | 7536640 | 682,1 | 650,0 |
| Remplacer en trois endroits | 512 | UTM RAW | par défaut | bloc désactivées | 22364160 | 721,1 | 689,3 |
| Ajouter 4 Mio et éditer ailleurs | 4096 | UTM QCOW2 | complète | bloc désactivées | 296026112 | 1092,9 | 1009,4 |
| Supprimer en trois endroits | 4096 | UTM QCOW2 | complète | bloc désactivées | 296026112 | 811,3 | 788,0 |
| Ajouter 4 Mio et éditer ailleurs | 512 | UTM QCOW2 | complète | bloc désactivées | 82116608 | 902,0 | 832,7 |
| Supprimer en trois endroits | 512 | UTM QCOW2 | complète | bloc désactivées | 82116608 | 757,3 | 720,4 |
| Ajouter 4 Mio et éditer ailleurs | 4096 | UTM QCOW2 | par défaut | bloc désactivées | 7602176 | 1212,4 | 1164,9 |
| Supprimer en trois endroits | 4096 | UTM QCOW2 | par défaut | bloc désactivées | 7602176 | 847,2 | 816,7 |
| Ajouter 4 Mio et éditer ailleurs | 512 | UTM QCOW2 | par défaut | bloc désactivées | 7536640 | 974,9 | 878,3 |
| Supprimer en trois endroits | 512 | UTM QCOW2 | par défaut | bloc désactivées | 7536640 | 811,5 | 761,4 |

Pour la grande édition multiple en secteurs de 512 octets, l’application mesure 894,4 ms en RAW et 926,2 ms en QCOW2. En secteurs de 4096 octets, elle mesure respectivement 1004,1 et 955,3 ms. Dans la comparaison dédiée à la préallocation QCOW2, le temps passe de 974,9 à 902,0 ms en 512 et de 1212,4 à 1092,9 ms en 4096. Le classement des formats varie avec la géométrie. La préallocation laisse subsister un dépassement en 4096.

Les répétitions ci-dessous utilisent le verrou des VM UTM du banc. Pour une édition localisée, deux processus vi relisent la même source et publient sous deux noms libres dans la même VM. Une relecture indépendante après redémarrage valide les deux destinations. Les grandes insertions et créations repartent d’un disque neuf.

| Scénario | Secteur | Commande → succès (ms) | OUI → succès (ms) |
|---|---|---|---|
| Ajouter 4 Mio et éditer ailleurs | 4096 | 759,7 | 692,4 |
| Insérer au milieu | 4096 | 526,8 / 334,3 | 493,4 / 312,1 |
| Créer 4 Mio | 4096 | 470,8 | 372,0 |
| Remplacer en trois endroits | 4096 | 810,1 / 761,3 | 773,5 / 728,7 |
| Ajouter 4 Mio et éditer ailleurs | 512 | 887,9 | 822,9 |
| Insérer au milieu | 512 | 803,4 / 730,9 | 775,9 / 701,8 |
| Créer 4 Mio | 512 | 409,6 | 316,7 |
| Remplacer en trois endroits | 512 | 715,6 / 561,4 | 659,3 / 534,9 |

Le contrôle du format conserve le profil invité et retire les mêmes traces bloc dans les deux variantes. Les essais de préallocation utilisent aussi ce profil sans traces bloc. Les témoins correspondent à des exécutions distinctes, avec une seule VM UTM active, et sans éviction forcée du cache hôte. Les deux traces bloc sont retirées uniquement dans les essais identifiés, toutes les traces IOMMU sont conservées. Le QCOW2 clairsemé utilise des clusters de 64 Kio. Une durée hôte exclusive pour APFS ou pour le SSD physique demanderait une trace hôte corrélée à chaque requête.

La première série a mesuré 1019,2–1163,6 ms pour la grande édition multiple. La répétition sur disques neufs dans la même application mesure 759,7–887,9 ms. Le gain observé dans la répétition ne garantit pas le seuil sur tous les démarrages.

### QEMU 10.0.12 livré avec UTM, exécuté directement

Le témoin démarre les bibliothèques exactes de l’application avec le même ELF système, le même firmware, le même TPM CRB, le même disque GPT/P9, un Cortex-A72 TCG et les mêmes traces de périphériques. Il emploie un socket série local et un moniteur QMP, sans SPICE. Chaque cas redémarre dans un nouveau processus QEMU puis compare la destination entière. Un changement de mode de lancement peut modifier la planification macOS, le transport de console et la journalisation. [Banc du moteur UTM](../tests/es_groupees_moteur_utm.py)

| Édition | Secteur | Image | Moteur commande → succès (ms) | Moteur OUI → succès (ms) | Moteur publication → F2, sonde (ms) | Moteur kick → used cumulé (ms) | Application QCOW2, commande → succès (ms) |
|---|---|---|---|---|---|---|---|
| Remplacer en trois endroits | 512 | raw | 108,6 | 101,9 | 105,0 | 12,1 | — |
| Remplacer en trois endroits | 512 | qcow2 | 112,6 | 105,1 | 114,7 | 10,5 | 674,3 |
| Remplacer en trois endroits | 4096 | raw | 110,4 | 103,4 | 105,8 | 10,8 | — |
| Remplacer en trois endroits | 4096 | qcow2 | 115,0 | 107,7 | 107,6 | 8,7 | 727,4 |
| Ajouter 4 Mio et éditer ailleurs | 512 | raw | 162,2 | 148,6 | 153,8 | 16,2 | — |
| Ajouter 4 Mio et éditer ailleurs | 512 | qcow2 | 159,5 | 146,7 | 146,2 | 11,0 | 1163,6 |
| Ajouter 4 Mio et éditer ailleurs | 4096 | raw | 155,6 | 143,0 | 153,0 | 16,5 | — |
| Ajouter 4 Mio et éditer ailleurs | 4096 | qcow2 | 154,7 | 142,0 | 146,0 | 11,4 | 1019,2 |

| QCOW2 512/4096 | Édition | SHA-256 source (ms) | SHA-256 candidat et extents (ms) | Validation du texte (ms) | Kick → used cumulé (ms) | Publication → F2 (ms) |
|---|---|---|---|---|---|---|
| Application | Remplacer en trois endroits | 83,8–87,9 | 123,9–154,1 | 30,7–35,3 | 35,3–57,9 | 432,5–524,7 |
| Moteur direct | Remplacer en trois endroits | 20,2–21,4 | 30,8–32,0 | 7,2–7,3 | 8,7–10,5 | 107,6–114,7 |
| Application | Ajouter 4 Mio et éditer ailleurs | 80,6–82,9 | 258,8–282,3 | 47,8–55,8 | 59,9–60,4 | 648,6–901,7 |
| Moteur direct | Ajouter 4 Mio et éditer ailleurs | 19,3–19,6 | 59,1–59,4 | 10,2–10,3 | 11,0–11,4 | 146,0–146,2 |

Les différences de `query-blockstats` suivent le disque `drivecible` entre l’édition terminée, avant `:w`, et le retour du message de succès. Ces mesures du nœud QEMU comprennent ses opérations de fichier et son ordonnancement. Les compteurs de l’anneau prennent leurs bornes dans le noyau invité. Les durées se recouvrent en partie.

| Édition | Secteur | Fichier | Opérations QMP IN/OUT/FLUSH | IN nœud (ms) | OUT nœud (ms) | FLUSH nœud (ms) | Kick → used invité (ms) |
|---|---|---|---|---|---|---|---|
| Ajouter 4 Mio et éditer ailleurs | 4096 | qcow2 | 143/45/2 | 2,2 | 2,7 | 1,2 | 11,3 |
| Ajouter 4 Mio et éditer ailleurs | 4096 | raw | 143/45/2 | 2,5 | 7,1 | 4,9 | 115,8 |
| Remplacer en trois endroits | 4096 | qcow2 | 143/25/2 | 2,3 | 1,5 | 1,2 | 9,3 |
| Remplacer en trois endroits | 4096 | raw | 143/25/2 | 2,2 | 4,2 | 0,7 | 11,1 |
| Ajouter 4 Mio et éditer ailleurs | 512 | qcow2 | 143/45/2 | 2,3 | 2,7 | 1,2 | 11,2 |
| Ajouter 4 Mio et éditer ailleurs | 512 | raw | 143/45/2 | 1,9 | 6,7 | 1,1 | 13,7 |
| Remplacer en trois endroits | 512 | qcow2 | 143/25/2 | 2,2 | 1,4 | 1,4 | 9,2 |
| Remplacer en trois endroits | 512 | raw | 143/25/2 | 2,2 | 4,1 | 0,6 | 11,1 |

Pour la grande édition multiple, les fenêtres SHA-256 source et candidat/extents totalisent 339,4–365,2 ms dans l’application UTM et 79,2–80,5 ms dans le moteur en lancement direct. Dans l’application, les notifications jusqu’à l’observation de used totalisent 59,9–60,4 ms, et les temps complets des deux FLUSH totalisent 3,2–4,0 ms. Les chronos des phases comprennent les interruptions et l’ordonnancement de l’invité. Ils situent la lenteur dans le profil d’exécution sans attribuer toute la différence au dispositif de stockage.

### Essai séparé de mise en veille du vCPU

| Environnement | Architecture | Secteur | Pilote multipage (ms) | Avec WFI/HLT (ms) |
|---|---|---|---|---|
| QEMU RAW | aarch64 | 4096 | 188,9 | 166,6 |
| QEMU RAW | x86_64 | 4096 | 265,5 | 259,8 |
| UTM QCOW2 | aarch64 | 512 | 1163,6 | 1596,6 |
| UTM QCOW2 | aarch64 | 4096 | 1019,2 | 911,2 |

Le témoin WFI/HLT conserve les contrôles de l’identifiant de tête, IRQ, délai borné, statut et FLUSH. Une seule grande édition multiple par configuration a été exercée. La variation entre les deux secteurs UTM et le dépassement en 512 excluent une adoption sur cette seule mesure. Les suites de coupure et de révocation portent sur le pilote multipage de référence. [Variante expérimentale de l’attente](../tests/fixtures/io-0166-dma-attente-experimentale.patch)

## Intégrité, coupures et régressions

- Les quatre cellules QEMU (deux architectures × deux secteurs) ont vérifié neuf frontières chacune. Elles couvrent les données, avant/pendant/après F1 et F2, le retrait, et une **écriture effectivement suspendue** au point QEMU `write_aio`. Le moniteur a confirmé `wait_break` avant l’arrêt du processus QEMU. Chaque image a été vérifiée avant et après remontage. Pendant F2 le nouvel état complet peut déjà être visible. L’appelant n’avait encore reçu aucun acquittement. [Banc de coupure](../tests/coupure_cow_groupee.py)

- Les 20 passages supplémentaires couvrent le retrait pendant une écriture bloquée chez le backend (reprise de la même commande, aucun kick suivant), les refus de rang/LBA/direction, le réemploi d’un cadre OUT réalloué à zéro après démontage, un désaccord simulé de longueur après un vrai DMA, un témoin de succès et une erreur EIO physique au secteur précédemment identifié. La relecture indépendante a retrouvé la source exacte et refusé toute publication incomplète. Les prêts concurrents, mutations concurrentes, suppressions et remplacements du nom ont exercé leurs bancs respectifs. [Injections réservées au laboratoire](../tests/fixtures/io-0166-dma-defauts.patch) · [Banc de concurrence](../tests/es_groupees_conflits.py)

- Sur une file limitée à huit entrées, 4 cellules QEMU ont observé un maximum de 6 pages par chaîne et des destinations identiques à l'oracle. Deux emplacements portent l'en-tête et le statut.

- L’application UTM a exécuté 6 arrêts forcés sur QCOW2. Après les données ou F1, la destination est absente. Après F2, elle est intégralement présente. Une exportation immédiatement après l'arrêt puis un deuxième contrôle après redémarrage ont vérifié les deux générations.

| Secteur | Point de coupure UTM | Générations à l’export immédiat | Après redémarrage et relecture | Destination | Acquittement reçu avant coupure |
|---|---|---|---|---|---|
| 512 | pages | 66 / 67 | 66 / 67 | absente | aucun |
| 512 | f1 | 66 / 67 | 66 / 67 | absente | aucun |
| 512 | f2 | 67 / 68 | 68 / 69 | intégrale | aucun |
| 4096 | pages | 66 / 67 | 66 / 67 | absente | aucun |
| 4096 | f1 | 66 / 67 | 66 / 67 | absente | aucun |
| 4096 | f2 | 67 / 68 | 68 / 69 | intégrale | aucun |

- Les suites NovaOS couvrent 25 démarrages, 24 groupes bloc, ELF, cycle de vie, IPC, quatre essais CLI, fichiers, espaces de noms, nsh, scripts, terminal, vi, écran, volumes, mémoire, progression de montage et performances. Le banc « 64 Mio » de la suite RAM ne dépasse pas le firmware dans les deux architectures. Les cellules 128 et 256 Mio sont reportées individuellement. Les 11 profils GICv3 et 23 profils DMA séquentiels ont tourné sur le moteur UTM avec les ROM expérimentales vérifiées.

- Sur Darwin ARM64 natif et x86_64 Rosetta, NovaFS a passé 196 tests standards et 198 instrumentés par architecture, 2 304 injections historiques d’ancres et 1 000 000 d’opérations différentielles. Les six tests ignorés par le lanceur standard gardent leur classement. Le banc CoW a exercé 486 injections supplémentaires par architecture. Les échanges P9 ARM64/x86_64, snapshots, clones, migration, chiffrement et compression sont consignés dans les preuves.

## Verdict par critère

| Périmètre | Sous 1 s, commande → succès | Temps (ms) |
|---|---|---|
| QEMU AArch64, matrice hors sonde | 38/38 | 110,1–268,9 |
| QEMU x86_64, matrice hors sonde | 38/38 | 171,9–355,2 |
| Application UTM, création et édition localisée à 4 Mio | 26/26 | 449,1–902,2 |
| Application UTM, insertion de 4 Mio dans une source de 4 Mio | 3/8 | 744,5–1205,9 |

Le seuil de 64 échanges avec contrôles est atteint dans tous les scénarios de référence instrumentés. Les validations d’intégrité et de retrait couvrent les pannes répertoriées. La cible temporelle étendue aux grandes insertions reste non qualifiée dans l’application UTM.

## Décision proposée

Conserver le prototype sur sa branche. Le regroupement réduit les requêtes physiques et la pression sur la file sans retirer les deux barrières. L’admission dans le jalon livré exige encore un temps de sauvegarde reproductible sous la seconde dans le profil UTM retenu. La priorité suivante est d’identifier ce qui ralentit l’émulation lancée par l’application, puis de mesurer le gain possible sur les deux calculs SHA-256 actuels du moteur. Les témoins utilisent le même code de hachage et la même politique de publication. Une accélération cryptographique ou une modification du profil de stockage nécessitent leurs propres mesures et arbitrage.

## Reproduction

Dans une copie obtenue par `git archive` de la branche de prototype, appliquer les fixtures avec `patch -p1 -F0`. La construction normale du dépôt ne charge aucune de ces fixtures. Le noyau invité conserve son numéro de version livré v0.16.5. Le générateur est un exemple Cargo explicitement activé.

| Archive de travail | Ordre des fixtures, après `io-0166-prototype` et `io-0166-plages` |
|---|---|
| Temps QEMU | `io-0166-charge-4m`, `io-0166-virtio-multipage` |
| Mesure QEMU | `io-0166-prototype-sonde`, `io-0166-plages-sonde`, `io-0166-charge-4m`, `io-0166-virtio-multipage`, `io-0166-dma-sonde` |
| Témoin 4 Kio | Les deux sondes historiques, la charge, puis `io-0166-dma-reference` |
| Concurrence | `vi-conflit-source`, `io-0166-course-nom`, `io-0166-charge-4m`, `io-0166-virtio-multipage` |
| Coupures et retrait | `io-0166-plages-coupure`, `io-0166-barrieres`, charge, pilote multipage, puis `io-0166-dma-coupure` |
| Défauts et réemploi | Archive de coupures puis `io-0166-dma-defauts` |
| Régressions | `io-0166-virtio-multipage`, sans fixture de charge de clavier |

Pour UTM, ajouter `io-0166-systeme-qualification` avant la charge et construire avec `--features systeme-gpt`. Le profil système privé réutilise le même transport après le passage du SID 56 du firmware au noyau. L'ELF d’un profil et son disque doivent provenir de la même campagne. Chaque sortie UTM doit désigner un nouveau répertoire. Les commandes suivantes s’exécutent depuis la racine de l’archive appropriée. Les variables `PROFIL_UTM`, `GENERATEUR` et `SORTIE` désignent respectivement le paquet système de test, le générateur et le dossier de résultats.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --features prototype-es-groupees --example es-groupees
GENERATEUR="$PWD/novaFS/target/aarch64-apple-darwin/release/examples/es-groupees"
make -C novaOS ARCH=aarch64 kernel
python3 -B novaOS/tests/es_groupees_elargies.py aarch64 512 inserer-milieu "$GENERATEUR" "$SORTIE/temps.json" --sans-sonde
python3 -B novaOS/tests/coupure_cow_groupee.py aarch64 512 "$SORTIE/coupures.json" --barrieres --multipage --dma-en-vol --taille 4194304
python3 -B novaOS/tests/coupure_cow_groupee.py aarch64 512 "$SORTIE/defauts.json" --defauts-dma --multipage --taille 4194304
python3 -B novaOS/tests/es_groupees_conflits.py aarch64 512 "$SORTIE/concurrence.json"
```

Répéter pour x86_64 et 4096, puis lancer les suites NovaOS et NovaFS des trois AGENTS.md. Pour la comparaison instrumentée, omettre `--sans-sonde` dans l'archive correspondante. La matrice UTM utilise le même pilote et l'oracle P9 hôte.

```sh
(cd novaOS && RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --offline --locked --release --target aarch64-unknown-none-softfloat --features systeme-gpt)
ELF="$PWD/novaOS/target/aarch64-unknown-none-softfloat/release/novaos"
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF" "$GENERATEUR" "$SORTIE/utm-512" 512 --cas ajout-4m-multiple --sans-sonde
```

Pour la file minimale, ajouter `--entrees-file 8` à l’essai instrumenté QEMU. Pour deux publications localisées dans la même VM UTM, ajouter `--repetition 2 --sans-sonde` avec `--cas remplacer-multiple`. Le témoin suivant exécute directement les bibliothèques du même UTM depuis cette archive.

```sh
python3 -B novaOS/tests/es_groupees_moteur_utm.py "$PROFIL_UTM" "$ELF" "$GENERATEUR" "$SORTIE/moteur-4096" 4096 --format qcow2 --cas ajout-4m-multiple
```

La géométrie et le format peuvent être changés séparément. Avec l’ELF instrumenté, ajouter `--sonde` au témoin du moteur. La fixture `io-0166-dma-attente-experimentale` se superpose uniquement à un invité multipage hors sonde destiné à l’essai WFI/HLT.

Les options `--format raw`, `--sans-traces-bloc` et `--preallocation` isolent respectivement le format brut, le coût des deux traces bloc et le provisionnement du QCOW2. Les traces IOMMU et les barrières du pilote restent actives. [Banc de l'application UTM](../tests/es_groupees_utm.py)

La qualification suivante suit le coût des empreintes, des extents, des copies et des allocations jusqu’à F2, avec les compteurs du disque dans l’application UTM. [Optimisation CPU et mesures individuelles](es-v0.16.6-cpu.md)
