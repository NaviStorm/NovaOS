# NovaOS v0.8.0 — validation de l’IPC

Les campagnes portent sur des noyaux de release construits depuis le contrat `7ca2e89`. La commande `git log -1 --format=%H -- docs/mesures-v0.8.0.md` identifie le commit livrant ces résultats. Chaque groupe d’acteurs possède des espaces d’adressage distincts et utilise la porte `syscall` en Ring 3 ou `svc #0x4e` en EL0. Le contrôleur lit les témoins dans ces espaces et utilise la console série pour rendre les résultats observables. Les mesures brutes contiennent les empreintes ELF, les durées, les histogrammes, les observations de courses, les bilans physiques et les empreintes des journaux. [Données x86_64](validation/v0.8.0/x86_64.json) · [Données AArch64](validation/v0.8.0/aarch64.json)

## Conditions

Mac ARM64, macOS 27.2, Rust stable 1.99.1 et QEMU 11.1.3. Les machines utilisent TCG, un seul CPU et le firmware EDK II fourni par QEMU. Q35 utilise le CPU `max`, le PIC, le PIT à 250 Hz et VGA. La machine `virt` utilise Cortex-A72, GICv2, le timer générique à 250 Hz et ramfb. Un démarrage ABI supplémentaire exerce PAN sur Cortex-A76. Deux campagnes d’endurance ont tourné simultanément dans des VM indépendantes. Les mesures de performance de référence à 256 Mio ont été réalisées successivement, après l’endurance.

## Scénarios et invariants

Les sondes courtes ont été exécutées avec 128 et 256 Mio, les courses et les benchmarks aux deux tailles. Le tableau indique le verdict sur x86_64 et AArch64. Les durées invitées de chaque sous-suite courte figurent sous `cas_ns` dans les deux relevés. [Plan et oracles normatifs](ipc-test-plan.md)

| Cas | x86_64 | AArch64 | Observation |
| --- | --- | --- | --- |
| T01 | Réussi | Réussi | Disposition et offsets vérifiés sur les deux cibles, vecteurs binaires, erreurs de version et de sélecteur, six appels historiques. |
| T02 | Réussi | Réussi | Clé exacte étrangère, anciens handles, identifiants de type erroné et quinze masques de droits. |
| T03 | Réussi | Réussi | Parent fermé encore structurel, révocation à travers deux processus, conservation de la branche sœur. |
| T04 | Réussi | Réussi | COPY, MOVE, droits réduits, refus, table pleine et conservation des descendants. Deux MOVE concurrents du même PID sont imposés par injection du deuxième TCB. |
| T05 | Réussi | Réussi | Huit dépôts en ordre, longueurs 0, 1 et 128, neuvième refusé, longueur 129 rejetée, dépôt survivant au producteur. |
| T06 | Réussi | Réussi | Rendez-vous antérieur à un dépôt plus récent et deux receveurs inscrits avant une tentative immédiate. |
| T07 | Réussi | Réussi | Fusion des masques 1 et 4, répétition, nouveau rang après drainage et rejet du masque nul. |
| T08 | Réussi | Réussi | Tick matériel pour un timeout unitaire. Horloge contrôlée pour d−1, d et d+1, bornes de délai, annulation de branche et conservation du résultat terminal. |
| T09 | Réussi | Réussi | Cinq ordres de dépôt, 10 000 fois chacun. Vérification de la file des prêts et des commutations par timer pendant une attente IPC. |
| T10 | Réussi | Réussi | Deux pages RW, retrait pendant le blocage, BAD_ADDRESS, 96 octets intacts, tête de 128 octets relue par un troisième processus et retour au bilan initial. |
| T11 | Réussi | Réussi | Révocation de COPY et MOVE en vol, révocation après commit et réveil périmé confronté à un TCB réutilisé. |
| T12 | Réussi | Réussi | Mort réelle du propriétaire et d’un détenteur par faute utilisateur, descendants survivants, sortie injectée sur la pile syscall d’un donneur ou receveur bloqué. |
| T13 | Réussi | Réussi | Quotas locaux et globaux, profondeur 8, files et pénuries physiques. Le plafond de 32 attentes et les quatre attentes par PID utilisent des TCB synthétiques sous protection du contrôleur. |
| T14 | Réussi | Réussi | Fin des compteurs externes et internes, rang saturé drainé, échéance débordante et horloge saturée dans une VM dédiée. |
| T15 | Réussi | Réussi | Comparaison des champs, du payload utile et des octets inutilisés de DATA, NOTIFY et CAP après réemploi des tampons. |
| T16 | Réussi | Réussi | Création, COPY, MOVE, attente, annulation, révocation et mort sur chaque cycle. Même bilan Buddy brut et zéro page de slab après chaque collecte. |
| T17 | Réussi | Réussi | Six échanges, deux charges et révocation d’un arbre de 32 nœuds et profondeur 8. Comparaison des trois seuils du contrat dans le contrôleur hôte. |
| T18 | Réussi | Réussi | Cinq transactions composées sur MOVE. Rejet atomique puis refus, échéance puis annulation, annulation avant échéance, fermeture et commit suivi de fermeture. Une seule issue par offre. |

Les observations de S01–S02 portent sur les droits locaux et leur atténuation, S03–S04 sur la révocation et les incarnations, S05–S07 sur le commit et la confidentialité, S08–S09 sur le réveil et les échéances, S10–S12 sur les quotas, la mort et la restitution. La matrice normative associe chaque invariant aux sondes ci-dessus. Ces vérifications expérimentales couvrent les ordres imposés sur CPU0. [Invariants S01 à S12](ipc-security.md)

### Régressions et endurance

Les commandes `make ARCH=x86_64 test` et `make ARCH=aarch64 test` ont chacune terminé leurs 24 démarrages. Elles couvrent l’affichage par comparaison de tous les pixels, la saisie AZERTY sur Q35, les exceptions de pagination, le Buddy, les erreurs et pénuries, les mutex, les piles, la préemption, les processus et les six appels historiques. La matrice `ram-test` a aussi été exécutée sur les deux cibles. À 64 Mio, le noyau est inaccessible dans ces configurations. Limine signale `High memory allocator: Out of memory` sur Q35 et EDK II signale `NewSize >= 0x08000000` sur virt.

| Cible | Temps hôte T16 (s) | Cycles complets | Bilans consécutifs | Cadres libres à chaque bilan | T09, commutations avec attente IPC à 256 Mio |
| --- | --- | --- | --- | --- | --- |
| x86_64 | 7 200,812 | 899 987 | 7 200 | 63 640 | 481 |
| aarch64 | 7 200,163 | 610 625 | 4 885 | 63 059 | 381 |

Le chronométrage commence au marqueur invité après l’amorçage et finit au premier bilan complet au-delà de 7 200 secondes hôte. Les deux démarrages de mise au point interrompus par architecture avant les campagnes de livraison sont exclus. Les journaux reproductibles suivent le modèle `build/serie-<architecture>-ipc-{test,races,perf}-<Mio>.log` et `build/serie-<architecture>-ipc-endurance-7200s.log`.

## Mémoire physique

Les valeurs ci-dessous sont en Kio. Les totaux incluent les cadres de l’image, les tables, les piles, les processus, le Buddy, les réservations d’amorçage encore retenues et, sur virt, la RAM du framebuffer. « Repos UI » est mesuré par le banc ABI après sa collecte. « Repos IPC » comprend la pile du contrôleur de test. Les pages de slabs sont déjà incluses dans le total système.

| Cible | VM (Mio) | À froid | Repos IPC | 17 processus | 64 endpoints | Après pénurie | Après destruction | Repos UI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| x86_64 | 128 | 40 360 | 936 | 2 588 | 2 860 | 936 | 936 | 936 |
| x86_64 | 256 | 40 616 | 1 192 | 2 844 | 3 116 | 1 192 | 1 192 | 1 192 |
| aarch64 | 128 | 29 460 | 2 740 | 4 392 | 4 664 | 2 740 | 2 740 | 2 676 |
| aarch64 | 256 | 29 724 | 2 996 | 4 648 | 4 920 | 2 996 | 2 996 | 2 932 |

Au point de saturation avec 64 endpoints et 256 nœuds, 71 pages de slabs sont engagées et 105 216 octets de structures sont occupés. L’inscription de 32 attentes synthétiques porte le compte à 73 pages, soit 292 Kio. La première création à froid réussit avec cinq cadres dont trois pour les slabs. Les deux autres assurent la traduction du même intervalle virtuel. Le pic ainsi compté atteint **300 Kio**. Le plafond structurel, qui réserve également de la place pour 32 tables locales et trois tables de pagination, est **312 Kio**, sous le budget de 512 Kio. Les pages entièrement vides sont rendues. À chaque point de repos comparable, le compteur IPC revient à zéro avant toute purge du Buddy.

| Cible | ELF sur disque (octets) | Image liée (Kio) | BSS (octets) | Domaine permanent (octets) | Pile du donneur suspendu T18 (octets) |
| --- | --- | --- | --- | --- | --- |
| x86_64 | 557 728 | 480 | 33 216 | 1 064 | 888 |
| aarch64 | 539 264 | 400 | 16 888 | 1 064 | 1 072 |

L’empreinte de pile est observée au point de suspension de l’offre. Le maximum transitoire à l’intérieur des sous-appels peut être supérieur. Les piles superviseur de 64 Kio et leurs gardes sont déjà comptées dans la RAM système.

## Latence et débit

Chaque ligne agrège cinq séries indépendantes de 10 000 allers-retours après 1 000 de chauffe. Chaque tour comprend deux livraisons. Pour COPY, la réponse est DATA vide. Pour MOVE, chaque réponse restitue la capacité déplacée. Le débit compte les deux réceptions terminées et utilise l’horloge monotone hôte entre les marqueurs de série. L’aller-retour s’étend de l’entrée dans le traitement IPC de la demande au retour du traitement de la réponse. La distribution repose sur des histogrammes bornés de huit classes par octave. Les quantiles utilisent la borne supérieure de la classe, le maximum est celui observé. Les fenêtres excluent l’amorçage et la publication des histogrammes.

| Cible | Charge invitée | Échange | Livraisons/s | p50 (µs) | p95 (µs) | p99 (µs) | Max (µs) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| x86_64 | isolée | DATA0 | 48 542 | 45,2 | 45,2 | 49,3 | 382,8 |
| x86_64 | isolée | DATA32 | 48 756 | 45,2 | 45,2 | 53,4 | 131,3 |
| x86_64 | isolée | DATA128 | 48 826 | 41,0 | 45,2 | 49,3 | 81,2 |
| x86_64 | isolée | NOTIFY | 51 466 | 41,0 | 41,0 | 49,3 | 76,2 |
| x86_64 | isolée | COPY | 47 272 | 45,2 | 45,2 | 53,4 | 543,2 |
| x86_64 | isolée | MOVE | 52 738 | 41,0 | 41,0 | 45,2 | 111,2 |
| x86_64 | concurrente | DATA0 | 24 930 | 82,1 | 90,3 | 98,5 | 205,4 |
| x86_64 | concurrente | DATA32 | 25 068 | 82,1 | 90,3 | 98,5 | 139,3 |
| x86_64 | concurrente | DATA128 | 25 157 | 82,1 | 90,3 | 98,5 | 175,4 |
| x86_64 | concurrente | NOTIFY | 25 971 | 82,1 | 82,1 | 90,3 | 158,3 |
| x86_64 | concurrente | COPY | 25 302 | 82,1 | 90,3 | 98,5 | 152,3 |
| x86_64 | concurrente | MOVE | 27 255 | 73,9 | 82,1 | 90,3 | 159,3 |
| aarch64 | isolée | DATA0 | 39 863 | 49,2 | 53,2 | 61,4 | 390,0 |
| aarch64 | isolée | DATA32 | 39 936 | 49,2 | 53,2 | 57,3 | 95,0 |
| aarch64 | isolée | DATA128 | 39 855 | 49,2 | 53,2 | 61,4 | 128,0 |
| aarch64 | isolée | NOTIFY | 42 692 | 49,2 | 49,2 | 53,2 | 173,0 |
| aarch64 | isolée | COPY | 37 831 | 53,2 | 57,3 | 65,5 | 175,0 |
| aarch64 | isolée | MOVE | 42 538 | 49,2 | 49,2 | 57,3 | 163,0 |
| aarch64 | concurrente | DATA0 | 20 089 | 106,5 | 106,5 | 114,7 | 182,0 |
| aarch64 | concurrente | DATA32 | 20 002 | 106,5 | 106,5 | 114,7 | 242,0 |
| aarch64 | concurrente | DATA128 | 19 992 | 106,5 | 106,5 | 122,9 | 207,0 |
| aarch64 | concurrente | NOTIFY | 20 650 | 98,3 | 106,5 | 114,7 | 209,0 |
| aarch64 | concurrente | COPY | 19 861 | 106,5 | 106,5 | 122,9 | 344,0 |
| aarch64 | concurrente | MOVE | 21 554 | 98,3 | 98,3 | 114,7 | 226,0 |

Les mesures de dépôt, d’admission à livraison, de reprise, de révocation et de section IRQ masquées sont disponibles par mode dans `histogrammes`. La reprise mesure le premier signal d’éligibilité jusqu’à la reprise de l’appel. Le chronométrage des sections IPC inclut la sélection du prochain TCB en cas de blocage et s’arrête avant le trampoline assembleur de commutation. Les compteurs sont TSC calibré sur 64 ticks PIT et CNTPCT à la fréquence CNTFRQ. Le tableau retient le cas de révocation d’un arbre saturé (100 révocations). Le débit en octets/s ci-dessous porte sur les deux messages utiles du tour DATA128.

| Cible | VM (Mio) | Compteur (Hz) | DATA128 utile (octets/s) | Révocation p99 (µs) | IRQ p99 (µs) | IRQ max (µs) |
| --- | --- | --- | --- | --- | --- | --- |
| x86_64 | 128 | 1 001 535 156 | 5 679 929 | 16,4 | 12,3 | 376,4 |
| x86_64 | 256 | 997 839 843 | 6 249 677 | 8,2 | 11,3 | 323,7 |
| aarch64 | 128 | 62 500 000 | 4 764 017 | 12,3 | 16,4 | 325,0 |
| aarch64 | 256 | 62 500 000 | 5 101 503 | 9,2 | 15,4 | 270,0 |

Sous contention, un troisième processus produit et consomme ses propres DATA et un quatrième effectue du calcul puis cède le CPU. Leurs compteurs au terme de la charge de 256 Mio figurent ci-dessous. Les taux des deux acteurs de premier plan sont chacun la moitié du débit de leur ligne. Le temps des acteurs de fond est estimé entre le début de la première série sous charge et le dernier relevé hôte. Il inclut les chauffes et les comptes rendus intermédiaires.

| Cible | DATA du producteur | DATA/s approx. | Tours CPU | Tours/s approx. | Préemptions | Erreurs ou expirations |
| --- | --- | --- | --- | --- | --- | --- |
| x86_64 | 610 725 | 25 541 | 612 784 | 25 627 | 5 979 | 0 |
| aarch64 | 610 816 | 20 311 | 612 594 | 20 370 | 5 004 | 0 |

## Portée

- La VM utilise un seul CPU. Le masquage local et l’ordre ordonnanceur → mémoire couvrent ce périmètre. Des verrous inter-CPU, des réveils distants et une preuve de conservation des mappings seront nécessaires avant le SMP.
- Le lanceur fournit un thread utilisateur par processus. La course entre deux MOVE de la même entrée et les limites de quatre attentes par PID et de 32 attentes globales sont vérifiées par injection sous contrôle du noyau. Les dons entre processus, les droits, les copies, les fautes et les échéances ordinaires passent par les vraies portes utilisateur.
- Le retrait de la deuxième page de T10, la mort pendant une attente et certains ordres aux frontières d’échéance sont des interventions privilégiées du banc. Les statuts, les octets et le bilan final sont observés séparément.
- Les seuils de performance concernent ces configurations QEMU. La garantie FIFO commence à l’admission. Les producteurs peuvent échouer sur un quota et les applications restent responsables de leur propre progression.

[Commandes de démarrage et prérequis](../README.md) · [Commandes de validation](ipc-test-plan.md)
