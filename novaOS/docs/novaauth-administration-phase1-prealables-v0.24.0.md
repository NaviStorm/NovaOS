# NovaAuth v0.24.0 — Administration, prérequis de la phase 1

État préalable au 21 novembre 2026. La suite est suivie dans le [bilan R1](novaauth-administration-r1-qualification-v0.24.0.md). Branche `codex/novaauth-administration-0240-phase1`, créée depuis `5c7f1bfa313a3859ccc0830660438e76d598acb1`. La [conception approuvée](https://github.com/NaviStorm/NovaOS/blob/2a406d0e7b41ff67ee6f50b84b4e8d9515b321b3/novaOS/docs/novaauth-administration-contrat-v0.24.0.md) reste conservée sur sa branche. Le mandat suivant autorise une première implémentation, sous réserve d’arbitrage des ambiguïtés de sécurité constatées avant codage. Ce document ne qualifie aucune nouvelle commande. Les [résultats hôtes intermédiaires](validation/novaauth-administration-phase1-fondations.json) distinguent les fondations testées des raccords restant à réaliser.

## Décisions applicables et périmètre

- Réactivation : autorise une **nouvelle authentification explicite**, avec les droits actuels. Même identité stable du compte, nouvelle identité de session. Aucune résurrection de session, ticket, handle, sous-délégation ou capacité révoqués. Aucun rejeu automatique des opérations `UNCERTAIN`.
- Création, consultation, renommage, désactivation/réactivation des utilisateurs ; création/consultation des groupes et ajout/retrait d’appartenance : dans le périmètre de la phase 1.
- **Aucune implémentation ni exposition de `userdel`, `groupdel`, `DeleteGroup`, purge, transfert de propriétaire ou option de suppression dans cette phase.** Les contrôles de refus ne les autorisent pas. L’ancienne bibliothèque `Change::Delete` et ses tests historiques sont conservés, sans nouvelle façade ni nouveau routage administratif.
- Préserver le refus actuel de désactiver ou supprimer **tout** administrateur. Aucune promotion, dégradation, suppression de facteur, attribution implicite par groupe nommé « admin », ni dérogation de récupération. Le test de suppression du dernier administrateur reste un test négatif de l’interface historique ; la suppression de groupe est une opération non exposée. Ce n’est pas une qualification de suppressions futures.
- Fichiers, propriétaires, ACL et protections conservés. Ni création automatique de home, ni suppression de données à la désactivation, ni réattribution de données à la réactivation. Aucun accès privé accordé à l’administrateur par son seul rôle.
- Les chantiers d’utilitaires restent en pause. Aucun changement du noyau ordinaire, de l’ABI v1, de ses 48 sélecteurs ou de NovaFS P9. Aucun merge, tag ou activation de production.

## Interfaces vérifiées sur la base officielle

| Élément réel | Constat | Conséquence |
| --- | --- | --- |
| [Change et Registry](../user/novaauth/src/lib.rs) | Create/Rename/Disable/AddGroup/AddMember/RemoveMember existent ; Enable absent ; Grant opaque, mutate exige admin et tête courante | Réutiliser State/apply, pas un deuxième registre. Ajouter seulement la réactivation autorisée et les vues minimales après définition des barrières. |
| [Ancien port de qualification](../user/novafs/files/auth_registry.rs) | Les mutations NA3 consultent `identity_authority()` et `lab::Ledger` dans NovaFS | Ne pas réintroduire cette autorité locale pour les commandes. Ces fonctions ne constituent pas une API du registre isolé. |
| [Vue isolée](../user/novaauth/src/recuperation_view.rs) et [serveur](../user/novaauth/service/src/bin/common/recovery_view.rs) | Lecture de facteur/dossier et plan de retrait de dossier ; tête indépendante revérifiée ; port 14 déjà possédé | Étendre le service existant vers les seules opérations autorisées, sans accès bloc du registre. |
| [Publication](../user/novaauth/service/src/bin/common/recovery_mutation.rs) | Préparation/relecture via NovaFS, décision indépendante signée, réconciliation et activation sur tête exacte | Réutiliser ce chemin ; ne pas présenter le retour de State/apply comme une confirmation durable. |
| [Sessions](../user/novaauth/service/src/sessions.rs) | inspect compare domaine, sujet et référence du facteur ; efface le bail si refus observé ; ne compare pas la tête | Un simple passage Disabled → Active n’établit pas la révocation irréversible d’un bail non consulté entre les deux. |
| [Vérificateur de nsh](../user/novaauth/service/src/bin/common/shell_verifier.rs) | Session liée à la vraie route, révocation via close/relay et expiration ; pas de route administrative d’ELF | Ne pas se fier à un ID reçu dans argv. Propagation et acquittement à préciser avant Enable. |
| [Démarrage ELF](../../components/exec-client/src/lib.rs) | NXEXEC01 : argv, locale réduite à des drapeaux, stdin/out/err et paire fichiers | Aucun handle administratif. Détourner la paire fichiers ou accorder un privilège au nom du binaire serait incorrect. |
| [Délégation de sorties](../kernel/src/sched/process/ipc/session_display_lab.rs) | Deux racines de sortie par enfant ; deux autres si accès fichiers ; stdin selon rôle | Le canal administratif doit avoir un budget explicite, sans cinquième racine. |
| [Admission indépendante](../user/novaauth/autorite_qemu/admission_exec.py) | Incarnation, génération/tête, instance, session, installation, ticket et opération liés ; drainage par incarnation | Pas encore de retrait durable par utilisateur/époque d’authentification. Ne pas renommer cette barrière en révocation d’utilisateur sans raccord. |

## Diagnostic de non-résurrection

Le témoin hôte `diagnostic-sessions.rs` de l’archive préalable utilise les sources intactes de `Sessions` à la base officielle. Une session est ouverte avec un défi signé fictif ; une nouvelle Binding conserve domaine/sujet/facteur et change uniquement la tête. Sans consultation intermédiaire avec `current=None`, l’ancien bail est accepté. Si un refus a été observé, le bail effacé reste refusé ; une nouvelle preuve permet une nouvelle session distincte.

**Portée : modèle hôte de bibliothèque, pas désactivation/réactivation d’un compte réel, pas qualification QEMU, VFS ou EXEC_TICKET.** Il démontre que l’ajout naïf de Enable est insuffisant. Il ne prouve pas une régression du système livré, où Enable est absent. Le code historique et ses attentes n’ont pas été changés.

Contrat de réalisation proposé : une époque d’authentification persistante propre au sujet, incluse dans l’état vérifié du triplet et dans les attestations privées du profil. Une désactivation clôt définitivement l’époque précédente. Une réactivation ne restaure pas cette époque ; elle requiert une nouvelle preuve et crée un nouveau bail. Un renommage ne clôt pas les sessions. Un retrait de groupe invalide seulement l’origine de droit concernée : la lecture directe indépendante reste utilisable. Ne pas remplacer ces règles par l’invalidation indistincte de toutes les capacités à chaque génération.

L’époque n’est pas un nouvel identifiant utilisateur. Elle ne remplace ni le contrôle du facteur ni la tête indépendante. Une valeur absente dans un ancien format n’autorise aucune migration permissive de session : l’activation de la variante doit commencer sans ancien bail et refuser un état non convertible. Le détail de la compatibilité du format applicatif devra être testé ; le format disque P9 reste inchangé.

## R1 — Route administrative d’ELF approuvée pour QEMU

Proposition limitée au profil QEMU : un canal privé de conversation entre l’ELF administratif attesté et le vérificateur NovaAuth, créé par le lanceur avant la première instruction. Le registre demeure derrière le vérificateur, sur leur route existante. Le courtier ELF ne lit aucune requête administrative et ne reçoit aucun droit nouveau. nsh transmet argv sans interpréter la syntaxe métier.

- L’admission exige une installation et une portée explicitement autorisées, une session active et une filiation noyau vers la vraie instance nsh. Le rôle admin du titulaire est revérifié côté registre pour chaque mutation. Une portée de lecture de soi ne vaut pas pouvoir d’administration.
- Aucun choix de PID, session, sujet administrateur ou destinataire à partir des arguments de commande. La route immuable provient du lanceur et porte instance enfant, installation, session, titulaire, domaine et incarnation. Le contenu IPC ne peut pas les remplacer.
- Une racine possédée par l’enfant, droits de sa racine neutralisés, deux feuilles SEND/RECEIVE seulement, une par participant. Pas de GRANT/MANAGE transmis. Rendez-vous requête/réponse corrélé et borné, pas de file non bornée.
- Publication des feuilles au vérificateur par le raccord privé de lancement ; pas d’énumération générale de processus ou de nouvelles opérations publiques. Au plus deux routes administratives actives dans ce premier banc, troisième refusée avant création ; capacité mémoire et table vérifiées avant attribution.
- Aucune paire fichiers dans l’ELF administratif. Les identifiants de comptes/groupes sont résolus par le registre ; les commandes n’ouvrent pas les fichiers privés. La limitation n’accorde aucun accès implicite aux fichiers de confirmation.
- Nettoyage en cas d’échec avant création, erreur du second étage, fermeture de session, sortie enfant ou panne du vérificateur. L’effacement des routes n’annule pas une mutation déjà admise ; conserver son issue par identifiant.

### Bloc privé proposé, distinct du contrat NXEXEC01

Le besoin est **un nouveau contrat privé de remise de capacité**, même sans nouveau sélecteur. NXADMIN1 est approuvé exclusivement comme nouveau contrat expérimental QEMU ; sa compatibilité reste à démontrer indépendamment du nombre de sélecteurs. Conserver NXEXEC01, ses décodeurs et les anciens ELF inchangés. Ne l’émettre que pour une installation explicitement attestée pour cette variante, jamais d’après son nom.

| Offset | Taille | Variante QEMU proposée NXADMIN1 |
| --- | ---: | --- |
| 0 | 8 | ASCII `NXADMIN1` |
| 8 / 16 / 24 | 8 chacun | Handles stdin / stdout / stderr locaux, mêmes sémantiques d’octets |
| 32 / 40 | 8 chacun | Zéro obligatoire : aucune voie de fichiers |
| 48 | 8 | Taille des arguments, au plus 2560 octets |
| 56 | 8 | Drapeaux existants ; bits inconnus refusés |
| 64 | 8 | Unique feuille locale du canal administratif |
| 72 | 8 | Réservé, zéro obligatoire |
| 80 | variable | Arguments selon le même encodage : compte sur 1 octet, longueurs u16 LE, UTF-8, au plus 32 arguments |

Entiers du bloc en little-endian. Le handle ne confère que la communication avec le vérificateur désigné ; ses droits effectifs dépendent de la session et du registre. Toute taille/version invalide est refusée. Le protocole applicatif et son numéro de séquence restent bornés ; ne pas exposer Grant ou les objets de stockage. L’aide locale peut s’exécuter sans utiliser le canal et sans mutation.

### Budget statique du raccord proposé

Ce budget est une **prévision à mesurer**, pas une preuve QEMU. Les pics historiques proviennent du [contrat de transport](novaauth-transport-autorite-v0.24.0.md#budget-ipc-et-mémoire-à-confronter-aux-mesures). Une racine compte même si ses droits sont neutralisés.

| Acteur | Base historique maximale | Delta de racines R1 | Attendu en admission/attente/échec |
| --- | ---: | ---: | --- |
| nsh | 4 | 0 | ≤4, y compris ticket et flux ; retour au niveau initial |
| Vérificateur NovaAuth | 4 | 0, feuilles reçues | ≤4 ; +1 handle par commande, au plus 2 |
| Registre isolé | 4 | 0 | ≤4, route existante |
| Signataire serveur | 4 | 0 | ≤4, route existante |
| NovaFS | 4 | 0 | ≤4, port triplet existant |
| Courtier et signataire de connexion | 4 chacun | 0 | ≤4, aucun rôle administratif |
| ELF administratif sans fichiers | 2 sorties au plus | 1 | ≤3 ; ≤4 si un stdin lui est imputé |
| Autres producteurs/lecteurs | ≤4 | 0 | Identiques ; aucun contournement de quota |

Par commande : +1 endpoint, +3 nœuds/handles (racine et deux feuilles), dont deux chez l’enfant et un chez le vérificateur. Au plus +2 endpoints/+6 nœuds pour deux commandes ; ajouter jusqu’à deux attentes par conversation active aux compteurs globaux. Aucun cadre de stockage ou processus de service supplémentaire proposé. Le bloc de 80+2560 octets tient dans une page de 4096 octets ; les tables privées et tampons IPC doivent être chiffrés lors du raccord, sans annoncer zéro mémoire supplémentaire. Refuser tout dépassement local/global avant attribution et mesurer chaque phase, la cinquième racine LIMIT et la restitution après échec. Une nouvelle paire fichiers ferait dépasser le budget enfant : elle reste interdite ici.

## R2 — Époque et clôture effective approuvées pour QEMU

L’état utilisateur, les baux et le drainage d’incarnation ne sont pas actuellement une unique barrière par sujet. Proposition : ajouter, seulement au protocole privé QEMU, le sujet stable et son époque dans les liaisons administratives/sessions/lancements, et une clôture durable de l’époque dans l’ordre de l’autorité existante. Aucun nouveau détenteur de clés, arbitre de droits ou pouvoir d’arrêt général.

1. Préparer sans effet, avec principal authentifié, opération, cible stable, tête attendue, identifiant unique et confirmation liée à la même session nsh. Les invocations successives peuvent être deux ELF attestés distincts, sans rendre la confirmation transférable.
2. Au commit d’une désactivation : bloquer localement les nouvelles demandes du sujet, préparer/relever le triplet, publier la décision indépendante et fermer durablement l’ancienne époque. Ne pas prétendre que l’heure de signature impose l’ordre distant.
3. Toute admission distante présentée après cette clôture est refusée, même avec un ancien paquet signé. Les admissions antérieures sont recensées par identifiant ; une réponse perdue, un timeout ou une instance inaccessible ne les annulent pas.
4. Propager l’époque clôturée aux vérificateurs, au lanceur et au VFS ; invalider les anciens baux, tickets, handles et sous-délégations pour les nouvelles opérations. Réutiliser les mécanismes de révocation existants et conserver les ressources en cours de drainage jusqu’à issue démontrée.
5. Retourner succès complet uniquement après décision durable, acquittements des effecteurs requis et issues des admissions antérieures établies. Entre-temps : état explicite EN_COURS/UNCERTAIN, consultation par identifiant, aucune répétition automatique.
6. La réactivation exige la fin de cette clôture, conserve l’ID utilisateur et les données et autorise seulement une nouvelle authentification. Elle ne réémet aucun droit ancien et ne remet aucune mutation en attente en circulation.
7. Les changements d’appartenance utilisent la révision d’origine du groupe ; les capacités indépendantes restent valables. Le retrait d’un groupe ne doit pas fermer indistinctement tous les droits d’un dossier.
8. Une restauration de P9 ou de tous les disques locaux ne peut pas abaisser l’époque ni effacer une clôture acquittée : activation refusée si génération/empreinte diffèrent de la tête durable. Si l’autorité est inaccessible, refus sûr.

La frontière exacte, le reçu durable de clôture et la correspondance avec les admissions déjà décidées doivent être figés avant implémentation. Le protocole historique par incarnation et ses assertions sont conservés ; ses tests ne prouvent pas automatiquement cette variante par sujet. **R1 et R2 ont été autorisés explicitement après présentation de ce document. Leur qualification reste à réaliser.**

## Actualisation des commandes et de la recette

Façades restant à réaliser sous les raccords R1/R2 approuvés : whoami, id, groups, useradd, usermod, groupadd. `usermod --enable` s’ajoute au périmètre approuvé ; aucune option --delete/--remove-data/--force/--admin. Pour l’inspection d’un groupe, prévoir une vue minimale nom/ID/appartenances autorisées, pas une lecture brute du registre. Ne pas afficher les facteurs ni les secrets.

Les statuts, préparation/confirmation/outcome et portées du contrat de conception restent des propositions de détail à figer dans R1/R2. Toute confirmation sensitive périmée ou portant sur une autre tête est refusée, sans répétition. La référence publique d’un facteur de useradd doit provenir du provisionnement de laboratoire attesté, jamais de la seule chaîne passée sur la ligne de commande. Aucune commande de gestion des facteurs n’est ajoutée ici.

La sélection linguistique nécessite de conserver la distinction signalée par D1 : le sélecteur ARC-026 accepte fr-CA/fr-FR, le mandat dit fr. Le bloc historique ne transporte qu’un booléen français ; ne pas prétendre tester la valeur littérale sans porter cette information. Aucun changement de locale des commandes existantes dans cette préparation.

| Scénario complémentaire | Exigence sur les quatre profils QEMU |
| --- | --- |
| Réactivation sans consultation intermédiaire | Authentifier A, conserver bail/ticket/handles, désactiver, acquitter les barrières, réactiver sans utilisation du bail entre les deux ; anciens objets refusés avant expiration, aucun PID créé |
| Nouvelle authentification | Après --enable, nouvelle preuve et ID de session distinct ; lecture réellement autorisée et lancement avec nouveau ticket ; anciens objets toujours refusés après ce succès |
| Origines de droits | Retirer le groupe G ; lecture directe conservée, écriture G refusée pour handles et sous-délégations ; même résultat après redémarrage et réauthentification |
| Concurrence et interruption | Désactivation/réactivation simultanées, perte du reçu, panne d’effecteur ; au plus une transition sur la tête attendue, pas de succès avant drainage ni mutation automatique répétée |
| Non-destructif | userdel/groupdel et options de suppression non exposés ; refus historique de Delete(admin), refus Disable(admin), aucun effet de données ou changement de propriétaire |
| Anti-rollback | Restaurer un état antérieur à désactivation/révocation ; ancienne époque refusée avec autorité conservée ; indisponibilité ne devient pas permission |
| Routage | Autre instance/session/installation et portée auto-déclarée refusées ; aucune lecture privée ; 5e racine LIMIT sans fuite ; aucun privilège des anciens ELF élargi |

Ces scénarios complètent les trente recettes de conception. Les recettes de suppression réelle et multi-admin avec promotion restent exclues ; les témoins négatifs ne sont pas une couverture fonctionnelle de ces opérations. Aucune cellule QEMU n’est déclarée exécutée dans cette préparation.

## État de livraison préalable — historique avant les essais R1

- Branche/worktree dédiés créés ; origin/main et conception vérifiés directement. Premières fondations R2 ajoutées derrière administration-lab, sans raccord invité ou activation ordinaire.
- Diagnostic de Sessions exécuté sur hôte Darwin ARM64 seulement ; ce n’est pas une qualification multiarchitecture.
- R1/R2 soumis avant raccord puis approuvés explicitement, conformément à la clause d’arbitrage préalable du mandat d’implémentation. R1 a été confirmé : la possession du canal ne dispense jamais du contrôle des capacités à chaque demande. L’autorisation de réactiver ne sera pas utilisée pour rouvrir les anciennes capacités.
- Réserves historiques conservées : 301 cadres AArch64 instrumentés et reproductibilité entre environnements. Aucune nouvelle campagne QEMU lancée, verrou commun non acquis ; attente et exécution QEMU = 0.
- Preuves : `OS-worktrees/archives-novaauth-administration-0240/phase1-prealable-5c7f1bf/`. Le manifeste du point de passage R1 est conservé dans ce répertoire. Les archives historiques restent intactes.

## Fondations R2 implémentées, raccord encore incomplet

La bibliothèque privée ajoute des plans de création, renommage, désactivation, réactivation, création de groupe et appartenance ; ils réutilisent State/apply et ne publient rien. Les vues filtrent l’annuaire. La réactivation exige un reçu terminal Ed25519 correspondant exactement au sujet, à l’époque, à la requête de désactivation, à sa génération et à sa tête précédente. L’émission de ce reçu et la barrière de tous les effecteurs restent à raccorder à l’autorité ; les tests hôtes utilisent une clé fictive, pas une décision distante réelle.

Le reçu utilise un corps canonique de 128 octets et une signature externe de 64 octets : serveur [0,16), domaine [16,32), sujet [32,48), époque u64 BE [48,56), requête [56,72), génération u64 BE [72,80), position durable u64 BE [80,88), terminal=1 [88,89), sept zéros [89,96), tête précédente [96,128). Signature sur `NovaAuth/QEMU/subject-retired/v1\0` suivie du corps complet. Une signature valide d’un état non terminal est refusée. La tête précédente est mémorisée dans le triplet de désactivation : aucun champ circulaire ne prétend contenir sa propre empreinte.

EpochSessions compose la table historique, lie les preuves à l’époque et mémorise au plus 64 clôtures par sujet/domaine. Une preuve déjà vérifiée avant le retrait ne peut ouvrir une session ensuite, même avec une vue devenue obsolète. La saturation ne supprime jamais une clôture : fermeture des baux, quarantaine et refus explicite. Le format historique des sessions n’est pas modifié. Les tests exercent cette course, l’expiration, la fermeture idempotente et la conservation d’un autre sujet/d’une époque nouvelle.

Les suites hôtes ont réussi : 16 tests du registre (1 test historique ignoré conservé) et 15 tests du service de sessions. Ces résultats ne démontrent ni le transport NXADMIN1, ni la création d’un processus, ni les droits VFS réels après réactivation. Les quatre profils QEMU restent à exécuter après raccord.

## Composants R1/R2 en cours, avant raccord fonctionnel

L’autorité réutilise son écrivain transactionnel pour publier la tête et les métadonnées de clôture dans le même COMMIT. Le profil administratif doit être enrôlé explicitement par l’administrateur indépendant ; aucun profil historique n’est converti automatiquement. Dans ce domaine seulement, une admission sans époque et une publication sur l’ancien protocole sont refusées. Le contrat historique reste inchangé dans les domaines non enrôlés. L’autorité conserve uniquement les identifiants/époques et décisions, pas une copie des trois bases.

Le reçu terminal par sujet exige les trois acquittements (vérificateurs, lanceur, VFS) et l’issue de toutes les admissions liées. Une filiation absente, une issue incomplète, une simple expiration ou un effecteur muet maintiennent le refus. Le registre devra attester ces acquittements après contrôle de leurs vraies routes ; les appels hôtes actuels ne les remplacent pas. Onze nouveaux témoins et les régressions existantes de l’autorité ont passé, soit 56 tests hôtes.

Le codec NXADMIN1 est disponible sous la seule feature administration-qemu de nova-exec-client. Les arguments réutilisent le décodeur commun ; le magic, l’en-tête, le canal et l’interdiction de fichiers sont distincts. Le premier codec a passé cinq tests hôtes (dont deux historiques NXEXEC01). Le raccord noyau décrit ci-dessous est désormais compilé ; aucune nouvelle commande n’est encore annoncée disponible. Une feuille valide ne sera jamais assimilée à une autorisation métier.


## Point de passage R1 — 22 novembre 2026, avant les premiers invités

Le raccord R1 est compilé dans une extraction explicite `profile.py --administration` : noyau, vérificateur et registre sur x86_64 et AArch64. Il n'est pas activé par les constructions ordinaires. Ces compilations emploient des données invalides de compilation pour les clés/certificats : elles ne démontrent aucune authentification QEMU. Aucun invité n'a encore été lancé dans cette phase ; les quatre recettes restent ouvertes.

Le lanceur remet une feuille privée NXADMIN1 avant la première instruction. Le vérificateur lit l'identité enfant/session/installation/titulaire/domaine depuis la route noyau, puis vérifie le bail. Le registre relit la tête authentifiée de l'autorité pour chaque contexte et chaque requête. Il recoupe le rôle de l'utilisateur avec l'intersection de la portée initialement déléguée et de la portée persistante actuelle de l'installation attestée. La portée appartient aux octets attestés : une mise à jour la retire et exige une nouvelle autorisation du provisionneur. Une application autorisée à administrer ne rend pas son utilisateur administrateur. Le canal ne permet ni lecture de fichiers ni désignation d'un autre principal.

Les consultations d'identité, de groupe et d'appartenance sont raccordées. Les préparations, confirmations et recherches d'issue de mutation restent **refusées** tant que R2 et ses effecteurs ne sont pas raccordés ; leur grammaire n'est pas une preuve de disponibilité. Aucun opcode de suppression de compte ou groupe n'existe dans NXACMD01.

Le protocole de rendez-vous réutilise IPC_SEND/RECEIVE historiques. Le client n'émet pas de dépôt asynchrone ; il attend la consommation de chaque trame avant de recevoir. Trame NXAFRM01 de 128 octets : magic [0,8), séquence u64 LE [8,16), taille totale u16 LE [16,18), offset u16 LE [18,20), taille utile u16 LE [20,22), direction [22,23), neuf zéros [23,32), charge utile [32,128). Charge logique bornée à 2048 octets ; R1 v1 emploie exactement 128 octets. Les séquences sont consommées dès la première trame ; une réponse perdue ne remet jamais la requête en circulation. Les erreurs empoisonnent la conversation du client, qui ferme sa feuille et indique UNCERTAIN. Aucun nouvel essai automatique n'est réalisé.

NXACMD01 conserve une grammaire canonique de 128 octets, avec sélecteurs de nom/identité et remplissage strictement nul. Le contexte de sécurité est distinct et privé au vérificateur ; il n'est jamais accepté depuis argv ou une charge libre du client. Le registre consomme ce contexte une seule fois. Deux routes ont deux réassembleurs indépendants, sans file ni allocation non bornée. La fermeture, l'expiration et la sortie de l'enfant retirent la racine et ses descendants. Le budget de racines reste à mesurer dans les invités ; les compilations et tests hôtes ne le qualifient pas.

Contrôles à ce point : 17 tests hôtes du registre réussis (un test auxiliaire historique ignoré, inchangé), 15 du service de sessions, 11 des codecs administratifs et historiques, neuf contrôles ABI réussis. Le client NXADMIN1 compile sur les deux cibles nues. Les premières tentatives ont utilisé le rustc Homebrew sans bibliothèque core de ces cibles ; les deux contrôles réussissent avec RUSTC désignant explicitement la toolchain stable déjà installée. Les journaux négatifs sont conservés. La comparaison des noyaux ordinaires, les anciens ELF en invité et la qualification fonctionnelle QEMU restent à réaliser sur le candidat stabilisé.

Réserves inchangées : 301 cadres AArch64 instrumentés et reproductibilité entre environnements. Ce point de passage ne qualifie ni R1 de bout en bout, ni les mutations administratives, ni la production.
