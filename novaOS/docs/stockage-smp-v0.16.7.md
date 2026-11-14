# NovaOS v0.16.7 — Qualification du stockage et du SMP

## Décision proposée à la MOA/MOE

Conserver le chemin classique et le polling VirtIO-blk en distribution. Réserver PLAN/BATCH et l’attente IRQ aux profils de laboratoire explicitement construits. Le support SMP nécessaire à une activation générale manque sur les deux architectures. Le refus historique pendant un montage AArch64/512 est localisé. Sa cause initiale reste ouverte.

Le code invité étudié est le commit publié `64603c4f9e5dcf73db26212a697f2624b2df36be`. Ses parents sont le système classique `3193bc91` et le candidat expérimental `f7654a3b`. L’audit du 14 novembre 2026 porte sur la branche `codex/stockage-smp-0167`. Les bancs de cette campagne sont au commit `f221e672776a638a9b08d0792a71336f074e66bf`. Les sondes s’appliquent exclusivement à des exports distincts. Les binaires de performance et de régression sont reconstruits depuis le commit publié. [Intégration v0.16.6](integration-v0.16.6.md)

## CPU présentés et CPU utilisés

| Invité QEMU 11.1.3 / TCG | vCPU énumérés par QMP | CPU observés pour relais, NovaFS, pilote et client | Secteurs et chemins |
|---|---|---|---|
| AArch64 virt/GICv2 | 1, 2 puis 4 | CPU 0 pour les quatre processus | 512 et 4 096, classique et groupé |
| x86_64 Q35 | 1, 2 puis 4 | CPU 0 pour les quatre processus | 512 et 4 096, classique et groupé |

Le banc lit `query-cpus-fast` pendant l’exécution, puis relève MPIDR_EL1 ou l’identifiant APIC lors des appels système. Chaque couple CPU/PID observé est journalisé une fois. Sur les 24 configurations, 50 montages sans injection ont atteint nsh. Chaque volume a gardé la même empreinte après fermeture. Les deux géométries utilisent un bloc P9 de 4 Kio, soit huit secteurs logiques de 512 octets ou un secteur logique de 4 096 octets. Les six premières collectes groupées sur x86_64 avaient exigé à tort zéro cadre DMA au niveau machine. Les six nouvelles exécutions contrôlent les neuf cadres permanents VT-d et zéro page utilisateur, page IPC ou processus résiduel. [Montages individuels et empreintes](validation/stockage-v0.16.7/montages.json) · [Propriété des cadres VT-d](es-v0.16.6-integration-dma.md)

### Limites de l’architecture actuelle

- **Amorçage et préemption.** Le noyau ne demande pas à Limine de lancer les processeurs secondaires. `spawn_on` accepte le masque 1. Un seul `Core` contient le thread courant, les prêts, les processus et le domaine IPC. Sa référence mutable repose sur une garde qui masque les interruptions du CPU appelant. La préemption fait bien alterner les processus sur le BSP. [Amorçage](../kernel/src/boot.rs) · [Ordonnanceur](../kernel/src/sched/mod.rs) · [Contrat des threads](ordonnancement.md)
- **IPC et capacités.** La résolution d’une génération, l’admission, la mise en attente et le réveil utilisent cette même sérialisation. Le mutex noyau publie son propriétaire et les dormeurs sous masque local. Il manque un protocole inter-CPU pour l’exclusion, les réveils et la restitution des espaces d’adressage. Sur x86_64, `invlpg` est local. Sur AArch64, `tlbi vmalle1is` couvre le domaine partageable interne, mais la gestion des racines et des vies de processus suppose un seul EL1 actif. [Mutex](../kernel/src/sched/sync.rs) · [Pagination ARM](../kernel/src/arch/aarch64/paging.rs) · [Pagination x86](../kernel/src/arch/x86_64/paging.rs)
- **Interruptions, DMA et mémoire.** Le gestionnaire VirtIO s’exécute sur le BSP. La publication de la chaîne est ordonnée vis-à-vis du périphérique. Les atomiques du bail et du compteur d’IRQ ne sérialiseraient pas, à elles seules, le contrôle de la capacité, la copie vers le client et un retrait simultané sur deux CPU. Le chemin actuel revalide le propriétaire avant la livraison. Le bail conserve les cadres, la requête et l’étape de retrait jusqu’aux acquittements matériels. [Portail de livraison](../kernel/src/sched/process/syscall.rs) · [Retrait DMA](../kernel/src/hardware/block/ownership.rs)
- **VFS, NovaFS et persistance.** Le service traite une requête à la fois par sa boucle de réception. Le plan est lié à la session, à la source et au nom autorisé. Les blocs CoW et leurs métadonnées précèdent F1, puis viennent l’ancre et F2 avant le succès. Cet ordre a été exercé en mono-vCPU. Une future exécution simultanée devra aussi garantir la durée de vie des pages immuables et la relation entre révocation, fin de DMA et publication. [Négociation](es-v0.16.6-negociation.md) · [Contrat de publication de l’expérience](es-v0.16.6-integration-dma.md)

Les lectures/écritures multiclients, ouvertures et fermetures simultanées, décès en vol, saturation et contention prolongée **sur plusieurs CPU actifs** restent à qualifier. Leur prérequis est une architecture SMP du noyau avec démarrage des secondaires, état par CPU, protection des tables partagées, réveil distant et synchronisation des invalidations. Ce chantier appelle un arbitrage distinct.

## Incident AArch64 / 512

Le journal original de v0.16.6 se termine avant l’invite nsh et avant l’injection « avant F1 ». Il compte 2 029 lectures bloc réussies, 2 032 allers-retours mesurés, aucune écriture ou barrière comptabilisée, puis le client PID 4 sort avec `IO=26`. NovaFS (PID 2) sort avec 5, le relais (PID 1) et le pilote (PID 3) avec 11. Le retrait aboutit et les compteurs DMA, processus et IPC reviennent à zéro. Le journal ne contient ni adresse de la lecture en erreur, ni premier statut interne, ni marque d’erreur de statut/longueur VirtIO. La série originale porte l’empreinte SHA-256 `e8580fb8c810cddb1f509007c2f701005db77a2c8eba7f6606d073f11fc8f976`. [Journal série original compressé](validation/stockage-v0.16.7/refus-original.serial.gz) · [Extraits originaux et témoins](validation/stockage-v0.16.7/montages.json)

Dans ce profil 42, le service lit l’ancre puis appelle `Engine::mount_current`. Celui-ci vérifie les deux générations et les blocs partagés avant le FLUSH de montage en lecture/écriture. Les lectures utilisent des pages de 4 Kio sur les deux géométries. La réponse publique `IO` regroupe les erreurs de transport et l’indisponibilité du périphérique. Le statut 5 lors de la fermeture de NovaFS ne démontre pas un refus initial de capacité. Les témoins à erreur identique peuvent terminer le service avec 5 (droit refusé) ou 11 (canal fermé). La progression du montage est émise toutes les 256 pages. Le rang 2 030 ne correspond pas à une émission de progression. [Service NovaFS](../user/novafs/files.rs) · [Périphérique IPC](../user/novafs/device.rs) · [Contrôle de la paire P9](../../novaFS/src/engine.rs)

Les témoins utilisent le fichier dense `f4194304` du banc de coupure et un volume de 4 096 blocs P9. Le témoin neuf retrouve le secteur physique QEMU 7 416 à deux passages. Il correspond au LBA invité 7 416 en géométrie 512 et au LBA 927 en géométrie 4 096. Les deux représentent le même décalage de 3 796 992 octets. Les droits de la boîte PAGE sont vérifiés avant transport. La découverte bloc observée annonce le masque 1 en classique et 3 en groupé. L’octroi du plan fichier dépend d’un montage réussi.

| Injection sur une VM indépendante | Chemins et secteurs | Observation |
|---|---|---|
| EIO sur la première lecture du secteur physique 7 416 | Classique/groupé × 512/4 096 | Quatre refus à la lecture 979, après 978 succès |
| EIO armée après 1 500 événements de lecture du backend, même secteur | Classique/groupé × 512/4 096 | Quatre refus à la lecture 2 030, après 2 029 succès |
| Notification de fin omise à la lecture 2 030 après livraison par VirtIO | Classique/groupé × 512/4 096 | Quatre refus. La file used contient 4 097 octets, le comptage de l’IRQ attendue expire, le portail remonte 12 puis le client reçoit 26 |

Dans les huit injections EIO, QEMU retourne une erreur de lecture. VirtIO écrit le statut 1, le portail PAGE retourne 12, la réponse bloc porte cet échec et le montage renvoie 26 au client. Les huit images restent identiques. Le journal de v0.16.6 ne porte pas la marque VirtIO présente dans les huit témoins. Un EIO injecté démontre la fermeture sûre sur ce chemin, sans établir que QEMU a retourné EIO lors de l’incident initial. Le témoin sans notification reproduit le rang, le nombre d’échanges et l’absence de message d’erreur périphérique. Sur le témoin groupé ARM/512, le plus long aller-retour mesuré atteint 244,429 ms, contre 1,407 ms dans la série originale. Une perte d’IRQ identique à ce témoin n’est donc pas établie. Les quatre retraits ont abouti et les quatre images ont gardé leur empreinte. La série ancienne ne permet pas non plus d’attribuer le refus à une géométrie, à la négociation BATCH ou à une préemption hôte.

Un second rejeu a lancé quatre QEMU AArch64 indépendants en parallèle sur l’hôte, chacun avec un seul vCPU, le profil groupé, des secteurs de 512 octets, la sonde de transport et son propre disque. Les 64 montages (16 par série) ont tous atteint nsh. Le nombre total de montages sans injection atteint 114, en incluant les 50 essais de topologie. Les 12 injections ont toutes refusé le montage. Aucun de ces essais n’a établi le déclencheur spontané de v0.16.6. Le correctif livré porte sur l’observabilité et la qualification. Toute correction de production du pilote ou modification des délais demanderait un premier statut interne recueilli sur une occurrence spontanée.

## Compatibilité, intégrité et sécurité

| Campagne rejouée | Portée | Résultat |
|---|---|---|
| Compositions de binaires | Deux architectures, modes 41/42, composants publiés et expérimentaux | 56 croisements de base, 10 frontières à géométrie inversée, 2 témoins bruts, 20 fichiers multipages. 88 publications et 88 remontages conformes (ARM 39 sur 512 et 5 sur 4 096, x86 en géométrie inverse) |
| Recréation des services | 64 sessions consécutives par architecture et géométrie | 256 copies relues, 256 noms occupés refusés, compteurs libres et tas stables après fermeture |
| Propriété DMA | Polling et IRQ expérimentale, 512 et 4 096 | 480 injections sur ARM et 448 sur x86, 512 décès de titulaires. Retrait incomplet conservé en quarantaine, livraison refusée si propriétaire, réservation ou adresse invalides |
| Saturation des descripteurs | Deux essais sur chacune des quatre configurations | Huit refus contrôlés, empreintes avant/après identiques |
| Droits et négociation | Tests de trames et réponses sur les deux cibles Darwin | Version, masque, champs réservés, session, droits, révocation et reconnaissance des réponses historiques contrôlés |
| Coupures et concurrence | QEMU AArch64/x86_64, deux géométries | 36 coupures groupées, 20 contrôles/défauts DMA, 4 coupures classiques. Remontage normal et vérification de la source et de la destination attendue. Modification concurrente, suppression et réemploi du nom détectés |
| Coupures UTM | Application UTM/QCOW2, pages, F1, F2, deux géométries | Six arrêts forcés de VM, aucune confirmation avant la coupure. Destination absente aux deux premières frontières et entière après F2 |
| NovaFS portable | Darwin ARM64 natif et x86_64 sous Rosetta | Par architecture, 196 tests standards, 199 instrumentés, 1 000 000 d’opérations différentielles, 2 304 injections sur les ancres et 486 injections CoW groupées |
| Ancien et nouveau lecteur P9 | Quatre croisements de géométrie et de cible Darwin | Écriture, publication groupée, retour en écriture avec l’ancien binaire, relectures indépendantes de la même génération |
| Régressions système | Classique et expérimental, deux architectures | Par profil et architecture, 25 démarrages, 24 groupes bloc, 44 groupes fichier, 24 groupes de noms, ELF/processus/IPC, CLI, nsh et vi. 11 scénarios firmware GICv3 et 23 DMA séquentiels sur le moteur livré par UTM |

Les 126 commandes dont le banc enregistre le statut et l’essai P9 interversions séparé ont abouti. Les six tests ignorés par défaut dans chacune des suites standards et instrumentées figurent dans le relevé portable. La campagne de propriété vérifie la restitution après retrait confirmé, et l’inventaire des cadres retenus lorsque l’acquittement manque. Les coupures ont interrompu des VM sur un Mac alimenté. La perte électrique du stockage hôte reste hors qualification. [Résultats et commandes de régression](validation/stockage-v0.16.7/regressions.json) · [Matrice IPC/VFS et DMA](validation/stockage-v0.16.7/compatibilite-dma.json) · [Relectures entre versions](validation/stockage-v0.16.7/interop-p9.json)

## Sauvegardes durables en mono-vCPU

Les 22 sauvegardes groupées ordinaires ont atteint l’acquittement durable en moins d’une seconde. Le maximum UTM est de 138,512 ms, contre 9 845,455 ms en classique. Les 18 sauvegardes groupées avec sonde sont également sous la seconde. Le compteur conservateur donne 36 à 56 IPC, avec un maximum de 64 opérations si on y ajoute aussi les copies locales du prêt. Les 36 pics de cadres des cellules instrumentées classique/groupé sont identiques, cellule par cellule, à la référence v0.16.6. Le maximum groupé, 9 766 cadres, correspond à 38,15 Mio pour la session entière.

La grille séquentielle alterne les deux chemins. Chaque écriture provient de vi. Le chronomètre hôte démarre à la commande de sauvegarde, inclut le dialogue de confirmation et se termine au message « fichier enregistré ». Le banc relit ensuite la destination avec un lecteur indépendant. Dans UTM, il redémarre également le système et relit le disque QCOW2 exporté. La création porte sur 4 Mio, la modification localisée se situe au milieu de 4 Mio et le scénario multiple insère 4 Mio dans 4 Mio avec d’autres éditions. Les mesures ordinaires et celles avec compteurs proviennent de passages distincts. [80 observations, CPU, compteurs, empreintes et rangs par cellule](validation/stockage-v0.16.7/performances.json)

L’hôte exécute macOS 27.2 (26B123). Les deux QEMU directs 11.1.3 utilisent TCG, 256 Mio et un disque RAW en cache writeback. UTM 5.0.8 utilise QEMU 10.0.12, la machine virt-10.0, un Cortex-A72 émulé, 256 Mio, GICv3 sans ITS, un QCOW2 clairsemé à clusters de 64 Kio et la politique hôte ordinaire. Chaque VM du banc est isolée, les charges de régression sont terminées avant la grille et aucune charge artificielle n’est prescrite. Les charges hôte instantanées sont consignées individuellement.

| Invité et chemin | n hors sonde | p50 (ms) | p95 (ms) | p99 (ms) | Maximum (ms) | Référence v0.16.6, maximum (ms) |
|---|---:|---:|---:|---:|---:|---:|
| QEMU ARM groupe | 6 | 92,074 | 128,519 | 128,519 | 128,519 | 131,940 |
| QEMU ARM classique | 6 | 4897,117 | 10403,825 | 10403,825 | 10403,825 | 10444,747 |
| QEMU x86 groupe | 6 | 140,882 | 191,471 | 191,471 | 191,471 | 198,201 |
| QEMU x86 classique | 6 | 4314,742 | 8755,626 | 8755,626 | 8755,626 | 8731,638 |
| UTM ARM groupe | 10 | 127,829 | 138,512 | 138,512 | 138,512 | 909,640 |
| UTM ARM classique | 10 | 9599,522 | 9845,455 | 9845,455 | 9845,455 | 61721,585 |

Cette campagne ne contient aucun dépassement groupé d’une seconde à attribuer à l’hôte ou à l’invité. Les queues de latence UTM de campagnes antérieures restent pertinentes pour une décision de production. [Étude de la variabilité hôte](integration-v0.16.6-performances.md)

Les percentiles ci-dessus décrivent les scénarios réunis par plateforme et par chemin. Le rang est arrondi au supérieur. Avec six à dix mesures par ligne, p95 et p99 rejoignent le maximum. Chaque cellule a une mesure ordinaire, sauf le scénario multiple UTM qui en a trois par géométrie et par chemin. Ces effectifs ne donnent pas une estimation fiable de la queue d’une distribution de production.

| Invité | Secteur | Édition | Classique (ms) | Groupé (ms) | IPC avec contrôles C/G | FLUSH C/G | Pic de session (cadres de 4 Kio) C/G |
|---|---:|---|---:|---:|---:|---:|---:|
| QEMU ARM | 512 | Création | 4825,892 | 74,709 | 84311 / 36 | 132 / 2 | 5613 / 4033 |
| QEMU ARM | 512 | Milieu | 4931,959 | 94,683 | 84336 / 43 | 134 / 2 | 5869 / 2990 |
| QEMU ARM | 512 | Multiple | 10333,814 | 128,189 | 168600 / 56 | 262 / 2 | 11961 / 9755 |
| QEMU ARM | 4096 | Création | 4823,851 | 75,131 | 84311 / 36 | 132 / 2 | 5613 / 4033 |
| QEMU ARM | 4096 | Milieu | 4897,117 | 92,074 | 84336 / 43 | 134 / 2 | 5869 / 2990 |
| QEMU ARM | 4096 | Multiple | 10403,825 | 128,519 | 168600 / 56 | 262 / 2 | 11961 / 9755 |
| QEMU x86 | 512 | Création | 4342,475 | 132,407 | 84311 / 36 | 132 / 2 | 5621 / 4044 |
| QEMU x86 | 512 | Milieu | 4314,742 | 140,992 | 84336 / 43 | 134 / 2 | 5877 / 3001 |
| QEMU x86 | 512 | Multiple | 8657,568 | 191,471 | 168600 / 56 | 262 / 2 | 11969 / 9766 |
| QEMU x86 | 4096 | Création | 4156,287 | 132,502 | 84311 / 36 | 132 / 2 | 5621 / 4044 |
| QEMU x86 | 4096 | Milieu | 4225,692 | 140,882 | 84336 / 43 | 134 / 2 | 5877 / 3001 |
| QEMU x86 | 4096 | Multiple | 8755,626 | 190,952 | 168600 / 56 | 262 / 2 | 11969 / 9766 |
| UTM ARM | 512 | Création | 4527,236 | 111,382 | 84311 / 36 | 132 / 2 | 5594 / 4014 |
| UTM ARM | 512 | Milieu | 4567,000 | 98,284 | 84336 / 43 | 134 / 2 | 5850 / 2971 |
| UTM ARM | 512 | Multiple | 9600,485 / 9699,300 / 9845,455 | 127,829 / 130,864 / 130,013 | 168600 / 56 | 262 / 2 | 11942 / 9736 |
| UTM ARM | 4096 | Création | 4543,422 | 103,202 | 84311 / 36 | 132 / 2 | 5594 / 4014 |
| UTM ARM | 4096 | Milieu | 4633,664 | 98,979 | 84336 / 43 | 134 / 2 | 5850 / 2971 |
| UTM ARM | 4096 | Multiple | 9599,522 / 9612,603 / 9740,302 | 130,981 / 136,027 / 138,512 | 168600 / 56 | 262 / 2 | 11942 / 9736 |

Le temps CPU est la différence utilisateur + système de tous les threads du processus QEMU mesurée par Darwin aux deux bornes de la sauvegarde. Les lectures du compteur encadrent le chronomètre. Le compteur invité « actif » du profil groupé soustrait le repos du temps écoulé et peut inclure des pauses de vCPU. Les deux mesures ont donc des périmètres distincts. La durée de FLUSH ci-dessous cumule les commandes physiques observées dans l’invité, sur le passage avec sonde. UTM fournit en plus les compteurs du backend et sa durée cumulée dans le JSON.

| Invité | Secteur | Édition | CPU moteur classique (ms) | CPU moteur groupé (ms) | Durée des FLUSH classique / groupé (ms) |
|---|---:|---|---:|---:|---:|
| QEMU ARM | 512 | Création | 4867,436 | 74,818 | 12,576 / 1,032 |
| QEMU ARM | 512 | Milieu | 4973,493 | 95,384 | 12,409 / 0,797 |
| QEMU ARM | 512 | Multiple | 10420,155 | 130,978 | 22,852 / 1,171 |
| QEMU ARM | 4096 | Création | 4864,041 | 75,219 | 12,463 / 0,808 |
| QEMU ARM | 4096 | Milieu | 4942,432 | 91,314 | 12,843 / 1,037 |
| QEMU ARM | 4096 | Multiple | 10489,303 | 128,954 | 37,050 / 1,141 |
| QEMU x86 | 512 | Création | 4372,753 | 131,511 | 9,985 / 1,323 |
| QEMU x86 | 512 | Milieu | 4343,401 | 138,979 | 11,014 / 2,313 |
| QEMU x86 | 512 | Multiple | 8718,725 | 194,793 | 19,180 / 1,734 |
| QEMU x86 | 4096 | Création | 4184,254 | 128,015 | 10,206 / 2,039 |
| QEMU x86 | 4096 | Milieu | 4251,985 | 140,947 | 11,215 / 1,691 |
| QEMU x86 | 4096 | Multiple | 8814,172 | 194,023 | 18,620 / 1,206 |
| UTM ARM | 512 | Création | 4579,815 | 115,924 | 20,218 / 1,144 |
| UTM ARM | 512 | Milieu | 4615,508 | 104,154 | 20,772 / 1,070 |
| UTM ARM | 512 | Multiple | 9702,934 / 9809,327 / 9957,825 | 136,761 / 140,227 / 139,031 | 40,375 / 1,307 |
| UTM ARM | 4096 | Création | 4598,639 | 107,694 | 20,714 / 1,020 |
| UTM ARM | 4096 | Milieu | 4690,910 | 106,907 | 20,085 / 1,160 |
| UTM ARM | 4096 | Multiple | 9702,985 / 9716,421 / 9844,760 | 139,949 / 145,555 / 143,080 | 40,745 / 1,406 |

Les compteurs des copies ci-dessous couvrent les dépôts/retraits IPC de page (PUT/TAKE), les copies locales explicites du prêt immuable et les pages effectivement copiées entre l’arène DMA multipage et le service. Ils ne couvrent pas les opérations internes du compilateur, de QEMU et de qcow2. Le chemin classique n’a pas d’arène multipage. [Comptage du chemin de sauvegarde](../tests/vi_ipc_sauvegarde.py)

| Édition | PUT + TAKE classique | PUT + TAKE groupé | Copies locales du prêt groupé | Pages de l’arène groupée, lecture / écriture |
|---|---:|---:|---:|---:|
| Création | 3260 | 16 | 6 | 18 / 1047 |
| Milieu | 3278 | 26 | 0 | 1037 / 1048 |
| Multiple | 6516 | 34 | 8 | 1038 / 2088 |


## Conditions d’une activation progressive

1. Distribuer le chemin classique avec PAGE lorsque les deux extrémités le négocient. Garder le profil expérimental constructible séparément.
2. Sur une nouvelle session expérimentale, vérifier la boîte et ses droits de direction, les offres du relais et du pilote, puis monter P9. Interroger le service sur PLAN et vérifier les droits d’écriture et d’ajout ainsi que la version, le masque et les champs réservés. La découverte ne confère aucune capacité.
3. Choisir le chemin classique **avant publication** sur une absence d’extension explicitement reconnue. Propager toute trame invalide, révocation, corruption, erreur de transport ou publication incertaine. Après redémarrage des services, rouvrir une session et refaire la négociation. L’endurance exerce la recréation des quatre services et la renégociation dans la même VM. Le remplacement à chaud d’un seul service au milieu d’une publication avec les autres survivants reste hors du contrat qualifié. Une perte d’acquittement interdit de relancer automatiquement l’écriture.
4. Pour modifier le défaut, exiger un diagnostic suffisant de l’incident, des mesures reproductibles et une qualification SMP dans le périmètre multicœur prévu. La décision de configuration appartient à la MOA/MOE. [Réponses historiques admises et contrat PLAN](es-v0.16.6-negociation.md)

## Reproduire le diagnostic

Depuis la racine du dépôt, utiliser un dossier neuf pour chaque export et pour chaque résultat. Le seul disque présenté à l’invité est une image créée dans un répertoire temporaire. Les variables ci-dessous décrivent le noyau candidat et le lecteur hôte construit à partir de la même révision.

```sh
ref=64603c4f9e5dcf73db26212a697f2624b2df36be
b=novaOS/build/rejeu-stockage-0167
python3 -B novaOS/tests/reconstruire_es_0166.py "$ref" "$b/profils" sans
lecteur="$b/profils/sans/novaFS/target/aarch64-apple-darwin/release/novafs"
python3 -B novaOS/tests/stockage_smp_montage.py construire "$ref" "$b/observation"
python3 -B novaOS/tests/stockage_smp_montage.py observer "$b/observation/coupe" \
    "$lecteur" "$b/arm-4cpu" aarch64 512 4
python3 -B novaOS/tests/stockage_smp_montage.py observer "$b/observation/coupe" \
    "$lecteur" "$b/arm-512-eio" aarch64 512 1 --eio 7416 --armer 1500
```

Pour isoler les étapes du transport dans un export neuf, ajouter `--transport` à `construire`. Pour le témoin dédié à la notification omise, utiliser `construire --irq-omise`, puis `observer` sans injection EIO. Le premier statut est conservé distinctement pour la file utilisée, l’IRQ, le portail, l’échange bloc, la collecte de page et la progression du montage. Pour le rejeu sous charge, lancer quatre fois `observer` en parallèle sur le même export groupé avec `--transport`, `aarch64 512 1 --repetitions 16` et quatre dossiers de résultats distincts. Chaque processus QEMU utilise son propre disque.

Pour les mesures, fournir un modèle .utm du système installé avec firmware privé et disque QCOW2. Le banc vérifie la provenance de chaque ELF et de l’oracle, prépare un disque jetable par cellule puis conserve la série et le résultat. Les suites de charge concurrente se terminent avant ce chronométrage.

```sh
python3 -B novaOS/tests/reconstruire_es_0166.py "$ref" "$b/profils" brut sonde utm utm-sonde --jobs 4
python3 -B novaOS/tests/integration_main_performances.py preparer "$ref" "$b/temoins"
modele=novaOS/build/io-0165/utm/NovaOS-nsh-systeme-6e5f7404.utm
python3 -B novaOS/tests/integration_main_grille.py campagne "$b/profils" "$b/temoins" "$modele" "$b/mesures"
python3 -B novaOS/tests/integration_main_grille.py bilan "$b/profils" "$b/temoins" "$modele" "$b/mesures" "$b/performances.json"
```
