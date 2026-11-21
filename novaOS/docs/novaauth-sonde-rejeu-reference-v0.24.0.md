# NovaOS v0.24.0 — Sonde de référence historique

## Objet et statut

Sonde autorisée par la MOA/MOE, qualifiée dans les quatre profils QEMU sur
`codex/novaauth-autorite-0240`, après le commit de cache
`590908252374503e51c44cc14423e34d2acb3031`.

Le témoin à deux VM vivantes exerce déjà le refus pour retrait d'une incarnation,
avec un ticket non expiré. Le rejeu après redémarrage demande une autre preuve :
les anciennes références doivent être présentées au noyau de la nouvelle instance.
Or `nsh` authentifie toute saisie avant son exécution. Après restauration non
approuvée, cette authentification est précisément refusée. Ce refus sûr ne prouve
pas que l'ancien entier local a été présenté à `SPAWN(EXEC_TICKET)`.

Le mandat demande de proposer le raccord minimal lorsque le banc ne permet pas
ce contrôle. La MOA/MOE a autorisé cette seule exception au garde frontal. Les contrôles
natifs et les droits du processus restent inchangés.

## Raccord autorisé, exclusivement dans l'export QEMU

- Une sonde de laboratoire dans le seul `nsh` désigné accepte une référence locale
  historique `u64`, avant la première authentification et avant toute émission
  de nouveau ticket. Elle ne s'exécute qu'une fois par démarrage du shell.
- La syntaxe de qualification est `ticket-reference <entier-decimal>` ; aucun
  chemin, option d'utilitaire ou instruction composée n'est interprété.
- La sonde appelle seulement `SPAWN(EXEC_TICKET, reference, 0)` avec les droits
  déjà détenus par cette instance. Elle n'appelle ni l'installateur ni une
  fonction de création de ticket, et ne crée aucune session.
- Le garde d'authentification reste obligatoire pour toutes les commandes
  ordinaires. Le branchement de la sonde est injecté uniquement par l'export
  `exec_admission` ; aucun code ni droit supplémentaire dans le noyau ordinaire.
- Le noyau et le registre conservent intégralement leurs contrôles d'instance,
  session, installation, incarnation et admission distante. La sonde ne leur
  transmet aucune instruction de contournement.
- Le résultat expose le statut natif, l'éventuel droit de collecte et les
  compteurs de ressources avant/après. Un succès inattendu invalide le test ;
  il n'est jamais transformé en refus attendu. Le témoin synthétique reste
  `cut --version`, sans accès à des données utilisateur.
- Budget additionnel : aucune racine IPC, aucun handle délégué, aucun service,
  aucune clé. Un tampon local borné suffit à la référence et au diagnostic.

## Recette de qualification

1. Relever la référence du ticket non consommé de A par observation GDB/QMP,
   sans modifier la mémoire invitée ni fabriquer le ticket.
2. Après retrait durable, présenter la référence dans B **avant tout nouveau
   ticket B**. Exiger un refus natif et aucun enfant.
3. Authentifier B normalement, puis émettre son nouveau ticket et vérifier son
   lancement légitime. La sonde ne peut plus être invoquée dans ce démarrage.
4. Redémarrer séparément A sur ses anciens disques. Présenter la référence avant
   authentification et vérifier le refus natif sans enfant, puis constater le
   refus d'activation de l'ancien registre.
5. Distinguer ce refus après redémarrage du refus pour retrait de la VM A restée
   vivante. Une référence invalide au démarrage n'est pas une preuve de retrait
   à distance, ni une preuve fondée sur l'expiration du ticket.
6. Exécuter sur AArch64/x86_64, P9/512 et P9/4096, avec journaux natifs et SHA-256.

Une même valeur numérique peut être réallouée ultérieurement à un autre objet
local. L'essai interdit donc d'émettre d'abord un nouveau ticket puis de prendre
une collision numérique pour la survie de l'ancien ticket.

## Limites et état des résultats

Le raccord est injecté par `exec_admission_profile.py`, dans
`components/shell/exec_reference_lab.rs`. La première saisie ferme la fenêtre
atomiquement, même si elle est invalide ou si l'authentification échoue. Les
anciennes formes SPAWN ne passent jamais par cette sonde.

Le statut attendu après redémarrage est `DENIED` (5), produit par la recherche
native de la référence dans la table vide de l'instance. Il doit être distingué
de `REVOKED` sur A vivante et non expirée après le retrait distant. Le banc
compare les compteurs avant/après et exige une seule invocation, y compris
après une seconde saisie de la commande de sonde.

Les quatre profils ont exécuté le véritable appel natif, avant authentification
et émission de ticket. Aucun refus du garde frontal n'est compté comme un refus
SPAWN. Les deux premiers essais x86 présentaient la référence dans B avant la
fin du drainage ; ils sont conservés, mais remplacés pour cette exigence par
les essais `-2`, qui placent le rejeu après `RETIREE`.

## Résultats QEMU

| Profil | A vivante, ticket non expiré | Rejeu B | Nouveau lancement B | A redémarrée |
|---|---|---|---|---|
| AArch64/P9-512 | REVOKED (10), aucun enfant | DENIED (5) | 0 | DENIED (5) |
| AArch64/P9-4096 | REVOKED (10), aucun enfant | DENIED (5) | 0 | DENIED (5) |
| x86_64/P9-512 | REVOKED (10), aucun enfant | DENIED (5) | 0 | DENIED (5) |
| x86_64/P9-4096 | REVOKED (10), aucun enfant | DENIED (5) | 0 | DENIED (5) |

La référence historique observée est `4611686018427388007` dans ces essais.
Elle est lue au point d'arrêt de `consume`, sans écriture mémoire par le banc.
Le même entier est comparé à la trace native de refus de A. Ce n'est pas un
identifiant universel : chaque table reste propre à son instance.

Dans B et A redémarrée, le premier appel retourne la valeur native zéro,
aucun enfant et des compteurs identiques avant/après : racines, handles,
endpoints, nœuds, attentes, processus, threads, droits de jonction et cadres.
Une deuxième saisie est refusée **sans second appel**. L'absence de `NSHOPEN1`
et d'émission de ticket avant la sonde est contrôlée dans les traces.

Le test natif de composant couvre douze saisies : bornes `u64`, dépassement,
signes, instruction composée, argument supplémentaire et fermeture par une
commande ordinaire ou de réauthentification. Ses appels système sont simulés ;
les preuves natives de refus sont exclusivement les recettes QEMU ci-dessus.
Les neuf contrôles ABI réussissent sans modification de leurs attentes.

## Raccords et correction associés

Le chargeur QEMU lit les installations auprès du registre isolé, qui vérifie
la tête fraîche et les droits. NovaFS fournit les octets attestés, sans revenir
à l'autorité locale. Les nouveaux blocs du vérificateur sont injectés après
les raccords historiques pour préserver leurs préimages et leur contrôle.
La reproduction de l'export confirme l'identité des sources compilées ; les
seuls écarts sont un test hôte et le générateur de volumes négatifs, hors
compilation du noyau, du shell et des services.

Un défaut distinct de nettoyage a été corrigé : l'invalidation d'un ticket
jamais présenté à l'admission faisait passer sa porte de `Dormant` à
`Cancelled`, sans opération distante à réconcilier. Sa fermeture attendait
alors indéfiniment un reçu impossible et aboutissait à `UNCERTAIN` (22).
Le correctif conserve la porte détachable tant que `waiting=false`, sous le
verrou du noyau. Dès qu'une admission peut exister, le parcours de
réconciliation historique reste obligatoire. Huit essais QEMU (expiration et
révocation avant admission, sur quatre profils) ferment le ticket avec succès,
sans admission distante ni enfant. Les échecs témoins antérieurs sont conservés.

## Preuves et réserves

- [Manifeste complémentaire](validation/exec-sonde-rejeu-0240.json).
- [Archive des preuves publiques](validation/exec-sonde-rejeu-0240.tar.gz),
  chaque membre décompressé vérifié avec SHA-256.
- Preuves durables, disques et résultats négatifs :
  `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/preuves-raccord-exec-42128c2/`.
- ISO/ELF exacts privés :
  `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/artefacts-prives-raccord-exec-42128c2/`.
  Ces artefacts contiennent des clés de laboratoire et ne sont pas publiés.

Les témoins de concurrence à deux threads, de lancement admis puis retardé et
les quatre ELF historiques antérieurement réussis restent archivés. Ils sont
identifiés comme preuves antérieures, pas comme de nouveaux essais sur l'ISO
12. Les essais VFS de récupération restent décrits dans le
[rapport du cache](novaauth-cache-recuperation-v0.24.0.md).

La présente livraison qualifie la sonde et les recettes indiquées. Elle ne clôt
pas l'ensemble du chantier d'admission distante : la matrice globale de courses,
pannes, refus d'installation et régressions doit encore être consolidée avant
une proposition de fusion. Le dépassement historique de 301 cadres instrumentés
AArch64 et les expirations de défis sous charge restent signalés. Aucune garantie
sur matériel, UTM ou autorité de production n'est revendiquée. Aucun tag ni fusion
n'est autorisé par ce rapport.


Les noyaux ordinaires ont été recompilés à chemin constant après le raccord :
identité binaire exacte sur les deux architectures, marqueur `NXREFERENCE` absent.
Les empreintes figurent dans `comparaison-ordinaires-sonde-12.json` de l'archive.
L'archive contient 1 018 membres, son SHA-256 est
`7ff9aea19f6e81f8db71354f939294b749736406cb7258b7c977b0af0ba0817d`.
L'inventaire durable associé recense 2 087 fichiers, y compris les échecs et
les images conservées hors des temporaires.
