# R2 — Raccord des effecteurs de désactivation

Raccord du contrat R2 approuvé, uniquement dans l'export QEMU. La qualification
ciblée ci-dessous ne vaut pas qualification globale R2. Aucun nouvel endpoint, sélecteur,
quota, détenteur de clé ou droit d'arrêt n'est ajouté.

## Séquence privée

1. NXADMIN1 transporte une confirmation depuis un ELF attesté. Le vérificateur
   inspecte sa session, son installation et ses portées ; le registre recontrôle
   le rôle, l'époque, la tête et la confirmation liée à la session.
2. Avant la confirmation, le vérificateur demande NXRTPRE2 au registre. Le corps
   reprend la commande canonique de confirmation, avec seulement son magic
   remplacé. Le registre retrouve le plan et sa cible stable. Une désactivation
   d'administrateur reste refusée par le moteur du plan.
3. Réponse privée de 128 octets : statut u64 LE [0,8), sujet [8,24), époque
   u64 LE [24,32), opération [32,48), zéros [48,128). Une cible nulle signifie
   qu'aucune clôture n'est nécessaire pour cette mutation non révocatrice.
4. Le vérificateur ferme l'époque dans sa table commune, puis ferme les routes
   correspondantes parmi les deux instances désignées. Les identifiants viennent
   des authentifications réelles mémorisées ; le client ne sélectionne pas une
   route. NXRSTOP1 invalide les tickets et délégations des descendants. Un retour
   CLOSED/REVOKED du transport n'est pas une preuve d'acquittement du noyau.
5. Sur sa conversation existante, NovaFS traite NXPKDROP pour la session exacte :
   abandon des préparations ELF et retrait filtré des handles du VFS historique.
   Les autres sessions sont conservées. Les mutations déjà engagées conservent
   leur issue propre ; un retrait ne transforme pas UNCERTAIN en succès.
6. Seulement après ces retours réels, NXRTACK2 reprend la même confirmation
   privée. Le registre mémorise les effecteurs acquittés pour cette intention.
   NXACMD01 confirme ensuite le plan, consommé avant toute écriture.
7. NovaFS prépare les trois objets ; l'autorité publie atomiquement leur tête et
   la clôture d'époque. Son reçu est vérifié et P9 relu. Le registre émet une
   seule fois ACKNOWLEDGE des effecteurs, puis consulte OUTCOME avec un défi frais.
8. COMPLETE exige le reçu terminal RETIREE authentifié pour le même serveur,
   domaine, sujet, époque, opération, génération et tête précédente. L'autorité
   conserve RETRAIT_EN_COURS tant qu'une admission antérieure n'a pas d'issue
   démontrée. Aucun délai ni injoignabilité ne prouve une annulation.

NXRTPRE2 et NXRTACK2 sont refusés sur le canal ELF, dont le décodeur conserve
NXACMD01 seulement. Ils utilisent la route privée vérificateur/registre déjà
autorisée et ne constituent pas une extension de l'API publique.

## Pannes et ressources

Le retrait local peut précéder une publication échouée : le refus reste en
place, sans réactivation automatique. Après perte d'un reçu, seuls l'identifiant
original et les consultations authentifiées sont utilisés. Un acquittement
d'effecteurs dont l'envoi a commencé n'est pas répété automatiquement. L'absence
de reçu terminal maintient UNCERTAIN ; aucune nouvelle authentification ne peut
rouvrir l'époque retirée. La réactivation reste à raccorder et à qualifier.

Les deux tables de conversations, les deux intentions et la seule mutation en
vol restent bornées. La taille de chaque intention reste contrôlée à compilation
(512 octets maximum). Les routes ne créent aucune racine IPC supplémentaire.
Les compteurs et la restitution doivent être mesurés dans les quatre invités.

La qualification doit distinguer retrait avant expiration, admissions déjà
acceptées, réponses perdues, fermeture d'une autre session et protection des
droits légitimes. La fermeture des routes de Bob ne qualifie pas à elle seule
les changements de groupe ni la réactivation. Les réserves historiques restent
ouvertes et les utilitaires en pause.


## Témoin ciblé du 22 novembre 2026 — x86_64/P9-512

À la demande MOA/MOE, cette étape se limite au ticket vivant Alice/Bob. Aucun
autre profil ni aucune autre opération R2 n'a été lancé après cette priorité.
La borne de travail actif est de 30 minutes, hors attente du verrou commun.

- Base publiée : `62407e7844c3605bb3cec70f90136c511173f70a`.
- Arbre des sources invitées : `e7ddc8ecea2220ce205a443dcd01de4203f6dbb8`.
- Arbre du banc réussi : `a7f86df67f28882d4ae17345d46ab177ff3ac182`.
- ISO exacte : `4fd9df53602629bf04b12894da84d391e6e6131745585a172302e26085c8e781`.
- QEMU 11.1.3, TCG, deux vCPU, 256 Mio, réseau désactivé, secteurs 512.
- Même ISO et copie neuve de la même genèse pour les trois derniers essais.

### Cause des échecs et correction du banc

La consultation de session attendait le vérificateur occupé par la transaction
administrative. Elle renvoie DENIED après clôture, et ne constitue pas l'oracle
SPAWN. Le témoin utilise désormais la consultation locale NXQUERY1, existante,
en lecture seule : WOULD_BLOCK tant que le ticket n'est pas consommé, REVOKED
après invalidation. Seul SPAWN constitue ensuite la tentative de lancement.
Aucun nouveau droit, message de mutation ou sélecteur n'est ajouté.

Le chargement de l'ELF usermod après émission complète du ticket dépassait sa
validité de cinq secondes. La pré-saisie de la ligne seule n'a pas suffi. Le
banc saisit maintenant la confirmation après PREPARED, sans LF, puis la déclenche
au début de l'émission du ticket de Bob. Les deux chargements utilisent leurs
routes isolées existantes. Aucun délai de sécurité n'a été augmenté. L'attente
locale d'observation est bornée à six secondes ; l'oracle exige toujours
strictement une présentation avant l'échéance réelle de cinq secondes.

| Essai, même ISO | Résultat | Marge avant expiration |
| --- | --- | ---: |
| Chargement séquentiel, reçu local | ÉCHEC conservé | −712 ms |
| Pré-saisie seule, confirmation après ticket | ÉCHEC conservé | −632 ms |
| Confirmation pendant émission du ticket | RÉUSSI | +996 ms |

Les essais antérieurs restent également archivés : arrêt sur DENIED de la
consultation de session, refus trop tardif après attente du vérificateur,
observation locale initialement limitée à quatre secondes. Ils ne sont pas
comptés comme réussites.

### Résultats observés

Alice est l'instance nsh PID 5, Bob l'instance nsh PID 11. Bob détient le ticket
local `4611686018427388047`, non consommé. Sa tentative SPAWN reçoit exactement
REVOKED (10) au tick 9439, avant l'échéance 9688, à 250 Hz : **996 ms de marge**.
Aucun enfant ni aucune admission supplémentaire n'est créé pour ce ticket.
L'enfant Bob PID 21 était le témoin négatif précédent, déjà collecté.

Les vrais ELF usermod d'Alice, PID 22/23/24, exécutent préparation, confirmation
et consultation du même identifiant. Tous ont un statut natif et shell 0,
stderr vide et stdout comparé octet par octet (134/70/70 octets).

Le refus Bob est observé **après l'effet local**, avant l'acquittement terminal
distant. La transaction est ensuite confirmée COMPLETE ; ce parcours ne doit
pas être présenté comme une consommation après réception du reçu durable final.
L'analyse indépendante en lecture seule vérifie les signatures du reçu de
publication et du reçu RETIREE, la génération 24 et une seule mutation
`d43dc3a7b0c6cf42407967eb5b506364`. L'époque 2 de Bob reste RETIREE ; l'époque 1
d'Alice reste ACTIVE. Le triplet relu sur P9 correspond à la tête authentifiée.
Alice, les groupes, les facteurs et les autres attributs de Bob sont inchangés ;
le TSV synthétique conserve ses douze octets exacts. Cette relecture hôte
complète les statuts et opérations réellement observés dans l'invité.

Les compteurs encadrant le refus sont identiques : 4 racines et 5 handles chez
Bob ; globalement 64 endpoints, 134 nœuds, 17 attentes, 19 processus, 74 cadres
IPC et 59088 cadres libres. Après fermeture des deux sessions : racines,
handles, attentes, pages, piles et threads restitués ; drainage final global
endpoints/nœuds/espaces/cadres IPC à zéro. Chaque acteur reste à quatre racines
maximum, cinquième demande refusée. Registre : 145 pages ; signataire : 66 pages,
avec compteurs stables avant/après le parcours.

### Preuves et portée

Racine durable : `OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/`.
Le parcours réussi est `ticket-vivant-e7ddc8e/r2-commandes-x86_64-512-desactivation-essai03-chevauchement/`.
La matrice, les scripts et les sources exactes sont dans
`etat-ticket-vivant-cible-20261122/`. Chaque membre du tar source a été relu
et comparé au blob Git correspondant. Les échecs sont conservés.

- `SHA256SUMS-ticket-vivant-cible-20261122` : 347 fichiers publics,
  SHA-256 `f2fa6b42b52dc55f73aba38697753d93f10ade304f3d8eef6059280d23d4f481`.
- Manifeste homonyme dans `OS-worktrees/artefacts-prives-administration-0240-r2/` :
  122 artefacts privés, SHA-256
  `807fc94a68f773b5f820e07abb4ba19dad6297e13629a2c36053503c83871f9d`.
  Les binaires de qualification contenant les secrets synthétiques restent
  dans cette archive privée, hors Git. Double lecture SHA-256 concordante.

Après la priorité, deux essais ciblés : un échec de pré-saisie puis un succès
de chevauchement, soit 101,302028291 s de banc et 0,000689084 s d'attente cumulée
du verrou. Acquisition/libération UTC : 10:43:41.850984–10:44:32.659197 et
10:45:20.433841–10:46:11.023205. Les durées de diagnostic et d'archivage sont
consignées séparément dans le compte rendu d'étape. Aucun conflit de
planification ni contournement du verrou. Aucune restriction de plateforme
n'a bloqué ce témoin.

**Qualification ciblée uniquement.** Les trois autres profils, la réactivation,
les appartenances et les scénarios R2 non encore qualifiés restent à réaliser.
Les contrôles ABI et les ELF historiques n'ont pas été rejoués dans cette étape
restreinte. Les réserves historiques de 301 cadres AArch64 instrumentés et de
reproductibilité entre environnements restent ouvertes. Aucune qualification
UTM, matérielle ou de production, aucune fusion ni aucun tag. Utilitaires en pause.
