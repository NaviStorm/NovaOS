# Propriétés de sécurité du contrat IPC

**Périmètre de la v0.8.0.** Un processus utilisateur peut choisir chaque bit des arguments, connaître une clé étrangère exacte, abandonner une opération ou provoquer une faute. Le noyau, sa MMU et le lanceur de confiance font partie de la base de confiance. Les threads d’un processus partagent son autorité. Les campagnes fonctionnelles utilisent QEMU mono-CPU. [Objets et opérations](ipc-architecture.md) · [Formats et erreurs](ipc-abi.md) · [Oracles de validation](ipc-test-plan.md)

## Invariants à maintenir

| Réf. | Propriété normative | Observation qui la réfuterait |
| --- | --- | --- |
| S01 | Toute utilisation d’un endpoint vivant exige une entrée du bon type dans la table de l’appelant, l’incarnation exacte de l’objet, une branche active et tous les droits requis. Créer un objet publie sa racine sous quota. | Un intrus en possession des 64 bits exacts d’une clé étrangère envoie, reçoit ou observe la présence d’une file. |
| S02 | Toute dérivation a le même objet et des droits inclus dans ceux du parent. MOVE conserve le nœud, les droits et les descendants. L’ancienne entrée expire à l’installation de la nouvelle. | Une copie de SEND obtient RECEIVE, ou deux processus conservent le même droit déplacé. |
| S03 | Révoquer un nœud avec MANAGE coupe toute sa descendance stricte, y compris à travers un ancêtre fermé et dans les autres processus. L’effet est logique avant le retour de l’appel. | Un don précédemment inscrit réussit après la révocation de sa source. |
| S04 | Un slot réutilisé change d’incarnation. Une clé externe n’est réémise ni pour un autre slot ni pour un autre processus pendant un démarrage. L’épuisement échoue sans rebouclage. | Un vieux numéro, un réveil retardé ou une référence dans un TCB atteint l’objet créé après fermeture. |
| S05 | Une livraison a au plus un destinataire et un point de commit. Le succès synchrone implique une sortie entièrement écrite. Un MOVE en échec ne modifie pas la source par lui-même. Un autre déplacement, une fermeture ou une révocation peut l’avoir invalidée indépendamment. | Un émetteur reçoit OK alors que le récepteur obtient BAD_ADDRESS, ou une même donnée est consommée deux fois. |
| S06 | Les copies ne traversent que des pages possédées et accessibles selon les permissions demandées. Toute la destination et les ressources de publication sont vérifiées avant la première écriture. | Le premier fragment est modifié alors que le second touche une garde, ou une VA noyau sert d’adresse de réception. |
| S07 | Tous les octets rendus sont définis. Le noyau efface les queues de payload, les champs inapplicables et les stockages réattribués. Les seules identités transmises sont les handles explicitement installés chez le receveur. | Une notification révèle des octets d’un message antérieur, un PID implicite, une adresse noyau ou le handle local d’un autre processus. |
| S08 | Un thread en attente possède une opération unique. L’inscription, le test du prédicat et l’état du TCB sont cohérents. Une finalisation réveille au plus une fois le même thread et n’insère jamais deux fois un TCB dans la file des prêts. | Réveil perdu entre vérification et blocage, thread repris après terminaison ou double complétion. |
| S09 | À un tick donné, l’échéance arrivée est traitée avant un nouveau commit sur l’opération. Une opération terminale ne change plus de résultat. L’annulation n’affecte que les attentes présentes dans sa portée. | Un rendez-vous annulé est livré plus tard, ou une annulation efface une notification déjà acceptée. |
| S10 | Chaque réservation entre immédiatement dans un quota. Les compteurs de files, nœuds, slots et pages restent bornés, même pendant une préparation interrompue. | Un neuvième dépôt entre sans libération de place, ou une erreur d’allocation laisse une capacité ou un crédit en suspens. |
| S11 | La mort retire les opérations du processus et détruit ses endpoints avant la réutilisation de son identité. Les références transitoires sont drainées avant la libération physique. | Un endpoint survit à son créateur avec un handle étranger encore valide, ou une charge synchrone est livrée après la mort de l’émetteur. |
| S12 | Chaque point de repos après collecte retrouve son bilan de pages dynamiques. Le total physique inclut le coût réel de l’IPC et du framebuffer adossé à la RAM. | Les compteurs logiques reviennent à zéro mais le nombre de cadres libres décroît d’un cycle à l’autre. |

## Frontières de confiance et points de commit

Le noyau résout une clé dans la table locale avant de consulter l’objet distant. Un refus d’autorité ordinaire renvoie DENIED pour un entier inventé, une clé valide ailleurs ou une branche périmée. Les accesseurs v0.7.0 de la table noyau supposent un PID interne valide. Il faut une résolution faillible de la valeur utilisateur avant tout accès supposant cette validité. Aucune recherche par PID, aucune adresse physique et aucune adresse de structure noyau ne sont des arguments IPC. La possession antérieure d’une clé peut rendre observable la révocation d’une opération déjà admise. Les statuts, le trafic et les temps d’exécution ne constituent pas un canal confidentiel entre détenteurs autorisés.

| Opération | Point qui ordonne les opérations concurrentes | État observable ensuite |
| --- | --- | --- |
| Créer ou dupliquer | Publication conjointe objet/nœud/entrée nécessaires après réservation | Handle utilisable, ou refus sans objet partiellement visible |
| Envoyer en asynchrone ou notifier | Insertion de la copie ou OU sur le masque sous protection de l’endpoint | La mort ou la révocation de l’émetteur ne retire plus ce contenu. La destruction de l’endpoint le détruit |
| Livrer DATA ou NOTIFY | Écriture complète de la structure de réception et retrait de la tête sous la même protection | Un seul receveur peut réussir, les bits délivrés sont effacés |
| Livrer CAP | Copie de la sortie, installation locale et, pour MOVE, invalidation de la source dans la même transaction | Le numéro chez le receveur est neuf. Les autres opérations attendant sur l’ancien handle déplacé sont finalisées REVOKED |
| Révoquer, annuler, fermer, expirer | Invalidation logique ou premier passage au résultat terminal sous protection commune | Le réveil physique peut suivre, aucune livraison ultérieure ne franchit cette décision |

Si une préparation a perdu son autorité avant admission, l’appel reçoit DENIED. Après admission, le statut terminal enregistre au contraire la cause qui a retiré cette autorité. La disparition de plusieurs endpoints d’un même propriétaire est une seule transition logique, avec priorité à la fermeture du canal après expiration des échéances dues.

Un message DATA contenant une valeur égale à un handle reste une suite d’octets. Seul le chemin CAP installe de l’autorité. Le champ `tag`, le payload et les bits d’une notification proviennent de l’application. Un service qui a besoin d’authentifier un client doit organiser des canaux et des droits appropriés.

Le noyau ne peut pas effacer une donnée qu’un receveur a déjà copiée dans son propre espace. Révoquer coupe les nouveaux usages du droit. Les copies asynchrones déjà publiées et les notifications fusionnées sur un endpoint survivant restent livrables. Une branche révoquée ne permet plus de recevoir ces données. L’offre d’un droit, exclusivement synchrone, est toujours contrôlable jusqu’au commit.

## Protocole de copie et synchronisation

1. Capturer les champs d’entrée une fois dans un tampon noyau borné. Conserver un identifiant de handle et de génération, la copie des octets et, pour une sortie, une VA numérique. Ignorer toute modification ultérieure du descripteur d’entrée, y compris son tag, son délai et son option ACCEPT_CAP.
2. Au premier blocage, avoir validé les bornes et les permissions initiales, puis relâcher tous les emprunts mémoire. Une offre en attente détient sa propre charge. Le thread receveur peut dormir avec une VA devenue inutilisable.
3. À la reprise du receveur, ordonner les échéances, l’endpoint, sa tête et les droits sous une protection commune. Valider à nouveau les générations et la plage de sortie complète, y compris deux pages de cadres physiques différents. Préparer l’image de sortie initialisée et tous les slots nécessaires. Un compteur épuisé lors de l’émission du handle CAP termine les deux côtés avec LIMIT.
4. Copier via les alias HHDM superviseur sous protection des mappings. Le maintien du verrou mémoire ou d’une protection équivalente couvre toute la copie et la publication logique du résultat. Une garde ne doit pas être relâchée entre la vérification et un accès ultérieur par VA brute. Aucun réveil n’autorise l’utilisateur à observer un handle utilisable avant le commit. Si un futur processus multithread lit simultanément son tampon, la synchronisation de cette lecture lui incombe.
5. Détacher les attentes et publier le résultat terminal avant de rendre les threads prêts. Ne conserver aucune référence Rust au scheduler, au tas, aux tables ou au processus pendant un changement de contexte.

La v0.7.0 offre le parcours de pages possédées, les copies via HHDM sous verrou mémoire, la limitation à deux segments et le contrôle du contexte de retour. Les contrôles matériels SMAP/PAN disponibles restent actifs. Les copies IPC tiennent dans la limite de 256 octets. La méthode `deliver` combine sortie, droits et file sous la même protection. En particulier, `Wait::Event(usize)` et `notify_one` ne portent ni incarnation d’endpoint, ni échéance IPC, ni résultat de transaction. [Mémoire utilisateur](../kernel/src/memory/user.rs) · [Dispatch et réveils](../kernel/src/sched/mod.rs)

**CPU0.** L’ordre d’acquisition est état IPC/ordonnanceur, puis mémoire. Les ressources pouvant exiger des pages sont préparées hors de la transaction finale, avec réservation bornée et revalidation. Aucun appel réentrant au Buddy, journal allouant, sommeil ou commutation sous un verrou de données. Le masque IRQ local couvre le passage critique entre inscription et décision de commutation. Le tick inspecte au plus 32 attentes, inscrit les résultats expirés et réveille les threads concernés. Le chemin IRQ ne copie pas de payload utilisateur, n’alloue pas et diffère la restitution physique. Les enregistrements en attente de restitution restent comptabilisés.

**Préparation SMP.** Conserver le même ordre global et une sérialisation commune des objets et tables de capacités touchés par un don ou une révocation. L’option initiale pour plusieurs CPU serait un verrou d’état partagé avec acquisition/release, puis la protection de l’espace mémoire cible. Une fragmentation ultérieure du verrou d’état imposerait un ordre total des identités d’objets et de processus, y compris pour les transactions à deux endpoints. La génération, l’état du processus et la référence active doivent empêcher la libération ou le remappage sur un autre CPU jusqu’à la fin de la copie. Il faudra publier le résultat avant le réveil distant et attendre les lecteurs et invalidations TLB nécessaires avant recyclage. Le masque IRQ actuel ne protège que CPU0. Ces conditions devront être démontrées avant toute activation d’un CPU secondaire.

## Courses à départager

| Ordre imposé par deux événements | Résultat attendu |
| --- | --- |
| Réception validée avant révocation | DATA est déjà livré. Pour un don COPY, un ancêtre révoqué ensuite peut couper immédiatement le handle nouvellement installé. Le succès de la livraison reste acquis |
| Révocation de la source avant livraison d’un don | Le donneur reçoit REVOKED. Le receveur n’obtient aucun CAP et réévalue sa file |
| Échéance atteinte avant validation sur la pile du receveur | L’opération expirée obtient TIMED_OUT, même si un réveil était déjà en file. Le partenaire encore vivant réévalue son prédicat |
| Annulation avant fin du délai puis arrivée du tick | L’attente annulée reste CANCELLED. Les nouveaux appels de la même branche sont admissibles |
| Sortie devenue RO ou absente avant réception | BAD_ADDRESS au receveur, destination intacte. L’élément conserve sa place et la source d’un MOVE reste valide |
| Mort du propriétaire de canal pendant un don | CLOSED pour les attentes sur le canal. La source survivante d’un don non livré demeure chez son détenteur |
| Deux MOVE concurrents du même handle | Un seul commit. Un autre appel déjà admis reçoit REVOKED. Un nouvel appel avec l’ancien numéro reçoit DENIED |
| Processus mourant après commit, avant retour utilisateur | La livraison compte une fois. La destruction normale des handles et objets de la victime s’applique, sans réémission du message |

## Risques résiduels et bornes

- **Interblocage applicatif.** Deux services peuvent chacun attendre une réception de l’autre, et le seul thread d’un processus peut tenter un rendez-vous avec lui-même. Les échéances finies et l’annulation rendent les ressources récupérables lorsque le timer progresse. Une boucle applicative qui réessaie éternellement reste possible.
- **Famine.** L’ordre FIFO empêche le dépassement d’une attente admise. Les essais non bloquants sur file pleine, les producteurs en concurrence pour l’admission et le temps d’exécution accordé par l’ordonnanceur n’ont aucune garantie de délai maximal. Un gestionnaire malveillant peut annuler sa branche. Donner uniquement SEND limite cette autorité.
- **Épuisement.** Un détenteur SEND peut remplir toutes les files pour lesquelles il est autorisé. L’imputation au créateur borne l’impact sur sa RAM. Les quotas globaux limitent l’impact sur le système mais ne réservent pas de capacité à un autre service. Les notifications coalescées bornent le stockage, sans limiter à elles seules la consommation de CPU.
- **Coût de révocation.** Une marche bornée sur les nœuds et attentes doit aussi trouver les dons pendants sur d’autres endpoints. La limite globale de 256 nœuds, la profondeur 8 et les 32 TCB bornent le travail. Mesurer la plus longue section IRQ masquées sous charge avant de relever ces plafonds.
- **Réemploi et redémarrage.** L’unicité vaut pour un démarrage. Les handles enregistrés sur disque ou envoyés à une autre VM ne constituent pas une autorisation. Un changement d’ABI, du domaine de confiance ou du mode de persistance nécessite un nouveau contrat.


## Vérification de la copie atomique

La sonde de copie v0.7.0 retirait une page entre deux copies immédiates. Elle ne pouvait pas détecter une livraison partielle lors de la reprise d’un appel bloqué. T10 installe une deuxième page RW à `DATA + 4096`. La destination de 184 octets commence 96 octets avant cette frontière. Après observation d’un véritable blocage IPC, le contrôleur retire la seconde page et le producteur dépose 128 octets. Le receveur obtient (BAD_ADDRESS, 0). Le banc relit chaque octet des 96 encore accessibles. Un troisième processus consomme la tête de tag 42 et contrôle l’intégralité du payload. La fin de la sonde compare le bilan brut du Buddy et les cadres libres à leur état initial. [Sonde T10](../kernel/src/sched/process/ipc/probes.rs) · [Comptes rendus par architecture](mesures-v0.8.0.md)
