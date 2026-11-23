# NovaAuth R2 — Contrat de revalidation sélective EXEC soumis à arbitrage

23 novembre 2026. Base publiée : `91d0da7766e56b2e60481d9768ce280952ec2022`.
Branche : `codex/novaauth-administration-0240-phase1`.
**Conception uniquement. Aucune activation ni modification de la validation des tickets.**
Le prototype présent dans le worktree reste non qualifié. Les essais Q2/Q3/Q4 de ce mandat utilisent les composants invités publiés, sans ce prototype.

## 1. Défaut et frontières de confiance

Dans la version publiée de `service/src/bin/common/exec_admission_guest.rs`, la pompe exige que la tête fraîche aux octets 80..112 soit égale à la tête d'émission du ticket, lue dans les mots 18..22 des métadonnées noyau. Sinon elle annule la porte locale et en réconcilie l'issue. Cette condition est sûre contre une acceptation aveugle d'un ancien état, mais invalide aussi un ticket personnel après une mutation de groupe indépendante.

Une différence de tête ne prouve ni la révocation des dépendances du ticket ni leur conservation. Supprimer cette comparaison serait insuffisant : il faut remplacer le cas « tête différente » par une décision authentifiée, fraîche et liée aux dépendances initiales. Une tête identique ne dispense pas non plus de vérifier les réservations de retrait, les époques et les barrières déjà actives.

R reste l'autorité de décision sur les permissions du triplet ; l'autorité indépendante garantit l'ordre durable, la tête, les incarnations et les admissions. NovaFS est le seul écrivain P9 et fournit les octets attestés, jamais la décision d'exécution. Le noyau QEMU assure l'identité d'instance, la consommation unique et la création. Le signataire authentifie les demandes de R sans recevoir le contenu utilisateur. `nsh` et le courtier ne peuvent attester une provenance ni choisir un remplacement.

Références : [contrat du retrait](novaauth-r2-retrait-groupe-contrat-v0.24.0.md), approuvé à partir de `0e7d0b2141e976e99d3a676957e0517eba0e8782` ; [dernier relevé mémoire et drainage](novaauth-r2-memoire-drainage-v0.24.0.md). Le présent document propose une extension privée QEMU distincte ; son adoption exige l'arbitrage demandé.

## 2. Analyse du prototype conservé

| Élément du prototype | Apport | Insuffisance avant adoption |
|---|---|---|
| `View::revalidate_execution` | Vérifie une installation et son image sur la vue courante, avec époque utilisateur | Ne constitue pas, seul, une preuve des dépendances historiques du ticket ni une admission atomique face à un retrait |
| `View::executable_origin` | Transporte une permission et sa révision | La lecture du paquet personnel n'est pas une autorisation EXEC issue de G ; il faut démontrer une dépendance d'exécution réelle |
| `membership_origin: [u64;3]` | Groupe et révision conservés dans Admission/Ticket | Une seule provenance ; zéro ne doit jamais signifier à la fois Direct et inconnu ; les provenances multiples ne sont pas représentées |
| `membership_close` et inspection EXEC | Recherche ciblée des tickets et des portes en vol | La présence d'un tuple ne prouve pas son authenticité ni la couverture des lecteurs/producteurs de pipe |
| Messages privés de liaison | Séparent les contrôles du CLI et de R | Liaison complète avec session, instance, installation, image, objet et décision d'émission à démontrer |
| Extension du profil | Rend le prototype compilable dans un export particulier | Export non qualifié, à ne pas utiliser pour les essais indépendants |

Les mesures conservées du prototype donnent Ticket 608→632 octets et Admission 584→608 sur les deux architectures. Elles ne démontrent **pas** le respect du budget agrégé avec deux routes et les lecteurs préparés. Les fichiers suivis et non suivis sont conservés avec leurs empreintes dans les preuves du mandat. Aucun de ces changements n'est inclus dans cette proposition documentaire.

## 3. Descripteur immuable proposé

Proposition préférée : un **descripteur durable de dépendances**, enregistré par R auprès de l'autorité avant de rendre un ticket utilisable. Ce descripteur n'est ni une admission ni une capacité. Son identifiant n'autorise rien à son porteur. Il ne peut être créé ou modifié sur une simple déclaration du CLI.

Le descripteur lie explicitement :

- version et domaine de signature privés ; serveur et domaine NovaAuth ; incarnation approuvée de R ; instance du titulaire et instance de l'effecteur EXEC ;
- identifiant stable de l'utilisateur, époque d'authentification, session authentifiée, instance `nsh` et route exacte ;
- identifiant de requête unique, identité de l'installation et son destinataire, version/architecture attestées, empreinte complète de l'image scellée, objet et volume effectivement chargés ;
- rôle du lancement, droits minimaux et objets nécessaires à l'exécution ; le rôle lecteur/producteur reste distinct de l'autorisation d'installation ;
- génération et empreinte initiales du triplet, révisions propres aux permissions, règles et installation ; échéance locale et domaine d'horloge de l'instance ;
- liste canonique des dépendances effectivement nécessaires, calculée par R et non choisie par l'application.

Les dépendances doivent venir des autorisations et délégations réellement nécessaires au lancement. Si les contrats existants n'attachent aucun droit de G à ce lancement, il est indépendant : le banc ne peut le déclarer « issu de G » par une étiquette. Une lecture de paquet ajoutée artificiellement ne démontre pas cette dépendance. Le futur raccord doit prouver ce lien sans créer une nouvelle capacité d'exécution ou un nouveau mode d'enrôlement.

Chaque dépendance porte un type explicite (Direct ou Group), le sujet, l'objet, l'opération/droits nécessaires, la révision de règle, la révision d'installation et, seulement pour Group, le groupe stable et la révision d'appartenance. Les champs non applicables ont une valeur canonique vérifiée ; type inconnu, doublon contradictoire, ordre non canonique, zéro ambigu et troncature sont refusés. Les empreintes SHA-256 et les identités ne sont jamais tronquées. Une liste vide n'est pas une preuve de droit personnel.

Borne de conception proposée : quatre dépendances par descripteur, corps canonique ≤ 1 024 octets, signature séparée de 64 octets. Les dépendances sont une **conjonction de conditions nécessaires**, non une liste d'alternatives laissées au client. Un ticket qui exige Direct pour un objet et G pour un autre dépend de G. Si H suffirait aussi à autoriser une opération initialement déléguée par G, l'ancien ticket G reste retiré : un nouveau ticket H peut être demandé selon les droits actuels. Aucun changement silencieux de provenance ne sauve l'ancien ticket. Au-delà de quatre dépendances : LIMIT, avant émission du ticket.

Le codage exact, les tailles natives et l'encodage canonique devront être vérifiés avant raccord ; la borne proposée n'autorise aucune extension automatique de format. Les messages IPC restent de **128 octets** : version, identifiant, index, total, longueur et empreinte du message complet lient les fragments. Un assemblage incomplet, dupliqué contradictoire, mélangé ou surnuméraire est refusé. Réutiliser les buffers bornés du protocole existant ; les enveloppes applicatives ne sont pas les métadonnées de liaison (56/256 octets). Aucune nouvelle cryptographie n'est proposée.

## 4. Émission et liaison non substituable

1. R vérifie l'instance, la session, l'époque et l'installation sur une tête indépendante fraîche, y compris les réservations et retraits actifs.
2. R calcule les dépendances depuis les objets de politique faisant autorité. Il lie les octets que NovaFS atteste aux octets scellés que le noyau chargera. Une permission READ du fichier du paquet ne suffit pas à conférer EXEC.
3. R enregistre le descripteur immuable par une opération privée authentifiée, avec comparaison de tête dans la transaction de l'autorité. Une tête modifiée provoque un refus/CONFLICT ; aucun ticket utilisable ne sort d'une issue inconnue.
4. Le noyau lie la référence au ticket de l'instance désignée sur la voie privée déjà authentifiée. Les métadonnées du ticket et le descripteur doivent correspondre intégralement. Une référence seule, même authentique, ne remplace pas ce contrôle.
5. R choisit et authentifie l'identifiant complet ; une valeur fournie par le client n'est pas une preuve. Toute collision avec un enregistrement différent est refusée transactionnellement. Le champ `Gate.operation` devient immuable après liaison, y compris dans Dormant ; une demande ultérieure ne peut pas le remplacer. L'identifiant unique est conservé jusqu'au résultat terminal. Une inscription perdue se consulte par identifiant, sans nouvelle inscription implicite. Une inscription sans ticket après panne est un enregistrement sans pouvoir, jamais une admission en vol ; son nettoyage ne permet pas de réutiliser son identifiant.

Les tickets historiques sans descripteur ne sont pas convertis en tickets Direct. Dans le profil expérimental, ils conservent leur contrôle historique strict. Les anciens appels SPAWN, NXEXEC01 et les contrats binaires des ELF existants restent inchangés. Un protocole privé versionné distingue la nouvelle liaison ; conserver 48 sélecteurs ne suffirait pas à prouver cette compatibilité.

## 5. Revalidation et admission indivisible face au retrait

À la présentation du ticket, le noyau conserve tous ses contrôles : titulaire/instance, référence générationnelle, session, expiration monotone locale, statut non révoqué, consommation unique et image scellée.

R retrouve le descripteur authentifié, compare la totalité de ses liaisons aux métadonnées noyau, obtient une tête courante authentifiée et vérifie le triplet contre cette tête. Il vérifie les dépendances initiales, leurs droits exacts et leurs révisions propres. Une modification de la seule génération globale ne suffit plus à annuler un ticket indépendant ; une modification de ses propres conditions ne lui accorde aucun droit nouveau.

R soumet la décision de revalidation et l'admission sous le **même identifiant**, avec la tête fraîche, le descripteur et toutes les provenances nécessaires. L'autorité vérifie dans une seule transaction durable : incarnation active, époque/session admissibles, tête comparée, absence de retrait réservé/en cours/terminal interdisant ces provenances et absence d'admission antérieure incompatible. Elle enregistre une seule position d'admission. Une vérification fraîche suivie d'une admission non liée serait une course interdite.

Si la tête change entre vérification et admission, l'admission échoue sans création. Une nouvelle consultation de tête est une lecture ; elle n'autorise pas à répéter une mutation d'issue UNCERTAIN. L'issue originale doit être retrouvée avant toute décision. Après un reçu d'admission, le noyau compare l'identifiant, l'instance, la position et la liaison complète, puis conserve la barrière locale existante `Requested → Admitted → Creating → Created/Cancelled → Settled` sous la même sérialisation.

Le retrait de G inventorie aussi chaque admission qui dépend de G parmi plusieurs origines. Une seule opération et un seul enfant existent ; chaque retrait concerné attend l'issue réelle de cette opération. Aucun compteur indépendant ne peut masquer l'opération partagée.

## 6. Décisions et statuts

| Situation | Condition et résultat |
|---|---|
| Ticket Direct/H ; seule G change | Même utilisateur, époque, session, instance, installation, image, objet, droits et révisions propres ; admission fraîche acceptée, un seul enfant |
| Ticket G ou Direct+G nécessaire | Ancienne provenance G barrée : REVOKED (10), avant expiration, aucun enfant nouveau |
| Ticket H dont la propre règle/installation change | Revalidation propre refusée ; ne pas déduire sa validité de l'absence de G |
| Ancien ticket après réactivation utilisateur ou réajout G | Toujours révoqué ; nouvelle époque/révision ne restaure pas l'ancienne référence |
| Expiré, consommé, déjà révoqué | Refus natif existant du chemin compétent ; ne pas le convertir artificiellement en preuve de retrait G |
| Identité/provenance/preuve altérée | DENIED/BAD_ARGUMENT/REFUSED suivant le contrôle exact ; aucune admission ni mutation autorisante |
| Tête comparée modifiée | CONFLICT (24) si ce contrat du composant s'applique ; aucun changement de droits implicite |
| Autorité ou reçu indisponible | Refus sûr/UNCERTAIN (22) selon qu'une soumission a eu lieu ; identifiant conservé, zéro répétition automatique |
| Borne atteinte | LIMIT natif, ressources provisoires restituées, jamais succès partiel |

Le banc doit consigner le premier statut natif, son composant et l'état local/distant, séparément du statut CLI. Les horloges hôtes ne se soustraient pas aux échéances invitées. Les délais TLS, baux et expirations restent identiques.

## 7. Révocation, drainage et acquittements

L'ordre est celui de l'admission durable, pas celui de la signature ou de la réponse. Une admission antérieure au blocage entre dans le drainage ; une admission postérieure fondée sur G est refusée. Une réponse d'admission retardée ne permet pas de contourner la porte locale annulée.

Conserver les trois classes distinctes : 1 VFS, 2 vérificateur/délégations, 4 EXEC. Les classes 2 et 4 peuvent partager l'identité authentifiée du vérificateur, mais jamais leur acquittement. Chaque ACK conserve classe, instance, retrait, opérations concernées et preuves durables. Les requêtes/réponses de 128 octets sont vérifiées intégralement. Une réponse unique ou un masque à zéro ne fabrique aucun ACK.

L'inventaire local doit couvrir aussi les tickets Dormant non encore présentés et les lecteurs de pipe préparés. Le `LIST` actuel ne retourne que les tickets en attente : il ne suffit pas à cet inventaire. La représentation proposée par référence nécessite donc un parcours privé borné du propriétaire R et une fermeture par identifiant lié, avec vérification noyau de R, de son instance, du titulaire et de la référence immuable. R consulte le descripteur authentifié ; le noyau ne prend jamais un nom de groupe fourni par le CLI pour une preuve. Ce complément privé est **à arbitrer avec ce contrat**, pas une fonctionnalité déjà disponible.

Le parcours peut réutiliser la sérialisation et les buffers de contrôle existants, mais doit prouver qu'une nouvelle liaison de ticket ne peut se glisser après l'inventaire : bloquer l'origine à R, réserver le retrait durable, recontrôler les liaisons en cours et fermer sous la sérialisation noyau avant ACK. Une référence enregistrée avant retrait, mais pas encore liée à un ticket, ne peut être liée depuis une vieille réponse sans ce recontrôle. Sans inventaire complet et protection de cette course, pas d'ACK EXEC. Une inspection ou une annulation adressée à une autre instance est refusée. Le parcours n'accorde aucun droit public général d'observation ou d'arrêt.

Pour EXEC, chaque admission antérieure doit être achevée avec l'enfant identifié, ou annulée irréversiblement après nettoyage. Un ticket non admis peut être rendu inutilisable localement ; aucune annulation distante d'un lancement admis ne s'en déduit. Les délégations G de l'enfant sont retirées, sans inventer un pouvoir général d'arrêt. Les tickets/enfants Direct ou H ne sont pas révoqués par cette opération.

RETRAIT_EN_COURS interdit durablement les nouvelles admissions concernées. RETIREE exige les issues individuelles, les barrières locales et les trois ACK durables requis. Ni timeout, ni coupure, ni mort supposée d'une ancienne VM ne prouve cette condition. Une nouvelle incarnation reste inactive si le drainage antérieur manque.

## 8. Reprise et anti-rollback

Un reçu historique permet de retrouver une décision, pas d'autoriser une nouvelle exécution. Après interruption, R retrouve le descripteur, l'admission et les réservations signées par identifiant, vérifie la tête durable et le triplet, puis consulte les issues. Il ne recrée ni ticket consommé ni ancienne identité de session. La fermeture locale reste effective même si le retrait distant attend son acquittement.

Restaurer P9 et les disques locaux ne restaure pas la tête externe, les époques, les barrières ni les identifiants consommés. L'autorité indisponible interdit une nouvelle admission. Une reprise administrative valide l'incarnation, jamais un ancien triplet ou une ancienne capacité. Un retrait incomplet dont les effecteurs ont disparu reste incomplet : le contrat existant ne fournit pas une preuve automatique de neutralisation. Une telle nouvelle procédure demanderait un arbitrage distinct.

## 9. Budget et stratégie de représentation proposée

Mesure publiée : **3 994 / 4 096 octets**, marge **102**. Ce document ne revendique aucune mesure de son mécanisme non implémenté.

Proposition à vérifier au layout natif : réutiliser l'identifiant de requête de 16 octets déjà présent dans `exec_snapshot_lab::Admission`, puis le champ `Gate.operation` de 16 octets dans le ticket. Il devient lié une seule fois avant exposition ; l'état Dormant continue de signifier « aucune admission ». La référence n'est pas un reçu. `Remote.head` conserve la tête d'émission complète : aucune suppression ou réinterprétation silencieuse de ce contrôle historique.

Le lecteur préparé de pipeline possède déjà sa liaison et sa tête mais pas cet identifiant : prévoir **16 octets par Reader**, au maximum un par route, deux routes, soit 32 octets utiles supplémentaires. Cette estimation ne vaut pas preuve de padding ni du pic de transfert Admission/Reader/Ticket. Aucun tableau de provenances supplémentaire n'est placé dans les tickets : le descripteur durable est interrogé de façon bornée et authentifiée. Le stockage externe de descripteurs reste lui aussi borné, avec quota, refus à saturation et rétention des résultats d'issue incertaine ; il ne constitue pas un contournement du budget invité.

Avant tout raccord après approbation : compiler les layouts ARM64/x86_64, compter les deux routes, lecteurs, tickets, options et états simultanés, vérifier le total ≤4 096 et les piles/allocations temporaires. Réutiliser les buffers existants sans conserver deux corps complets simultanément. Si la référence ou le corps ne tient pas, suspendre ; pas de hash tronqué, pas de champ de sécurité supprimé, pas de hausse implicite. Les 32 octets sont une **estimation de conception**, pas un total qualifié de 4 026.

IPC : mêmes conversations privées, aucun processus, racine ou handle permanent supplémentaire. R, NovaFS, vérificateur, signataires, courtier et chaque `nsh` restent chacun ≤4 racines pendant émission, revalidation, concurrence, erreurs et reprise. Les messages fragmentés utilisent une seule conversation sérialisée, sans masquer plusieurs racines derrière un courtier. Mesurer avant/pendant/après, cinquième demande LIMIT et zéro allocation résiduelle.

## 10. Matrice proposée après arbitrage

Chaque ligne est à exécuter sur x86_64/P9-512, x86_64/P9-4096, AArch64/P9-512 et AArch64/P9-4096. **Toutes sont non exécutées pour ce nouveau contrat.** Les tests hôtes et le prototype ne les remplacent pas.

| ID | Préconditions et entrelacement | Preuve exigée |
|---|---|---|
| E1 | Ticket Direct émis avant retrait G | Même référence, conditions propres valides, un PID, sortie/statut exacts |
| E2 | Ticket H émis avant retrait G | Même référence, H conservé, un PID ; ancienne génération globale différente |
| E3 | Ticket réellement dépendant de G | REVOKED avant échéance, aucun PID, provenance attestée |
| E4 | Direct+G, H+G, Direct+H nécessaires | Deux premiers refusés, dernier admis ; aucune substitution d'origine |
| E5 | Altérations identité/session/objet/image/provenance/preuve | Refus au composant compétent, aucun droit ni mutation autorisante |
| E6 | Consommé, expiré, révoqué, malformé ; rejeu après boot | Statuts natifs distincts, jamais seconde création |
| E7 | Mutation entre lecture de tête et admission | Comparaison atomique, refus ou admission ordonnée puis drainage |
| E8 | Admission acquittée, création retardée, retrait concurrent | RETIREE impossible tant que l'issue n'est pas démontrée |
| E9 | Réponse perdue inscription/admission/issue/ACK | Consultation même ID, un seul lancement/mutation, trois ACK distincts |
| E10 | Deux consommateurs, deux routes, lecteur/producteur | Atomicité, identités isolées, références et preuves propres à chaque ELF |
| E11 | Restauration locale et autorité indisponible | Ancien état/ticket inutilisable, aucun succès déduit du cache |
| E12 | Saturation, fragments contradictoires, interruption, mémoire | Rejets bornés, drainage, ressources et budget exacts |

Archiver commit/arbre, hashes binaires, origines et IDs non secrets, position durable, chronologie monotone invitée, échéance/marge, PID et statuts, sorties binaires, inventaire ACK, compteurs de ressources. Conserver la réserve instrumentée AArch64 de 301 cadres et la reproductibilité entre environnements. Ne pas relancer les recettes historiques non affectées.

## 11. Risques résiduels et critères de refus

- R reste dans la base de confiance pour les décisions de politique ; un canal authentifié ne rend pas juste une mauvaise décision. Les tests doivent confronter les dépendances calculées au triplet et aux octrois effectivement transmis.
- Le descripteur rend les dépendances traçables mais ajoute des écritures et des consultations durables. L'indisponibilité reste un refus sûr ; les délais et baux ne sont pas prolongés pour absorber ce coût. Préparer les octets immuables avant authentification ne dispense pas de revalider les droits au moment requis.
- La borne de quatre dépendances et celle du stockage externe peuvent refuser un lancement complexe. Ce refus est explicite ; aucune dépendance n'est omise pour faire tenir la demande. La purge ne peut supprimer une issue UNCERTAIN ou permettre de recycler une référence historique.
- Les références privées, identités et empreintes ne sont pas des secrets d'autorisation, mais leur divulgation n'autorise ni remplacement ni lecture des données. Aucun contenu de ticket, clé ou fichier privé n'est journalisé.
- Une panne après admission ne vaut pas annulation ; la disponibilité peut rester bloquée tant que la création ou l'annulation de l'opération n'est pas démontrée. Ce risque ne se résout pas en déduisant un ACK de la mort de la VM.
- La conservation d'une référence ancienne n'implique pas la conservation de ses droits après changement propre. Les témoins positifs Direct/H doivent garder strictement les mêmes conditions propres, et les témoins négatifs modifier une dépendance nécessaire identifiée.

## 12. Arbitrage ciblé demandé

Valider ou amender : (a) descripteur immuable enregistré avant émission, lié aux références noyau ; (b) dépendances nécessaires explicites, sans substitution de provenance ; (c) revalidation et admission liées à la même tête dans l'ordre durable ; (d) représentation bornée réutilisant les identifiants existants, avec vérification du layout avant raccord ; (e) types et fragmentation privés versionnés, sans changement public ; (f) inventaire privé incluant Dormant et lecteurs préparés, fermeture liée et protection de la course émission/retrait. Cette dernière obligation est indispensable au choix de ne pas dupliquer toutes les provenances dans le noyau.

Le prototype actuel ne satisfait pas encore ces obligations et reste désactivé. Q2/Q3/Q4 indépendants peuvent progresser ; les résultats réels figurent dans un rapport séparé. Aucune clôture R2, qualification de production, fusion ou création de tag n'est autorisée par ce document.
