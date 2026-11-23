# NovaAuth R2 — Retrait sélectif : état intermédiaire d’implémentation

23 novembre 2026 — branche `codex/novaauth-administration-0240-phase1`.
Contrat approuvé : `0e7d0b2141e976e99d3a676957e0517eba0e8782`,
[contrat de retrait de groupe](novaauth-r2-retrait-groupe-contrat-v0.24.0.md).
Commits de code : `bc3e75160d22a0e47fa017f8580e719916335f25` (fondations),
**`b84f896007bc02e8ebfad04fe10462f6538542aa`** (progression du drainage).

**Implémentation partielle, qualification hôte uniquement.** Aucun essai QEMU de
retrait sélectif n’a été exécuté. La commande de retrait reste refusée par le
service : les effecteurs réels ne sont pas encore raccordés. Aucun succès
administratif terminal ne doit être déduit des résultats ci-dessous. R2 reste
ouvert et nécessite un audit de clôture après sa qualification effective.

## 1. Ensemble réalisé

Les chemins de cette section sont relatifs à `novaOS/user/novaauth/`.

- `src/membership.rs` prépare le triplet sur la vue vérifiée et la tête exacte.
  Le contrôle administratif précède l’exposition de la provenance. Le sujet,
  le groupe et la révision proviennent du registre, jamais de la seule demande
  CLI. La préparation ne publie rien et ne vaut pas acquittement.
- `State::permission` réutilise la décision historique, déplacée sans changement
  de ses conditions. La vue isolée peut vérifier une origine directe ou de groupe
  et comparer exactement les révisions d’une permission conservée. Une nouvelle
  tête exige une revalidation ; elle n’efface pas à elle seule les droits directs
  ou issus d’un autre groupe. Le futur effecteur devra aussi vérifier le lease,
  la session, l’ACL et la barrière de provenance.
- `administration_publication(RemoveMember)` refuse le reçu historique UPDATE
  sans drainage. L’ancienne API permettait de construire cette métadonnée, même
  si le service ne rendait pas la commande opérationnelle. Le nouveau refus
  empêche son utilisation accidentelle lors du raccord.
- `autorite_qemu/membership_retirement.py` conserve la réservation, la publication,
  l’inventaire des instances attendues, les admissions et leurs issues dans
  l’autorité indépendante. Les opérations terminées restent consultables ; une
  réponse perdue ne déclenche aucune seconde mutation.
- Le drainage d’incarnation et celui de l’époque utilisateur consultent également
  les opérations par provenance. Sans cette vérification cumulative, le code
  ajouté aurait pu laisser un reçu global ignorer une opération de groupe encore
  en attente. Les nouveaux tests reproduisent et ferment cette omission.
- `recuperation/src/membership.rs` vérifie les formats canoniques, les signatures,
  les défis, les portées, les positions durables et les issues individuelles.
  La consultation signée des réservations est distincte de celle de la tête :
  une tête identique ne prouve pas l’absence de retrait réservé.
- Une route de signature privée est compilable sous la nouvelle feature
  `membership-r2-guest`, désactivée dans les profils existants. Elle conserve
  l’échéance TLS de 10 secondes et les attentes IPC bornées existantes.
  L’adaptateur hôte `membership_rpc.py` refuse toute activation implicite.

Aucun code de noyau, NovaFS, nsh, ELF administratif ou registre ABI n’a été
modifié. NXADMIN1, NXEXEC01 et les 48 sélecteurs restent inchangés. Les nouveaux
messages ci-dessous appartiennent exclusivement au protocole privé expérimental.
Le nombre de sélecteurs ne constitue pas, à lui seul, une preuve de compatibilité.

## 2. État durable et limites du raccord

L’autorité traite une seule mutation d’appartenance en cours par serveur/domaine.
La réservation est persistante avant le blocage local futur. Elle ferme les
publications concurrentes de tête, mais les demandes déjà émises peuvent encore
être admises avant l’ordre durable du retrait, dans l’inventaire réservé.

`RESERVEE → RETRAIT_EN_COURS → RETIREE` suit l’ordre SQLite durable de l’autorité.
Une admission de G après publication est refusée. Une admission antérieure doit
avoir une issue individuelle : accomplie avec identité/preuve d’effet non nulle,
ou annulée irréversiblement. Ni timeout, ni réponse absente, ni redémarrage ne
fabriquent une annulation. Tous les acquittements d’une même classe sont requis ;
un masque de classes ne remplace pas la liste de leurs instances.

L’autorité fait confiance au registre isolé pour attester les faits des effecteurs,
conformément au contrat. Les tests hôtes signent des **faits simulés**. Ils ne
prouvent ni révocation de handle invité, ni fermeture d’une délégation, ni absence
de création d’un processus NovaOS. Le succès TLS et son accusé vide ne valent
jamais reçu terminal. La publication P9 et la décision indépendante restent deux
preuves distinctes.

Le code conservateur bloque un drainage incomplet. Il ne contient aucune procédure
pour neutraliser artificiellement un effecteur disparu. Une réservation non
résolue peut donc maintenir l’indisponibilité de la mutation et de la reprise
d’incarnation ; il est interdit de la convertir en réussite pour débloquer le banc.
La consultation après réouverture de la base hôte est testée. Le redémarrage d’un
registre/NovaFS invité et la récupération administrative de cette nouvelle classe
d’opération restent à raccorder et à qualifier.

## 3. Encodage privé implémenté

Chaque entier est canonique en big-endian. Une tête privée occupe 112 octets :
serveur 16, domaine 16, incarnation 32, époque d’incarnation 8, génération 8,
empreinte 32. La signature Ed25519 utilise un préfixe distinct pour chaque opération
`NovaAuth/QEMU/membership/<opération>/v1\0`.

| Message privé | Numéro | Corps avant signature | Objet |
|---|---:|---:|---|
| RESERVE | 54 | 225 + 33 × N, N entre 1 et 32 | Tête, opération, nouvelle empreinte, administrateur/époque, sujet/groupe/révision, effecteurs triés |
| PUBLISH | 55 | 128 | Tête exacte et opération réservée |
| EFFECT | 56 | 161 | Tête, opération, classe et instance acquittée |
| ADMIT | 57 | 241 | Tête, provenance, effecteur, opération, époque d’authentification et session |
| FINISH | 58 | 194 | Tête, opération, effecteur, issue et preuve/identité de 32 octets |
| QUERY | 59 | 160 | Tête, identifiant et défi frais |
| PENDING | 60 | 144 | Tête et défi frais de consultation des réservations |

Ces numéros ne sont pas des sélecteurs ABI. Le signataire reçoit aussi la portée
extérieure serveur/domaine/incarnation (64 octets), qu’il compare au corps. Le
plus grand message, signature et encadrement compris, occupe 1 425 octets, sous la
borne existante de 2 048. Les métadonnées de liaison ne sont pas agrandies.

Le reçu `NAMBRE01` conserve les **248 + 64 = 312 octets** du contrat approuvé.
Le masque attendu est vérifié contre l’inventaire initial connu du registre.
La preuve d’issue individuelle `NAMBOU01` occupe 522 octets, dont la requête
originale d’admission, les positions durable d’admission et de terminaison et la
signature. L’identité d’un enfant ne devra pas être remplacée par un simple statut
« succès » dans le futur raccord EXEC. La liste signée des réservations occupe
209 octets si elle est vide, 266 avec l’unique réservation en cours.

Les vecteurs Rust/Python sont synthétiques et publics. Leur graine déterministe
de test ne représente aucune clé de laboratoire ni aucune autorité déployée.

## 4. Tests réellement exécutés

Les commandes exactes, révisions, empreintes de sources, logs positifs et négatifs
sont archivés dans `retrait-selectif-0e7d0b2`.

| Contrôle | Résultat observé | Portée et preuve |
|---|---|---|
| Autorité indépendante, suite complète hôte | **199/199** | `authority-regression-final.log`, 21,211 s hôte ; 18 nouveaux tests sélectifs à ce stade |
| Régressions après correction de la progression | **42/42** | `authority-progress-final.log`, 6,980 s hôte ; 20 tests sélectifs et 22 administratifs |
| Suite sélective après ajout du test de saturation | **19/19** | `authority-selective-final.log`, 4,710 s hôte ; 18 cas précédents plus limite de 64 admissions |
| Identités, ensemble Rust | 24 succès et 1 entrée ignorée par architecture | `identity-*-apple-darwin.log` ; l’entrée ignorée est le sous-processus lancé par sa recette, pas un succès ajouté au total |
| Administration, ajustement final | **11/11 par architecture** | `identity-final-*-apple-darwin.log`, remplace le sous-ensemble initial de 10 cas |
| Récupération, ensemble Rust | 37 succès par architecture | `receipt-*-apple-darwin.log` |
| Codecs sélectifs, ajustement final | **6/6 par architecture** | `receipt-final-*-apple-darwin.log`, remplace le sous-ensemble initial de 5 cas |
| Vérifications des cibles sans système hôte | 6 `cargo check` réussis, puis 2 contrôles ciblés du dernier changement d’identité | `compilations-natives-final.json`, `compilation-identity-commit.json` ; ce ne sont pas des exécutions d’ELF invités |
| Contrôles ABI historiques | **9/9**, 48 sélecteurs | `abi.log` |
| Quatre ELF historiques dans QEMU | **Non exécuté** | Aucun nouveau binaire invité ni campagne de compatibilité n’a été construit/exécuté |

Darwin ARM64 natif et x86_64 sous Rosetta sont distingués des cibles
`aarch64-unknown-none-softfloat` et `x86_64-unknown-none` et des invités QEMU.
Les campagnes se recouvrent : leurs nombres ne doivent pas être additionnés.
Les contrôles finaux couvrent 201 cas hôtes distincts au total, par réutilisation
des cas inchangés de la suite complète et rejeu des 42 cas affectés.

Le témoin d’identité utilise un moteur P9 simulé sur secteurs 512 et 4096. Il
vérifie Direct/G/H, l’absence d’augmentation de l’époque d’authentification pour
un simple retrait, le refus de l’ancienne tête, le nouvel ajout avec une révision
supérieure, la protection administrative et le retrait d’un sujet désactivé sans
réactivation. Les résultats restent des contrôles de bibliothèque hôte.

Les tests d’autorité couvrent notamment concurrence admission/publication et
réservation/réservation, refus d’origine retirée, issue inconnue, acquittement
manquant/substitué, réouverture de la base, réponse perdue, séparation des identifiants,
barrières d’époque/incarnation et saturation. Le 65e enregistrement en attente
rend LIMIT sans nouvelle ligne ; une issue terminale libère une place, mais ne
permet pas de réutiliser l’identifiant consommé. Les refus hôtes `Refus` ne sont
pas présentés comme des statuts natifs QEMU observés.

### Échecs conservés

Les premiers essais conservent une erreur de fixture (nom de groupe en majuscule),
une révision future proposée avant mutation, un nom de méthode de fixture erroné,
un lancement unittest depuis un mauvais répertoire, et un prérequis
`NOVA_DETENTEUR` absent. Les corrections ont porté sur les fixtures ou l’environnement.
La suite complète avec le détenteur natif disponible passe. Le test `authority-progress-before.log` reproduit aussi une absence de progression
après FINISH : le fait était durable, mais le reçu utilisateur restait absent. Le
correctif réutilise la progression historique après authentification du fait ;
QUERY reste sans mutation. Les 42 tests affectés passent ensuite. Une importation de
fixture dupliquait initialement onze tests ; elle a été corrigée et les comptes
finaux excluent ce doublon. `rustfmt` n’était pas installé ; aucun outil n’a été
installé et les sources n’ont pas été reformattées globalement.

## 5. Matrice QEMU et ressources

| Profil | Droits Direct/G/H | Admission/retrait et refus natif | Panne/reprise | Restitution/quotas |
|---|---|---|---|---|
| AArch64/P9-512 | Non exécuté | Non exécuté | Non exécuté | Non mesuré |
| AArch64/P9-4096 | Non exécuté | Non exécuté | Non exécuté | Non mesuré |
| x86_64/P9-512 | Non exécuté | Non exécuté | Non exécuté | Non mesuré |
| x86_64/P9-4096 | Non exécuté | Non exécuté | Non exécuté | Non mesuré |

Motif commun : le raccord fonctionnel aux effecteurs est incomplet. Lancer le banc
historique ne démontrerait pas ces chemins nouveaux. Aucun verrou QEMU acquis,
aucune campagne lourde lancée, **attente du verrou : 0 s ; exécution QEMU : 0 s**.
Le verrou commun reste obligatoire pour la suite. Aucun conflit de campagne n’a
été provoqué.

L’inventaire natif de 32 effecteurs mesure **1 536 octets** sur les deux hôtes.
Avec le corps de reçu (248) et deux buffers de 128, ce sous-ensemble mesure
**2 040 octets** ; une assertion compile sur les deux cibles natives sous 4 Kio.
Ce n’est pas le budget total du raccord. La branche du signataire possède aussi
un buffer de signature borné à 1 345 octets (contre 384 sur la branche
administrative existante), ainsi que les buffers RPC/TLS préexistants. Leur pic
réel de pile et leur coexistence avec l’état du registre restent à mesurer avant
activation. Aucun nouveau quota ni budget mémoire n’est accordé ou revendiqué.

Aucun nouveau processus ni appel de création de racine IPC n’est ajouté, mais les
pics, handles, endpoints, cadres, réveils et restitution réels de l’invité ne sont
pas encore mesurés. Les données anciennes ne remplacent pas ces mesures.

## 6. Acquis préservés et suite nécessaire

Les **2 986 fichiers** du manifeste de la campagne précédente ont été relus :
aucune divergence SHA-256. Archive antérieure :
`b06b4d1e52cf56e32cb25cf958e4b04bb62375a6ca7d497e9fcdf76e9fe338b8` ;
son manifeste : `e54b173ec9e6e088c856714eb2473e320a809d9a1243fad608d9c1fd9dd4d337`.
Les 52 parcours de cette campagne et les anciennes qualifications S1–S9 conservent
leurs artefacts et leur portée. Ils ne qualifient pas les modules sélectifs ajoutés.
Le [rapport précédent](novaauth-r2-cinq-fonctions-v0.24.0.md) reste inchangé.

Travaux encore nécessaires dans le périmètre approuvé :

1. Raccorder PREPARE/CONFIRM/OUTCOME au plan sélectif, à la réservation signée et
   aux deux preuves de publication (autorité et NovaFS), sans reçu UPDATE substitut.
2. Porter la provenance jusque dans les leases et sous-délégations du VFS isolé.
   L’ancien effecteur utilisant l’autorité locale ne doit pas être réintroduit.
   Maintenir les mêmes handles Direct/H lorsque leur revalidation réussit.
3. Sous la synchronisation d’admission réelle, bloquer G, inventorier les opérations,
   fermer seulement ses délégations et récupérer les acquittements exacts. Raccorder
   l’origine à l’admission EXEC existante : ADMIT privé ne la remplace pas et ne
   permet aucun lancement par lui-même. Démontrer la correspondance entre les
   opérations individuelles et les enfants effectivement créés.
4. Raccorder les consultations après redémarrage et les pertes d’acquittement.
   Une instance redémarrée ne peut acquitter à la place d’une ancienne instance.
   Ne rouvrir aucune origine réservée sur la seule foi de la tête P9.
5. Mesurer le budget natif complet, puis activer explicitement le raccord dans le
   seul profil QEMU et qualifier les quatre profils, avec tests de refus, concurrence,
   persistance et restitution. Rejouer les régressions réellement affectées et les
   quatre ELF historiques avant toute conclusion de compatibilité invitée.

Aucun blocage de plateforme ou nouvelle autorisation de sécurité n’a été rencontré.
La livraison est intermédiaire : le travail restant est de l’intégration et de la
qualification, pas une validation acquise. Les réserves historiques **301 cadres
instrumentés AArch64** et **reproductibilité entre environnements** sont conservées.
Aucune fusion, aucun tag, aucune activation ordinaire ; les utilitaires restent en pause.

## 7. Preuves scellées

[Références et empreintes](validation/novaauth-r2-retrait-selectif-v0.24.0/preuves.json),
[manifeste](validation/novaauth-r2-retrait-selectif-v0.24.0/SHA256SUMS),
[matrice QEMU](validation/novaauth-r2-retrait-selectif-v0.24.0/matrice-qemu.json).

71 fichiers de preuve ; archive SHA-256 :
`15f548703effa7a0b0302ce101d85b91fe23627a3cda5f07352327252ebe0c27`.
Manifeste SHA-256 :
`289120e9ccc1f99dfa118c8a70ed6ffb12a40f3d2eb630df3759632635e9a24b`.

Au scellement, 77.44 minutes murales depuis le début, sans attente QEMU.
Les durées monotones des commandes hôtes mesurées sont conservées dans `temps.json`.
Les premières commandes exploratoires ne permettent pas une ventilation exacte
développement/tests ; aucune précision artificielle n’est annoncée. La publication
est horodatée séparément à côté de l’archive, sans modifier le manifeste scellé.
