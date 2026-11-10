# Durcissement du noyau dans NovaOS v0.5.1

## Sections critiques

Le verrou du Buddy sérialise l’état du tas, les balises, les sentinelles et les opérations sur les cadres et les mappings. Le noyau masque les IRQ locales, acquiert le verrou avec l’ordre acquire, puis crée l’emprunt mutable de l’état. À la sortie il publie le verrou libre avec l’ordre release avant de restaurer le masque initial. Un second appel depuis le même CPU se bloquait auparavant dans la boucle d’acquisition. Le verrou compare désormais l’identité de son propriétaire et produit le diagnostic terminal `Reentrant` avant de former un second emprunt. Le diagnostic utilise la pile et le port série.

Sur Q35, le jeton utilise l’identifiant APIC initial de CPUID. Sur virt, il conserve les quatre niveaux d’affinité de MPIDR_EL1. Zéro est réservé à l’état libre. [Manuel Intel, instruction CPUID](https://cdrdv2-public.intel.com/868137/325462-089-sdm-vol-1-2abcd-3abcd-4.pdf) · [Registre d’affinité Arm](https://developer.arm.com/documentation/101430/0102/Register-descriptions/AArch64-system-registers/MPIDR-EL1--Multiprocessor-Affinity-Register--EL1)

La création et la collecte des piles empruntent le gestionnaire de pages sous ce même verrou. Les chemins protégés n’allouent aucun objet Rust et ne peuvent ni dormir ni céder le CPU. Lors d’une réallocation avec déplacement, l’allocateur réserve le nouveau bloc sous verrou, copie la charge utile avec les deux blocs vivants, puis libère la source sous verrou. Le tick et l’IRQ clavier travaillent sur les structures préallouées de l’ordonnanceur.

## Contention et intégrité

Le mutex de la v0.5.0 se rendait disponible avant la reprise du dormeur et choisissait le premier emplacement de thread correspondant. Un concurrent pouvait le reprendre entre-temps. Chaque attente d’événement porte désormais un rang croissant. Sous masque IRQ, le déverrouillage réveille le plus ancien dormeur du canal et lui réserve la propriété. Le test FIFO place deux dormeurs dans un ordre connu, puis fait redemander immédiatement le mutex à l’ancien propriétaire. Les deux dormeurs doivent passer les premiers, dans leur ordre d’inscription.

L’audit du Buddy recalcule le plus grand ordre disponible depuis les feuilles de chaque racine active et rejette les divisions invalides ou deux jumeaux libres restés séparés. Il confronte les nombres et tailles cumulées des blocs vivants à des compteurs maintenus sur les allocations, redimensionnements et libérations. Il ignore les descendants périmés d’un bloc fusionné. La vérification des balises et des sentinelles se fait à la réallocation et à la libération. Les travailleurs vérifient aussi les octets de données dont ils sont propriétaires.

L’audit de l’ordonnanceur contrôle la présence unique de chaque thread prêt dans la file, l’unicité du thread courant, l’identité des emplacements et le nombre de threads terminés attendant collecte. Il vérifie chaque témoin de pile et les bornes de SP, y compris pour les dormeurs. Deux travailleurs assembleur contrôlent leurs registres au passage du timer matériel, sans cession volontaire.

## Reproduire les mesures

Depuis la racine du dépôt, sur macOS avec les cibles Rust et QEMU du projet installés, exécuter les deux suites. Chacune comprend une campagne courte d’au moins 15 secondes.

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
```

Les campagnes longues peuvent tourner simultanément dans deux terminaux. `DURATION` est un nombre entier de secondes strictement positif et vaut 7 200 par défaut. L’option `-i` de `caffeinate` inhibe la veille automatique du Mac pendant la commande.

```sh
caffeinate -i make ARCH=x86_64 endurance DURATION=7200
caffeinate -i make ARCH=aarch64 endurance DURATION=7200
```

Chaque campagne conserve le même noyau, le même tas et la même VM jusqu’au dernier bilan. Le banc démarre son horloge monotone à la réception du bilan initial. Il quitte QEMU au premier bilan complet après la durée demandée. Il exige la progression des compteurs sur chaque période et interrompt le test après 120 secondes sans bilan, y compris si le compteur du noyau se fige. Les résultats sont écrits après la sortie réussie de QEMU dans `build/endurance-<architecture>-<durée>s.json`. Le journal série correspondant est conservé dans `build/serie-<architecture>-sched-endurance-<durée>s.log`.

Chaque période comprend les opérations suivantes.

1. Deux threads de calcul commencent les sondes de registres. À la première période puis toutes les 64 périodes, le contrôleur immobilise les cadres physiques par lots de 32, jusqu’à huit cadres libres ou moins. Il vérifie l’échec de deux demandes, l’intégrité du bloc conservé, le retour des cadres et le succès des mêmes demandes après libération.
2. Trois travailleurs manipulent chacun seize vecteurs de tailles et durées de vie différentes. Ils contrôlent le contenu de chaque vecteur avant de le redimensionner ou de le libérer. Ils incrémentent aussi un compteur partagé en dormant sous mutex, ce qui force les blocages et les réveils.
3. Le contrôleur crée, attend et collecte des lots de douze threads courts. Pendant au moins 1 250 ticks par période, il vérifie les terminaisons et sonde environ une fois par seconde invitée les structures mémoire, les piles et les progrès individuels des trois travailleurs.
4. Le contrôleur arrête et collecte les cinq travailleurs permanents de la période. Il compare les compteurs de sorties et de collectes au nombre attendu. Il purge le suffixe libre du tas, puis exige le même nombre de blocs vivants, d’octets Buddy occupés, d’octets engagés et de cadres physiques libres qu’au départ. Seuls idle et le contrôleur doivent subsister.

Le champ `allocations` compte les réservations de vecteurs qui ont demandé une augmentation de capacité. `minimum` est le plus petit nombre de cadres libres observé par les audits de fragmentation, hors phase de pénurie. Le tas occupé est mesuré en tailles de blocs Buddy, arrondies à la puissance de deux. Les pages de pile, de métadonnées et de tables de traduction participent au bilan des cadres physiques.

## Résultats des régressions

Suites exécutées le 10 novembre 2026 sur macOS ARM64 avec Rust stable 1.99.1 et QEMU 11.1.3 en TCG, un CPU et 256 Mio par VM.

| Mesure des suites `make test` | x86_64 | AArch64 |
| --- | ---: | ---: |
| Scénarios QEMU réussis | 20/20 | 19/19 |
| Préemptions sur 102 ticks avec trois threads prêts et aucune cession | 102 | 102 |
| Préemptions de `sched-stress` sur 15 162 ticks | 15 162 | 15 162 |
| Contrôles de registres A / B de `sched-stress` | 3 781 / 3 781 | 3 781 / 3 781 |
| Durée hôte de `sched-stress`, amorçage compris | 62,1 s | 92,3 s |
| Durée de l’endurance courte, amorçage exclu | 15,493 s | 16,049 s |
| Threads collectés pendant la campagne courte | 7 539 | 5 026 |

Les sondes ont confirmé le masquage d’une IRQ timer pendante pendant la possession du verrou du Buddy, puis sa livraison après libération. Le transfert FIFO du mutex, la réentrée contrôlée et l’injection d’une incohérence des métadonnées ont produit les résultats attendus sur les deux cibles. L’échec d’allocation physique a préservé les données et les compteurs avant restitution et nouvelle tentative.

La comparaison par empreinte des images intégrales a vérifié la bannière des deux architectures et la saisie AZERTY sur Q35. Les scénarios de MMU, d’isolation des racines, de gardes, de permissions et d’allocations Rust ont également réussi. L’absence de framebuffer, testée sur Q35, a produit le diagnostic et l’arrêt attendus.

## Campagnes de deux heures

Le 10 novembre 2026, les deux VM ont terminé avec un code de sortie nul. Le banc a contrôlé chaque bilan reçu sur le port série. Les durées excluent l’amorçage UEFI.

| Mesure | Q35 x86_64 | virt AArch64 |
| --- | ---: | ---: |
| Durée hôte mesurée | 7 204,275 s | 7 202,941 s |
| Périodes complètes avec retour au bilan initial | 1 429 | 973 |
| Ticks matériels livrés | 1 800 969 | 1 229 233 |
| Préemptions matérielles entre threads ordinaires | 1 796 675 | 1 226 304 |
| Threads terminés et collectés | 3 591 041 | 2 445 101 |
| Attentes sur le mutex de contention | 450 132 | 306 491 |
| Incréments vérifiés sous mutex | 451 561 | 307 464 |
| Réservations des vecteurs de fragmentation | 5 375 862 | 3 660 378 |
| Audits en cours de fragmentation | 5 716 | 3 892 |
| Pénuries physiques suivies de récupération | 23 | 16 |
| Minimum de cadres libres échantillonné en phase de fragmentation | 53 790 | 56 377 |
| Vérifications de registres A / B | 449 012 / 449 012 | 306 221 / 306 221 |
| Cadres physiques libres avant → après | 53 905 → 53 905 | 56 492 → 56 492 |
| Octets de données du tas engagés avant → après | 4 096 → 4 096 | 4 096 → 4 096 |
| Octets de blocs Buddy vivants avant → après | 4 096 → 4 096 | 4 096 → 4 096 |
| Blocs vivants avant → après | 1 → 1 | 1 → 1 |

À chaque point de repos, l’unique bloc vivant appartenait à la table préallouée des threads. Les quatre mesures mémoire sont revenues exactement à leur valeur initiale dans chaque période. Toutes les pénuries attendues ont laissé une trace de récupération dans le journal série. Aucune fuite ni corruption n’a été détectée sur ces parcours.

## Limites

La validation porte sur un seul CPU émulé par VM. Avant le démarrage de processeurs secondaires, il faudra protéger les files et les propriétaires de mutex entre CPU, gérer les réveils et les invalidations TLB distants et établir l’ordre des verrous. Le jeton APIC x86 actuel est limité à huit bits. Une topologie x2APIC plus large exigera des identifiants uniques vérifiés au démarrage. Le spinlock du Buddy n’offre aucune borne de latence. Les extensions importantes et l’audit complet masquent les IRQ pendant leur parcours.

Les comparaisons vérifient le retour à l’état de référence aux points de repos et les invariants échantillonnés sous charge. Elles couvrent les séquences exercées dans QEMU. Un dépassement sautant à la fois sentinelle et garde, un ordre de prise contradictoire de plusieurs mutex, un arrêt matériel non masquable et l’exécution sur machine physique restent des risques distincts.
