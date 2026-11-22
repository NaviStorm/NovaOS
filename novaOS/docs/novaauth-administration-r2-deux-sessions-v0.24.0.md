# NovaAuth v0.24.0 — R2 : coexistence des sessions

Référence examinée : `85d7d5bf4dc9c3eac9cf61811d0fa5a5a4f3638e`. Raccord borné approuvé par la MOA/MOE ; le budget ci-dessous précède son implémentation. La qualification effective reste à réaliser.

## Limite constatée

- [Route du lanceur](../kernel/src/sched/process/ipc/session_relay.rs), `register` : une seule `s.processes.relay`, refus d'une seconde route vivante. `Route` contient un seul shell, courtier et bail.
- [Vérificateur](../user/novaauth/service/src/bin/common/shell_verifier.rs) : un seul `Login` et un seul `live`. `open` refuse une seconde session.
- [Signataire](../user/novaauth/service/src/bin/shell-signer.rs) : facteur de laboratoire Alice et cible nsh fixée par le noyau.
- [Raccord du shell](../kernel/src/sched/process/ipc/service/shell_auth_lab.rs) : slots 11–20 occupés par nsh/signataire/courtier. Le [client historique Bob](../kernel/src/sched/process/ipc/service/auth_lab.rs) emploie aussi les slots 15–18. Son inclusion dans l'ELF ne démontre pas sa coexistence avec nsh.
- [Export EXEC](../user/novaauth/service/exec_admission_profile.py) : recettes Alice/Bob VFS et vrai nsh sont explicitement des profils séparés. R1 réutilise le second.

Ces constats sont issus de l'audit des sources, pas d'un échec QEMU inventé. Aucun essai n'a été lancé pour redémontrer cette limite statique.

## Recette concernée

Alice doit administrer depuis une session authentifiée pendant que Bob, non administrateur, conserve sa session et un ticket valide, non consommé et non expiré. Après désactivation durable, puis réactivation de Bob, ses anciens objets doivent rester inutilisables ; une nouvelle authentification doit permettre les droits actuels.

Fermer Bob avant d'authentifier Alice ne constitue pas cette preuve : le ticket serait déjà révoqué. Désactiver Alice pour simplifier le témoin enfreindrait la protection du dernier administrateur. Un test hôte ne prouve pas la révocation des instances invitées.

## Proposition : deux routes indépendantes, seulement dans l'export R2 QEMU

1. Deux instances clientes désignées par le lanceur ; facteurs séparés dans leurs signataires. Aucun choix de PID, principal ou signataire par les arguments d'un ELF administratif.
2. Table de deux routes au lieu du singleton. Chaque entrée lie instance, titulaire, session, époque, courtier et feuilles exactes. Une troisième route échoue avant allocation.
3. Deux contextes du vérificateur : défis, baux, confirmations et séquences séparés. La route du registre est multiplexée avec les identités vérifiées, pas avec celles déclarées par le client.
4. Un seul registre isolé, un seul signataire serveur, une seule autorité indépendante et NovaFS comme unique écrivain. Aucun nouveau privilège métier pour les commandes.
5. Câblage privé sans superposition des mots de démarrage ; feuilles avec droits minimaux. Aucun contournement des quotas ni droit d'arrêt général.
6. Le retrait R2 ferme toutes les routes de l'époque ciblée et leurs descendants. Il conserve l'administratrice, les autres sujets et les origines indépendantes. Un effecteur ou une admission non réconciliés maintiennent UNCERTAIN et interdisent la réactivation.
7. NXEXEC01, NXADMIN1, les anciennes formes SPAWN, les 48 sélecteurs, P9 et le noyau ordinaire gardent leurs contrats. Le passage à deux routes reste une modification architecturale privée, non couverte par la seule validation du canal administratif R1.

## Budget préalable et acceptation

Le budget R1 observé atteint déjà quatre racines pour le vérificateur, NovaFS, le registre et le signataire serveur. Le raccord ne peut donc pas ajouter arbitrairement une racine à ces acteurs.

Avant intégration fonctionnelle : dresser la table complète propriétaires/feuilles/processus pour le câblage retenu, chiffrer l'état borné supplémentaire, puis mesurer chaque acteur avant, pendant et après admission, erreur, révocation et drainage. Maximum quatre racines ; cinquième demande LIMIT sans allocation résiduelle. Le présent document ne présente pas encore un budget mesuré pour deux routes. Tout dépassement impose un nouvel arbitrage.

Sur les quatre profils : substitutions entre routes, confirmation transférée, refus non administrateur, anciens tickets avant expiration, réactivation sans inspection intermédiaire des anciens objets, authentification nouvelle, conservation des droits actuels, retrait sélectif du groupe, concurrence et réponses perdues. Vérifier la restitution des handles, endpoints, threads et processus et distinguer mémoire active, cache réutilisable et mémoire inexpliquée.

## Autorisation et travail indépendant

La décision MOA/MOE autorise deux routes dans le profil QEMU, à condition de démontrer préalablement le budget et de préserver les frontières de sécurité. Les trois fichiers de raccord préparatoire non aboutis sont copiés intégralement et relus dans l'archive de reprise avant leur remise à la référence publiée ; aucun changement préexistant n'a été supprimé.

Les façades des six commandes, leurs tests de grammaire et la consultation authentifiée de l'historique peuvent progresser séparément. Elles ne constituent pas encore une qualification R2 de bout en bout. Aucun acquittement simulé ne remplacera les effecteurs invités et aucune mutation ne sera annoncée complète sans ses garanties.

## Budget statique préalable — 22 novembre 2026

### Sources et limites

Audit de l'export exact R2 `85d7d5bf4dc9c3eac9cf61811d0fa5a5a4f3638e`, avec les observations R1 archivées dans `phase1-prealable-5c7f1bf/r1-aarch64-512-complet1/invite/invite.serial`. Une observation R1 ne vaut pas mesure R2.

Dans `ipc/mod.rs`, `create` refuse la cinquième racine **avant** de réserver endpoint, nœud et handle. Le pool global contient 64 endpoints et 256 nœuds ; chaque table locale contient 32 handles ; le noyau possède 32 places de processus. Le compte de racines inclut les canaux de supervision et l'admission distante, pas seulement les canaux fonctionnels.

Le câblage naïf est exclu : Bob supervisé aurait deux racines de supervision et deux d'authentification ; `exec_authority_lab.rs` crée encore une racine pour l'admission. Le total serait cinq. Deux copies naïves des services ajouteraient aussi douze endpoints aux 49 du plateau R1 et porteraient à 65 le lancement d'un ELF administratif (quatre endpoints transitoires).

### Câblage complémentaire proposé, soumis à arbitrage

Chaque session reçoit une conversation privée duplex, mécanisme déjà employé entre registre et effecteurs : une racine appartenant au shell, deux feuilles `SEND|RECEIVE` remises exclusivement au shell et au vérificateur, puis droits de la racine réduits à zéro. Aucun `GRANT`, `MANAGE`, création d'autorité ni duplication de droits n'est remis. Ce remplacement des deux directions d'authentification est réservé au profil à deux routes. Les formats de requêtes et les vérifications serveur restent identiques. Il ne change pas les voies NovaFS ni les pipes.

Alice conserve le shell interactif existant. Bob exécute le même ELF nsh dans un mode de recette explicite, supervisé, avec authentification et lancement natifs mais sans port général de fichiers ni terminal d'Alice. Le mode de recette ne décide aucun droit. Chaque identité conserve son signataire de facteur et son courtier propres : aucun partage de leurs clés ou de leurs routes. La seule route registre/vérificateur est sérialisée, avec séquence globale et contexte consommé par requête.

| Acteur | Racines au repos | Pic de la recette R2 | Origine des racines |
| --- | ---: | ---: | --- |
| Gestionnaire des services | 4 | 4 | Ports historiques ; seules des feuilles supplémentaires lui arrivent |
| Relais bloc/journal | 4 | 4 | Ports historiques inchangés |
| NovaFS | 4 | 4 | Bloc et supervision/configuration historiques ; autres conversations en feuilles |
| Pilote stockage | 3 | 3 | Chemin bloc historique |
| nsh Alice | 1 | 2 | Authentification duplex ; admission distante temporaire |
| nsh Bob | 3 | 4 | Supervision ×2, authentification duplex ; admission distante temporaire |
| NovaAuth vérificateur | 4 | 4 | Supervision ×2, NovaFS ×2 ; sessions, signataires, courtiers et registre en feuilles |
| Registre isolé | 4 | 4 | Supervision ×2, conversation vérificateur, conversation NovaFS ; admission en feuille |
| Signataire serveur 23 | 4 | 4 | Supervision ×2, registre, transport hors P9 |
| Signataire facteur Alice | 4 | 4 | Supervision ×2, dialogue facteur ×2 |
| Signataire facteur Bob | 4 | 4 | Supervision ×2, dialogue facteur ×2 |
| Courtier Alice | 4 | 4 | Supervision ×2, transfert privé ×2 |
| Courtier Bob | 4 | 4 | Supervision ×2, transfert privé ×2 |
| ELF administratif enfant | 0 | 3 | stdout, stderr, conversation NXADMIN1 ; aucun fichier utilisateur |
| Services historiques 1, 2, 9, 10 | 2 chacun | 2 chacun | Supervision historique |
| Journal 16 | 4 | 4 | Supervision ×2 et NovaFS ×2 |

Au plateau, le total prévu est `49 - 1 + 3 + 4 + 4 = 59` endpoints. La consommation d'Alice ajoute au plus une admission et trois canaux d'enfant : 63. Une tentative simultanée d'admission de Bob porte ce total à 64. Aucun second enfant n'est promis dans cette situation saturée : une allocation impossible rend `LIMIT`, ferme les allocations provisoires et ne confirme pas une révocation. La preuve du refus par révocation est faite avec des ressources disponibles, après collecte du client d'Alice, et avant expiration du ticket de Bob.

Un lancement légitime de Bob est également vérifié après restitution du client administratif : pic 63 pour un enfant administratif, 62 pour un enfant sans NXADMIN1. Ces limites ne promettent pas une capacité illimitée de lancement concurrent ni des pipes dans le shell de recette Bob. Les pipes historiques et les scripts du shell interactif gardent leurs recettes séparées.

### Phases, erreurs et restitutions

| Phase | Total maximal prévu | Condition de sortie |
| --- | ---: | --- |
| Construction incrémentale des deux routes | 59 | Chaque création/derivation échouée retire ses provisoires ; aucun remplacement d'une route vivante |
| Deux sessions ouvertes, ticket Bob non consommé | 59 | Aucun endpoint d'admission avant consommation |
| Mutation Alice avec enfant administratif | 63 | Un enfant et une admission distinctement tracés |
| Tentative Bob pendant l'admission Alice | 64 | Refus sûr ou admission ordonnée ; jamais cinquième racine |
| Mutation Alice collectée, refus du ticket Bob | 60 au plus | Cause de révocation distincte de LIMIT et de l'expiration ; aucun PID interdit |
| Nouvelle authentification et lancement Bob | 63 au plus | Nouvelle session et nouveau ticket ; anciens objets toujours invalides |
| Fin de chaque lancement | 59 | Aucun canal temporaire, handle ou processus résiduel |
| Fermeture des sessions | 59 ou moins | Les canaux de dialogue persistants ne sont pas des délégations de session ; tous les droits de session sont retirés |
| Collecte des trois acteurs Bob | Plateau historique diminué de 1 | Restitution des onze racines et de leurs descendants ; retrait séparé des canaux d'Alice en fin de famille |

Le gestionnaire passe de 26 à **32 handles**, sans marge pour démarrer un autre service avant collecte ; un refus de démarrage supplémentaire doit être explicite et sans fuite. Le vérificateur passe de 11 à 15 handles permanents (Alice perd une feuille ; Bob apporte une feuille d'authentification, deux de facteur et deux de courtier), puis au plus 19 avec les quatre conversations administratives bornées. Le registre conserve sept handles permanents et au plus deux feuilles d'admission supplémentaires. Les nœuds passent de 102 à 124 au repos ; les canaux temporaires restent sous la borne 256. Les 18 processus permanents et deux enfants prévus restent sous 32. Les attentes et les pages de slabs restent à mesurer ; aucune hausse de seuil mémoire n'est déduite de ce calcul.

### Isolation et validation avant qualification

Le multiplexage ne porte pas sur les droits : le noyau choisit la route d'après l'instance et le canal vérifiés, puis la session, le nonce d'admission et les références générationnelles. Le vérificateur ne déduit jamais l'identité d'un nom de commande ou d'argv. Les réponses sur la conversation duplex exigent le tag, la taille et l'état attendus ; une requête reçue à la place d'une réponse est refusée. Les séquences du registre et de NovaFS ne repartent pas de zéro entre les routes.

Le retrait Bob invalide ses offres, tickets, handles et sous-délégations ; aucun `NXPKDROP` global ne peut abandonner une admission d'Alice. Les transitions déjà admises se drainent selon leur identifiant. Les trois effecteurs n'acquittent que leurs faits établis. La réactivation exige le reçu durable terminal et une nouvelle authentification ; elle ne réutilise aucun objet clos.

Ce budget est un **calcul statique de la proposition complémentaire**, pas une validation du raccord. Il ne couvre pas à lui seul les contrôles de capacités décrits ci-dessous. Les quatre profils doivent relever racines, endpoints, handles, processus et cadres avant/pendant/après, injecter les échecs d'allocation, vérifier `LIMIT` sans variation résiduelle, et couvrir les permutations de fermeture et de révocation. Une divergence avec ce budget ou une nécessité de modifier une garantie impose l'arrêt du raccord et un arbitrage.

## Point de suspension : deux adaptations non couvertes par le câblage actuel

L'autorisation de deux routes est conservée. L'audit montre toutefois que leur
simple duplication ne respecte pas les limites. La réalisation proposée demande
de préciser deux contrats privés avant le raccord fonctionnel :

1. **Authentification duplex.** Remplacer les deux racines unidirectionnelles de
   chaque shell par la conversation duplex décrite ci-dessus. Chaque pair reçoit
   `SEND|RECEIVE` sur sa feuille dédiée au même endpoint ; aucun droit de dérivation.
   Le contrôleur de remise de stdout (`session_display_lab.rs::intercept`) exige
   actuellement une racine SEND et une feuille RECEIVE du vérificateur. Il faut
   remplacer ce contrôle dans le seul export R2 par une vérification de la
   référence exacte de la conversation, de son propriétaire, de ses deux pairs
   et de leurs instances. Cela ne peut pas être présenté comme une simple
   multiplication des routes inchangées.
2. **Collecte des sorties sans console pour Bob.**
   `session_display_lab.rs::attach` exige aujourd'hui les bits de terminal
   OUTPUT et ERROR chez le shell propriétaire. Bob n'en dispose pas et doit
   rester sans accès au terminal d'Alice. Proposer un collecteur borné dans le
   mode de recette du même ELF nsh : lecture de ses seuls flux enfants, contrôle
   binaire des témoins synthétiques et restitution au banc par sa supervision.
   L'admission doit exiger la route Bob désignée par le noyau, sa session et son
   enfant réels, sans ajouter de droit CONSOLE ni transférer un droit d'Alice.
   Les anciennes gardes des autres profils restent intactes.

Le risque principal est un routage confus entre deux incarnations ou une
extension involontaire de l'accès aux sorties ; les contrôles doivent comparer
les références générationnelles, les propriétaires et les sessions. Une lecture
sur la mauvaise route, un handle d'Alice présenté par Bob, une réponse ancienne
ou une fermeture concurrente doivent être refusés. Une conversation duplex ne
constitue pas un droit de lancement, une confirmation administrative ni un
acquittement de révocation.

Un prototype de transformation de l'export a été **compilé seulement** sur les
deux ISA pour examiner le premier point. Aucune VM ni recette fonctionnelle n'a
été exécutée avec ce prototype. Ses sources et ses binaires privés sont
conservés dans `raccord-deux-sessions-01` et
`artefacts-prives-administration-0240-r2/duplex-c92fdfb`. Le prototype a été retiré
du chemin actif après copie intégrale et comparaison SHA-256. L'export R2 de la
branche conserve donc son câblage publié précédent. Aucun contrôle historique
n'a été désactivé ou réécrit dans les sources ordinaires.

**Arbitrage ciblé demandé :** autoriser ces deux adaptations exclusivement QEMU,
avec la topologie chiffrée, ou conserver le raccord à deux sessions suspendu.
Pas d'augmentation de quotas, nouveau sélecteur, droit de console partagé,
activation ordinaire ou fusion. Les mesures et recettes des quatre profils
restent obligatoires après cet arbitrage.

## Preuves de cette étape

Les neuf contrôles ABI passent sur `c92fdfb6c371e4d2af00a519e00adca59385de7c`.
Les sources du noyau ordinaire, le registre ABI et le moteur/format P9 sont
identiques à `85d7d5b` sur le périmètre contrôlé. Cela ne démontre pas une
qualification fonctionnelle du raccord ni une reproductibilité inter-environnements.

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/raccord-deux-sessions-01`.

Manifeste `SHA256-budget-deux-sessions-01.txt`, 10 fichiers relus, SHA-256
`449d967ff134514bc24c4208a5a5625a943a919ad8c3be37bec40a1a23893fcd`.

Les deux noyaux expérimentaux compilés restent dans l’archive privée indiquée
par `artefacts-prives.json`, avec leurs octets présents et relus. Ils ne sont
ni publiés dans Git ni présentés comme qualifiés. Aucune campagne QEMU n’a été
lancée ; attente de verrou et exécution QEMU nulles. Les deux réserves historiques
restent ouvertes.
