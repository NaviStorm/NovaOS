# NovaAuth R2 — Correction ciblée du banc P9/4096

## Verdict du 22 novembre 2026

**ÉCHEC de l'unique essai autorisé. Aucun rejeu après cet échec.** La correction
résout la consultation des événements et avance la préparation d'Alice, mais
ne suffit pas à obtenir un ticket Bob sous un bail valide sur ce profil.
Aucune qualification globale R2 ni validation du témoin P9/4096 n'est revendiquée.

- Point de départ : `5ebda14fb6af15ec4fcfb6fae2e9e8ca6a6001fd`.
- Correctif testé : `63f6e19d9161833e0763e07e9940243f7474114d`.
- Arbre du banc testé : `c3009614f0c24b48ba73c2515868c34a26906cbb`.
- Sources invitées inchangées : arbre `e7ddc8ecea2220ce205a443dcd01de4203f6dbb8`.
- ISO réutilisée : `4fd9df53602629bf04b12894da84d391e6e6131745585a172302e26085c8e781`.
- Genèse P9/4096 : `43c92266917eac0e41231c8997de2812a34223a219eb9ad49588b349586d7dd1`.
- QEMU 11.1.3, TCG, deux vCPU, 256 Mio, réseau désactivé.

La [coordination](validation/novaauth-r2-correction-ticket-20261122-coordination.json)
conserve la commande exacte et les empreintes du noyau, du registre, de l'ISO
et des disques initiaux. Copies neuves, aucune recompilation invitée.

## Correction et périmètre

Le précédent parcours attendait la fin des contrôles Bob avant de démarrer la
préparation d'Alice, puis cherchait un marqueur potentiellement dépassé par le
curseur de lecture de cette commande. Le banc prépare maintenant Alice dès que
le lanceur est prêt, pendant les contrôles de Bob. Seul `--ticket-vivant` change
d'ordonnancement ; les autres lots gardent leur ordre historique.

Un journal d'événements hôte garde une borne de début indépendante du curseur
des commandes. Chaque étape doit être observée exactement une fois pour le PID
Bob désigné. Le collecteur existant lie les deux PID à leurs instances et sessions
réelles, distinctes. Les enfants d'Alice sont identifiés par ces liaisons noyau,
même lorsque leurs traces se mêlent aux contrôles de Bob. Le contrôle final des
sorties Bob reste intact. Les marqueurs de fin sont distincts par étape
`NXR2_0_`, `NXR2_1_`, `NXR2_2_`.

Aucun droit ni message invité supplémentaire, aucune écriture GDB en mémoire,
aucun changement du collecteur, de l'ISO, des baux de 60 secondes, des tickets
de cinq secondes, des défis ou du délai TLS de dix secondes. L'attente préparatoire
invité de quinze secondes reste inchangée. Aucun renouvellement implicite.

Cinq tests hôtes passent : événement déjà reçu, événement futur, mauvais PID ou
événement d'un parcours antérieur, doublon, acteur ambigu. Une erreur de chemin
lors de leur préparation a été corrigée avant leur exécution ; elle n'a lancé
aucun invité. Ces tests hôtes ne constituent pas une qualification QEMU.

## Résultats réellement observés

| Contrôle | Résultat |
| --- | --- |
| Deux authentifications et deux sessions distinctes | Observées, défis vérifiés avant expiration |
| Préparation Alice | PID 21, natif/shell 0, stdout exact 134 octets, stderr vide |
| Corrélation des trois événements Bob | Observée ; aucune attente d'un marqueur dépassé |
| Ticket Bob valide non consommé | NON ÉMIS : NSHEXBG1 refuse le bail expiré |
| Révocation R2 par Alice | NON EXÉCUTÉE : session Alice expirée |
| REVOKED avant expiration du ticket | NON EXERCÉ |
| Aucun enfant créé par l'ancien ticket | Ticket non émis ; scénario demandé NON EXERCÉ |
| Reçu durable RETIREE | NON EXERCÉ ; zéro mutation administrative |
| Restitution locale | Bob PID 11 et ELF PID 21/22 restitués |
| Drainage global final invité | NON DÉMONTRÉ ; arrêt hôte après timeout du banc |

[Diagnostic structuré](validation/novaauth-r2-correction-ticket-20261122-diagnostic.json)
et [matrice](validation/novaauth-r2-correction-ticket-20261122-matrice.json).

## Blocage exact

| Acteur / opération | Décision, ms | Échéance, ms | Retard | Statut |
| --- | ---: | ---: | ---: | --- |
| Bob, début de ticket NSHEXBG1, tag 9 | 88284 | 78956 | 9328 ms | DENIED (5) |
| Alice, validité NSHLIVE1, tag 7 | 88288 | 74668 | 13620 ms | DENIED (5) |

Les baux ont commencé respectivement à 18956 ms et 14668 ms. Le refus Bob arrive
ici au **début** de la préparation du ticket ; dans l'essai précédent, il arrivait
à NSHEXEND. Ce sont des refus de baux expirés, pas une preuve de révocation R2.

La confirmation est envoyée après observation de l'événement corrélé de Bob,
mais le shell la refuse avant de lancer l'ELF administratif. Le banc attend alors
le marqueur de fin de l'étape 1, absent puisque la commande n'a pas été exécutée,
puis se termine en échec. Il n'exécute aucune seconde mutation ni nouvelle tentative.

La préparation d'Alice aboutit avant la fin des contrôles préalables de Bob.
Leur chevauchement n'a pas fourni une chronologie suffisante pour garder les
baux valides jusqu'à l'émission. La cause détaillée du coût des contrôles de Bob
n'est pas établie par cet unique essai ; aucune attribution à la charge hôte.
Le premier échange TLS mesuré dure 9764 ms (736–10500), sous sa borne 10736 ms.
Les deux défis sont vérifiés avant leur limite de 1,5 seconde.

La prochaine étape doit revoir l'enchaînement entre contrôles préalables,
authentification et émission, tout en gardant l'intention d'Alice liée à sa
session et sans renouvellement silencieux pendant une transaction. Aucune
nouvelle tentative, instrumentation invitée ou correction supplémentaire n'est
appliquée dans cette livraison. L'objectif du témoin reste ouvert.

## Persistance et ressources

La lecture seule de l'autorité retrouve la génération 23 et zéro `admin_changes`.
Alice et Bob gardent leurs époques ACTIVE initiales ; aucun reçu de retrait R2
n'existe. Le triplet P9 est identique à la genèse et le TSV synthétique conserve
ses douze octets. Cette vérification hôte ne remplace pas une recette VFS invitée.

Le pic des racines reste à quatre pour chaque acteur ; cinquième racine refusée
par LIMIT. Registre : 145 pages, signataire : 66 pages ; compteurs encadrants
stables. Les restitutions des PID 11, 21 et 22 donnent zéro racine, handle, attente,
page, pile et thread. Le relais hôte est drainé. Les autres services invités
restent présents avant l'arrêt QEMU : aucune absence globale de fuite n'est déduite.

## Durées et coordination

Le 22 novembre 2026, verrou commun acquis à **12:17:26.637740 CET**, libéré à
**12:20:18.050655 CET**. Attente : **0,000507 s**. Exécution du banc :
**171,366921375 s**. Aucun conflit de planification ni campagne parallèle lancée.
Aucune restriction de plateforme n'a bloqué cette étape.

Le budget actif commence à 12:09:59.935842 CET. Les durées finales de développement,
diagnostic et archivage/publication, séparées du banc et de l'attente, sont
conservées dans `bilan-correction-ticket-5ebda14.json`, à côté des manifestes.

## Preuves conservées

Racine durable :
`OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/correction-ticket-5ebda14/`.
Parcours : `r2-commandes-x86_64-4096-desactivation-essai-unique/`.
Les échecs et preuves antérieures sont conservés sans modification.

- Manifeste public `SHA256SUMS-correction-ticket-5ebda14` : **48 fichiers**, 2662762 octets.
  SHA-256 `4ad5bab6d9635c405102a27b8ba9e8a2b1495848206f408b8279a38da2f4ea2d`.
  [Copie publiée](validation/novaauth-r2-correction-ticket-20261122.sha256), chemins relatifs à cette racine.
- Racine privée `OS-worktrees/artefacts-prives-administration-0240-r2/correction-ticket-5ebda14/`.
  Manifeste homonyme dans son parent : **21 fichiers**, 25396100 octets.
  SHA-256 `4208a2285e50c459187728682f4713aed19421830aaeca3f28ab91d648c22be5`.
  Secrets et binaires de laboratoire restent hors Git.

Deux lectures SHA-256 concordantes de chaque fichier. Le correctif et le compte
rendu sont publiés uniquement sur `codex/novaauth-administration-0240-phase1`.
Les profils AArch64, les autres scénarios R2, les contrôles ABI et ELF historiques
n'ont pas été exécutés dans ce mandat ciblé. L'acquis 512 antérieur n'est pas rejoué.
Les réserves historiques AArch64 de 301 cadres et de reproductibilité restent
ouvertes. Aucun changement du noyau ordinaire, des 48 sélecteurs ou de NovaFS P9.
Aucune fusion, aucun tag ni activation ordinaire ; utilitaires en pause.
