# NovaOS v0.16.6 — Correctifs avant intégration des E/S groupées

**Avis du 14 novembre 2026 : fusion possible après décision explicite MOA/MOE, avec les E/S groupées désactivées par défaut.** Les deux défauts qui motivaient l’avis de report ont été corrigés sur le candidat qualifié. Les 88 sauvegardes des campagnes de compatibilité ont supporté un second démarrage et une vérification exacte. Les essais DMA n’ont laissé aucun cadre de session sans propriétaire. Les 86 commandes de régression et les 94 benchmarks ont abouti. L’activation en production, la promesse d’une seconde sous toute charge et l’ouverture à un environnement SMP restent hors de cet avis.

## Résultats de la qualification

| Environnement | Candidat hors sonde | p50 (ms) | p95 (ms) | p99 (ms) | Maximum (ms) | ≥ 1 s | Pic de session témoin → candidat (cadres de 4 Kio) |
|---|---:|---:|---:|---:|---:|---:|---|
| qemu-aarch64 | 18 | 94,546 | 135,668 | 135,668 | 135,668 | 0 | 9754 → 9755 |
| qemu-x86_64 | 18 | 141,555 | 208,361 | 208,361 | 208,361 | 0 | 9765 → 9765 |
| utm-aarch64 | 18 | 130,850 | 671,918 | 671,918 | 671,918 | 0 | 9735 → 9736 |

Ces percentiles regroupent six cellules par plateforme. Les tableaux par scénario donnent les trois valeurs individuelles. Les pics de session mesurent l’écart entre les cadres libres au repos et leur minimum sur la session entière du système invité. Les relevés par PID de QEMU échantillonnent la pile et les données privées aux passages du banc. Le relevé ordinaire UTM porte sur les cadres de la session. Les quatre séries UTM conservées par Instruments fournissent en plus les observations par PID. Leur maximum sur vi est de 33 608 Kio en édition multiple. Le pic global des sessions ordinaires a augmenté d’un cadre (4 Kio) sur AArch64 direct et UTM et a conservé sa valeur sur x86_64.

Le maximum observé sur les 20 sauvegardes avec compteurs est de 47 échanges interservices et dons acquittés. Avec les appels de contrôle, il atteint **57**. En comptant aussi les copies locales du prêt, le maximum est de **65**. Chaque sauvegarde instrumentée a exécuté deux FLUSH. Les 34 commandes mesurées par HMP dans UTM ont aussi exécuté deux FLUSH et leur disque a été relu après redémarrage. Les sondes distinguent les réceptions et dépôts locaux, les appels de file, les transactions IPC et les soumissions physiques. Le compteur de découverte dans la fenêtre de sauvegarde vaut 0. [Mesures individuelles, compteurs et phases](validation/es-v0.16.6-correctifs-mesures.json)

Les mesures de référence relancées, les exécutions avec sonde invitée et les quatre traces Instruments sont conservées séparément des 54 mesures ordinaires du candidat. Les 94 commandes se sont terminées en moins d’une seconde.

Les QCOW2 de cette série étaient clairsemés, avec des clusters de 64 Kio et sans charge artificielle ajoutée. La politique processus était ordinaire. Dans les 33 fenêtres UTM disposant de ticks CPU hôte, l’activité globale du Mac allait de 12,295 % à 51,923 %. Dans la fenêtre restante, le compteur de ticks n’a pas avancé. L’audit antérieur a conservé neuf dépassements sur trente mesures de polling, jusqu’à **1 951,191 ms** sous UTM. Une borne absolue à une seconde reste ouverte sur cet environnement. [Conditions et extrêmes historiques](es-v0.16.6-irq.md) · [Influence de la politique hôte](es-v0.16.6-qos.md)

## Périmètre vérifié

Campagne du 14 novembre 2026. La référence de l’audit est `fac0342ea47f4ded2c522eae7bd7f4ea3ec10a08`. Le candidat de code est `13df1dce48677f6fd92256b99b0ab0948bde803a`. Le lecteur, les services et les contrats publiés proviennent de `3193bc9108bdc771f4627f57c84e093375cc9ea4` (NovaOS v0.16.5, NovaFS v0.9.13 P9). La branche de qualification assemble les fonctions groupées dans des exports privés à partir de fixtures. L’export brut implémente IPC_FEATURES et la découverte PAGE. Le plan groupé et les lots bloc nécessitent les fixtures. L’annonce PLAN_V1 exige en outre le profil 42. La qualification de l’IRQ utilise un profil distinct. [Audit initial](es-v0.16.6-integration.md) · [Reconstruction des profils](../tests/reconstruire_es_0166.py)

Entre l’audit initial et ce candidat, Git recense 39 fichiers modifiés, dont les bancs et les fixtures. Les ajouts d’ABI portent sur les trames de découverte du bloc et de la boîte IPC, leur sélecteur et la validation du côté noyau. Les autres changements fonctionnels touchent le courtier de vi, les services de stockage et leur relais, le bail VirtIO-blk, les deux implémentations IOMMU et les portails de transfert. Les corrections de crédit et de SG sont appliquées dans les profils qui activent ces voies. [ABI native](../abi/native.rs) · [Format des trames bloc](../abi/block.rs) · [Découverte et droits de la boîte](../abi/bulk_call.rs) · [Propriété du transport](../kernel/src/hardware/block/ownership.rs)

## Coexistence des composants

Trois hypothèses de version étaient inscrites dans le chemin d’exécution. Le courtier étendait l’ancienne réponse INFO de vi de dix à onze octets. Il attribuait également à l’ancien service P9 la capacité de publier un plan en reconnaissant la longueur de LOOKUP_AT. Le client bloc essayait la voie PAGE dès qu’il disposait d’un canal local. Ces hypothèses expliquaient les refus d’ouverture, de sauvegarde et de montage qui touchaient 14 des 32 compositions initiales.

La découverte s’effectue sur le canal et la session effectivement utilisés. Le noyau contrôle le handle, le processus, la génération et la direction. Le relais et le pilote doivent chacun reconnaître la découverte bloc. Le service de fichiers annonce le plan selon les droits de la session, le volume monté et le transport négocié. vi interroge son courtier avant d’entamer un transfert. Les opérations historiques conservent leur représentation. Seul le refus attendu de la découverte déclenche le repli. Une erreur de transport ou une réponse ambiguë après publication interdit une deuxième tentative automatique. [Trames, validations, replis et matrices de démarrage](es-v0.16.6-negociation.md)

Les 32 croisements historiques ont tous publié les octets attendus sur le candidat, contre 18 avant correction. Par architecture, un refus à l’ouverture, quatre refus de sauvegarde et deux refus de montage ont disparu. L’extension porte le total à **68 sauvegardes et 68 seconds démarrages avec relecture**, dont dix essais dans la géométrie inverse et deux témoins de la branche brute. Le second démarrage réutilise le disque du premier. La matrice mixte exerce une source de 18 octets et une destination de 19 octets. Le complément ci-dessous couvre des fichiers de plusieurs pages sur des piles mixtes. Les transferts de plusieurs Mio sont chronométrés sur les piles de référence et du prototype. Le lecteur indépendant vérifie les octets de la source et de la destination. Les noyaux et composants publiés sont identifiés par leur empreinte de l’audit initial.

Le complément multipage compte **20 sauvegardes et 20 seconds démarrages**, sur cinq compositions dans chaque architecture. L’édition insère 80 octets UTF-8 à l’offset 4 096 dans des sources de 12 et 24 Kio de part et d’autre du seuil paginé de vi. Les destinations font 12 368 et 24 656 octets. Les 20 sorties complètes de `cat`, la source et la destination ont été vérifiées. Un premier contrôle du cas AArch64 avec le nouveau vi sur le système publié traitait une ligne de télémétrie du noyau sur l’UART comme des données de `cat`. Après filtrage de cette seule forme et réexécution avec le même ELF, le contrôle a réussi. Le lecteur P9 vérifie indépendamment les octets de chaque fichier. Le total des deux campagnes est de **88 sauvegardes suivies de 88 remontages**. [Compositions multipages, SHA et incident de lecture série](validation/es-v0.16.6-negociation-pages.json)

La découverte bloc intervient une seule fois, sur une session vide, avant la première commande de transfert. Sur cette requête précise, le refus historique BAD_ARGUMENT ou UNKNOWN_CALL indique que le chemin sectoriel doit être employé. Le même BAD_ARGUMENT reçu en pleine opération serait ambigu. DENIED, REVOKED, CLOSED et les erreurs de transport se propagent à l’appelant. Une révocation n’autorise aucun repli vers une opération moins restrictive.

## Révocation et propriété des cadres

L’ancienne ouverture pouvait perdre l’inventaire de 16 cadres AArch64 ou de 7 cadres x86_64 après allocation. En fermeture, consommer le transport avant l’acquittement du retrait pouvait laisser un domaine matériel actif sans objet logiciel capable d’en reprendre le nettoyage. Le bail Rust et l’objet de retrait conservent désormais le disque, le domaine, la file en vol, les cadres et l’étape acquittée. Le retour à l’allocateur exige la confirmation du retrait. Une panne durable peut donc immobiliser des cadres précisément inventoriés. Les neuf cadres des tables VT-d restent attachés à la machine pour empêcher un intervalle sans traduction entre deux sessions. [Machine à états et propriété](es-v0.16.6-correctifs-dma.md)

La qualification a révélé une seconde frontière de révocation. Un résultat lu par DMA dans une page privée doit encore être livré au processus. Une première correction contrôlait le bail à la copie mais pouvait créditer la réservation du lot mono-page avant un refus de livraison. Sur le BSP qualifié, la validation des deux extrémités, celle du bail matériel, la copie vers le destinataire et le crédit de réservation doivent former une seule opération indivisible vis-à-vis de la révocation. L’acquittement du retrait interdit toute nouvelle admission. Une demande de retrait alors que le matériel travaille peut encore répondre WOULD_BLOCK.

Le banc DMA a démarré 24 invités QEMU. Il totalise 928 injections et 512 cycles avec décès du titulaire, en polling et IRQ, sur les deux architectures et les deux géométries. Après retrait acquitté, le nombre de cadres privés de session est revenu à zéro dans chaque cellule. Les contrôles de livraison ont exercé le secteur, la page, le lot mono-page et le scatter-gather. Pour le lot et le scatter-gather, les 48 livraisons refusées après DMA ont conservé leurs octets sentinelles, laissé le crédit à zéro et refusé une deuxième soumission. En incluant les deux portails historiques, les quatre voies totalisent 32 livraisons positives et 64 refus attendus. Les 24 images de sortie sont identiques à leur témoin. [Relevé des injections et des cadres](validation/es-v0.16.6-correctifs-dma.json)

Les 256 cycles d’endurance ont monté, modifié puis fermé le même volume 64 fois par couple architecture/secteur. Chaque tentative sur le nom occupé a été refusée. Chaque copie autorisée a été relue intégralement. Après chaque fermeture, le nombre de processus et de pages IPC est revenu à zéro. Les cadres libres ont conservé la valeur du premier cycle, 62 544 sur ARM et 63 095 sur x86. Le tas occupé stabilisé mesurait respectivement 10 272 et 9 248 octets. Dans les quatre cellules, les deux générations initiales 71 et 72 ont abouti à 136 et 137. [Endurance par invité](validation/es-v0.16.6-correctifs-endurance.json)

La borne de l’arène et de son socle matériel est de 144 cadres de 4 Kio (576 Kio), dont au plus deux banques de 64 pages. Elle exclut le tas et les pages IPC. Les sondes de sauvegarde ont observé 65 pages d’arène en création et 80 dans les deux autres scénarios. Un retrait effectif ferme la soumission et la livraison. Un OUT déjà en vol peut atteindre le disque pendant le retrait. Une réponse incertaine impose de vérifier la publication sur le support. Le témoin ARM exerce un SPI isolé puis une vraie complétion. La variante expérimentale IRQ masque la ligne au huitième parasite. La synchronisation par masque local a été exercée sur un vCPU. Une intégration SMP exigerait de sérialiser ces opérations entre cœurs.

Les tests hôtes ont exercé les 256 masques de découverte vi, les identifiants de protocole, les deux sens des messages, les champs réservés du bloc, les erreurs de transport et le repli historique. Les deux invités ont interrogé de vrais handles avant liaison, après liaison puis après retrait, y compris avec une version, une direction ou des droits invalides. Les modèles Darwin ont exercé l’échec de chaque allocation initiale et les entrelacements de réveil de l’IRQ optionnelle. [Trames et droits](validation/es-v0.16.6-correctifs-decouverte.json) · [Modèles ARM64 et Rosetta](validation/es-v0.16.6-correctifs-modeles.json)

**Saturation des files.** Les essais IPC T05/T06 remplissent la FIFO de huit messages, vérifient le refus WOULD_BLOCK du neuvième en mode non bloquant, le réveil d’un émetteur après retrait et l’ordre de livraison. Pour le bloc, le banc QEMU maintient une lecture SG en vol, présente un nouveau tag valide associé à une autre adresse et vérifie le refus de la seconde réservation. Les huit séquences sur les deux architectures et les deux géométries couvrent libération normale et retrait concurrent. Ce témoin s’exécute en polling avec une réservation bloc à une place. Dans chaque cas, le compteur du backend progresse de deux lectures, sans écriture ni FLUSH. L’ancien handle est refusé après fermeture et la capacité fraîche permet la reprise. Les octets du disque sont identiques avant et après les quatre invités. [Saturation et reprise du bail](validation/es-v0.16.6-correctifs-saturation.json)

Le montage du banc de saturation a montré une contrainte x86_64. Les tables des processus recopient les entrées hautes du PML4 au lancement. La première réservation du transport installe la fenêtre MMIO VT-d dans la table maître. Un banc qui créait les processus avant cette première réservation a donc déclenché une faute de page au registre VT-d +0x34, avant toute soumission. Les deux lanceurs x86 ouvrent d’abord le transport puis démarrent le groupe. Les appels natifs d’un client ne créent pas cette fenêtre. La suite de reprise a exercé la mort du pilote et le redémarrage du service dans cet ordre. Une attribution future de ce premier transport à des processus déjà vivants demanderait de revoir la propagation des mappings superviseur. [Création des tables](../kernel/src/memory/tables.rs) · [Installation MMIO](../kernel/src/memory/mod.rs) · [Lancement des services](../kernel/src/sched/process/storage.rs)

## Format disque et droits publiés

Quatre essais interversions ont exercé les secteurs de 512 et 4 096 octets, avec créateur publié ARM64 et moteur candidat x86_64, puis dans l’ordre inverse. Le créateur a produit la génération 66. Le candidat a publié la génération 67. De retour sur le moteur publié, une nouvelle écriture a abouti à la génération 69, relue à l’identique par les deux lecteurs. Le SHA-256 de l’image reste identique pendant les deux relectures. Entre l’audit initial et le candidat, `git diff fac0342..13df1dc -- novaFS` est vide. La comparaison avec la version publiée confirme aussi l’identité du codec, des ancres, des enregistrements, de la cryptographie et du code de migration. [Banc d’interopérabilité](../../novaFS/lab/es-groupees/interop-integration.py)

Le service écrit les blocs CoW et le manifeste puis attend F1. Il publie ensuite l’ancre alternée et attend F2 avant d’acquitter la sauvegarde. Une erreur entre écriture et acquittement de l’ancre exige un remontage et une inspection. La publication garde les règles P9 de validation des générations, des empreintes et des références. [Ordre de publication](es-v0.16.6-integration.md#publication-coupure-et-compatibilité-disque)

## Coupure de la VM et reprise

L’application UTM a forcé l’arrêt de six VM avec `utmctl stop --force`. L’export immédiatement après coupure puis le disque obtenu après le redémarrage ont été vérifiés indépendamment. Aucune des six publications n’avait encore acquitté la sauvegarde auprès de vi. Dans chaque géométrie l’ancienne source est restée lisible et inchangée.

| Frontière | Secteurs | Générations juste après la coupure | Générations après redémarrage | Destination relue |
|---|---|---|---|---|
| Premier lot de pages privées écrit, avant F1 | 512 et 4 096 | 66 et 67 | 66 et 67 | Absente |
| F1 acquittée, avant publication de l’ancre | 512 et 4 096 | 66 et 67 | 66 et 67 | Absente |
| F2 acquittée, avant réponse à vi | 512 et 4 096 | 67 et 68 | 68 et 69 | Complète dans les deux exports |

Les générations initiales étaient 65 et 66. L’ouverture d’un fichier et sa relecture peuvent publier les métadonnées d’accès. Le verdict de récupération repose sur l’inspection des ancres et la vérification du contenu complet de la source et de la destination. [Résultats des six coupures UTM](validation/es-v0.16.6-correctifs-coupures-utm.json)

## Régressions et incidents des candidats intermédiaires

Au premier passage sur le candidat de code final, les 86 commandes historiques ont réussi. Elles comprennent, pour chaque architecture, les 25 démarrages, 24 groupes bloc, 44 groupes fichier, 24 groupes d’espace de noms, les contrôles ELF/processus/IPC, la CLI, nsh, vi et la mémoire. Le moteur livré avec UTM a exécuté en direct les 11 scénarios GICv3 et les 23 scénarios DMA séquentiels, avec reprise.

NovaFS a passé, par architecture Darwin, 196 tests standards, 199 instrumentés, un million d’opérations différentielles, 2 304 injections historiques sur les ancres et 486 injections CoW. Les compilations du cœur portable sur cibles nues et les échanges avec les lecteurs publiés complètent ces tests. Les résultats NovaFS sont distincts des régressions exécutées dans les invités NovaOS.

Les 36 interruptions QEMU et les 20 injections de fautes DMA ont été suivies d’un remontage et d’une comparaison des octets. Dans les quatre couples architecture/secteur, le banc de concurrence a exercé modification de la source, refus de suppression d’un fichier ouvert, remplacement du nom et conservation des modifications locales. Les tests de mémoire sur vi donnent le même pic que le témoin pour chacun des fichiers de 8 Kio, 64 Kio, 1 Mio et 4 Mio. Sur le fichier de 4 Mio, les trois points de mesure sont 82, 98 et 114 pages privées, soit un maximum observé de 456 Kio sur les deux architectures. Ce banc d’ouverture et d’édition localisée se distingue des vecteurs qui génèrent plusieurs Mio de texte neuf. Dans ces derniers scénarios, les relevés du PID 5 dans QEMU atteignent 6 600 Kio pour l’insertion locale, 12 872 Kio pour la création et 33 608 Kio pour l’édition multiple. Les deux architectures et les deux versions donnent les mêmes maxima hors sonde. Les contrôles mémoire d’amorçage sont qualifiés à 128 et 256 Mio. Le profil 64 Mio n’a pas atteint le contrôle ABI. [86 commandes, incidents et essais P9 interversions](validation/es-v0.16.6-correctifs-regressions.json)

Quatre essais QEMU sur un volume occupé laissaient 145 blocs libres sur 1 280. La tentative a conservé la source et les 327 680 octets du fichier tiers. La destination est restée absente. La paire de générations après remontage correspond au témoin d’ouverture sans sauvegarde. Le test `file_8` limite également la virtqueue à huit descripteurs et observe six pages de données par chaîne au maximum.

Le candidat intermédiaire `af0fd10` avait connu trois expirations du banc bloc en 4 Kio, deux sur ARM et une sur x86. Son contrôleur historique attend 250 ticks à 250 Hz pour une réponse émise après quatre parcours de 32 transferts et deux FLUSH. Dix essais ARM comparant témoin et intermédiaire ont ensuite réussi, six sous charge en parallèle et quatre au repos. Au repos, la somme des quatre parcours vaut 0,817 et 0,819 s pour l’intermédiaire, 0,851 et 0,826 s pour le témoin. Le délai préexiste aux correctifs. Il porte sur la réception IPC du banc bloc. Les quatre intervalles matériels mesurés et les ticks de l’ordonnanceur peuvent diverger quand le Mac suspend le vCPU. La cause exacte des trois dépassements reste indéterminée. Le candidat final a passé ces groupes dès le premier essai.

## Conditions de chronométrage

Mac Apple M4 Max, 16 cœurs logiques, 128 Gio, macOS 27.2 (26B123). QEMU direct 11.1.3 en TCG sur RAW et application UTM 5.0.8 avec son moteur QEMU 10.0.12 sur QCOW2. Le profil UTM utilise Cortex-A72 en TCG, un vCPU, 256 Mio, GICv3 sans ITS et le disque VirtIO sous IOMMU. Le moniteur du moteur confirme sa version. Chaque essai démarre avec son propre disque. Les trois vecteurs couvrent la création d’un fichier dense de 4 Mio, l’insertion de Ω au milieu d’un fichier de 4 Mio et l’ajout de 4 Mio avec modifications supplémentaires dans une source de 4 Mio. La qualification utilise le chemin autorisé de vi vers un **nom libre**. Le contenu final est comparé intégralement, puis son SHA-256 comparé à celui de l’audit précédent.

La fenêtre chronométrée va de la commande de sauvegarde jusqu’au message de succès de vi. Elle comprend la confirmation automatisée et le retour de F2. Les lectures, écritures et FLUSH de VirtIO couvrent une partie de cette fenêtre. Les sondes invitées enregistrent la préparation et la publication, ainsi que l’activité de l’ordonnanceur invité. Une durée comptée active par cet ordonnanceur peut inclure un vCPU suspendu par l’hôte. Les traces Instruments mesurent séparément les états réellement observés du thread vCPU macOS. Les compteurs HMP du disque mesurent les opérations du backend QEMU. Quand macOS refuse la lecture directe du conteneur UTM, le banc inspecte un export réalisé par l’application avant le premier démarrage. Le rapport conserve cette provenance de la géométrie QCOW2. Ces intervalles se recouvrent et ne s’additionnent pas.

Pour chaque cellule, l’ELF témoin hors sonde correspond, par son SHA-256, à celui dont l’audit initial a publié les mesures. Chaque cellule corrigée est mesurée trois fois hors sonde et une fois avec sonde. L’ordre témoin/candidat alterne au premier passage. Un seul essai de performance est lancé à la fois. Les percentiles utilisent le rang supérieur. Avec trois observations dans une cellule, p95 et p99 y sont égaux au maximum observé. [Lanceur et vecteurs](../tests/qualification_es_corrigees.py)


### QEMU AArch64, disque brut

| Secteur | Sauvegarde | Avant (ms) | Après, trois essais (ms) | p50 | p95 / p99 / max | IPC avec contrôles | Soumissions bloc, dont FLUSH |
|---:|---|---:|---|---:|---:|---:|---|
| 512 | Création de 4 Mio | 76,542 | 76,769 / 79,879 / 80,028 | 79,879 | 80,028 | 36 | 43, dont 2 |
| 512 | Insertion de Ω au milieu | 94,187 | 94,401 / 115,901 / 95,790 | 95,790 | 115,901 | 43 | 169, dont 2 |
| 512 | Ajout de 4 Mio et éditions multiples | 131,940 | 127,900 / 133,858 / 135,155 | 133,858 | 135,155 | 56 | 190, dont 2 |
| 4096 | Création de 4 Mio | 86,199 | 78,801 / 78,400 / 77,642 | 78,400 | 78,801 | 36 | 43, dont 2 |
| 4096 | Insertion de Ω au milieu | 94,697 | 94,546 / 100,031 / 93,622 | 94,546 | 100,031 | 43 | 169, dont 2 |
| 4096 | Ajout de 4 Mio et éditions multiples | 135,274 | 134,729 / 135,668 / 132,350 | 134,729 | 135,668 | 56 | 190, dont 2 |

### QEMU x86_64, disque brut

| Secteur | Sauvegarde | Avant (ms) | Après, trois essais (ms) | p50 | p95 / p99 / max | IPC avec contrôles | Soumissions bloc, dont FLUSH |
|---:|---|---:|---|---:|---:|---:|---|
| 512 | Création de 4 Mio | 143,187 | 131,512 / 141,555 / 132,139 | 132,139 | 141,555 | 36 | 43, dont 2 |
| 512 | Insertion de Ω au milieu | 150,307 | 140,436 / 146,054 / 134,895 | 140,436 | 146,054 | 43 | 169, dont 2 |
| 512 | Ajout de 4 Mio et éditions multiples | 218,920 | 201,170 / 205,283 / 195,820 | 201,170 | 205,283 | 57 | 190, dont 2 |
| 4096 | Création de 4 Mio | 142,384 | 136,285 / 133,356 / 128,991 | 133,356 | 136,285 | 36 | 43, dont 2 |
| 4096 | Insertion de Ω au milieu | 146,976 | 137,936 / 145,879 / 149,520 | 145,879 | 149,520 | 43 | 169, dont 2 |
| 4096 | Ajout de 4 Mio et éditions multiples | 214,995 | 200,837 / 201,383 / 208,361 | 201,383 | 208,361 | 56 | 190, dont 2 |

### UTM AArch64, QCOW2

| Secteur | Sauvegarde | Avant (ms) | Après, trois essais (ms) | p50 | p95 / p99 / max | IPC avec contrôles | Soumissions bloc, dont FLUSH |
|---:|---|---:|---|---:|---:|---:|---|
| 512 | Création de 4 Mio | 380,142 | 498,653 / 101,083 / 452,885 | 452,885 | 498,653 | 36 | 43, dont 2 |
| 512 | Insertion de Ω au milieu | 95,289 | 94,408 / 97,738 / 521,050 | 97,738 | 521,050 | 43 | 169, dont 2 |
| 512 | Ajout de 4 Mio et éditions multiples | 136,064 | 272,413 / 130,850 / 132,507 | 132,507 | 272,413 | 56 | 190, dont 2 |
| 4096 | Création de 4 Mio | 101,451 | 108,842 / 474,671 / 104,688 | 108,842 | 474,671 | 36 | 43, dont 2 |
| 4096 | Insertion de Ω au milieu | 94,997 | 93,559 / 669,734 / 94,429 | 94,429 | 669,734 | 43 | 169, dont 2 |
| 4096 | Ajout de 4 Mio et éditions multiples | 147,203 | 151,269 / 671,918 / 127,836 | 151,269 | 671,918 | 56 | 190, dont 2 |

### CPU et E/S en édition multiple

| Environnement | Secteur | Durée commande avec sonde (ms) | Invité actif / repos (ms) | FLUSH physiques F1 / F2 vus par l’invité (ms) | Lectures / écritures | FLUSH backend total (ms) |
|---|---:|---:|---:|---:|---:|---:|
| qemu-aarch64 | 512 | 135,570 | 126,564 / 2,467 | 1,306 / 0,376 | 143 / 45 | — |
| qemu-aarch64 | 4096 | 138,287 | 129,064 / 2,482 | 1,261 / 0,396 | 143 / 45 | — |
| qemu-x86_64 | 512 | 195,703 | 189,126 / 1,212 | 1,392 / 0,402 | 143 / 45 | — |
| qemu-x86_64 | 4096 | 198,696 | 189,265 / 3,765 | 1,359 / 0,040 | 143 / 45 | — |
| utm-aarch64 | 512 | 136,392 | 124,906 / 0,000 | 1,413 / 0,067 | 143 / 45 | 1,369 |
| utm-aarch64 | 4096 | 152,645 | 135,670 / 4,770 | 1,358 / 0,072 | 143 / 45 | 1,329 |

### Préemptions du vCPU relevées par Instruments

| Secteur | Sonde invitée | Commande (ms) | vCPU en exécution (ms) | Dont cœurs E / P (ms) | Préempté (ms) | Bloqué (ms) | Sans état (ms) |
|---:|---|---:|---:|---:|---:|---:|---:|
| 512 | Non | 160,175 | 149,725 | 21,753 / 127,972 | 0,039 | 8,846 | 0,000 |
| 512 | Oui | 169,666 | 157,575 | 22,523 / 135,052 | 1,992 | 8,511 | 0,000 |
| 4096 | Non | 163,207 | 152,197 | 25,616 / 126,582 | 0,355 | 9,068 | 0,000 |
| 4096 | Oui | 699,492 | 599,345 | 599,345 / 0,000 | 68,201 | 18,908 | 0,000 |

La plus lente des 54 sauvegardes ordinaires a pris **671,918 ms**, dont 586,884 ms après consentement. Les 143 lectures, 45 écritures et deux FLUSH de son backend UTM ont cumulé respectivement 11,118, 14,697 et 3,049 ms. L’insertion locale à 669,734 ms a cumulé 35,947, 24,757 et 3,748 ms dans ces catégories. Le thread vCPU de ces deux commandes ordinaires n’a pas été tracé.

La plus lente des 94 commandes a pris **699,492 ms** avec les deux sondes, sur UTM/4 096. Instruments attribue 599,345 ms d’exécution à des cœurs d’efficacité et 68,201 ms à la préemption. La même édition avec trace hôte seule a pris 163,207 ms, dont 126,582 ms sur des cœurs de performance et 0,355 ms préemptée. Dans le premier essai, les deux FLUSH de QEMU ont cumulé 4,014 ms. La résidence du vCPU et sa préemption expliquent une part observable de la variabilité de cet échantillon. Ces quatre traces et la comparaison avec ou sans sonde ne suffisent pas à expliquer chaque latence ordinaire. Les valeurs individuelles, incertitudes d’alignement des horloges, placements et statistiques CPU/stockage restent consultables dans le relevé.

La sonde QEMU x86_64/512 sur l’édition multiple compte 17 échanges moteur/bloc, 17 bloc/pilote, 7 nsh/moteur, 4 vi/nsh et 2 dons acquittés. Leur somme est 47. Les contrôles du noyau et du prêt font monter le décompte à 57, avec trois fermetures locales dans la fenêtre contre deux sur l’essai x86_64/4 096. Ajouter les huit copies mémoire locales du prêt donne 65 opérations au sens large. Ces copies ne provoquent pas d’aller-retour entre processus. Le précédent audit avait déjà observé, en création AArch64, trois fermetures locales au lieu de deux sur une répétition. Ici les deux grandes éditions x86 présentent la même répartition des échanges interservices. [Comparaison des compteurs du témoin](es-v0.16.6-integration.md#sauvegardes-de-référence)

## Reproduire les essais

Les deux premiers arguments du banc de performances sont les exports QEMU et UTM utilisés par l’audit initial. Leur noyau est identifié dans chaque résultat par son SHA-256. Le répertoire de construction du candidat doit être neuf. Le dernier argument désigne un répertoire de résultats qui conserve chaque mesure et son journal. Pour UTM, fournir un profil de qualification système AArch64 avec son firmware expérimental, ses périphériques et son disque modèle. Le banc importe une copie QCOW2 par essai. [Profil et construction du banc UTM](../tests/es_groupees_utm.py) · [Matrice mixte et commandes de reproduction](es-v0.16.6-negociation.md#reproduction)

```sh
revision=13df1dce48677f6fd92256b99b0ab0948bde803a
campagne=novaOS/build/rejouer-correctifs-0166
python3 -B novaOS/tests/reconstruire_es_0166.py "$revision" "$campagne" \
    brut regression sans sonde coupe defauts course utm utm-sonde utm-coupe
python3 -B novaOS/tests/qualification_es_corrigees.py \
    "$REFERENCE_QEMU" "$REFERENCE_UTM" "$campagne" \
    "$campagne/sans/novaFS/target/aarch64-apple-darwin/release/examples/es-groupees" \
    "$PROFIL_UTM" "$campagne/perfs"
python3 -B novaOS/tests/qualification_es_bilan.py "$campagne/perfs" "$campagne/mesures.json"
```

Les commandes exactes et les cibles Darwin et invitées de la suite de régression sont consignées dans le relevé. Les bancs de découverte, de durée de vie DMA et d’endurance utilisent leurs propres profils. [Découverte des droits](../tests/qualifier_decouverte.py) · [Fautes du pilote](../tests/dma_propriete.py) · [Cycles de montage](../tests/endurance_sessions.py)

## Domaine de validité

Les coupures QEMU et l’arrêt forcé de l’application UTM interrompent des invités sur un Mac alimenté. Elles permettent de comparer l’état immédiatement lisible et l’état remonté, y compris près des barrières. La persistance après perte physique d’alimentation dépend aussi du cache et du stockage hôtes. Aucun banc d’alimentation dédié n’a été fourni. P9 maintient le refus de montage normal d’une paire d’ancres invérifiable. Les versions publiées, le format P9 et les modalités d’activation de PC1 relèvent de leurs décisions existantes. [Ancres et garanties P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)
