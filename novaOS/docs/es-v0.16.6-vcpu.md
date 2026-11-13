# NovaOS v0.16.6 — Latence du vCPU et de la publication sous UTM

## Protocole

Branche `codex/es-groupees-0166`, base vérifiée `b79d1133dd0dbde9aabfaa88e14a3c593708f4b2`. La référence fournie dans le mandat omet le `d` après `b79d1133dd0` et ne désigne aucun objet Git. Le préfixe `b79d1133` désigne le commit attendu de qualification CPU. Le relevé de référence avait enregistré quatre dépassements sur 126 sauvegardes dans l’application UTM. Trois concernaient une insertion de 4 Mio dans un fichier de 4 Mio. Le quatrième concernait une insertion de deux octets UTF-8 en tête du fichier. L’ELF UTM hors sonde provient du dernier audit. Une variante de son ELF instrumenté chronomètre séparément les deux FLUSH et l’occupation du thread idle. [Mesures de référence](es-v0.16.6-cpu.md)

Chaque essai UTM démarre un disque QCOW2 jetable depuis l’application 5.0.8 avec son moteur QEMU 10.0.12, un Cortex-A72 TCG, 256 Mio, le firmware EDK2 expérimental, GICv3 sans ITS et VirtIO-IOMMU. Chaque succès est suivi d’un redémarrage, d’un contrôle GPT et P9 et d’une relecture indépendante, octet par octet, de la source et de la destination. Les mesures commencent avec la commande `w copie` et finissent à la réception du succès, après F2. L’instant d’envoi du consentement `OUI` est horodaté séparément. Un blocage du pilote, un refus de consentement ou une révocation conservent leurs effets habituels. [Banc de sauvegarde et de relecture](../tests/es_groupees_utm.py)

| Horloge ou compteur | Ce que mesure l’instrument |
|---|---|
| Darwin monotone | Temps écoulé du point de vue du terminal, y compris le traitement de ses entrées et sorties. |
| `proc_threadinfo` | Temps CPU utilisateur et système du seul thread TCG. Le PID du moteur est recoupé avec l’UUID SPICE. Le moniteur `info cpus` et le nom Darwin `CPU 0/TCG` permettent d’identifier le thread. Le CPU comprend l’émulation et les travaux QEMU exécutés sur ce thread. Deux lectures complémentaires de `PROC_PIDTHREADCOUNTS` aux bornes donnent le temps du seul vCPU par niveau de cœur, avec les noms de `hw.perflevelN.name`. Les horodatages et les éventuels refus restent dans chaque relevé. |
| `proc_pid_rusage` | CPU de tous les threads du moteur et temps sur les cœurs de performance. Les compteurs de temps exécutable et de temps sur cœurs P concernent l’ensemble des threads du processus. |
| Ordonnanceur NovaOS | Le compteur architectural mesure, entre les marqueurs de publication, le temps où l’ordonnanceur a sélectionné un thread actif ou le thread idle. Une pause de macOS suspend le travail invité et peut allonger ces durées. Le moteur direct 11.1.3 fournit en complément des mesures CPU par phase et un décompte des instructions invitées. |
| VirtIO et QCOW2 | Temps invité cumulé entre notification et observation de la complétion, temps individuel de chaque requête FLUSH, temps de chaque lot de publication et compteurs de `drivecible` en HMP. Les lots F1 et F2 comprennent des écritures et leur barrière. Le temps QEMU du nœud inclut ses files et les attentes du système hôte. |

Les horloges couvrent des fenêtres qui se recoupent. Soustraire à la durée totale les attentes VirtIO et le CPU de tous les threads compterait deux fois certains intervalles. Le relevé d’état Darwin demandé toutes les 10 ms permet de localiser des intervalles avec peu d’exécution du vCPU. Deux observations indiquant « exécutable » ne prouvent pas que le thread soit resté continûment prêt entre elles. Les écarts entre prélèvements et les bornes autour de la confirmation sont conservés. [Sonde Darwin](../tests/vcpu_darwin.py) · [Implémentation des compteurs dans XNU](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/bsd_kern.c) · [Sémantique des états du thread](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/thread.c) · [Compteurs par niveau de performance dans XNU](https://github.com/apple-oss-distributions/xnu/blob/main/doc/observability/recount.md)

XNU comptabilise le temps exécutable depuis le réveil du thread jusqu’à son blocage, y compris son exécution. La lecture peut omettre la tranche en cours. La différence signée entre le temps exécutable de toute la tâche et son CPU est un indicateur global. Elle ne borne pas de façon démontrée le temps passé en file d’attente par le seul vCPU. [Compteur de tâche](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/task.c#L6387-L6432) · [Lecture du timer](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/timer.h#L104-L112)

Sur six suspensions imposées à un processus enfant du banc, le système a confirmé un arrêt de 250 à 255 ms et le thread a consommé zéro temps CPU pendant cet intervalle. Son état échantillonné était « attente », aussi observé dans les six essais comportant un sommeil volontaire. L’écart maximal entre le temps CPU du thread selon Darwin et l’horloge CPU indépendante du témoin est de 0,039 ms. Six étalonnages complémentaires avec le décompte par niveau de cœur ont donné au maximum 0,089 ms d’écart entre la somme P+E et le témoin indépendant. La lecture de `ktrace info` a échoué sur ce Mac. Aucun évènement exact de préemption du vCPU UTM n’a donc été obtenu par cette interface. [Étalonnage et mesures individuelles](validation/es-v0.16.6-vcpu-etalonnage.json)

## Charge de l’hôte

Les quatre calibrations concomitantes aux régressions ont duré 800,856 ms (repos), 476,705 ms (CPU), 632,332 ms (mémoire) et 156,589 ms (stockage). La première tentative d’import a échoué pendant un autre essai UTM ; une autre tentative a trouvé une invite HMP antérieure à la réponse attendue. Les deux incidents précédaient toute sauvegarde. La série contrôlée emploie un seul banc UTM et lit maintenant la réponse complète `info cpus`. [Relevés de calibration](validation/es-v0.16.6-vcpu-pilotes.json)

Les répétitions utilisent le même Mac M4 Max à 16 cœurs et 128 Gio sous macOS 27.2 (26B123, Darwin 27.2.0, `xnu-13432.41.4~25`), avec des disques logiquement identiques, des clusters QCOW2 de 64 Kio et des secteurs de 512 ou 4 096 octets. Le banc alterne l’ordre des profils et garde les sauvegardes de l’application en série. Les essais de calibration concomitants aux régressions sont identifiés séparément. Les applications ordinaires du Mac et les caches APFS continuent à fonctionner. [Ordre des essais](../tests/vcpu_campagne.py)

| Profil | Charge appliquée pendant la commande |
|---|---|
| Repos | Aucune charge ajoutée par le banc. L’activité des autres processus est mesurée. |
| CPU | 12 processus effectuent chacun des SHA-256 sur un tampon privé de 256 Kio. Le premier calcul de chaque processus précède la mesure. |
| Mémoire | Un processus alterne les copies entre deux zones résidentes de 256 Mio initialisées au préalable. Cet essai sollicite la bande passante. L’indicateur de pression mémoire, les pages compressées et le swap permettent de préciser le régime réellement atteint. |
| Stockage | Un processus réécrit séquentiellement un fichier temporaire de 32 Mio sur le même volume hôte. Chaque écriture de 4 Mio utilise `F_NOCACHE` et `fsync`. Le processus se termine après la mesure. |

Deux relevés de CPU global, de pression et d’activité de `disk0` encadrent la mesure. Leur fenêtre inclut les requêtes de surveillance précédant et suivant la commande. Le disque interne dessert d’autres processus. Les volumes affichés par `iostat` ont la résolution de sa sortie texte. Le fichier de charge est distinct du QCOW2 de la VM. Un relevé de `pmset` à 20 h 32 CET indiquait le secteur branché, sans avertissement thermique ni avertissement de performance enregistré. La fréquence instantanée et les événements thermiques à chaque sauvegarde restent inconnus. [Mesure et charges hôtes](../tests/charge_hote.py)

## Témoins QEMU et calcul invité

Les trois moteurs lancés directement donnent les résultats suivants pour les quatre modifications en secteurs de 512 et 4 096 octets. Les mesures sont en millisecondes, avec trois démarrages sans sonde invitée par combinaison. Les régressions en parallèle ont constitué une charge variable pour ces moteurs directs. [Scénarios, résultats individuels et percentiles par cellule](es-v0.16.6-vcpu-qemu.md)

| Moteur | Disque | n | p50 | p95 | p99 et maximum | ≥ 1 s |
|---|---|---:|---:|---:|---:|---:|
| QEMU 11.1.3 AArch64 | RAW | 24 | 152,565 | 511,758 | 583,624 | 0 |
| QEMU 11.1.3 x86_64 | RAW | 24 | 236,983 | 586,211 | 629,793 | 0 |
| Moteur QEMU 10.0.12 d’UTM, AArch64 | QCOW2 clairsemé | 24 | 128,397 | 378,357 | 386,306 | 0 |

Le profilage léger de la publication d’une destination de 8 Mio a mesuré **132–139 ms de CPU du thread TCG sur AArch64 et 215–228 ms sur x86_64** avec QEMU 11. Les SHA-256 de la source et de la candidate représentent 48–49 % et 66–69 % de ce temps. Les adresses et générations font partie des empreintes P9. Les contrôles de texte et de parcours d’extents peuvent revisiter des données déjà parcourues. Sur l’invité ARM64 direct, l’ensemble de la validation du texte représente 8,21–8,50 ms CPU et les extents 4,01–4,16 ms. Supprimer leurs doubles parcours ne pourrait économiser qu’une fraction de ces durées dans cette configuration. Le comptage lourd de chaque instruction, exécuté séparément, a fait dépasser une seconde à 12 des 16 sauvegardes. [Profilage TCG et analyse du moteur](es-v0.16.6-vcpu-qemu.md) · [Mesures des 128 exécutions directes](validation/es-v0.16.6-vcpu-qemu.json)

Sur le moteur livré avec UTM lancé directement, l’exécution instrumentée la plus lente a pris 872,809 ms. L’invité a cumulé 618,784 ms entre notification VirtIO et observation de fin, le nœud QEMU a compté 8,314 ms de lectures, 78,979 ms d’écritures et 7,993 ms pour les deux FLUSH. L’attente observée par le pilote comprend la reprise d’exécution et le traitement des interruptions. Sa durée ne suffit pas à accuser le SSD.

## Application UTM, allocation et barrières

Sur les **60 sauvegardes instrumentées en politique ordinaire**, le délai médian a été de 471,551 ms, p95 de 870,718 ms et p99 de 992,339 ms. Sur **15 sauvegardes avec l’ELF ordinaire, 14 ont respecté le seuil**. Le minimum est de 133,816 ms, la médiane de 612,207 ms et p95/p99 de 1 149,587 ms. Le relevé complet comprend chaque horodatage, la confirmation, les compteurs par liaison, par opération de bloc et les bornes de chaque échantillon vCPU. [Mesures individuelles et percentiles par cellule](validation/es-v0.16.6-vcpu-utm.json)

### Grande insertion combinée, secteurs de 4 096 octets, QCOW2 clairsemé

Les latences sont en millisecondes. Les parts de CPU sur les cœurs P couvrent tous les threads du moteur. Les valeurs individuelles suivent l’ordre des répétitions.

| Charge | Sonde invitée | n | Commande → succès, valeurs individuelles | p50 | p95 | p99 | CPU sur cœurs P par répétition (%) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Repos | oui | 3 | 143,250 / 699,505 / 940,457 | 699,505 | 940,457 | 940,457 | 100,0 / 0,0 / 0,0 |
| Repos | non | 3 | 684,684 / 674,728 / 564,720 | 674,728 | 684,684 | 684,684 | 0,0 / 0,0 / 0,0 |
| CPU | oui | 3 | 397,731 / 531,609 / 615,924 | 531,609 | 615,924 | 615,924 | 0,0 / 0,0 / 0,0 |
| CPU | non | 3 | 623,267 / 176,392 / 169,524 | 176,392 | 623,267 | 623,267 | 0,0 / 71,5 / 76,4 |
| Mémoire | oui | 3 | 156,972 / 859,251 / 781,863 | 781,863 | 859,251 | 859,251 | 93,9 / 0,0 / 0,0 |
| Mémoire | non | 3 | 857,792 / 150,353 / 133,816 | 150,353 | 857,792 | 857,792 | 0,0 / 92,6 / 100,0 |
| Stockage | oui | 3 | 148,540 / 737,490 / 153,265 | 153,265 | 737,490 | 737,490 | 96,9 / 0,0 / 96,6 |
| Stockage | non | 3 | 152,455 / 1149,587 / 137,329 | 152,455 | 1149,587 | 1149,587 | 97,2 / 0,0 / 100,0 |

### Comparaison des allocations en 512 et 4 096 octets

Chaque cellule indique la médiane et le maximum en millisecondes, suivis de son effectif.

| Charge | 512 clairsemé | 512 préalloué | 4096 clairsemé | 4096 préalloué |
| --- | --- | --- | --- | --- |
| Repos | 135,600 / 654,675 (n=3) | 826,066 / 870,718 (n=3) | 699,505 / 940,457 (n=3) | 137,618 / 828,744 (n=3) |
| CPU | 173,511 / 479,754 (n=3) | 169,656 / 515,011 (n=3) | 531,609 / 615,924 (n=3) | 176,240 / 578,708 (n=3) |
| Mémoire | 153,619 / 720,665 (n=3) | 148,709 / 650,949 (n=3) | 781,863 / 859,251 (n=3) | 734,065 / 920,741 (n=3) |
| Stockage | 760,816 / 992,339 (n=3) | 163,136 / 729,670 (n=3) | 153,265 / 737,490 (n=3) | 514,868 / 762,455 (n=3) |

La cartographie de l’export produit par UTM avant le premier démarrage, vérifiée avec `qemu-img` 11.1.3, confirme l’allocation logique sur 72 des 75 disques. Dans les trois premiers passages, la protection du conteneur macOS a empêché la lecture directe du paquet importé et le banc ne réalisait pas encore cet export. Le QCOW2 préalloué couvre l’intégralité des 81 788 928 ou 295 698 432 octets virtuels. En clairsemé, la couverture est de 7 208 960 octets pour les secteurs de 512 et de 7 274 496 à 7 340 032 octets en secteurs de 4 096, selon l’ELF. La préallocation ne fixe ni la latence des lectures, ni le placement du thread, ni le coût futur d’une écriture APFS. Aucune direction de gain systématique ne ressort des quatre profils et des deux géométries.

En politique ordinaire, les 15 passages hors sonde disposent tous de compteurs par niveau de cœur. Les quatre passages avec au moins 90 % du CPU du vCPU sur les cœurs de performance ont pris **133,816–152,455 ms**, avec une priorité effective initiale de 31. Les neuf passages avec au moins 90 % sur les cœurs d’efficacité ont pris **564,720–1 149,587 ms**, avec une priorité effective initiale de 4. Les deux autres ont pris 169,524 et 176,392 ms. Le compteur distingue les cœurs pour 70 des 75 essais, et les instructions et cycles par niveau pour 63. Les premières exécutions n’enregistraient pas encore ces champs. Les bornes de temps CPU et le compteur de cœurs P du processus complet sont disponibles dans les 75 cas.

La charge de douze travailleurs CPU a porté l’activité globale à 80,065–94,413 % sur les fenêtres disponibles. La charge mémoire a exercé deux zones de 256 Mio avec une pression normale et sans nouvel événement de compression ni de swap. Le compteur de décompression a progressé de deux pages sur une seule fenêtre. La charge de stockage a atteint 7 834,70 Mio d’E/S globales pendant l’intervalle le plus lent. Les temps extrêmes sont possibles sans saturation globale du CPU. L’exécution dépend aussi du type de cœur affecté et du travail exécuté pendant la complétion des E/S.

Le contre-exemple à une explication par la seule allocation se trouve en secteurs de 4 096 sur un QCOW2 entièrement préalloué, sans charge ajoutée. Les trois répétitions du même scénario ont donné **136,696 / 828,744 / 137,618 ms**. Les compteurs du seul vCPU donnent respectivement environ **130 / 678 / 130 ms de CPU**, avec 3,054 / 3,105 / 3,058 milliards d’instructions hôtes. La deuxième répétition a utilisé exclusivement les cœurs d’efficacité, la première et la troisième presque exclusivement les cœurs de performance. Le temps écoulé des deux phases SHA est passé de 58,558 à 328,431 puis 57,829 ms. Les FLUSH de la deuxième répétition ont pris 6,576 et 0,293 ms. La différence touche l’exécution du calcul et la disponibilité du vCPU, même pour un disque et un contenu identiques.

Les 60 sauvegardes instrumentées en politique ordinaire donnent 41 à 57 échanges, contrôles de capacités et du prêt inclus (37 à 47 allers-retours et dons acquittés avant ces contrôles). L’indicateur additionnant aussi les copies locales du prêt monte à 65 opérations. Ces copies synchrones n’impliquent pas d’aller-retour entre services. Chaque publication a envoyé deux FLUSH. Les temps des requêtes FLUSH mesurés par le pilote sont pour F1 **2,798 / 7,879 / 19,007 ms** et pour F2 **0,150 / 0,385 / 0,618 ms** (p50/p95/p99). Le maximum de 19,007 ms sur F1 ne suffit pas à expliquer les écarts de centaines de millisecondes. Le cumul des attentes VirtIO `kick` → observation de `used` donne 31,953 / 83,731 / 199,454 ms. [Convention de comptage](es-v0.16.6-dma.md#publication-et-méthode-de-mesure)

La sauvegarde instrumentée la plus lente a pris 992,339 ms sur un disque clairsemé en 512 octets sous charge de stockage. Le temps avant confirmation était de 56,155 ms, contre 936,185 ms après confirmation. Le vCPU a comptabilisé 687,241 à 710,953 ms de CPU. Les requêtes F1 puis F2 ont pris 15,131 et 0,269 ms. L’attente VirtIO cumulée était de 76,259 ms et l’ordonnanceur invité a sélectionné idle pendant 184,635 ms. Ces deux durées peuvent se chevaucher. L’écart entre le temps de commande et le CPU comptabilisé est de 281,386 à 305,098 ms. Les deux instantanés hôte encadrants indiquent une pression mémoire normale, zéro swap et une activité globale de 6 543,50 Mio sur `disk0`, qui comprend la charge artificielle. Le pic de la session invitée était de 9 736 cadres de 4 Kio (38,03 Mio) pour la grande édition mixte. Les deux générations ont été vérifiées après redémarrage.

### Dépassement reproduit avec l’ELF ordinaire

Le deuxième essai en secteurs de 4 096 sur QCOW2 clairsemé, avec charge de stockage, a pris **1 149,587 ms**, dont 46,716 ms avant `OUI` et 1 102,872 ms jusqu’au succès après `OUI`. Sur la fenêtre de la commande, le compteur échantillonné du vCPU est compris entre **1 012,887 et 1 025,990 ms de CPU**. Une deuxième API Darwin, aux bornes immédiatement extérieures, donne 1 026,001 ms sur le seul niveau *Efficiency*, zéro sur *Performance*, 4,118 milliards d’instructions hôtes et 1,175 milliard de cycles. Les priorités effective et de base du vCPU valaient 4 aux deux bornes. Même la borne basse du temps CPU dépasse le seuil à elle seule. L’écart entre durée écoulée et CPU comptabilisé vaut 123,597 à 136,700 ms. Le plus long intervalle intérieur échantillonné a duré 12,535 ms, et le plus long intervalle avec zéro CPU comptabilisé a duré 12,530 ms. Cette série ne montre pas de suspension continue de plusieurs centaines de millisecondes. Les événements exacts de préemption ou de blocage à l’intérieur des intervalles restent inconnus.

Le nœud `drivecible` a enregistré 143 lectures (253,182 ms cumulées), 45 écritures (23,075 ms) et deux FLUSH (3,139 ms). La charge artificielle partageait le disque hôte et a réalisé 1 970 écritures synchronisées de 4 Mio pendant son activation, qui encadre la commande. Le compteur global `disk0` a progressé de 7 834,70 Mio entre instantanés. L’activité CPU globale était de 11,314 %, l’indicateur de pression mémoire de 1 aux deux bornes et le delta de swap nul. Dans le pilote utilisé, `Queue::completed` attend par scrutation l’index `used`, puis `finish` vérifie la livraison de l’IRQ, les défauts de l’IOMMU et le statut. Une complétion retardée peut ainsi consommer simultanément des instructions TCG et du temps de service du backend. [Scrutation bornée](../kernel/src/hardware/block/io.rs) · [File VirtIO](../kernel/src/hardware/block/queue.rs) · [Chemin multipage de la fixture](../tests/fixtures/io-0166-virtio-multipage.patch)

Le découpage invité par phase et les deux latences F1/F2 individuelles sont indisponibles sur cet ELF. On ne peut pas imputer un nombre précis de cycles à la boucle ou aux SHA durant ce passage. Après redémarrage, les générations 68 et 69 étaient vérifiables. Le SHA-256 de la destination relue est `c82aa6dfd3dcce1bb85d1fe60dec9073ea80ae37a0465c152d7c7d41fd5d6a25`.

## Politique du moteur UTM sous charge CPU

Douze nouvelles VM ont exécuté la grande édition combinée, en 4 096 octets sur QCOW2 clairsemé, avec les douze travailleurs CPU. Le banc a alterné deux demandes `taskpolicy` sur le seul moteur de la VM identifiée par son UUID. Le mode `-b` demande l’arrière-plan, `-B` en demande la sortie. Les durées ci-dessous sont en millisecondes. [Compte rendu individuel des douze VM et percentiles](validation/es-v0.16.6-vcpu-politique.json)

| Demande | ELF | Trois répétitions | p50 | p95 | p99 | CPU du vCPU sur cœurs P par répétition |
|---|---|---|---:|---:|---:|---|
| `-b` | Ordinaire | 496,445 / 464,619 / 611,307 | 496,445 | 611,307 | 611,307 | 0 / 0 / 0 % |
| `-B` | Ordinaire | 436,616 / 170,826 / 173,941 | 173,941 | 436,616 | 436,616 | 0 / 78,4 / 82,4 % |
| `-b` | Instrumenté | 585,468 / 834,277 / 489,434 | 585,468 | 834,277 | 834,277 | 0 / 0 / 0 % |
| `-B` | Instrumenté | 180,774 / 171,886 / 168,257 | 171,886 | 180,774 | 180,774 | 77,4 / 83,8 / 94,5 % |

En arrière-plan, les six vCPU sont restés sur les cœurs d’efficacité et leur priorité effective initiale était 4. Après demande de sortie d’arrière-plan, cinq des six ont commencé à la priorité 31. Le sixième a gardé la priorité 4 et s’est exécuté entièrement sur les cœurs d’efficacité. Les six sauvegardes instrumentées ont chacune compté 56 échanges avec contrôles. Les douze ont redémarré et relu la source et la destination. Ce témoin exerce la politique processus, qui peut agir sur le CPU et les E/S. Il ne permet pas d’imposer le placement d’un thread ni d’isoler à lui seul une causalité matérielle.

## Les quatre dépassements antérieurs

Les temps sont en millisecondes sur des démarrages indépendants. Le chronomètre englobait la saisie de la commande, l’invite, la réponse automatique et la publication durable. Les quatre destinations ont été relues en génération 69. [Relevés individuels de l’audit CPU](validation/es-v0.16.6-cpu.json)

| Modification d’une source de 4 Mio | Secteur | QCOW2 | Commande → OUI | OUI → succès | Total |
|---|---:|---|---:|---:|---:|
| Insertion de 4 Mio et éditions multiples | 4 096 | Clairsemé | 84,480 | 918,268 | 1 002,748 |
| Insertion de 4 Mio et éditions multiples | 4 096 | Préalloué | 83,580 | 993,434 | 1 077,014 |
| Insertion de 4 Mio au milieu | 512 | Clairsemé | 85,479 | 997,719 | 1 083,198 |
| Insertion de Ω en tête | 512 | Clairsemé | 21,361 | 2 675,626 | 2 696,987 |

Dans les trois grandes insertions, le cumul des deux phases a franchi le seuil. Pour l’insertion de Ω, 99,2 % du délai a suivi le consentement. Les quatre démarrages historiques n’avaient ni la sonde de thread, ni un relevé par FLUSH sur l’exécution lente. Une mesure ultérieure du même scénario ne permet pas d’attribuer rétrospectivement ces quatre événements à un composant précis.

## Intégrité et régressions

Les 91 commandes du banc ont terminé avec un code zéro. Les 20 cibles NovaOS par architecture incluent 25 démarrages, 24 groupes bloc, trois contrôles ELF/processus/IPC et quatre scénarios CLI. Les profils à 128 et 256 Mio ont démarré sur les deux architectures. À 64 Mio, le firmware a empêché le démarrage sur les deux architectures. Le journal distingue cet essai non exécuté des contrôles réussis. [Commandes, résultats, SHA-256 des journaux et des ELF](validation/es-v0.16.6-vcpu-regressions.json)

- **Concurrence et révocation.** Quatre séries de concurrence, 36 interruptions et 20 essais de défaut ou de révocation sur QEMU couvrent AArch64 et x86_64, secteurs 512 et 4 096. Les volumes fragmentés, le refus sur saturation et la file de huit entrées ont été exercés. Les bancs AArch64 ont rejoué les 11 scénarios GICv3 et les 23 de DMA séquentielle.
- **Reprise sur QCOW2 dans UTM.** Six arrêts forcés de la VM, après préparation des pages, à F1 et à F2, dans les deux géométries. Aucun acquittement à l’appelant avant arrêt. Après redémarrage, la source était intacte dans tous les cas. À la préparation des pages et à F1, le remontage a validé 66/67 avec destination absente. À F2, il a validé 68/69 avec destination complète. Le Mac est resté alimenté pendant ces essais. La persistance après perte physique d’alimentation reste hors de cette qualification.
- **NovaFS.** Par architecture Darwin (ARM64 natif, x86_64 sous Rosetta), 196 tests standards et 199 instrumentés réussis, six ignorés dans chacune de ces suites. Les 2 304 injections historiques, les 486 injections de publication groupée et le million d’opérations différentielles ont été rejoués. Les échanges P9, F01 et LZ4 entre architectures et les compilations du cœur portable sur les cibles nues ont réussi.

## Arbitrage d’intégration

Le plafond de 64 échanges mesurés sur le chemin de sauvegarde est respecté. Le délai de publication durable inférieur à une seconde dans UTM reste **non qualifié**. Le cas reproduit dépasse le seuil par son seul coût CPU sur les cœurs d’efficacité. La comparaison à allocation et contenu identiques montre un deuxième régime beaucoup plus rapide sur les cœurs de performance. Des attentes supplémentaires du backend et de l’invité apparaissent selon la charge. La répartition de ces attentes entre file prête macOS, traitement QEMU et temps matériel d’APFS ou du SSD reste partiellement indéterminée. Les mesures historiques n’identifient pas individuellement la cause des quatre dépassements antérieurs.

Conserver le prototype isolé sur sa branche. Pour viser un délai systématique, qualifier en priorité une comparaison entre la scrutation actuelle et une attente de complétion signalée par l’IRQ sur le même profil TCG, sous charge de stockage. Elle devra garder une borne de temps, la validation indépendante de `used`, du statut et de la révocation, ainsi que l’ordre F1/F2. Quantifier séparément les octets de texte revisités et les recherches d’extents avant de modifier le moteur. Un réglage de priorité ou une préallocation seule ne fournit pas d’engagement sur les temps de réponse. Cette campagne porte sur les bancs et les sondes du prototype, sans changement de production ni de format P9.

## Reproduction

Utiliser le protocole de construction du prototype CPU pour les deux ELF AArch64 UTM. Dans l’archive instrumentée, ajouter en dernier la fixture `io-0166-vcpu.patch` puis reconstruire la cible `aarch64-unknown-none-softfloat`, en release avec `systeme-gpt`. Les deux ELF utilisés ont pour SHA-256 `27f221da5570d2b156c1220175dfc6a84baad25df2b68b53e8ea5a09074eb16b` (ordinaire) et `113f1930e26fef6258daf0af22fd3821a2556b69993ac7c9202fb26dbe99a480` (sonde). Le générateur est l’exemple NovaFS `es-groupees` sur ARM64 Darwin. [Construction et fixtures CPU](es-v0.16.6-cpu.md#reproduire-une-cellule) · [Sonde de l’ordonnanceur et des deux FLUSH](../tests/fixtures/io-0166-vcpu.patch)

```sh
patch -d "$ARCHIVE_SONDE" -p1 -F0 -i "$PWD/novaOS/tests/fixtures/io-0166-vcpu.patch"
(cd "$ARCHIVE_SONDE/novaOS" && RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --locked --offline --release --target aarch64-unknown-none-softfloat --features systeme-gpt -p novaos)
python3 -B novaOS/tests/vcpu_campagne.py --hors-sonde-charge "$PROFIL_UTM" "$ELF_SONDE" "$ELF_ORDINAIRE" "$GENERATEUR" "$SORTIE/utm"
python3 -B novaOS/tests/vcpu_analyse.py --sortie "$SORTIE/utm.json" "$SORTIE/utm"
```

Le témoin de politique alterne, sous la même charge CPU, `--arriere-plan` et `--sortie-arriere-plan` sur le PID du moteur de la seule VM jetable. Pour le reproduire, utiliser le scénario `ajout-4m-multiple` en 4 096 octets, clairsemé, répéter trois fois avec chaque ELF et varier l’ordre des deux politiques. Passer `--sans-sonde` uniquement à l’ELF ordinaire.

```sh
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF_SONDE" "$GENERATEUR" "$SORTIE/temoin-bg-1" 4096 --cas ajout-4m-multiple --profil-charge cpu --arriere-plan
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF_ORDINAIRE" "$GENERATEUR" "$SORTIE/temoin-hors-bg-1" 4096 --cas ajout-4m-multiple --profil-charge cpu --sortie-arriere-plan --sans-sonde
```

Lancer les essais à partir d’un répertoire de résultats neuf. Les quatre profils sollicitent uniquement des fichiers et VM de test. Le post-traitement conserve chaque horodatage Darwin et chaque résultat en octets relus. Il calcule les percentiles empiriques au rang supérieur. Dans une cellule de trois répétitions, p95 et p99 désignent tous deux le maximum observé.
