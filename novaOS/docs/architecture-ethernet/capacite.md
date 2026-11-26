# Budgets d’admission et montée en charge

## Dimensionnement d’un port

Choisir le profil à partir de la RAM réellement disponible après démarrage, du pire cas de trafic à absorber et des bornes conjointes du paquet, de l’adaptateur et de la carte. Les tailles qui suivent sont des **hypothèses de dimensionnement**, en Kio de 1 024 octets. Elles ne constituent ni une mesure, ni la promesse que NovaOS démarre avec 8 Mio. Une profondeur matérielle, ses entrées inutilisables et les crédits logiciels sont trois quantités distinctes.

Pour chaque sens, choisir un nombre de files, une profondeur compatible (alignement, pas, minimum, maximum), le nombre de descripteurs par trame, les entrées réservées au matériel et la longueur des tampons. Borner les crédits par le plus petit des nombres de places matérielles utilisables, de buffers DMA, de slots CPU, d’entrées de suivi et de droits de session disponibles à l’étape concernée. La somme des crédits distribués aux clients respecte la capacité réelle de la file. Réserver du temps de traitement au retrait et aux petits flux sous saturation. Mesurer l’intervalle maximal entre drainages, puis comparer les crédits matériels au produit débit en trames/s × intervalle. Une profondeur minimale incompatible entraîne un refus d’admission.

**Exemple chiffré reproductible.** Pages de 4 Kio, Q paires de files de profondeur d dans chaque sens, un tampon de 2 Kio par descripteur. Les deux sens ne partagent aucune page DMA. W est le nombre pair de slots CPU par sens et participant, **partagés entre les files**. Trois relations de données par instance (client–port, port–pilote, pilote–adaptateur), deux participants par relation. Une seule famille d’ELF, une unité de traduction dans cet exemple.

| Poste | Hypothèse de calcul en Kio |
|---|---:|
| P, infrastructure permanente du réseau après activation | 128 provisionnés une fois, services de gestion et contrôle commun inclus. Distinguer dans la mesure ce qui existait déjà pour le disque |
| F, marginal par famille de pilote chargée | 64 pour les pages de code partageables de la famille. Si les images ne peuvent partager de cadres en lecture seule, imputer leurs pages résidentes à chaque instance |
| A, DMA accessible au NIC | `4 × Q × d + 24 × Q`. Données dans deux sens et six pages d’anneaux par paire dans cet exemple de géométrie. Remplacer par les tailles arrondies et permissions de chaque famille |
| T, tables de traduction privées | `16 × Q`, hypothèse compacte à réviser selon topologie et étendue IOVA |
| Fenêtres CPU privées | `24 × W`, soit trois relations × deux participants × deux sens × W × 2 Kio. Changer le calcul avec les arrondis de page si W est impair |
| Autre mémoire privée | 192 pour données, piles et tables CPU des trois processus par instance, 32 pour IPC, 32 pour copies transitoires d’un lot maximal de huit trames de 2 Kio par sens. Ajuster si plusieurs lots restent en attente |
| Suivi des tickets et descripteurs | `4 × plafond(2 × Q × d × 64 / 4096)` pour 64 octets par entrée, arrondis à la page |
| I, total d’une instance | Somme de A, T, fenêtres, autres postes privés et suivi. DMA inclus **une seule fois** |
| H, quarantaine | Pour une génération retirée, compter les pages réellement encore épinglées et ses reçus. Dans l’exemple, `A + T` chiffre une hypothèse de rétention privée. Ajouter tout état CPU et toute page de contrôle réellement immobilisés |

Au pic, calculer `P + somme(F des familles chargées) + somme(I des instances actives) + H des anciennes générations + transitoires supplémentaires`. Une page active qui devient bloquée change de catégorie. Si elle reste imputée à I, ne pas l’ajouter une seconde fois à H. Compter les cadres partagés une seule fois, documenter séparément le coût réservé en espace virtuel et les buffers de la future pile ou du pair. Budgéter également le chevauchement de deux images pendant la préparation d’un remplacement, jamais deux maîtres du même groupe.

## Profils proposés selon la RAM

Les plafonds réseau ci-dessous sont des propositions de politique couvrant P, F, instances, transitoires et réserve de panne. L’allocateur doit encore vérifier la marge système, les pages effectivement libres et les ressources de chaque unité. Les colonnes Q/d/W décrivent un candidat à évaluer, y compris si le parc matériel exige une autre valeur.

| RAM machine | Plafond réseau candidat | Q / d / W | Lot max. candidat | I par NIC | A + T | Politique d’admission |
|---|---:|---|---:|---:|---:|---|
| 8 Mio | 512 Kio | 1 / 16 / 2 | 1 | 412 | 104 | Catalogue passif par défaut. Un seul port de cet exemple demanderait déjà 604 Kio hors réserve. Il est refusé |
| 32 Mio | 2 Mio | 1 / 16 / 2 | 2 | 412 | 104 | Un port prioritaire, deuxième soumis aux ressources réelles et à une réserve de retrait |
| 64 Mio | 4 Mio | 1 / 32 / 2 | 2 | 476 | 168 | Un ou plusieurs petits ports si la famille accepte cette profondeur et si les quotas CPU/IPC le permettent |
| 128 Mio | 8 Mio | 1 / 64 / 4 | 4 | 656 | 296 | Une paire par port au départ. Agrandir d seulement après mesure de la pression |
| 256 Mio | 16 Mio | 2 / 128 / 8 | 8 | 1 584 | 1 104 | Deux paires pour un matériel qui les annonce, avec budget par file et service du disque préservé |
| Plusieurs Gio | 128 Mio, exemple sur 4 Gio | 4 / 256 / 16 | 8 | 5 024 | 4 256 | Séparer affinités et unités IOMMU. La taille RAM seule ne lève aucun plafond de processus, d’IRQ ou d’IPC |

Les profils conservent la provision prudente de 32 Kio de copie même pour les petits lots. Le couple minimum/maximum et les pas déclarés priment. Le VirtIO historique utilisait une seule paire de profondeur 16, soit **22 cadres NIC** dont 16 de données et six d’anneaux. Le scénario Killer proposé à 256 entrées donne **1 036 Kio** de pages NIC. Ces deux géométries ne sont pas interchangeables. Pour l’Intel, recalculer séparément les deux anneaux RW arrondis, les données TX/RX directionnelles et les réserves, sans appliquer automatiquement la géométrie à six pages. [Audit VirtIO](https://github.com/NaviStorm/NovaOS/blob/56db7d5c0d04490cd284b91567cbbd4736534266/novaOS/docs/virtio-net-audit-cloture-20261126.md) · [Budget Killer publié](https://github.com/NaviStorm/NovaOS/blob/00335af793e456b6fa6546514f65305f9c114165/novaOS/docs/killer-e2400-architecture.md)

## De 1 à 100 interfaces simultanées

Projection à paramètres constants, une famille, une unité, hors réserve de faute. Chaque cellule mémoire représente `192 + N × I` en Kio. Le pic d’une panne d’unité commune demande une analyse de tous les domaines dépendants. Une politique qui réserve la rétention d’une ancienne génération doit en plus couvrir H et le coût de sa réconciliation ou bloquer l’ouverture suivante.

| Interfaces N | Petites, I = 412 | Deux paires, I = 1 584 | Quatre paires, I = 5 024 | Processus d’instance / endpoints dédiés proposés |
|---:|---:|---:|---:|---:|
| 1 | 604 | 1 776 | 5 216 | 3 / 7 |
| 2 | 1 016 | 3 360 | 10 240 | 6 / 14 |
| 4 | 1 840 | 6 528 | 20 288 | 12 / 28 |
| 16 | 6 784 | 25 536 | 80 576 | 48 / 112 |
| 100 | 41 392 | 158 592 | 502 592 | 300 / 700 |

Une centaine de références compatibles dans un catalogue indexé par bus et ID occupe des entrées de manifeste sur disque et un index borné en RAM. Elle n’alloue aucun anneau, vecteur ni processus avant attachement. Cent ports actifs nécessitent au contraire cent baux indépendants, les ressources de la table, plus les services communs et le matériel correspondant. Pour N familles distinctes, ajouter `(N − 1) × 64` Kio à l’exemple si le coût partagé de chacune est identique. Le parc cible doit préciser le nombre de familles actives, de clients, de files et d’unités.

## Faisabilité IPC et CPU

Premier profil proposé, **un seul consommateur autorisé par port**. La pile future peut servir plusieurs applications avec sa politique propre. Une session de capture supplémentaire exige un acteur/chemin isolé et son budget. Les contrôles de cycle de vie passent par le lanceur et la fermeture du bail. Aucun endpoint par file, IRQ ou trame.

| Acteur | Racines créées pour ce profil | Feuilles ou limites |
|---|---:|---|
| Gestionnaire de périphériques | 2 pour la paire d’inventaire avec le gestionnaire de pilotes | Paire réservée à ces deux autorités, changements multiples identifiés par révision |
| Gestionnaire de pilotes | 2 pour la paire de politique/administration et 1 pour sa commande privée de lancement | Feuilles de l’inventaire. Le lancement n’emprunte pas les racines de NovaAuth ou de nsh |
| Service Ethernet, processus de port **par instance** | 4 pour deux paires privées, port–consommateur et port–pilote | Une requête et une réponse par relation. Les notifications sont sur la direction retour |
| Pilote isolé **par instance** | 0 | Quatre feuilles directionnelles, deux avec le port et deux avec l’adaptateur. Aucun GRANT/MANAGE |
| Adaptateur du courtier **par instance** | 2 pour la paire pilote–adaptateur et 1 pour la commande privée médiée par le superviseur | Ainsi sept endpoints créés au total par instance. Le reçu de la porte privée est collecté via la même relation authentifiée, comme obligation de conception à qualifier |
| Consommateur de trames | 0 pour la liaison | Deux feuilles par interface. La pile devra répartir les ports entre processus consommateurs avant d’épuiser ses slots de handles |
| Noyau et superviseur interne | 0 créé pour recevoir les commandes privées | Toute racine de commande figure chez son propriétaire ci-dessus. Une réalisation qui nécessite un endpoint de réponse supplémentaire doit réviser le bilan |

Pour les commandes privées de lancement et de médiation, le modèle à qualifier utilise une racine de commande scellée reconnue à la porte, une séquence et une zone CPU de résultat privée à l’instance, récupérable après réponse incertaine. Les bornes de cette zone et la vérification de l’émetteur relèvent du superviseur. Une commande en attente par relation suffit au profil de base. Le protocole ordinaire `ipc_send` conserve son reçu de transport. Tout besoin d’une nouvelle primitive ou d’un accès aux pages d’un autre acteur passe par l’arbitrage IPC.

Le lanceur installe des feuilles SEND/RECEIVE atténuées sur chaque paire à un seul pair, sans délégation libre par les clients. Le destinataire reconnaît la session par l’endpoint privé sur lequel il reçoit. `ReceiveV1` ne fournit pas d’identité d’émetteur pour authentifier une session multiplexée à partir d’un champ déclaré. Vérifier aussi les racines utilisées en dehors du réseau, dès la préparation et pendant le remplacement. À quatre racines préexistantes, refuser toute création supplémentaire. Un échec en cours de réservation retire toutes les capacités déjà préparées.

**Limites réelles de la base.** 32 slots de processus, ordonnanceur lui aussi borné, 64 endpoints, 256 nœuds, 32 handles par table locale, 32 attentes IPC, huit dépôts par endpoint et huit boîtes de 4 Kio au total. Avant admission, inventorier l’usage préexistant, réserver par propriétaire et garder les marges de retrait. Pour N = 4, 12 processus et 28 endpoints **s’ajoutent** au reste du système. Une variante qui réserverait une boîte par relation en demanderait déjà 12 et serait refusée. Pour N = 16 et N = 100, les processus et endpoints requis dépassent les tables actuelles même à vide. Une future révision devra dimensionner pools, identifiants, bitsets, verrous, équité et quotas globaux avec un coût mesuré. La borne de quatre racines par acteur reste un invariant. [Table de processus](../../kernel/src/sched/process.rs) · [Ordonnanceur](../../kernel/src/sched/mod.rs) · [Tables IPC](../../kernel/src/sched/process/ipc/mod.rs) · [Boîtes](../../kernel/src/sched/process/ipc/bulk.rs)

**Montée en débit.** En Ethernet sans VLAN, une trame minimale occupe 84 octets-temps sur le fil (64 avec FCS, 8 de préambule/SFD, 12 d’intervalle). À 100 Gbit/s, cela représente environ 148,81 millions de trames/s **par sens**, ou 18,60 millions de lots de huit par seconde. Pour 1 514 octets hors FCS, le plafond est d’environ 8,13 millions de trames/s. Une copie complète à 100 Gbit/s de données représente jusqu’à 12,5 Go/s lus et autant écrits, hors coûts de cache. Additionner les sauts et directions réellement actifs. Ces ordres de grandeur justifient l’étude d’affinités par file, de budgets de drainage, d’un service de la file lente et, ultérieurement, de prêts de pages. Le débit de ligne des contrôleurs 2,5/5/10/25/100 Gbit/s est une entrée de dimensionnement. Aucune valeur de débit NovaOS sur ces matériels n’est qualifiée.
