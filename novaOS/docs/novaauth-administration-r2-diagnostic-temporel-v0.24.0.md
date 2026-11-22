# R2 — Diagnostic temporel P9/4096, sans nouvel invité

## Périmètre et verdict

Analyse du 22 novembre 2026, depuis `0e5004d1d78bcc5d61211b48c69ca44135fb7d09`.
Trace du banc `63f6e19d9161833e0763e07e9940243f7474114d`, ISO inchangée
`4fd9df53602629bf04b12894da84d391e6e6131745585a172302e26085c8e781`.
**Zéro lancement QEMU, zéro compilation, aucun changement des mécanismes invités.**

La cause immédiate est démontrée : le bail de Bob couvre les contrôles préalables,
le lancement et l'exécution d'une sonde négative, sa collecte, puis une attente
fixe de 15 secondes avant le ticket du témoin. Le chevauchement avec Alice ne
retire pas ces opérations du bail de Bob. L'événement nommé « préparation terminée »
est produit par expiration d'un délai, pas par acquittement de la préparation d'Alice.
Il ne constitue donc pas une synchronisation explicite entre les deux parcours.

La décomposition exhaustive des coûts IPC, stockage, vérification et
ordonnancement n'est **pas** mesurable dans les traces disponibles. La cause du
surcoût P9/4096 par rapport à P9/512 reste à établir ; ne pas l'attribuer à la charge
hôte, ni annoncer une cause racine du système démontrée. Le défaut d'ordonnancement
et les refus d'expiration, eux, sont établis.

## Horloges et signification des mesures

- `NXAUTH` et `NXBAIL` utilisent NOW, en millisecondes monotones invitées.
- `NAEXEC attente` utilise les ticks du même noyau, HZ=250 : conversion exacte
  `ms = ticks * 4` dans ce démarrage. Ce n'est pas une horloge hôte.
- Le noyau implémente NOW avec `ticks()*1000/HZ` dans
  `novaOS/kernel/src/sched/process/ipc/service.rs`; HZ est défini dans `sched/mod.rs`.
- Les fichiers de coordination, `rpc-public.log` et `tls-attentes.log` utilisent
  `time.monotonic_ns()` côté hôte. Aucune soustraction avec NOW n'est effectuée.
- Les intervalles invités sont des temps écoulés, incluant blocages et attente,
  **pas des temps CPU**. Les pauses du collecteur et le temps hôte ne peuvent pas
  être retranchés des baux pour déclarer une admission valide.

Le bail commence à `sessions.open(..., session_start, 60_000)`, après vérification
du défi et relecture de l'état courant. Il ne commence ni au début du défi, ni au
retour de la réponse dans nsh. Le retour consomme déjà une partie de sa durée.
Source : `service/src/bin/common/shell_verifier.rs`, méthode d'authentification.

## Chronologie invitée complète dans les limites des traces

Tous les nombres de ce tableau sont des millisecondes du même démarrage invité.

| Étape | Alice | Bob | Nature de la preuve |
| --- | ---: | ---: | --- |
| Émission du défi | 12520 | 16820 | NXAUTH |
| Limite du défi | 14020 | 18320 | NXAUTH |
| Signature observée | 12520 | 16820 | NXAUTH |
| Vérification de réponse | 13592 | 17888 | NXAUTH ; succès avant limite |
| Création de session / départ du bail | 14668 | 18956 | échéance − 60000, conforme au code |
| Fin du traitement OPEN | 15748 | 20016 | NXAUTH ; reste 58920 / 58940 ms |
| Expiration du bail | 74668 | 78956 | NXAUTH |
| Refus du paquet réservé à Alice | — | 28624–29704 | DENIED, bail valide |
| Refus du paquet inconnu | — | 31828–32892 | DENIED, bail valide |
| Présentation du ticket ELF préalable à SPAWN | 35044 | 50880 | ticks 8761 / 12720 |
| Expiration de ce ticket préalable | 38972 | 54800 | ticks 9743 / 13700 |
| Création et exécution ELF préalable | PID 21 | PID 22 | observées, sans horodatage propre |
| PREPARED / stdout 134 octets / sortie 0 | observé | — | collecteur ; pas d'instant exact |
| Fin contrôles négatifs et collecte | — | observée | après SPAWN sonde, sans instant exact |
| Attente préparatoire | — | 3750 ticks demandés | 15000 ms programmées, pas une mesure exacte |
| Début du ticket vivant NSHEXBG1 | — | 88284 | DENIED 5, bail expiré |
| Validation de session pour confirmation | 88288 | — | DENIED 5, bail expiré |
| Ticket vivant émis | absent | absent | aucun handle du témoin |
| Révocation administrative / reçu RETIREE | non exécutés | non exécutés | zéro mutation administrative |
| Consommation du ticket vivant | — | non exécutée | aucun résultat REVOKED à revendiquer |
| Drainage global invité | non démontré | non démontré | arrêt hôte après timeout |

**Rectification sémantique :** `NAEXEC attente` marque l'entrée du chemin d'attente
pour admission distante, avant sa confirmation ; ce n'est pas l'instant de
l'admission durable ni celui de la création du PID. Les deux tickets préalables
sont consommés pour les ELF déjà observés, pas le ticket vivant recherché.

Les positions des événements dans la trace établissent l'ordre des événements
non horodatés, pas leur durée. La préparation d'Alice se termine avant le marqueur
de fin des contrôles Bob. L'hôte pré-saisit ensuite la confirmation et transmet LF
au marqueur Bob. La synchronisation du lecteur série est correcte sur ce second
essai, mais le marqueur reste déclenché par le délai autonome de Bob.

## Ce qui consomme le bail

Pour Bob, les intervalles mesurables sont :

- Session → première décision négative : **9668 ms** ; contient les échanges
  initiaux et de résolution, pas une mesure pure de lecture disque.
- Session → présentation SPAWN de la sonde négative : **31924 ms**, soit 53,2 % du bail.
- SPAWN de la sonde → refus du ticket vivant : **37404 ms** ; contient l'admission,
  l'exécution de la sonde, ses requêtes, sa collecte et le délai préparatoire.
  Impossible de répartir exactement ces 37404 ms entre ces composants.
- Session → refus du ticket vivant : **69328 ms** ; dépassement de **9328 ms**.
- Alice : session → refus de confirmation **73620 ms**, dépassement **13620 ms**.

Les 15 secondes programmées expliquent une consommation volontaire du budget,
mais on ne peut pas conclure que les retirer suffirait : cela déplacerait aussi
le début d'émission, la course avec la préparation d'Alice et la durée du ticket.
Ce contrefactuel n'a pas été exécuté.

Le code `Login::ticket` effectue NSHEXBG1, NSHEXARG, NSHEXEND ; `admission` recontrôle
la session et la tête dans `inspected_binding` à chaque demande. Les chargements
associent titulaire, session, tête, nonce, identité d'installation et octets attestés.
`exec_package_read.rs` vérifie révision, architecture et empreinte des octets lus
avant transfert au lanceur. Ces vérifications ne sont pas du travail jetable.

## Travail possible avant authentification

| Travail | Avant le bail du témoin ? | Condition |
| --- | --- | --- |
| Compilation, signature de paquets de laboratoire, copies de disques, fixtures | Oui | déjà réalisé hors session ; conserver les empreintes |
| Récupération 2-sur-2 initiale et activation du registre | Oui | déjà avant authentification dans ce parcours |
| Comparaison de fichiers publics du banc, préparation des chaînes de commandes | Oui | aucune décision de droits ni donnée privée |
| Contrôles négatifs du paquet et de la sonde Bob | Parcours séparé | exigent leur propre session authentifiée ; conserver leurs résultats, ne pas les supprimer de la qualification globale |
| PREPARED administratif d'Alice | Non, pas anonymement | lié à sa session et à l'intention ; ne pas le transférer après réauthentification |
| Résolution autorisée et chargement attesté pour Bob | Non avec le contrat actuel | la session, le titulaire et la tête sont nécessaires |
| Ticket, admission distante, décision de révocation | Non | conserver tous les contrôles de fraîcheur et d'incarnation |

Un cache éventuel des octets publics ne pourrait pas cacher une décision de droits.
Déplacer le chargement autorisé avant la session nécessiterait un nouveau contrat :
ce n'est pas la correction de banc retenue.

## Correction minimale proposée, non implémentée

1. Séparer les contrôles négatifs préalables du témoin temporel. Ils restent dans
   leurs recettes de sécurité ; leur validité documentaire doit être vérifiée.
   Le nouveau témoin ne prétendra plus les réexécuter dans le bail de Bob.
2. Préparer l'intention d'Alice sous sa session valide, vérifier son stdout et
   pré-saisir sa confirmation, sans LF. Garder la même session pour sa confirmation.
3. Déclencher explicitement la phase Bob seulement après cet événement. Bob ouvre
   alors sa propre session et demande immédiatement le ticket, sans sonde négative
   ni sommeil de 15 secondes. Aucune réauthentification pendant une mutation.
4. Corréler les événements avec un identifiant de parcours, les instances Alice/Bob,
   l'intention d'Alice puis la session Bob nouvellement créée. L'événement de début
   d'émission permet le chevauchement du chargement de confirmation avec celui du ticket.
5. Exiger le ticket effectivement émis, WOULD_BLOCK avant consommation, REVOKED
   avant son échéance native, aucun enfant, puis reçu RETIREE et drainage final.
   Ne jamais déduire la validité d'un simple marqueur « émission commencée ».

**Limite concrète du raccord actuel :** Bob est un parcours compilé autonome dans
`administration_bob_shell.rs`, modifié à l'export par `administration_live_ticket_profile.py`.
Son canal de supervision existant ne décrit pas de commande ARMEMENT ; le message
accepté après fermeture est celui de terminaison. Le journal hôte observe seulement
les événements. Le collecteur est en lecture seule : il n'autorise aucune écriture
GDB ni transfert d'un droit entre Alice et Bob.

Il faut donc préciser un signal de qualification unitaire vers la seule instance
Bob, transporté par son superviseur, avant d'implémenter ce protocole. Ce signal
ne doit qu'autoriser la progression du banc, jamais authentifier Bob ni lui attribuer
des capacités. Sa route, son encodage, sa corrélation et son nettoyage restent à
valider sur les interfaces réelles. Ne pas prétendre que cette commande existe déjà.
Les doubles signaux, signaux périmés ou destinés à une autre instance doivent être
refusés. Aucun nouveau quota ou privilège général ne serait acceptable.

La correction complète n'est donc pas évidente et strictement hôte : **aucun
raccord supplémentaire n'est implémenté** dans ce diagnostic. Un simple autre
sleep ou un nouvel ordre de frappes hôte ne fournirait pas la garantie demandée.
Le prochain essai QEMU reste suspendu jusqu'à présentation et validation de ce raccord.

## Durées hôtes, séparées

Essai existant seulement : banc **171,366921375 s**, bloc hôte contenant la VM
**170,365153375 s**, attente du verrou **0,000507 s**. Ces intervalles n'ont pas
les mêmes bornes que les étapes invitées et ne sont jamais soustraits aux baux.

`rpc-public.log` contient 42 opérations, somme de temps de traitement hôte
**0,260688626 s**. Cette somme exclut TLS, transport invité, files et chargement.
Elle ne représente pas la durée totale d'authentification.

`tls-attentes.log` contient 58 attentes de disponibilité du prochain ClientHello :
somme **111,888227920 s**, maximum **95,192260041 s**. Le relais préconnecte le socket ;
cette attente inclut notamment l'inactivité finale. **Ce ne sont ni 58 handshakes
chronométrés ni 111 secondes de calcul TLS.** L'unique échange complet horodaté
côté invité dure 9764 ms (736–10500), avant le bail Alice, sous sa limite 10736 ms.

## Preuves et travaux de cette étape

[Chronologie structurée](validation/novaauth-r2-chronologie-20261122.json) et
[manifeste](validation/novaauth-r2-chronologie-20261122.sha256).
Copies durables dans `OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/diagnostic-temporel-0e5004d/`.
Onze fichiers relus, SHA-256 du manifeste :
`f048c896aba0fc2950aad66835946cc2966b2b8d06d1fcff4590bd746b1525bd`.
Les originaux ont été vérifiés contre le manifeste de l'essai, sans les modifier.

Aucun nouveau test QEMU ou test hôte de raccord : seul un calcul et contrôle hors
ligne des traces a été effectué. Aucun correctif d'exécution appliqué. Les cinq
tests hôtes précédents restent des acquis de synchronisation, non une preuve du
nouvel ordonnancement proposé. Réserves historiques conservées ; utilitaires en pause.
Publication documentaire R2 seulement. Aucune fusion, aucun tag, aucune activation.
