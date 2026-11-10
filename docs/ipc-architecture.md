# Contrat IPC proposé pour NovaOS v0.8.0

**Statut — conception soumise à validation, base `4900cf5`.** Les règles ci-dessous spécifient le comportement futur sur CPU0. Les formats, les propriétés à protéger et leurs oracles de test sont définis dans les documents associés. [ABI IPC](ipc-abi.md) · [Sécurité](ipc-security.md) · [Plan de validation](ipc-test-plan.md)

## Point de départ

La v0.7.0 expose six appels natifs. Son seul droit transférable par le lanceur, `WaitRight`, concerne la récolte du statut d’un processus. La table des processus et l’ordonnanceur ont chacun 32 emplacements. Un processus dispose actuellement d’un thread utilisateur. `Event` réveille un thread sur CPU0 et `Wait::Process` sert à l’attente d’une terminaison. Le tick matériel est configuré à 250 Hz. Un endpoint, sa file et les capacités ci-dessous seraient de nouveaux objets. [Contrat existant](abi.md) · [Processus](../kernel/src/sched/process.rs) · [Attentes et sélection du thread](../kernel/src/sched/mod.rs)

## Autorité, identité et propriété

Un **endpoint** est une boîte de réception unicast. Plusieurs détenteurs peuvent produire ou recevoir. Chaque élément livré a exactement un destinataire. Pour répondre, un service doit obtenir séparément un droit d’envoi vers un autre endpoint. Le tag d’un message est une donnée de l’application.

| Droit | Bit | Autorité conférée par ce bit |
| --- | ---: | --- |
| `SEND` | 1 | Offrir des données, en déposer dans la file, signaler des bits de notification. |
| `RECEIVE` | 2 | Prélever les données, les bits ou une capacité offerte sur cet endpoint. |
| `GRANT` | 4 | Dériver le droit présenté et autoriser le passage de capacités sur un canal. Un don exige aussi `SEND` sur le canal. |
| `MANAGE` | 8 | Révoquer ses descendants et annuler les attentes engagées sur le canal par sa branche. |

Trois identités ont des durées de vie différentes.

1. **Objet** — une référence noyau contient type, slot et incarnation. À la réutilisation du slot, l’incarnation change. Cette règle vaut aussi pour les nœuds de dérivation et les références d’attente. Un couple périmé ne peut désigner son successeur.
2. **Nœud de dérivation** — il porte une référence à l’objet, un parent éventuel et un masque immuable. L’arbre reste attaché au même objet. La racine appartient au processus créateur et porte `0xF`. Le propriétaire est identifié avec sa génération.
3. **Handle** — un mot opaque `u64` indexe une entrée dans la table **du processus appelant**. Une entrée vivante désigne un seul nœud et un nœud a au plus un handle vivant. Le noyau propose un numéro d’émission global non nul sur 62 bits, monotone pendant le démarrage, avec les deux bits hauts `01`. Chaque installation, déplacement compris, reçoit un numéro neuf. Le format reste privé. Zéro signifie absence. Les identités internes utilisent également des générations contrôlées. Un compteur épuisé refuse toute création qui en dépend avec `LIMIT` et permet encore le drainage et la destruction. Aucune génération ne reboucle.

La preuve d’autorité est la correspondance complète dans la table locale, le type, la validité de la branche et les bits requis. Un PID, un TID, une adresse, des octets reçus ou la clé du `process_wait` existant ne constituent pas cette preuve. Un handle déjà attribué au même processus est utilisable par tous ses threads. Le modèle protège les frontières de processus.

### Opérations sur l’arbre

| Opération | Effet atomique |
| --- | --- |
| Créer | Réserver l’objet, ses huit places de file et la racine. Publier le premier handle dans la table du créateur. En cas d’échec, aucune publication. |
| Dupliquer localement | Exiger `GRANT` sur la source. Créer un enfant avec un sous-ensemble non vide des droits du parent. Attribuer un handle local neuf. |
| Donner par copie | Exiger `SEND\|GRANT` sur le canal et `GRANT` sur la source. Au rendez-vous accepté, installer chez le receveur un enfant atténué. La source subsiste. |
| Donner par déplacement | Mêmes contrôles. La source doit être un nœud non racine et le masque demandé doit être identique. Au rendez-vous, éteindre le handle source et installer un handle neuf sur **le même nœud**. Ses descendants et sa position dans l’arbre subsistent. |
| Fermer un handle ordinaire | Retirer le handle et invalider les opérations en attente qui l’utilisaient. Les descendants persistent. Garder un nœud structurel sans handle tant qu’il relie des descendants, puis élaguer récursivement les feuilles sans handle. |
| Révoquer | `MANAGE` invalide tous les descendants stricts du handle présenté, y compris ceux détenus par d’autres processus. Le handle présenté et les branches sœurs subsistent. Retirer aussi les offres qui dépendaient d’un handle invalidé, quel que soit leur canal. |
| Fermer la racine | Détruire l’endpoint, écarter ses données et notifications restantes, terminer ses attentes et invalider tout son arbre. La racine peut être copiée sous forme d’enfant. Seul le créateur peut fermer la racine. |

Pour déléguer un accès révocable individuellement, le propriétaire garde un enfant doté de `MANAGE|GRANT|SEND` et donne par copie un descendant `SEND` au client. Révoquer cet enfant coupe le client. Un gestionnaire délégué ne peut ni révoquer ses ancêtres ni détruire l’endpoint en fermant son propre handle.

**Amorçage proposé.** Le lanceur de confiance peut créer la racine pour un service et dériver, depuis celle-ci avec `GRANT`, un handle atténué pour un client avant leur première exécution. Le manifeste de lancement nomme le destinataire et le masque. Chaque processus a un emplacement d’amorçage facultatif, alias d’un handle de sa table et compté une seule fois. Le groupe est publié après réservation complète, ou entièrement abandonné. `ipc_bootstrap` relit cet alias. Après fermeture, déplacement ou révocation, l’alias reste vide. Un processus sans alias peut créer son propre endpoint. La découverte de services par nom et la distribution dynamique par PID restent hors du contrat.

### Exemple de délégation et de révocation

Le service S possède la racine de sa boîte E. S et le client C ont déjà un canal de configuration K issu de l’amorçage. S y détient SEND et GRANT, C y détient RECEIVE.

1. S appelle `ipc_dup` sur sa racine E avec le masque 13 (SEND, GRANT, MANAGE) et conserve ce handle A.
2. Depuis K, S offre une copie de A avec le masque 1. C appelle `ipc_receive` sur K avec ACCEPT_CAP. Le handle H reçu par C désigne un enfant de A. S garde A.
3. C dépose le message M avec H en mode asynchrone et obtient OK. Une tentative de duplication de H donne DENIED.
4. S appelle `ipc_revoke(A)`. La valeur de retour est 1 si H est le seul descendant vivant. Une nouvelle émission de C avec H donne DENIED.
5. S reçoit encore M avec sa racine E. Le dépôt avait réussi avant la révocation. Fermer A ensuite laisse E utilisable jusqu’à la fermeture de sa racine.

## Messages, notifications et ordre

| Élément | Stockage et point de succès |
| --- | --- |
| Données asynchrones | Le noyau copie de 0 à 128 octets et un tag de 64 bits. Le succès intervient au dépôt dans la FIFO de huit places. Une FIFO pleine renvoie immédiatement `WOULD_BLOCK`. |
| Données synchrones | Une offre possède une copie immuable du même message et une échéance finie. L’émetteur ne réussit que lorsque le receveur a obtenu la copie complète. Un rendez-vous ne promet aucune réponse applicative. |
| Don de capacité | Une offre synchrone porte le mode, le tag, le masque et les identités validées de la source et du canal. Elle ne réserve aucune autorité visible au destinataire avant l’acceptation. Un don passe par un canal sur lequel les deux interlocuteurs détiennent leurs propres droits. |
| Notification | `SEND` ajoute par OU un masque non nul de 64 bits au mot en attente. Le premier signal prend un rang. Les suivants gardent ce rang. Une réception livre tous les bits et les efface ensemble. Le nombre de répétitions et l’identité des signaleurs ne sont pas conservés. |

Chaque endpoint ordonne ses dépôts asynchrones, ses offres synchrones et sa notification par **rang d’admission**. Le prochain élément est le plus ancien disponible parmi les trois. Un seul rang est attribué par transaction. Une notification après drainage prend un nouveau rang. L’ordre des receveurs est FIFO et un nouvel appel ne dépasse pas un receveur déjà éligible. Les offres ont leur propre capacité bornée et peuvent attendre alors que la FIFO asynchrone est pleine. Un rang peut être renuméroté à vide quand aucune entrée ne le référence. Une saturation du compteur refuse les nouvelles admissions avec `LIMIT` jusque-là.

**Livraison.** Le premier receveur reprend sur sa pile noyau et revalide sa destination. Le producteur et le timer se limitent à publier un élément ou à rendre un receveur éligible. Le noyau choisit à nouveau la tête lors de la reprise. Copier la sortie complète, publier un éventuel handle, retirer l’élément et fixer le résultat du producteur synchrone forment une seule transaction. Une erreur d’adresse retire uniquement la tentative du receveur. L’élément garde son rang. Les octets d’une sortie en erreur restent intacts.

Pour une offre de capacité en tête, le receveur exprime son accord par `ACCEPT_CAP`. Un refus explicite termine les deux appels avec `REFUSED`. Une table destinataire pleine termine les deux avec `LIMIT`. Dans ces deux cas l’offre quitte la file, le handle source reste utilisable et aucun handle destinataire n’est émis. Ces règles empêchent une offre inexécutable de bloquer définitivement les données suivantes.

### Attentes et terminaison

Une opération bloquante suit `PRÉPARÉE → EN_ATTENTE → TERMINÉE(résultat)`. Une satisfaction immédiate relie directement les deux extrêmes. Être prêt dans la file du scheduler ne vaut pas livraison. Chaque opération en attente garde l’identité complète de son thread, des handles dont elle dépend, son échéance et son type. Le couple prédicat/inscription est vérifié dans la même section protégée que le changement d’état du TCB. Un événement survenu juste avant l’inscription est donc consommable immédiatement. Après un réveil, le noyau relit le résultat ou le prédicat avant tout nouveau blocage. Si une autre transaction a retiré l’offre sans terminer la réception, le receveur garde son rang, son crédit et son échéance d’origine. Un TCB déjà READY reçoit son résultat sans nouvelle insertion dans la file des prêts.

- Une réception à délai zéro essaie une fois. Une offre synchrone impose un délai positif car seul le receveur peut valider la livraison. Les délais positifs vont de 1 à 15 000 ticks livrés. À l’admission, le noyau calcule une seule échéance par addition vérifiée. Le sommeil ou un réveil parasite ne la prolonge pas.
- Avant chaque décision de livraison, d’annulation, de révocation ou de fermeture, les opérations concernées dont `ticks >= échéance` sont terminées avec `TIMED_OUT`. Le tick rend aussi les attentes expirées prêtes. À tick égal, l’expiration l’emporte. Pour les autres courses, la première transaction validée fixe définitivement le résultat. La reprise en espace utilisateur peut intervenir plus tard. Une addition d’échéance hors u64 donne LIMIT à l’admission. L’horloge IPC doit saturer avant tout retour à zéro. À saturation, les attentes positives existantes expirent et les essais immédiats restent utilisables.
- `ipc_cancel(h)` exige `MANAGE`. Il termine avec `CANCELLED` les appels encore en attente sur cet endpoint dont **le handle de canal** appartient au nœud présenté ou à ses descendants. Une seconde annulation sans nouvelle attente réussit avec zéro. Les dépôts asynchrones et les bits déjà acceptés subsistent.
- La fermeture ou le déplacement d’un handle ordinaire termine avec `REVOKED` les appels encore dépendants de son ancienne entrée, y compris les dons sur d’autres endpoints. Le déplacement en train de réussir est exclu. La destruction de l’endpoint donne `CLOSED` aux attentes sur son canal, puis `REVOKED` aux offres sur d’autres canaux dont la source disparaît. Un appel ultérieur avec le vieux numéro reçoit `DENIED`.
- Mort normale ou faute confinée suivent la même procédure. Marquer l’espace du processus mourant inaccessible à de nouvelles transactions, retirer ses propres attentes, détruire ses endpoints créés, puis fermer ses handles importés. Tous les endpoints du même créateur mourant sont marqués fermés ensemble avant de résoudre les dépendances distantes. Pour un don resté en attente dont le canal et la source disparaissent dans cette même transition, CLOSED sur le canal a priorité sur REVOKED. La mort de l’acteur termine ses propres appels sans retour utilisateur. Ce nettoyage logique précède la réutilisation des slots de processus et de threads. La restitution physique peut être différée jusqu’à la disparition des dernières références internes transitoires. Un producteur mort laisse ses dépôts asynchrones déjà acceptés sur un endpoint survivant. La perte d’un simple receveur laisse les autres receveurs et les offres non livrées poursuivre jusqu’à leur échéance.

## Ressources et sobriété

| Ressource | Limite proposée | Imputation |
| --- | ---: | --- |
| Endpoints | 4 créés par processus, 64 en tout | Créateur, jusqu’à destruction |
| Handles installés | 32 par processus, racine et amorçage inclus | Détenteur, jusqu’au retrait |
| Nœuds de dérivation | 32 par endpoint, 256 en tout, profondeur maximale 8 (racine à 0) | Endpoint source, y compris ancêtres structurels et réservations de copie |
| Dépôts asynchrones | 8 par endpoint, 128 octets utiles chacun | File du créateur. Au plus 32 places pour ses quatre endpoints |
| Offres synchrones | 8 par endpoint, données et dons confondus | Thread producteur |
| Receveurs inscrits | 8 par endpoint | Thread receveur |
| Appels IPC en attente | 1 par thread, 4 par processus, 32 en tout | TCB de l’appelant, offre ou réception |
| Notifications | 1 mot de 64 bits par endpoint | Endpoint, inclus dès sa création |

La limite globale de nœuds borne également à 256 le nombre de handles vivants. Les files asynchrones peuvent contenir au plus 64 Kio de payload au total et les offres bloquées au plus 4 Kio de payload supplémentaires. Les quotas incluent les réservations jusqu’à leur publication ou leur libération. Un endpoint créé réserve sa file pour éviter une allocation lors d’un envoi asynchrone. Proposer un double par don réserve le nœud enfant avant de bloquer. Cette réservation reste invisible et dépend de la source pour son annulation. La table locale est provisionnée pour ses 32 entrées au premier handle. L’installation finale chez un receveur déjà détenteur de `RECEIVE` peut alors utiliser un slot libre sans allocation à ce point. Le chemin timer ne construit ni ne libère de page. Les métadonnées et charges rendues sont effacées avant réattribution.

**Enveloppe à vérifier.** Une page de 4 Kio par endpoint couvre ses huit dépôts et ses têtes de file, soit au plus 256 Kio. Des pools en pages partagées entre entrées prévoient 256 nœuds de 64 octets (16 Kio), 32 × 32 slots de 32 octets (32 Kio) et 32 attentes de 256 octets (8 Kio). Les plafonds de taille de ces enregistrements sont des contraintes de représentation à vérifier à la compilation. Le sous-total est de 312 Kio. Le budget physique dynamique maximal est de 512 Kio, tables de traduction additionnelles, arrondis, pools temporairement incomplets et réservations compris. Des pages de pool entièrement vides sont restituées. Après destruction complète et collecte, le delta dynamique doit être nul. Le répertoire et les compteurs permanents disposent de 4 Kio de données statiques au plus. Le code et ces données entrent dans le plafond global de **8 Mio de RAM physique au repos**. Le contrôleur d’admission doit compter les pages promises et refuser avec LIMIT une opération qui franchirait le plafond global, même si un slot logique est libre. Les tampons de préparation bornés sur les piles noyau déjà mappées sont mesurés en profondeur de pile. La référence historique à 256 Mio est de 968 Kio sur Q35 et 2 740 Kio sur virt, ramfb inclus. [Mesures v0.7.0](mesures-v0.7.0.md)

## Choix coûteux à modifier

Les handles locaux sans réemploi, la racine liée au créateur, la révocation transitive, le point de succès d’un envoi synchrone, la survie des dépôts asynchrones et la fusion des notifications déterminent ce que les services peuvent déduire d’un retour. Les modifier impose une migration de protocole, voire une nouvelle ABI. Les trois dispositions binaires et le maximum de 128 octets sont également figés pour cette extension. Les capacités de file et quotas pourront être relevés après mesure en préservant leurs erreurs et l’ordre FIFO.

Une extension de mémoire partagée ou de transfert sans copie demanderait son propre type d’objet, des droits de mappage, des règles de prêt, d’épinglage et de révocation des traductions, avec quotas de cadres et invalidations TLB. Aucun champ du contrat proposé n’autorise à interpréter une adresse transmise comme une page partagée.
