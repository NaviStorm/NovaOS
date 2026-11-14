# NovaOS v0.17.0 — Latence IPC à un, deux et quatre vCPU

À quatre vCPU, la latence médiane d’un couple diminue de 59,5 % sur AArch64 et de 33,7 % sur x86_64. Avec quatre couples épinglés, le débit global x86_64 recule de 12,6 %. Ce dernier résultat maintient la qualification sur la branche SMP.

## Périmètre et mesures de référence

Le candidat de développement utilise `codex/smp-0170`. La référence système est `43be64a31178155ca8c1db4218de96e88cf5a410`. La référence instrumentable `f356970` ajoute le banc aux mêmes chemins d'exécution. Les relevés individuels associent chaque démarrage à l'empreinte du noyau. [Mesures individuelles et synthèse](validation/ipc-smp-phase3-v0.17.0.json) · [Qualification précédente](smp-phase2-v0.17.0.md)

La phase 2 mesurait réellement un couple, deux canaux, des messages de 128 octets et 1 000 allers-retours à chaque topologie. Le contrôleur lançait ensuite des lectures de la mémoire des acteurs avec des cessions du CPU jusqu'à leur réponse. Il participait ainsi à la contention qu'il mesurait. Les compteurs de garde incluaient les tests précédents de calcul et de création des processus. Les valeurs historiques 97,924 / 224,980 / 528,086 µs sur ARM et 61,211 / 104,999 / 193,741 µs sur x86 sont des moyennes **par tour** dont le tableau de phase 2 donne la médiane entre répétitions.

La reprise à l'identique du banc historique a donné les médianes suivantes. Chaque cellule contient trois démarrages QEMU distincts. L'ordre des topologies alterne. [Banc historique](../kernel/src/sched/process/ipc/probes/perf.rs)

| µs par tour | 1 vCPU | 2 vCPU | 4 vCPU |
|---|---:|---:|---:|
| AArch64 avant | 92,408 | 172,274 | 365,299 |
| AArch64 après | 92,720 | 158,562 | 159,624 |
| x86_64 avant | 60,309 | 87,079 | 143,402 |
| x86_64 après | 64,104 | 93,963 | 96,918 |

## Protocole de phase 3

Chaque couple utilise deux processus au niveau utilisateur, deux canaux et les appels natifs ordinaires. Il échange 1 000 requêtes et 1 000 réponses de 128 octets. Chaque réception vérifie le type, la longueur et le tag. Les tests de contrat vérifient séparément les octets. La préparation des capacités et la collecte finale sont hors fenêtre. Le contrôleur s'endort sur la fin du client. Chaque client enregistre sa sortie à la dernière réponse. L'horloge invitée va du départ commun jusqu'à la dernière sortie. Les valeurs par couple incluent donc le délai initial de mise en exécution. Le temps hôte entre marqueurs série inclut le retour du contrôleur et la collecte du flux par le lecteur. Il peut subir la mise en tampon de la sortie. Aucune de ces deux horloges ne donne le temps CPU exclusif de l'invité. [Acteurs](../user/ipc.rs) · [Contrôleur](../kernel/src/sched/process/ipc/probes/multicore.rs) · [Lanceur](../tests/ipc_multicoeur.py)

- **A, charge constante** — Un couple, migration autorisée sur tous les vCPU, sans travailleur supplémentaire.
- **B, charge proportionnelle** — Un couple épinglé par vCPU. Le débit global divise le nombre total d'allers-retours par le temps de fin du dernier client. Les distributions de chaque couple sont conservées séparément.
- **C, affinités** — Les deux acteurs sont sur CPU0, sur CPU0 et CPU1 respectivement, ou libres. Le cas distant commence à deux vCPU.
- **D, charge indépendante** — Un couple libre partage la machine avec un travailleur de calcul, d'allocation/libération de pages de 4 Kio, ou de montage/consultation/démontage NovaFS. Le travailleur disque est attaché au BSP. Le volume de test est ouvert en lecture seule et son empreinte est contrôlée après extinction. La fenêtre VFS attend deux soumissions bloc du montage suivant. Le nombre de soumissions durant chaque lot permet de vérifier le recouvrement même lorsque la session se termine après le client IPC.

Le Mac exécute macOS 27.2 (26B123). Les invités QEMU 11.1.3 utilisent TCG, 256 Mio, `virt` Cortex-A72/GICv2 ou `q35` x86 `max`. Les résultats couvrent les vCPU émulés. Les timers de préemption tournent à 250 Hz. Le compteur ARM est à 62,5 MHz et le TSC x86 est étalonné à chaque démarrage. La série zéro chauffe l'invité. Les sept suivantes entrent dans les statistiques. Les quantiles p95/p99 de lots sont au rang supérieur, la médiane est la moyenne des deux valeurs centrales si nécessaire. L'IQR est la différence entre les rangs 75 et 25 %. Les queues de distribution des lots de 1 000 tours et celles des messages individuels ont deux unités d'observation distinctes. La charge hôte (1/5/15 minutes) est archivée par démarrage. L'affectation physique des threads TCG aux cœurs P/E et la durée de chaque préemption par macOS restent inconnues.

## Diagnostic et sûreté

1. **Durée de vie des attentes.** Le dernier retrait d'une page de slots libérait son adresse virtuelle. L'attente suivante recréait la traduction et ses invalidations sous la garde de l'ordonnanceur. Le témoin de phase 3 compte les allocations et restitutions de pages. Le pool des 32 attentes peut maintenant détenir deux cadres privés au maximum, alloués à la demande dans l'alias direct superviseur. Un retrait efface le slot, la collecte rend le cadre dès que sa dernière attente disparaît et une nouvelle allocation le remet à zéro. Les compteurs de pages gardent le sens « attribution/restitution » dans les deux versions. Seule l'ancienne version devait modifier une PTE à chaque attribution et retrait. Les autres pools et les quotas conservent leur contrat. [Pool](../kernel/src/sched/process/ipc/pool.rs) · [Allocation des cadres](../kernel/src/memory/frames.rs)
2. **Changements d'espace ARM.** Chaque processeur utilise son propre ASID, dérivé de son indice, avec une racine basse vide de transition et des feuilles utilisateur non globales. Le recyclage de cet ASID invalide désormais les traductions sur ce processeur. Les modifications de PTE et la restitution des cadres conservent l'invalidation du domaine de partage et ses barrières. La sonde TLB compte les invalidations partagées instrumentées. Cette propriété suppose que chaque ASID reste propre au PE qui l'emploie. [Pagination AArch64](../kernel/src/arch/aarch64/paging.rs) · [Architecture des traductions Arm](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/LearnTheArchitecture-MemoryManagement-101811_0100_00_en.pdf)
3. **Réveils distants.** L'ancien réveil diffusait l'IPI aux pairs quand un CPU admissible était idle. La nouvelle sélection privilégie le dernier CPU s'il est idle et autorisé, puis le premier autre CPU admissible et idle. GICv2 utilise le bit de cible matériel lu sur chaque CPU, GICv3 l'affinité du PE, xAPIC l'ID LAPIC. GICv3 conserve la diffusion de repli pour un Aff0 hors plage gérée. L'IPI invite à reconsulter la FIFO. L'élection revérifie le masque sous la garde. Plusieurs réveils peuvent viser le même CPU avant sa commutation. Le timer continue de faire progresser les prêts. [Ordonnanceur](../kernel/src/sched/mod.rs) · [GICv2](../kernel/src/arch/aarch64/gicv2.rs) · [GICv3](../kernel/src/arch/aarch64/gicv3.rs) · [xAPIC](../kernel/src/arch/x86_64/lapic.rs)
4. **Garde du noyau.** L'entrée d'appel natif valide l'identité et marque le processus en appel système dans la même acquisition. Le propriétaire seul manipule la profondeur de récursion avec les IRQ locales masquées. Les instructions atomiques de lecture-modification-écriture pour cette profondeur sont remplacées par des lectures et écritures ordinaires atomiques relaxées. Le transfert de propriété conserve l'acquisition/libération et l'ordre du changement de pile. La boucle d'attente traite les arrêts et demandes d'invalidation. Les mesures intermédiaires portent sur des ensembles de changements. Elles ne permettent pas de répartir précisément les gains entre instructions. [Garde](../kernel/src/sched/gate.rs) · [Entrée native](../kernel/src/sched/process/syscall.rs)
5. **Défaut x86 découvert avec le disque.** En mono-vCPU, l'espace utilisateur pouvait naître avant le premier mappage du BAR VirtIO. Son PML4 copiait alors une entrée haute vide. Une IRQ disque ultérieure lisait le registre ISR à `0xffffc90000121000` dans cet espace et déclenchait une faute en superviseur. Le démarrage SMP créait plus tôt la branche MMIO pour le LAPIC, ce qui expliquait la différence entre topologies. Le noyau réserve désormais la table supérieure de la fenêtre MMIO avant de créer un processus. Elle reste superviseur, initialement sans feuille. Chaque pilote vérifie et mappe ses périphériques comme auparavant. Le montage du témoin x86 à un CPU échoue avant la première série. Les résultats après correction couvrent effectivement cette cellule. [Tables et propriété des branches](../kernel/src/memory/tables.rs) · [Initialisation x86](../kernel/src/arch/x86_64/paging.rs)

La FIFO partagée parcourt au plus les slots des threads, sélectionne le premier prêt admissible et donne la priorité aux prêts sur idle. Elle reste sous la même garde que la table de capacités, les endpoints, les réservations, la publication et la génération du dormeur. Les accès utilisateurs vérifient handle local, droits, longueur et adresse. La livraison revérifie l'autorité. Le décès et la révocation publient un résultat terminal unique. La collecte vérifie les slots actifs avant restitution. L'allocation des pages conserve l'ordre garde d'ordonnancement puis allocateur. Les chemins de mise en attente et de commutation gardent les IRQ masquées pendant le transfert de propriété. Le balayage des attentes pour choisir un offreur ou un receveur, les copies bornées et la sérialisation de plusieurs couples restent des coûts internes. Le banc ne sépare pas précisément les délais dus à TCG, aux interruptions émulées et à l'ordonnancement de macOS. [IPC et attente](../kernel/src/sched/process/ipc/traffic.rs) · [Invariants](ipc-security.md)

## Résultats IPC

La campagne appariée a alterné le témoin et le candidat, dans les deux ordres, après la fin des autres campagnes du projet. Chaque cellule contient trois démarrages et 21 lots mesurés par version. Les VM ont démarré une à une sur le même Mac. Les tâches ordinaires de macOS sont restées actives. Le tableau donne p50 / p95 / p99 des **moyennes par tour de chaque lot** à charge constante. Le débit est la médiane des débits individuels.

| Architecture | vCPU | Avant, µs/tour | Après, µs/tour | Débit avant → après, tours/s | IQR après, µs/tour | Hôte après, ms/lot p50 |
|---|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 | 55,8 / 58,8 / 58,9 | 56,2 / 58,9 / 59,4 | 17 919 → 17 806 | 1,0 | 56,5 |
| aarch64 | 2 | 86,5 / 91,2 / 91,9 | 67,6 / 69,1 / 69,4 | 11 558 → 14 802 | 2,5 | 68,2 |
| aarch64 | 4 | 171,1 / 191,2 / 214,6 | 69,3 / 73,3 / 75,1 | 5 843 → 14 427 | 1,9 | 70,2 |
| x86_64 | 1 | 41,1 / 42,0 / 42,2 | 41,8 / 42,6 / 42,8 | 24 304 → 23 898 | 1,0 | 42,1 |
| x86_64 | 2 | 51,4 / 52,6 / 55,5 | 56,4 / 59,0 / 60,4 | 19 437 → 17 742 | 2,2 | 56,7 |
| x86_64 | 4 | 84,8 / 92,0 / 93,4 | 56,2 / 58,5 / 59,0 | 11 790 → 17 780 | 1,8 | 57,5 |

| Architecture | Gain médian à 4 vCPU | Rapport 4/1 avant → après | Variation à 1 vCPU |
|---|---:|---:|---:|
| aarch64 | 59,5 % | 3,07 → 1,23 | 0,6 % |
| x86_64 | 33,7 % | 2,06 → 1,34 | 1,7 % |

Pour B, chaque couple livre 1 000 allers-retours et dispose d'un seul vCPU admissible. Les durées sont calculées depuis le départ commun. La dispersion entre couples et chaque durée individuelle sont archivées.

| Architecture | vCPU/couples | Débit global avant → après, tours/s | Après, µs/tour p50 par couple, ordre CPU | p99 du dernier couple, µs/tour |
|---|---:|---:|---:|---:|
| aarch64 | 1 | 17 765 → 18 106 | 55,2 | 58,9 |
| aarch64 | 2 | 12 983 → 18 330 | 109,1 / 109,1 | 114,2 |
| aarch64 | 4 | 6 833 → 15 838 | 249,5 / 247,1 / 251,2 / 252,0 | 288,3 |
| x86_64 | 1 | 24 316 → 23 737 | 42,1 | 43,5 |
| x86_64 | 2 | 21 890 → 20 404 | 96,9 / 98,0 | 100,7 |
| x86_64 | 4 | 17 187 → 15 014 | 265,6 / 264,5 / 257,4 / 246,5 | 274,2 |

Les cellules d'affinité et de charge indépendante proviennent de la matrice large. D'autres VM de qualification pouvaient être actives sur l'hôte. Deux démarrages et quatorze lots par cellule. Chaque paire de valeurs donne la médiane avant → après en µs par tour. Le témoin VFS utilise le même déclenchement après deux soumissions bloc que le candidat.

| Architecture | vCPU | Même CPU | CPU distincts | Libres | Calcul | Mémoire | NovaFS |
|---|---:|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 | 57,5 → 57,0 | s.o. | 57,8 → 56,2 | 10 000,3 → 9 961,5 | 9 935,8 → 9 953,3 | 167,5 → 165,8 |
| aarch64 | 2 | 69,3 → 59,7 | 131,4 → 96,0 | 95,9 → 74,4 | 76,3 → 60,9 | 88,1 → 60,0 | 298,9 → 128,4 |
| aarch64 | 4 | 95,2 → 58,9 | 299,2 → 105,0 | 231,7 → 71,2 | 297,9 → 75,5 | 288,6 → 73,5 | 705,2 → 192,2 |
| x86_64 | 1 | 42,1 → 43,3 | s.o. | 42,0 → 42,6 | 8 014,2 → 7 990,3 | 7 993,1 → 7 975,9 | faute MMIO → 144,9 |
| x86_64 | 2 | 46,5 → 46,2 | 81,0 → 82,9 | 58,3 → 77,8 | 46,8 → 49,2 | 50,5 → 50,9 | 133,8 → 123,6 |
| x86_64 | 4 | 42,1 → 47,3 | 147,7 → 88,4 | 105,9 → 59,3 | 124,9 → 73,0 | 127,1 → 63,5 | 241,3 → 199,0 |

Le travailleur de calcul ou de mémoire reste prêt sans céder volontairement. En mono-vCPU, le couple doit partager les quanta avec lui. Le résultat inclut cette attente d'ordonnancement. Chaque série valide la terminaison des acteurs. Le journal distingue les intervalles hôte des intervalles invité ainsi que les extrêmes sous charge.

## Instrumentation et contention

Les données suivantes proviennent des 21 lots appariés instrumentés pour chaque cellule. Les compteurs de la garde sont cumulés par CPU pendant les fenêtres actives. Le rapport utilise les différences entre deux fins de série, puis la médiane des lots. Les durées représentent des sommes sur les CPU et peuvent se recouvrir. Le maximum de détention par CPU et son site sont cumulatifs depuis le démarrage. Les histogrammes IPC sont réinitialisés à chaque série, avec huit classes par octave et la borne supérieure du quantile. Les nouveaux compteurs des pages et du délai prêt→élu sont compilés exclusivement dans `ipc-smp-lab`. Les sondes déjà présentes dans le noyau restent inactives en usage normal.

| Architecture | vCPU | Acquisitions avant → après | Contestées avant → après | Attente totale, ms | Détention totale, ms |
|---|---:|---:|---:|---:|---:|
| aarch64 | 1 | 20 030 → 16 026 | 0 → 0 | 0,0 → 0,0 | 51,9 → 50,7 |
| aarch64 | 2 | 24 537 → 20 142 | 8 693 → 7 116 | 84,9 → 32,0 | 127,6 → 72,2 |
| aarch64 | 4 | 33 456 → 20 391 | 21 548 → 7 792 | 524,0 → 37,9 | 263,3 → 78,6 |
| x86_64 | 1 | 20 028 → 16 026 | 0 → 0 | 0,0 → 0,0 | 36,9 → 40,2 |
| x86_64 | 2 | 24 226 → 20 205 | 6 809 → 7 585 | 15,0 → 15,4 | 50,1 → 52,8 |
| x86_64 | 4 | 32 287 → 20 280 | 16 056 → 7 747 | 107,0 → 20,2 | 71,8 → 54,7 |

Les nombres suivants sont les médianes des quantiles **des tours individuels**, chacun observé sur 1 000 réponses par série. La dernière colonne conserve le plus grand tour individuel enregistré par le candidat.

| Architecture | vCPU | p50/p95/p99 avant, µs | p50/p95/p99 après, µs | Maximum après, µs | Surcoût des sondes après, % sur le lot |
|---|---:|---:|---:|---:|---:|
| aarch64 | 1 | 61,4 / 73,7 / 90,1 | 61,4 / 73,7 / 81,9 | 186,0 | 17,0 |
| aarch64 | 2 | 131,1 / 229,4 / 245,8 | 90,1 / 106,5 / 122,9 | 395,0 | 36,0 |
| aarch64 | 4 | 294,9 / 393,2 / 458,8 | 90,1 / 114,7 / 163,8 | 301,0 | 40,8 |
| x86_64 | 1 | 44,9 / 49,0 / 65,3 | 45,1 / 53,3 / 73,5 | 489,2 | 17,7 |
| x86_64 | 2 | 53,4 / 65,7 / 82,1 | 57,5 / 65,5 / 90,1 | 619,0 | 9,3 |
| x86_64 | 4 | 73,7 / 122,8 / 180,1 | 57,2 / 73,6 / 98,1 | 689,4 | 11,9 |

| Médianes à quatre vCPU par lot | ARM avant → après | x86 avant → après |
|---|---:|---:|
| Commutations | 3 364 → 2 181 | 2 115 → 2 037 |
| Migrations | 1 337 → 168 | 269 → 41 |
| Envois IPI de réveil | 1 957 → 2 000 | 1 950 → 1 998 |
| Invalidations partagées instrumentées | 4 933 → 46 | 325 → 46 |
| Inscription → réveil cumulée, ms | 132,9 → 65,2 | 50,7 → 44,0 |
| Réveil → élection cumulée, ms | 56,7 → 30,2 | 22,2 → 18,7 |
| Attributions de page au pool | 518 → 14 | 93 → 6 |
| Ticks idle cumulés | 165 → 59 | 58 → 48 |

Les quatre lignes suivantes reprennent la plus longue garde mesurée pendant les fenêtres instrumentées à quatre vCPU. Le temps est celui de l'horloge invitée. Le site désigne l'acquisition extérieure.

| Architecture | Version | CPU | Maximum de détention, ms | Site de la garde |
|---|---|---:|---:|---|
| aarch64 | avant | 1 | 6,264 | `kernel/src/sched/mod.rs:445:17` |
| aarch64 | apres | 1 | 7,391 | `kernel/src/sched/process/syscall.rs:157:5` |
| x86_64 | avant | 2 | 4,962 | `kernel/src/sched/mod.rs:445:17` |
| x86_64 | apres | 1 | 4,776 | `kernel/src/sched/process/syscall.rs:94:25` |

Un envoi IPI est compté au CPU émetteur. Une diffusion et une notification ciblée comptent chacune pour un envoi. Le nombre de destinataires matériels a donc changé même dans les cellules où le nombre d'envois a augmenté. Les maxima individuels du verrou et les sites d'appel de chaque CPU figurent dans les données brutes. La garde englobe les opérations appelées depuis ce site. Un long intervalle peut comprendre la désélection du vCPU par l'hôte. Le temps d'inscription IPC cumule les attentes de chacun des deux partenaires. L'histogramme `livraison` couvre le dépôt jusqu'au receveur, `appel` inclut l'attente de l'appelant, et `reprise` couvre le chemin de reprise instrumenté. Le compteur supplémentaire `pret_ns` va précisément du passage à READY à la sélection du TCB. Ces durées se chevauchent.

Les essais de mise au point sans sonde, sept lots à quatre vCPU par étape, donnent 201,75 → 100,76 → 72,19 µs/tour sur ARM et 94,18 → 79,95 → 55,22 µs/tour sur x86. La première étape associait les cadres directs, l'ASID local et l'entrée d'appel regroupée. La deuxième ajoutait le ciblage. Ces essais servent au diagnostic. La comparaison appariée fournit les valeurs de décision. Les captures de stockage et les tests fonctionnels utilisent des noyaux distincts des noyaux de chronométrage.

### Verrou avec un couple épinglé par vCPU

Le banc `paires-sonde` conserve la charge de B et instrumente trois démarrages par cellule et par version, soit 21 lots. Les compteurs de garde couvrent tous les CPU. L’histogramme d’aller-retour suit le premier couple sur CPU0 et valide 1 000 réponses par lot. Les compteurs démarrent avant le lancement des acteurs et s’arrêtent après que le contrôleur a annoncé leurs durées sur la console. Le chronomètre de la dernière colonne s’arrête à la sortie du dernier client. Ces bornes expliquent aussi pourquoi la détention cumulée peut dépasser cette durée. Les deux colonnes de garde donnent des ms cumulées par lot.

| Architecture | vCPU | Acquisitions | Contestées | Attente, ms | Détention, ms | Invalidations partagées | Lot instrumenté, ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| aarch64 | 2 | 40 652 → 32 292 | 26 613 → 23 880 | 175,0 → 125,2 | 198,0 → 147,4 | 4 104 → 88 | 218,3 → 160,5 |
| aarch64 | 4 | 82 046 → 88 777 | 80 873 → 86 986 | 2 032,1 → 1 158,4 | 753,3 → 435,9 | 8 195 → 174 | 718,6 → 432,9 |
| x86_64 | 2 | 40 492 → 32 507 | 24 966 → 23 007 | 94,0 → 99,7 | 100,0 → 106,2 | 91 → 88 | 104,9 → 110,7 |
| x86_64 | 4 | 82 532 → 66 254 | 75 280 → 55 753 | 757,3 → 798,1 | 296,6 → 309,6 | 180 → 174 | 262,1 → 275,5 |

À quatre vCPU x86_64, les acquisitions diminuent de 19,7 %, mais l’attente cumulée augmente de 40,8 ms et la détention de 13,0 ms par lot. Le rapport entre détention cumulée et acquisitions passe d’environ 3,59 à 4,67 µs. Les invalidations passent de 180 à 174. Le même compteur passe de 8 195 à 174 sur ARM. La forte réduction des invalidations ARM explique ainsi une différence entre architectures. Sur x86_64, les compteurs localisent le surcoût sous charge proportionnelle dans la durée des sections et l’attente des concurrents. La part exacte de chaque parcours IPC et des désélections du vCPU hôte reste ouverte.

## Mémoire, intégrité et régressions

Sur des invités de 128 Mio à quatre vCPU, la mesure résidente au repos est passée de 796 à 794 pages sur ARM et de 853 à 856 sur x86. En saturation, elle est passée de 1 277 à 1 275 et de 1 334 à 1 337 respectivement. L'IPC atteint 71 pages sur les deux architectures et revient à zéro après collecte. Le nombre de cadres libres retrouve la valeur de repos. Le pool conserve au plus deux cadres de 4 Kio pour les attentes et deux handles noyau. Sur x86 mono, la première branche MMIO coûte une table de 4 Kio dès le démarrage. En SMP elle dessert également le LAPIC. Les écarts de pages résidentes comprennent l'image noyau et ses alignements. La sonde active réserve sept histogrammes de 256 compteurs de 32 bits, avec leurs métadonnées, le temps d'une série. [Comptabilité mémoire](../kernel/src/memory/mod.rs) · [Histogrammes](../kernel/src/sched/process/ipc/metrics.rs)

Les courses T09 exercent cinq ordres, chacun 10 000 fois sur **chaque** architecture et à 1/2/4 vCPU. La suite vérifie une seule issue terminale, les générations, l'échéance et l'unicité du réveil. Elle a enregistré 693 / 787 / 406 préemptions sur ARM et 575 / 770 / 227 sur x86. Les autres scénarios exercent la saturation, les transferts et la révocation de capacités, la destruction du destinataire, la mort du propriétaire, l'annulation, le timeout, la réutilisation d'identifiants et les refus d'accès. Le chemin de publication est indivisible sous sa garde. Les points d'injection sont situés aux frontières où une préemption est possible. T17 a également réussi sur chaque architecture à quatre CPU, avec 60 séries mesurées, 13 groupes d'histogrammes, un producteur concurrent et un travailleur de calcul. [Contrats des tests IPC](ipc-test-plan.md) · [Campagne T09/T17 et endurance](../tests/ipc.py)

À quatre vCPU, l'endurance IPC a réalisé **29 924 cycles en 300,191 s sur AArch64** (62 481 cadres libres à chaque mesure) et **27 575 en 300,992 s sur x86_64** (62 994 cadres libres). L'endurance de l'ordonnanceur a duré 63,799 et 63,707 s respectivement. Chacune a retrouvé le nombre de cadres libres et l'occupation initiale du tas. Le producteur et le travailleur du test T17 ont progressé sur les deux cibles, avec zéro erreur et 20 740 / 17 674 préemptions. Ce sont des observations sur les ordonnancements exercés. La garde inter-CPU actuelle n'impose aucune borne théorique de famine.

Le banc de démarrage historique a passé 25 scénarios par architecture. Le démarrage SMP et les invalidations distantes ont passé à 1/2/4 CPU. Les tests d'arrêt couvrent arrêt normal, panique, secondaire absent et CPU muet, avec contrôle des acquittements. Le profil FP expérimental a passé FP/NEON sur ARM GICv2 et GICv3, x87/SSE2 et AVX sur le modèle QEMU Intel compatible XSAVE et la voie x87/SSE2 sur le modèle privé d'AVX/XSAVE. Les clients vérifient la préemption, la migration lorsque plusieurs CPU sont actifs et le réemploi du slot. Le profil distribué reste softfloat. Les résultats AVX portent sur ce modèle QEMU. Les extensions AVX-512, SVE et SME restent hors périmètre. [Campagne FPU et arrêts](../tests/smp_phase2.py) · [Limites matérielles](smp-phase2-v0.17.0.md)

Les suites ELF, cycle de vie des processus, VFS/fichiers, espaces de noms, CLI, scripts et terminal nsh, ainsi que vi, sont passées sur les deux architectures. Le banc vi sur les deux géométries a vérifié le refus concurrent, la révocation, l'intégrité des octets et la réouverture après redémarrage. Les campagnes ont suivi les révisions de la branche. Le contrôle des courses et du chemin IPC a tourné sur le noyau ordinaire, les mesures finales sur des ELF à empreinte conservée, et le disque x86 mono sur le correctif MMIO. Les noyaux du profil FPU, des coupures et du stockage ont chacun leur construction dédiée.

Sur chaque architecture, les 24 groupes bloc couvrent entre autres les deux géométries, les refus d'écriture, le retrait DMA et les erreurs. Les essais complémentaires de propriété à 1/4 CPU et 512/4 096 octets totalisent 928 injections, 512 cycles de session, vérification des deux portails révoqués et comparaison des images. Les 36 coupures QEMU groupées couvrent neuf frontières par architecture/géométrie, dont F1, F2 et l'écriture en vol. Les 36 volumes ont remonté avec relecture exacte et destination entière ancienne ou publiée selon la frontière. Ces essais laissent le Mac alimenté. Le moteur AArch64 fourni avec UTM a rejoué les 11 cas DXE (reprise comprise) et les 23 cas de politique DMA séquentielle. Le premier choix de firmware pour l'injection séquentielle appartenait à la campagne monodisque. Le rejeu avec la variante `retrait-transfert` a confirmé la quarantaine, les deux plages encore épinglées et la reprise après coupure. [Propriété DMA](../tests/dma_propriete.py) · [Coupures groupées](../tests/coupure_cow_groupee.py)

NovaFS v0.9.13 P9 a passé, en Darwin ARM64 natif et x86_64 sous Rosetta, **196 tests standards, 199 instrumentés et 1 000 000 opérations différentielles par architecture**. Chaque suite de base a six cas déclarés ignorés. Le banc d'ancres a exercé séparément 2 304 injections par architecture, avec 1 288 états anciens, 656 nouveaux et 360 refus. Les arbres `novaFS/`, `components/` et `novaOS/abi/` sont identiques à la référence de phase 2. La distribution conserve l'IPC classique et le polling bloc. Les E/S groupées requièrent la construction expérimentale explicite.

## Stockage de 4 Mio

Chaque essai insère au total 4 Mio dans une source dense de 4 Mio par plusieurs éditions. La mesure va de la commande à la confirmation durable et comprend les deux FLUSH. Le profil utilise quatre vCPU avec des secteurs de 4 096 octets. Le lecteur indépendant vérifie la destination de 8 Mio. Dans l'application UTM 5.0.8, le profil QCOW2 ForceMulticore utilise QEMU 10.0.12, GICv3 sans ITS, TPM CRB et redémarrage autonome. Les deux relevés sont conservés, y compris le premier essai UTM à 712,553 ms. L'effectif ne permet pas de prédire la queue de distribution du stockage.

| Profil groupé | Mesure initiale, ms | Confirmation finale, ms | F1 / F2 finales, ms | Échanges avec contrôles | Pic de cadres de session |
|---|---:|---:|---:|---:|---:|
| QEMU AArch64 | 227,352 | 227,209 | 2,085 / 0,427 | 56 | 9 751 |
| QEMU x86_64 | 287,973 | 349,713 | 1,671 / 0,243 | 56 | 9 754 |
| Application UTM AArch64 | 712,553 | 229,060 | 1,282 / 0,133 | 56 | 9 732 |

Le relevé donne 47 échanges, 56 avec les contrôles locaux, et 64 dans le décompte étendu incluant les copies locales du prêt. Les relevés portent sur le mode groupé. Le hash de la destination est identique dans les trois environnements. Les tests de chemins classiques, la validation de fichiers P9 et les coupures les exercent séparément. [Banc de sauvegarde](../tests/integration_main_performances.py) · [Mode par défaut](integration-v0.16.6.md)

## Débit x86_64 et décision de poursuite

À quatre vCPU, AArch64 gagne 9 005 tours/s avec quatre couples. Sur x86_64 le candidat en perd 2 173. À deux vCPU x86, la médiane d’un couple libre augmente également de 51,4 à 56,4 µs. Ces résultats dépassent la seule question de la latence à quatre vCPU. Les valeurs à affinité fixe montrent en particulier que les échanges distants profitent du ciblage, alors que le couple épinglé sur CPU0 passe de 42,1 à 47,3 µs à quatre vCPU. L’attente de la garde diminue aussi à quatre vCPU dans le scénario instrumenté à un seul couple libre. Les huit acteurs épinglés continuent à sérialiser leurs appels à travers cette même garde.

Trois comparaisons supplémentaires ont été effectuées sur QEMU x86_64 dans des copies du noyau. Le premier essai réutilise l’indice du propriétaire dans les méthodes de l’ordonnanceur déjà protégées. Il évite des instructions CPUID et la recherche de l’identifiant matériel dans la topologie. Le deuxième transmet aussi une référence à la garde déjà acquise aux chemins de livraison IPC et de commutation. Sa référence est le noyau de départ instrumentable. Le troisième ajoute un indicateur d’invalidation TLB en cours. Sans cet indicateur, la génération xAPIC reste non nulle après la première invalidation et la boucle d’attente recherche son indice de CPU même lorsque les acquittements sont complets. Les trois comparaisons chronomètrent le lot jusqu’à la sortie du dernier client. Chaque ligne possède son propre témoin et son propre intervalle de charge hôte. [Variante de garde](../tests/fixtures/ipc-smp-phase3-garde-x86.patch) · [Variante TLB additionnelle](../tests/fixtures/ipc-smp-phase3-tlb-x86.patch)

| Essai et témoin | Démarrages × lots, par cellule et version | A, 2 vCPU | A, 4 vCPU | B, 2 couples | B, 4 couples |
|---|---:|---:|---:|---:|---:|
| Indice du propriétaire, contre le candidat | 2 × 7 | 56,13 → 54,61 | 56,53 → 54,90 | 97,72 → 91,95 | 277,46 → 245,90 |
| Indice et garde, contre le témoin de phase 3 | 5 × 7 | 56,21 → 54,16 | 78,47 → 53,96 | 91,63 → 90,99 | 238,11 → 236,03 |
| Indice, garde et indicateur TLB, contre indice et garde | 3 × 7 | 53,46 → 53,64 | 53,24 → 51,25 | 92,30 → 90,97 | 237,11 → 233,93 |

Les durées du tableau sont les médianes en ms pour 1 000 tours par couple. Avec la deuxième variante à quatre couples, quatre démarrages sur cinq progressent et le cinquième passe de 233,69 à 237,25 ms. Le p95 global descend de 273,07 à 258,96 ms. À deux couples, trois démarrages sur cinq reculent, et le p95 passe de 94,69 à 95,74 ms. L’indicateur TLB supplémentaire donne un p95 de 242,58 ms contre 241,43 à quatre couples. Les gains médians de ces variantes se situent dans la dispersion entre démarrages. Elles sont livrées sous forme de correctifs de laboratoire, hors des constructions normales. Pour reproduire la deuxième variante, appliquer le correctif de garde sur une copie de `3b9c3f6`. Pour la troisième, appliquer également le correctif TLB. Utiliser le lanceur avec `--arch x86_64 --cpus 4 2 --cas paires libre` et l’effectif correspondant à chaque ligne.

**Recommandation.** Conserver le développement sur `codex/smp-0170`. Avant intégration, ventiler les durées mesurées sous la garde entre les parcours d’attentes, la validation mémoire et la livraison dans le cas de plusieurs couples épinglés. Rejouer ensuite les mesures sans sonde pour vérifier le débit et les queues de latence. Le maintien du débit à charge proportionnelle sur x86_64, la borne de famine et les queues de latence sur matériel réel restent ouverts. Une variante sélectionnée sur ce chemin devra repasser les mêmes courses, endurances et contrôles de restitution.

## Reproduction

Depuis `novaOS/`, construire dans un répertoire distinct pour chaque révision. Utiliser `f356970` pour le témoin, puis le commit du candidat pour l'après. Pour le témoin VFS synchronisé, reporter sur `f356970` les lectures du compteur de soumissions et l'attente des deux soumissions du contrôleur de laboratoire. Pour le témoin à plusieurs couples instrumentés, reporter également les deux conditions qui activent et épinglent les acteurs de la sonde dans `d79a32b`. Garder le sélecteur privé du client correspondant à son propre noyau. La cellule VFS x86 mono du témoin déclenche le défaut décrit plus haut.

```sh
export RUSTC="$(rustup which --toolchain stable rustc)"
for cible in aarch64-unknown-none-softfloat x86_64-unknown-none; do
    rustup run stable cargo build --offline --locked --release \
        --features ipc-smp-lab --target "$cible" --target-dir "$BUILD_LAB"
done
python3 -B tests/ipc_multicoeur.py "$BUILD_APRES" build/resultats-larges --repetitions 2
python3 -B tests/ipc_multicoeur.py "$BUILD_APRES" build/resultats-apparies \
    --temoin "$BUILD_AVANT" --repetitions 3 --cas libre paires sonde
python3 -B tests/ipc_multicoeur.py "$BUILD_APRES" build/resultats-garde-paires \
    --temoin "$BUILD_AVANT" --repetitions 3 --cas paires-sonde --cpus 2 4
```

`BUILD_LAB` désigne un répertoire de construction neuf. `BUILD_AVANT` et `BUILD_APRES` désignent les deux répertoires obtenus. Exécuter la comparaison appariée seule. Pour reproduire le témoin historique, utiliser le noyau ordinaire et `NOVA_ARCH=aarch64 PYTHONPATH=tests python3 -B -c 'import smp; smp.run("aarch64", 4)'`, puis changer la cible et le nombre de CPU. Pour T09 et l'endurance, utiliser `NOVA_VCPUS=4 make ARCH=aarch64 ipc-races ipc-endurance DURATION=300`, puis la cible x86. Les procédures de construction des invités expérimentaux de stockage, de FPU et des firmwares sont celles de la phase 2. [Reproduction de phase 2](smp-phase2-v0.17.0.md#régressions-et-reproduction)

## Topologies à qualifier ultérieurement

Toutes les lignes suivantes sont des essais **à réaliser**. Le nombre de threads vus par l'invité ne prouve aucune correspondance avec les cœurs physiques de l'hôte.

| Topologie future | Paramétrage ou mesure | Contrôles spécifiques |
|---|---|---|
| Deux sockets, cœurs distincts | QEMU direct `-smp 4,sockets=2,cores=2,threads=1` | Un couple intra-socket et inter-socket, identifiants LAPIC/MPIDR et cibles IPI |
| SMT x86 | `-smp 4,sockets=1,cores=2,threads=2` | Frères SMT contre cœurs distincts, égalité de service et latences extrêmes |
| Deux nœuds NUMA | Lier explicitement les objets mémoire et les vCPU aux deux nœuds | Allocations locales/distantes, latence mémoire, pression sur la FIFO |
| Intel et AMD | Modèles QEMU identifiés par CPUID, puis matériel réel | xAPIC, restauration des états étendus effectivement annoncés, migration et révocation |
| AArch64 | Plusieurs affinités/clusters GICv3, puis matériel ciblé | Destinations SGI, ASID par PE, invalidations distantes et barrières avant restitution |
| Cœurs hétérogènes | Relever les topologies invité et hôte séparément | Placement P/E observable, QoS, activité par thread TCG et mesures à charge comparable |

Étendre les effectifs à 8/16/32 vCPU exigera une décision ultérieure, la vérification des bornes et des topologies par le lanceur, puis une nouvelle campagne de sûreté. Cette phase couvre exclusivement 1/2/4 vCPU.

## Traçabilité Git

| Commit sur la branche SMP | Objet |
|---|---|
| `f356970d27cf8e8fad24d487a3debdc4d2a0be18` | Banc avant optimisation, acteurs et instrumentation |
| `23fdfe26d4d1234a86c8c7892b16e55e669e05ce` | Cadres privés, ASID local, acquisition groupée et IPI ciblé |
| `f2bc82d90ab1231b0740031af5765a8134657373` | Compatibilité des fixtures de qualification du stockage |
| `d8cbec369f07ed2b77103ace4e52c5242d0004b3` | Sélecteurs privés distincts pour les deux laboratoires |
| `86a9abaddc0cc85e0afc067b1894c81245cdc198` | Correction MMIO x86 et vérification des soumissions bloc |
| `3b9c3f604b73e3bf2aeabed0a42f7840f0f671d0` | Déclenchement VFS, provenance ELF, contrôleur série et mesures appariées |
| `d79a32b9dc2552658f4009f11e227f6c8d80e9f0` | Sonde de la garde sous charge proportionnelle et contrôle des effectifs |

La référence de `main` pendant cette qualification est `3e14a50bb5bc7960d16c1fbcfc7f49a681d6e7c6`.
