# NovaAuth R2 — Droits VFS réels et reçus sélectifs persistants

23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Référence fonctionnelle `1d5a678a5c8451c01606b102d997e76681eceee2`, publication
initiale `c4ff08c0621c51ad03d17dd4af023d9cbbad241f`.
Code livré : `cddbaaf4047e3768954765c143edcea841d6a3c9` ; arbre
`04c95cfcbc2c4f82a93b9cc0869a189849570a9e`.

**R2 reste inachevé.** Cette livraison raccorde et teste des droits VFS réels
Direct/G/H et la consultation d'un retrait terminal après arrêt complet.
Elle ne qualifie pas les tickets EXEC dépendants de G, les opérations G en vol,
la concurrence Q3 ni la reprise froide d'un retrait encore incomplet.
Les acquittements EXEC observés portent sur l'inspection de tickets indépendants,
pas sur le retrait d'un ticket G.

## Chemin effectivement raccordé

Le profil privé `membership_r2` installe le raccord uniquement dans un export
QEMU. Les sources du noyau ordinaire, les 48 sélecteurs ABI, NovaFS P9 et les
composants CLI ordinaires sont inchangés. L'extension utilise les conversations
privées existantes ; aucune racine IPC supplémentaire n'est créée.

Le noyau QEMU fournit au vérificateur l'empreinte du véritable ELF `nsh` et lie
la route à son instance. Le registre résout cette empreinte dans les installations
attestées du titulaire, refuse une sélection absente ou ambiguë, vérifie une tête
indépendante fraîche puis appelle `View.permission`. La décision contient la
provenance Direct ou le groupe et les révisions de règle, d'appartenance et d'image.
Le nom d'une commande ou un handle fourni seul ne constitue jamais une autorisation.

`Directory::membership_grant` reçoit cette décision sur le port privé du registre.
Le lease est lié au titulaire, à la session, à l'objet P9 et à l'origine.
Chaque réutilisation revalide ces éléments. NovaFS reste l'unique écrivain :
les opérations réelles utilisent `prepare_write_id(...).commit()` puis
`read_access_id`. Le registre reçoit des faits corrélés et leur empreinte,
jamais le contenu du fichier.

Pour G et H, chaque opération utilise une admission durable individuelle
`ADMIT 57`, vérifiée par `QUERY 59`, puis un `FINISH 58` après le fait authentifié
NovaFS et une nouvelle vérification de son issue. L'identifiant lie la requête,
le titulaire, l'instance, la session, l'époque, la révision d'appartenance et
l'effecteur NovaFS. **Les lectures Direct ne passent pas par cette admission de
groupe** : elles conservent leur contrôle de permission fraîche et leur lease
personnel. Une admission durable générique de chaque opération Direct n'est pas
revendiquée.

La commande native `usermod --remove-group base alice` exécute réellement
PREPARE, CONFIRM et OUTCOME dans des ELF distincts depuis `nsh`. Elle conserve
les contrôles administratifs côté NovaAuth et la protection transactionnelle du
dernier administrateur. La séquence reste : réservation signée, inspection EXEC,
fermeture VFS sélective, publication atomique du triplet, relecture P9, issues
individuelles, acquittements des instances attendues, reçu frais `RETIREE`.

## Droits et données témoins

La fixture de laboratoire attribue explicitement à Alice trois origines sur
le même objet synthétique : Direct en lecture, G et H en lecture/écriture.
La première lecture restitue exactement `41 09 42 09 43 0a 44 09 45 09 46 0a`.
Les écritures G et H modifient volontairement le premier octet, avec relecture
et comparaison des douze octets dans l'invité ; les révisions P9 doivent croître.

Après retrait de G, son ancien lease rend **REVOKED (10), zéro octet**.
Les leases Direct et H restent utilisables ; H produit une nouvelle révision.
La fermeture invitée indique **un lease retiré, deux conservés**.
Les faits individuels durables de G puis H puis H sont tous terminaux.
Ce témoin n'est ni une simulation hôte de permissions ni une preuve exhaustive
des sous-délégations ou de tous les utilitaires.

Après arrêt complet, une nouvelle incarnation explicitement approuvée récupère
le triplet chiffré par le protocole 2-sur-2 existant. Dans une nouvelle session,
Alice retrouve le résultat, lit Direct, reçoit le refus G puis écrit et relit H.
Les anciennes références locales ne sont pas utilisées comme des handles neufs.

## Contrats privés additifs

Les trames VFS restent de 128 octets. `NXMGDAT1` transporte session (32), objet
(8), groupe (16), référence locale (8), opération (1), droits demandés (1), puis
zéros réservés. Le vérificateur ajoute l'instance et l'image attestée dans une
trame de contexte séparée. `NXMGGO01` lie le tag, l'objet, la référence, l'origine,
les trois révisions, les droits et l'identifiant/position d'admission durable.
Les tailles, champs réservés, versions, références substituées et répétitions
sont refusés. Une file occupée ne remplace jamais une requête en cours.

La consultation privée additive `HISTORY 61` n'est **pas un sélecteur ABI**.
Les opérations 54–60 et le reçu QUERY antérieur restent inchangés.
Requête : tête authentifiée 112 octets, identifiant 16, défi frais 32, soit 160.
Réponse canonique : `NAMBHI01` (8), requête complète (160), taille big-endian (2),
réservation d'origine (225 + 33 × nombre d'effecteurs), reçu signé (312),
signature d'enveloppe (64). Taille observée pour trois effecteurs : **870 octets** ;
maximum pour 32 effecteurs : **1 827 octets**, vérifié par encodage signé sur les
deux architectures natives, avec rejet du 33e effecteur et d’un octet excédentaire, puis contre la capacité du
transport. La séparation de domaine est `NovaAuth/QEMU/membership/history/v1`.

Le registre vérifie les deux signatures, le défi, le serveur/domaine,
l'identifiant, l'auteur authentifié et la tête P9 contre la tête indépendante
fraîche. L'historique conserve les anciennes identités d'effecteurs. Une nouvelle
incarnation ne peut pas les remplacer ni émettre leur acquittement.
Le reçu terminal est consulté sans réexécuter la mutation ; les états Reserved
et Draining restent **UNCERTAIN (22)**. Les contrôles historiques ne sont pas
assouplis pour cette variante.

La notification privée `NXMGEND1` n'est envoyée qu'après validation de RETIREE.
Elle permet un deuxième retrait tout en conservant la barrière précédente.
Le stockage borné refuse un troisième retrait avec LIMIT ; il ne supprime
aucune ancienne barrière. Deux retraits et cette saturation sont testés nativement,
mais un deuxième retrait administratif réel n'a pas encore été qualifié sous QEMU.

## Matrice de qualification

La matrice machine, les statuts, les révisions, les identifiants durables et les
chemins de preuve sont dans
[les résultats consolidés](validation/novaauth-r2-droits-reels-v0.24.0/resultats.json).

| Recette | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Q1 VFS réel Direct/G/H | Réussie | Réussie | Réussie | Réussie |
| Tickets EXEC réellement issus de G | Non raccordés | Non raccordés | Non raccordés | Non raccordés |
| Q2 opérations G en vol / réveils | Non exécutée | Non exécutée | Non exécutée | Non exécutée |
| Q3 concurrence invitée | Non exécutée | Non exécutée | Non exécutée | Non exécutée |
| Q4 réponse PUBLISH perdue à chaud | Réussie | Réussie | Réussie | Réussie |
| Q4 réponse ACK durable perdue à chaud | Réussie | Réussie | Réussie | Réussie |
| Q4 ACK jamais admis : UNCERTAIN maintenu | Réussie | Réussie | Réussie | Réussie |
| Q4 reprise froide après RETIREE | Réussie | Réussie | Réussie | Réussie |
| Q4 reprise froide d'une opération incomplète | Non raccordée | Non raccordée | Non raccordée | Non raccordée |

Les **16 recettes finales réussissent** : 4 avec arrêt/reprise, 12 avec perte
ou interruption ciblée, soit **20 démarrages invités et 52 admissions VFS
individuelles terminées**. La campagne totale conserve **33 essais : 24 réussis,
9 échoués et diagnostiqués**. Ces neuf échecs ne sont ni effacés ni comptés comme
réussites du candidat final.

Pour les pertes durables, CONFIRM rend `UNCERTAIN (22)` / statut CLI 20, puis
OUTCOME retrouve le résultat avec statut 0 et une seule mutation. Quand l'ACK
n'a pas été admis, CONFIRM et OUTCOME restent à 22/20 ; le reçu reste
`RETRAIT_EN_COURS`, aucun effecteur n'est déclaré durablement acquitté et le
marqueur `RETIREE` est absent. Dans ces trois cas, G est refusé à 10/0 octet
et les lectures Direct et écritures/lectures H restent fonctionnelles.


Les résultats antérieurs restent conservés avec leur portée : le retrait sans
lease G actif et les 52 parcours historiques ne sont pas assimilés à ces nouvelles
recettes. Aucun rejeu de campagne historique n'est effectué sans nécessité.
Les quatre ELF historiques ne sont pas réexécutés ici ; aucune nouvelle preuve
binaire ou qualification globale de ces ELF n'est revendiquée.

## Tests hôtes et natifs

- Autorité, historique signé, profils et collecteur : **43 tests hôtes réussis**.
- Vue/administration : **25 tests réussis et un ignoré** par architecture Darwin (`processus_autorite`, sous-processus privé lancé par sa recette).
- Récupération/protocole : **43 tests réussis** par architecture Darwin.
- Coordinateur et disposition mémoire : **15 tests réussis** par architecture.
- Effecteurs VFS/EXEC et file du port : **18 tests réussis** par architecture.
- Registre ABI : **9/9** sur les sources et chacun des deux exports ; les 48 sélecteurs restent inchangés. Le premier appel sur export sans métadonnées Git avait quatre erreurs d’infrastructure ; l’index temporaire inclut ensuite les 693 sources exportées et lit les références historiques du dépôt, sans modifier les assertions.
- Constructions invitées x86_64 et AArch64 : réussies, ISO `novaauth-sessions`
  extraite et comparée au noyau attendu.

Darwin ARM64 et x86_64 sous Rosetta sont distingués des invités QEMU.
Les tests de coordinateur utilisent explicitement des doubles de RPC/stockage ;
ils ne constituent pas des acquittements d'effecteurs invités.

## Corrections et essais négatifs conservés

Les traces originales restent archivées, y compris les échecs :
1. Lecture d'un indice de configuration hors borne historique ; utilisation du
   port privé existant au lieu d'étendre `runtime::config`.
2. Première admission VFS avant enrôlement de l'époque de la fixture ; enrôlement
   indépendant déjà prévu déclenché aussi avant ADMIT VFS, sans changement de droit.
3. Trace finale supprimée par le quota DEBUG, puis offsets erronés du marqueur ;
   réduction des traces redondantes et construction vérifiée, sans relever le quota.
4. Réécriture de données identiques : P9 n'incrémentait légitimement pas la révision ;
   données synthétiques distinctes pour prouver une écriture effective.
5. Collecte d'une révision partielle (`1` au lieu de `193`) ; attente du LF complet.
6. Saturation/rejeu sur le port VFS : refus borné sans terminer NovaFS ni perdre
   la requête initiale.
7. Capture froide : marqueur final au-delà du quota ; suppression d'une trace
   redondante uniquement dans la combinaison capture/sélectif, contrôles inchangés.
8. Reprise froide : le banc envoyait `exit` après `whoami`, avant la sonde VFS ;
   fermeture différée et test hôte sur la commande réellement construite.

9. Cumul capture et pertes à chaud : plafond hôte de 128 connexions atteint
   après FINISH H, avant sa consultation finale. Le bail restait valide
   (décision invitée 22 328 ms, échéance 65 600 ms). Les essais de pertes utilisent
   désormais le profil sans capture indépendante ; la reprise froide conserve
   le profil avec capture. Aucun plafond ni délai n'est augmenté.

## Ressources et durées

Les mesures détaillées sont dans la matrice : pics de racines par PID,
handles, attentes, pages et piles du registre/signataire, cadres IPC globaux,
phases RAM et collecte finale. Les six valeurs des mesures TLS/registre sont,
dans l'ordre : racines, handles, attentes, pages, pile, cadres IPC globaux.
Le retour des compteurs locaux et le drainage global sont vérifiés séparément.
Une mémoire résidente du moteur P9 ne doit pas être appelée fuite sans analyse.
Les pics et minima publiés sont les extrema des points instrumentés archivés,
pas une mesure continue de chaque allocation.

Le contrôle additionnel mesuré totalise **4 082 octets** sous la borne existante
de 4 096 : il inclut les états sélectifs, le contexte, la file, les objets des
leases et les références du shell. Ce sous-total n'inclut ni les piles TLS ni le
tas du moteur P9. Aucun budget n'est relevé.

Arrêté des mesures : **23 novembre 2026, 16:43:44 CET** (15:43:44 UTC).
Début du travail : 14:59:58 CET. Attente du verrou : **0.013208 s** ;
exécution des bancs : **20 min 14.013 s** ; développement,
diagnostic, compilations, tests natifs et documentation hors banc :
**83 min 32.952 s** à cet arrêté. La préparation de la livraison
et sa publication suivent cet arrêté, dans la limite globale de 120 minutes.
Aucun conflit de planification observé ; les **33 acquisitions/libérations** sont
consignées individuellement et tous les verrous ont été libérés.
Les temps hôtes ne sont jamais soustraits aux échéances invitées.

Sur les quatre recettes froides finales, les échanges initiaux TLS horodatés
prennent de **2 916 à 3 300 ms** sur l'horloge invitée, sous la limite de 10 s.
Cela ne remplace pas les autres échéances du signataire, qui restent imposées ;
le quota de traces peut limiter les échanges individuellement observables.
Le profil de pertes atteint au maximum **128 connexions**, sans relever cette
borne. Le dépassement du profil combiné reste archivé.

Registre avant/après : racines 4, handles 7, attentes 0, pile 64 pages ;
153 pages de code/données sur x86_64, 152 sur AArch64. Signataire : racines 4,
handles 6, attentes 0, pile 64 ; 69 pages sur x86_64, 61 sur AArch64.
La dernière collecte de chaque invité final indique zéro processus/espace,
endpoint, nœud, thread et cadre IPC temporaire. Le drainage de supervision
QEMU n'est pas assimilé aux réveils d'opérations en vol de Q2.

La réserve historique **301 cadres instrumentés AArch64** reste ouverte.
La réserve de reproductibilité entre environnements reste distincte de la
compatibilité fonctionnelle et de l'identité des artefacts utilisés.
L'avertissement historique `rust-objcopy`/`libLLVM.dylib` est conservé ; les deux
constructions réussissent sans modification de la configuration du Mac.

## Identité et preuves

L'instantané de compilation est `16eb8598ebb778baae64d438ad2db2b943e5d44d`.
Le profil avec capture utilise cet instantané. Le profil des pertes à chaud
compile directement `cddbaaf4047e3768954765c143edcea841d6a3c9`, sans capture ;
les deux profils ont des empreintes de noyau/registre distinctes, attendues et
archivées séparément. Les empreintes de `nsh` sont identiques par architecture.
Le commit livré diffère de l’instantané 14 seulement par la correction de sortie du banc hôte et
son test. Les arbres des services, bibliothèques, noyau, ABI, NovaFS et composants
utilisés pour la compilation sont identiques. La preuve et le diff exact sont
archivés dans `applicabilite-candidat.json` et
`difference-compilation-14-code.patch`. Il ne s'agit pas d'une comparaison
binaire entre environnements distincts. La publication ajoute aussi des
assertions natives sur les tailles maximales dans le fichier de test du
protocole ; elles ne modifient aucun binaire invité qualifié.

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/achevement-selectif-1d5a678-20261123.tar.gz`.
**2019 fichiers**, tous relus dans l'archive et comparés au manifeste.
SHA-256 : `1de6ae50b5ebce1fa287bf55f6fa229d7a3bae9753a111073dbbc2b9ac04e2f2`.
Manifeste SHA-256 : `d12aa04862737baa9634c1956cc1f9c19f24e2434fa6ef4f0651d40702544122`.
Voir [archive](validation/novaauth-r2-droits-reels-v0.24.0/archive.json) et
[manifeste](validation/novaauth-r2-droits-reels-v0.24.0/MANIFEST.sha256).
Les 14 instantanés de sources sont conservés dans un pack Git vérifié, sans
création de branche ni tag. Les ISO et ELF de laboratoire restent dans les
répertoires privés durables indiqués par les manifestes d'artefacts ; ils ne
sont pas publiés avec les clés incorporées. Le contrôle des preuves n'a trouvé
aucune des cinq clés privées connues ni marqueur PEM privé ; cette recherche
bornée ne prétend pas détecter tout secret inconnu.
L'archive antérieure est relue et inchangée :
`4c4f26be5a73ae404e67040f84b49643825281d004dfb234340b9ac97b1f4d2c`.

## Travaux restant nécessaires

1. **EXEC dépendant de G** : l'inspection actuelle ne prouve que l'indépendance
   des tickets personnels. Il manque la provenance attestée de groupe à
   l'émission du ticket et son admission/issue individuelle. Une provenance
   inconnue continue de bloquer l'acquittement, jamais de devenir personnelle.
2. **Q2** : relier les requêtes VFS encore en vol et les écritures PREPARE annulées
   à leurs faits durables, réconcilier les pertes ADMIT/FINISH et démontrer les
   réveils. Le raccord actuel conserve UNCERTAIN après une émission ambiguë. Cette
   conversation I/O est alors bloquée, y compris pour ses demandes indépendantes,
   jusqu’à une réconciliation encore à raccorder : la conservation de leur
   disponibilité dans ce cas de panne n’est pas qualifiée.
3. **Q3** : exercer des admissions et retraits réellement concurrents dans l'invité,
   puis vérifier l'issue de chaque opération ; les tests hôtes ne suffisent pas.
4. **Q4 non terminal** : reconstruire les opérations en cours depuis les
   réservations signées et l'inventaire, sans attribuer les acquittements d'une
   ancienne instance à la nouvelle. L'autorité refuse l'activation tant que le
   drainage n'est pas démontré. Un redémarrage seul ne prouve pas l'annulation.
5. Qualifier un retrait administratif ultérieur et les sous-délégations réelles.

Ces limites maintiennent le refus sûr, mais empêchent de déclarer la révocation
sélective entièrement qualifiée ou R2 terminé. Aucune fusion, aucun tag,
aucune activation ordinaire ; les utilitaires restent en pause.
