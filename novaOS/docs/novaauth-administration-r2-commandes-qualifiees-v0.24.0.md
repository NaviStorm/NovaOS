# NovaAuth R2 — Commandes et premières mutations qualifiées

Point de passage du 22 novembre 2026, uniquement en qualification QEMU.
**R2 reste incomplet ; ce rapport ne propose ni fusion ni activation ordinaire.**
Il complète le [provisionnement de Bob](novaauth-administration-r2-provisionnement-duplex-v0.24.0.md)
et le [contrat de confirmation](novaauth-administration-r2-confirmation-session-v0.24.0.md).

## Sources et profils réellement éprouvés

Les consultations emploient l'invité `2f86d1c3e4d7866e8fa693aaaec91dffa990466b`.
Les mutations emploient l'invité `64edf213b1e46656450f8191a2bd07a4c314363f`.
Les corrections ultérieures concernent le banc hôte ; les douze parcours finaux
de mutation ont utilisé les mêmes ISO et exécutables invités. Le SHA du banc de
chaque tentative figure dans sa coordination, jusqu'à
`8718b0def42263a23d1dda95294ab5d41f39d245`.

Chaque essai indépendant démarre sur une copie neuve de la même genèse de son
profil. Les quatre profils sont AArch64 et x86_64, secteurs P9 de 512 et 4096
octets. QEMU 11.1.3, TCG, deux vCPU, 256 Mio, `-nic none` ; les commandes
complètes, SHA des ISO/noyaux/registres et images initiales sont archivés.
L'autorité durable et les clés de laboratoire restent distinctes des volumes
invités. Les exportations explicitement transformées par `profile.py` sont
archivées en plus des sources Git : les sources Git seules ne décrivent pas
tous les fichiers de cet export expérimental.

## Résultats invités

| Parcours | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
| --- | --- | --- | --- | --- |
| `whoami`, `id`, `groups` depuis le vrai `nsh` | réussi | réussi | réussi | réussi |
| `useradd`, `usermod`, `groupadd --version` | réussi | réussi | réussi | réussi |
| Création `groupadd equipe-r2`, préparation puis confirmation | réussi | réussi | réussi | réussi |
| Reçu de création supprimé, puis consultation de la même issue | réussi | réussi | réussi | réussi |
| `usermod --disable alice --prepare`, dernier administrateur | refus attendu | refus attendu | refus attendu | refus attendu |

Les six ELF administratifs sont donc effectivement exécutés ; `--version` ne
qualifie pas les mutations de `useradd` et `usermod`. Les sorties sont comparées
octet par octet, séparément de leur rendu console. `whoami` produit `alice\n`,
`groups` produit `base\n`, et `id` produit les 111 octets attendus incluant les
identifiants stables de la genèse. Les témoins de version produisent la chaîne
0.24.0/NXADMIN1 attendue. PID, statut de l'enfant, statut du shell, stdout et
stderr sont conservés par commande.

Une création confirmée produit une seule mutation de génération 23 vers 24.
La relecture indépendante de P9 vérifie le triplet complet : utilisateurs,
facteurs et groupes antérieurs inchangés ; seul `equipe-r2` est ajouté. Le
reçu Ed25519 et la tête durable concordent. Le fichier privé témoin reste intact.
Les quatorze parcours de consultation/version, y compris les deux échecs
initiaux, conservent exactement leur triplet de génération 23.

La perte injectée supprime **le reçu applicatif après décision durable**. Elle
ne simule pas une coupure physique du réseau. La confirmation retourne le
statut de commande 20 avec le diagnostic natif `UNCERTAIN=22`. La consultation
du même identifiant retourne COMPLETE et statut 0. Une seule mutation existe
dans l'autorité et dans P9 ; aucune nouvelle soumission n'est effectuée. Sur
4096, la consultation suit une réauthentification **explicite**, hors de la
transaction de préparation/confirmation ; ce résultat n'est pas attribué au
bail initial. Aucun délai de sécurité n'a été augmenté.

Le refus du dernier administrateur provient du moteur transactionnel utilisé
par la préparation côté registre : natif `DENIED=5`, statut de commande 3,
stdout vide et stderr exact. Il n'y a aucune publication ni modification des
utilisateurs. Ce témoin ne qualifie ni une suppression de compte ni une course
entre plusieurs administrateurs ; ces opérations ne sont pas autorisées ici.

## Diagnostics conservés

Quatorze parcours de consultation/version : douze réussites, deux échecs.
Quinze parcours de mutation/refus : douze réussites, trois échecs.
Ces nombres comprennent les essais de diagnostic et ne constituent pas une
mesure statistique d'intermittence.

| Échec conservé | Cause observée et correction |
| --- | --- |
| Six commandes réunies, x86_64/512 | Borne de 128 connexions du relais hôte atteinte après cinq commandes. Lots indépendants, même borne. |
| Trois consultations, x86_64/4096 | Troisième admission à 115076 ms, après l'échéance du bail à 113400 ms. Parcours indépendants avec authentification neuve ; aucun renouvellement silencieux. |
| Création, x86_64/4096, essai 01 | Publication réussie puis collecteur refusant une succession de session explicitement demandée. Une seule succession autorisée pour la même instance, après fermeture des sorties ; anciennes sessions rejetées. |
| Création, AArch64/512, essai 01 | Après un arrêt GDB, pas non ciblé sans progression du PC, puis observation répétée. Sélection désormais du vCPU indiqué par le paquet d'arrêt et `vCont;s:<thread>` ; progression vérifiée, aucun déplacement artificiel du PC. |
| Reçu supprimé, x86_64/512, essai 01 | Statuts 20/22 corrects mais diagnostic anglais comparé à du français. Le bloc de lancement porte réellement des drapeaux de locale nuls ; oracle anglais explicite. |

Le protocole GDB distingue le thread arrêté du thread courant des opérations
du débogueur : [paquets GDB](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Packets.html),
[GDB sous QEMU](https://www.qemu.org/docs/master/system/gdb.html).
La trace réussie AArch64 montre `T05thread:02;` avec `qC=QC01`, puis sélection
de 02 et progression du PC. Quatorze tests hôtes vérifient les limites du
collecteur et la sélection du vCPU ; ils ne remplacent pas les résultats invités.
L'ancien échec du collecteur de la campagne de provisionnement, moins
instrumenté, n'est pas rétroactivement déclaré expliqué par cette observation.

## Ressources, compatibilité et coordination

Dans les douze parcours finaux de mutation, le pic par acteur reste inférieur
ou égal à quatre racines, la cinquième demande retourne LIMIT et les compteurs
globaux d'endpoints, nœuds, espaces et cadres IPC reviennent à zéro après arrêt.
Chaque enfant collecté restitue racines, handles, attentes, pages, pile et
threads. Les mesures complètes par PID sont dans la matrice et les traces.

| Acteur résident | AArch64, deux géométries | x86_64, deux géométries |
| --- | --- | --- |
| Registre isolé, service 22 | 136 pages avant/pic/après | 142 pages avant/pic/après |
| Signataire, service 23 | 58 pages avant/pic/après | 66 pages avant/pic/après |

Ces mesures de pages résidentes ne sont pas des mesures d'allocations actives.
Les pages d'allocateur/cache NovaFS, les piles et la mémoire rendue sont
conservées séparément dans les traces ; aucune conclusion générale d'absence
de fuite n'est déduite de ces seuls compteurs. Réserves historiques maintenues :
301 cadres instrumentés AArch64 et reproductibilité entre environnements.

Les neuf contrôles ABI et l'index des 48 sélecteurs passent sur 8718b0d avec
Apple Clang. Les quatre ELF historiques restent couverts par la campagne
`historiques-4481e46` : aucun changement de noyau, ABI, loader historique ou
format P9 depuis cette campagne dans les commits de ce point de passage.
Il s'agit d'une justification de réutilisation, pas d'un nouveau rejeu ni d'une
preuve d'identité binaire des noyaux ordinaires. NXADMIN1 reste expérimental.

Verrou commun acquis et libéré pour chaque essai, sans campagne concurrente :
consultations 955,303 s d'exécution et 0,005069 s d'attente ; mutations 1137,049 s
et 0,005481 s. Total essais : 2092,352 s, soit 34 min 52,352 s. Les acquisitions
et libérations UTC exactes figurent dans les matrices/coordination ; aucun
conflit de planification n'y a été observé. Le diagnostic hors banc n'a pas
été chronométré continûment sur les reprises de contexte : sa durée totale
n'est pas disponible et n'est pas assimilée au temps d'essai.

## Archives et limites restantes

Archive durable : `OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/`.
Le manifeste additif `SHA256SUMS-commandes-mutations-20261122` couvre 1362 fichiers,
672727491 octets, relus deux fois sans écart. SHA-256 :
`a72990ab95462664af21a69e3c50801c21502ccd8ccdad2981c68499575d8170`.
Il conserve les échecs, les images P9, les matrices, les exports sources et les
paramètres du banc. Les manifestes antérieurs ne sont ni remplacés ni modifiés.

Les binaires contenant des secrets QEMU restent hors Git dans
`OS-worktrees/artefacts-prives-administration-0240-r2/`. Leur manifeste additif
de même nom couvre 413 fichiers, 113945067 octets, également relus deux fois.
SHA-256 : `1c240953700779b1078ebdbdf0358c0a38820874f43abf3c5b28de152cc173eb`.
La recherche des secrets de laboratoire connus dans les 23 nouveaux blobs Git
jusqu'à 8718b0d n'a trouvé aucune correspondance ; ce contrôle n'est pas une
preuve universelle d'absence de secrets.

Restent ouverts : révocation du ticket Bob avant expiration et concurrence,
désactivation/réactivation avec acquittements réels des effecteurs, création
avec facteur attesté, changements d'appartenance avec origines distinctes,
récupération durable des issues après redémarrage, propagation de la locale,
et les cellules invitées encore manquantes des neuf attaques R2. Le renommage
est raccordé mais n'est pas qualifié par cette campagne. Les négatifs de
signature/destinataire au provisionnement restent des tests hôtes tant que
le parcours équivalent n'a pas été exercé dans l'invité.

Les refus existants sont conservés tant que ces garanties ne sont pas démontrées.
Aucune suppression de compte/groupe, fusion, version officielle, activation
ordinaire ou reprise des utilitaires n'est autorisée par ce point de passage.
