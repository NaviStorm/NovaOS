# NovaAuth v0.24.0 — Encodage des enveloppes de récupération 2-sur-2

**Spécification soumise à validation ciblée le 17 novembre 2026.** Branche `codex/novaauth-autorite-0240`, contrat approuvé au commit `2ae44a24835a9afef2f27f43aa11748184642488`. Cette livraison porte sur l’encodage, sa conformité et les contrôles à ajouter. L’autorisation du raccord contesté reste attendue.

## 1. Références et résultat de la revue

Le contrat approuvé prévoit déjà les requêtes scellées vers les deux détenteurs, leur signature par l’incarnation et des clés de réception propres à chaque détenteur. Il fixe aussi une remise de part de 324 octets. Ses sections 3 et 4 désignent une unique clé X25519 de réception dans l’incarnation de récupération. Le point 5 du nouveau mandat exige des destinataires et des clés distincts pour les deux remises. La variante proposée au §5 apporte deux destinations cryptographiques dans la même instance approuvée. Elle nécessite une validation explicite de l’évolution du contexte. [Contrat approuvé](novaauth-recuperation-2-sur-2-v0.24.0.md)

| Exigence | Contrat approuvé | Encodage proposé et écart à valider |
| --- | --- | --- |
| Deux demandes confidentielles | §1 et §4, clés de réception propres à G et A | Deux requêtes de 292 octets, chacune scellée à sa clé épinglée |
| Authentification de la demande | §3.5, signature de possession de R | Signature Ed25519 des 228 premiers octets |
| Remise de chaque part | §4, corps de 154 et enveloppe de 324 octets | Format conservé, signature par le détenteur de rôle |
| Destination des remises | §3–4, une incarnation et une clé de réception | Proposition de deux clés de réception de rôle, détenues par cette même incarnation |
| Autorité sur les clés | §1 et §5, épinglage indépendant, double contrôle des changements | Enregistrement additionnel des deux clés publiques de réception des requêtes |
| Fraîcheur et ordre durable | §3–4, consultation courante et réservation unique | Vérification en ligne, consommation par opération/rôle et réconciliation des reçus |
| Bornes | §6, deux pages, quatre racines, métadonnées de 56 octets | Fragmentation des enveloppes dans les bornes existantes, mesures du raccord encore à réaliser |
| Compatibilité | Profil QEMU et interfaces privées | Contrôles ABI historiques conservés, aucune modification de leur résultat attendu proposée |

Les travaux fonctionnels locaux antérieurs restent hors du commit documentaire. Leur contexte de récupération contient actuellement une clé destinataire et une clé de possession. Leur brouillon de table `recovery_keys` contient quatre clés publiques de détenteurs et manque les deux clés de réception des requêtes. Le raccord de requête de 292 octets a été refusé avant exécution de l’édition. Son implémentation et sa qualification de bout en bout restent à réaliser.

## 2. Conventions et suite cryptographique

Les offsets des tableaux sont inclusifs et commencent à zéro. Les intervalles `[a,b)` utilisés pour les signatures excluent `b`. Les identifiants et empreintes sont des suites d’octets brutes. Les entiers multioctets de l’application sont non signés, en ordre réseau big-endian. Aucun séparateur, bourrage implicite, encodage hexadécimal ou terminateur NUL n’est ajouté aux messages.

H désigne SHA-256. G est le gardien, rôle 1. A est le détenteur administrateur, rôle 2. R est l’incarnation de récupération approuvée. M est le manifeste complet de la sauvegarde. Les messages ont des longueurs fixes et refusent tout octet manquant ou surnuméraire.

Les enveloppes utilisent HPKE Base avec DHKEM(X25519, HKDF-SHA256), identifiant `0x0020`, HKDF-SHA256 `0x0001` et AES-256-GCM `0x0002`. L’encapsulation fait 32 octets et le tag AEAD 16 octets. L’authentification applicative de l’émetteur repose sur sa signature Ed25519 de 64 octets. La bibliothèque doit valider les clés, les entrées KEM et les signatures selon ces primitives. [HPKE, suites et validation](https://www.rfc-editor.org/rfc/rfc9180.html#section-7) · [Ed25519](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1)

La version du message sélectionne cette suite fixe. Un changement de suite exige une nouvelle version explicitement reconnue. Chaque enveloppe utilise une encapsulation fraîche et un contexte HPKE à usage unique. HPKE dérive son nonce AEAD interne. Le nonce applicatif de 32 octets sert à corréler une demande et son résultat. Les règles applicatives d’ordre et d’unicité complètent HPKE. [Intégration applicative et rejouements](https://www.rfc-editor.org/rfc/rfc9180.html#section-9.7)

## 3. Requête canonique de 292 octets

### Enveloppe extérieure

| Offset début | Offset fin | Taille | Champ | Encodage et contrôle |
| ---: | ---: | ---: | --- | --- |
| 0 | 7 | 8 | Magie | ASCII exact `NASHRQ01` |
| 8 | 8 | 1 | Version | u8 égal à 1 |
| 9 | 9 | 1 | Rôle destinataire | u8, G = 1 ou A = 2 |
| 10 | 41 | 32 | H(R) | SHA-256 du contexte canonique approuvé |
| 42 | 57 | 16 | Identifiant de remise | Identifiant opaque aléatoire, réservé durablement pour opération/rôle |
| 58 | 89 | 32 | Encapsulation HPKE | Encapsulation destinée à G.req ou A.req |
| 90 | 211 | 122 | Corps chiffré | Exactement les 122 octets du corps intérieur chiffrés |
| 212 | 227 | 16 | Tag AEAD | Tag AES-256-GCM |
| 228 | 291 | 64 | Signature | Ed25519 par la clé de possession de R sur `[0,228)` |

Somme vérifiée de l’enveloppe proposée, `8 + 1 + 1 + 32 + 16 + 32 + 122 + 16 + 64 = 292`.

### Corps intérieur avant chiffrement

| Offset début | Offset fin | Taille | Champ | Contrôle après déchiffrement |
| ---: | ---: | ---: | --- | --- |
| 0 | 0 | 1 | Version | 1 |
| 1 | 1 | 1 | Rôle | Identique au rôle extérieur et au détenteur local |
| 2 | 33 | 32 | H(R) | Identique à l’extérieur et à l’approbation courante |
| 34 | 65 | 32 | H(M) | Identique au manifeste enregistré et au contexte |
| 66 | 73 | 8 | Époque des clés | u64 big-endian, époque active du jeu épinglé |
| 74 | 89 | 16 | Identifiant de remise | Identique à l’extérieur et à la réservation |
| 90 | 121 | 32 | Nonce de demande | Frais, propre à cette opération et à ce rôle |

Somme vérifiée du corps, `1 + 1 + 32 + 32 + 8 + 16 + 32 = 122`.

### Couverture cryptographique

La signature couvre la magie, la version, le rôle, H(R), l’identifiant, l’encapsulation, le chiffré et le tag, dans leur représentation binaire reçue. Le préfixe `NASHRQ01` fournit le domaine de message signé. Une remise utilise un autre préfixe.

Le champ HPKE `info` contient exactement les octets ASCII `NovaAuth/recovery/request/v1`, sans NUL terminal. Les données associées AEAD sont les 58 octets `[0,58)`. Le KEM lie l’encapsulation à la clé publique destinataire. La signature couvre aussi cette encapsulation. Les champs du corps, y compris le nonce, sont confidentiels. Les 90 premiers octets extérieurs et la signature restent observables par les relais.

Le détenteur reçoit R et ses preuves publiques dans un échange borné associé. Il vérifie les signatures d’approbation et l’état courant de l’autorité, puis utilise la clé de possession approuvée pour contrôler la signature extérieure. Il déchiffre avec sa propre clé de réception épinglée, vérifie le tag et compare tous les champs répétés. Le rôle doit également correspondre à l’identité du détenteur et à sa clé. Une divergence ferme la demande.

Un H(R) reçu ne suffit jamais à enregistrer une nouvelle clé ou à approuver une incarnation. R doit être retrouvé et vérifié auprès de l’autorité indépendante. La décision historique et la validité courante doivent toutes deux être contrôlées.

## 4. Liaison complète au contexte

Le contexte v1 proposé dans le travail local mesure 344 octets. La version v2 du §5 le porterait à 376. Toutes les approbations, demandes, remises et confirmations portent le H(R) de la version exacte choisie.

| Offset début v1 | Offset fin v1 | Taille | Champ |
| ---: | ---: | ---: | --- |
| 0 | 7 | 8 | Magie `NARECV01` |
| 8 | 23 | 16 | Identifiant de récupération |
| 24 | 39 | 16 | Identité du serveur |
| 40 | 55 | 16 | Domaine NovaAuth |
| 56 | 87 | 32 | Ancienne incarnation |
| 88 | 119 | 32 | Nouvelle incarnation |
| 120 | 127 | 8 | Époque attendue de la route, u64 big-endian |
| 128 | 135 | 8 | Génération durable du registre, u64 big-endian |
| 136 | 167 | 32 | Empreinte de tête du triplet |
| 168 | 199 | 32 | H(M), manifeste de sauvegarde |
| 200 | 207 | 8 | Époque des clés, u64 big-endian |
| 208 | 239 | 32 | Défi frais de l’autorité |
| 240 | 247 | 8 | Échéance du défi dans le domaine monotone de l’autorité, u64 big-endian |
| 248 | 279 | 32 | Clé publique X25519 de réception de R |
| 280 | 311 | 32 | Clé publique Ed25519 de possession de R |
| 312 | 343 | 32 | SHA-256 de l’exécutable de récupération attesté |

La génération du registre et l’époque des clés sont indépendantes. Les quatre entiers u64 désignent des compteurs ou échéances vérifiés avant conversion vers le stockage hôte. La proposition retient la borne maximale `2^63 - 1`, compatible avec les entiers SQLite. L’époque initiale de route peut valoir zéro. Les générations, époques de clé et échéances doivent être positives. Un dépassement est refusé.

| Liaison demandée | Engagement et vérification nécessaires |
| --- | --- |
| Serveur et domaine | Champs de R, identité TLS du serveur et périmètre durable de l’autorité concordants |
| Incarnation approuvée | Nouvelle incarnation, mesure et clés de R attestées ensemble sur la route privée du registre |
| Défi frais | Défi et échéance de R vérifiés et consommés dans la transaction d’approbation |
| Sauvegarde | H(M), génération et empreinte du triplet dans R, manifeste enregistré sous cette tête |
| Détenteur | Rôle signé, clé HPKE épinglée de ce rôle et époque immuable du jeu de clés |
| Récupération | Identifiant de R, H(R), identifiant de remise et nonce liés dans la réservation durable |
| Autorisation actuelle | Consultation fraîche authentifiée et ordre durable de l’autorité, renouvelés aux points de décision |

Le registre vérifie la liaison entre sa route noyau, sa mesure et les clés générées en mémoire privée. Le signataire 23 authentifie le transcript de cette instance. L’administrateur et G vérifient indépendamment la cible. L’autorité enregistre le H(R) approuvé et retire l’ancienne route dans la même décision durable. Un PID déclaré par le demandeur ne remplace aucune de ces preuves.

Le manifeste contient les références du triplet et ses empreintes. Après récupération, le registre compare les trois objets à la tête indépendante et obtient le reçu final de publication. NovaFS garde les lecteurs en quarantaine pendant la préparation transactionnelle.

## 5. Destinations distinctes et deux clés de réception de l’incarnation

### Sens des échanges

| Sens | Destinataire cryptographique | Clé de déchiffrement | Signature de l’émetteur |
| --- | --- | --- | --- |
| Requête R → G | Gardien G | G.req, détenue exclusivement par G | R.possession |
| Requête R → A | Détenteur A | A.req, détenue exclusivement par A | R.possession |
| Remise G → R selon le contrat approuvé | Incarnation R | R.rx | G.sign |
| Remise A → R selon le contrat approuvé | Incarnation R | R.rx | A.sign |

Les deux premières lignes satisfont la séparation des destinataires et des clés des requêtes. Les deux dernières utilisent la même clé destinataire dans le contrat approuvé. Les signatures de rôle et encapsulations indépendantes ne démontrent donc pas deux clés destinataires distinctes pour les remises.

### Variante proposée pour satisfaire le point 5

Créer dans **la même incarnation approuvée** deux paires X25519 indépendantes, R.rx.G et R.rx.A. La destination de la part G devient le récepteur de rôle G de cette instance, celle de la part A son récepteur de rôle A. Chaque clé privée reste dans cette instance. Sa clé de possession commune authentifie son identité et ses ACK.

Le contexte v2 emploie `NARECV02`. Les champs et offsets 8 à 247 sont conservés. Les quatre derniers champs deviennent les suivants.

| Offset début v2 | Offset fin v2 | Taille | Champ |
| ---: | ---: | ---: | --- |
| 248 | 279 | 32 | R.rx.G, réception de la part G |
| 280 | 311 | 32 | R.rx.A, réception de la part A |
| 312 | 343 | 32 | R.possession |
| 344 | 375 | 32 | Mesure de l’exécutable |

Le H(R v2), signé par la cible, le signataire du serveur et l’administrateur selon leurs mandats, engage les deux clés. G et A contrôlent indépendamment le même contexte approuvé. Les clés R.rx.G et R.rx.A doivent être différentes et leurs empreintes doivent figurer dans la cérémonie. Le détenteur choisit exclusivement la clé correspondant à son rôle approuvé.

Les tailles extérieures restent 292 et 324 octets, car les enveloppes portent H(R). La version 1 de l’enveloppe désigne son format extérieur. La version 2 du contexte est vérifiée lors de la résolution du H(R) approuvé. La taille de R augmente de 32 octets. L’instance conserve une clé privée X25519 supplémentaire de 32 octets. Les contextes cryptographiques et leur pile restent à mesurer. Aucun nouveau processus, droit noyau ou canal IPC n’est requis par cette proposition.

**Validation ciblée requise pour cette variante.** Deux clés destinataires sont proposées dans une seule instance autorisée à recombiner les parts. Une exigence de deux processus destinataires physiquement distincts nécessiterait un autre contrat de recombinaison et un arbitrage de frontière de confiance.

Une opération v1 reste identifiée comme v1. Une récupération v2 exige une nouvelle approbation complète. Toute tentative de changer de contexte pendant une opération est refusée.

### Enveloppe de remise de 324 octets

| Offset début | Offset fin | Taille | Champ |
| ---: | ---: | ---: | --- |
| 0 | 7 | 8 | Magie `NASHAR01` |
| 8 | 8 | 1 | Version 1 |
| 9 | 9 | 1 | Rôle G ou A |
| 10 | 41 | 32 | H(R) |
| 42 | 57 | 16 | Identifiant de remise |
| 58 | 89 | 32 | Encapsulation HPKE vers le récepteur approuvé du rôle |
| 90 | 243 | 154 | Corps chiffré |
| 244 | 259 | 16 | Tag AES-256-GCM |
| 260 | 323 | 64 | Signature Ed25519 G.sign ou A.sign sur `[0,260)` |

Le corps clair reprend les 122 octets de la requête et ajoute la part aux offsets 122 à 153. Le champ HPKE `info` vaut exactement `NovaAuth/recovery/delivery/v1`. Les données associées couvrent `[0,58)`. Le récepteur exige le rôle attendu, la clé destinataire de ce rôle, le même H(R), H(M), identifiant, nonce et époque.

## 6. Épinglage et enregistrement des clés des détenteurs

Chaque époque contient six clés publiques de rôle distinctes, G.dec et A.dec pour les encapsulations de sauvegarde, G.req et A.req pour recevoir les requêtes, G.sign et A.sign pour signer les remises. Les clés d’approbation administrative, de décision de l’autorité et d’identité TLS gardent leurs usages séparés.

Les clés G.req et A.req réalisent une fonction déjà prévue au §1 du contrat. Elles protègent les demandes contre les relais intermédiaires. Leur enregistrement permet au registre de sélectionner une destination authentifiée. Elles ne donnent à aucun acteur le pouvoir de changer la tête NovaAuth.

### Proposition additive de persistance

Ajouter une table privée versionnée de clés de réception, indexée par serveur, domaine, époque et rôle. Chaque ligne contient la version, la suite, la clé publique de 32 octets, son identifiant SHA-256 et l’empreinte de l’acte d’enregistrement. Une contrainte d’unicité interdit le remplacement d’une clé dans la même époque. L’acte durable engage le jeu complet ordonné des six clés et son état actif ou révoqué.

Conserver les tables et reçus historiques. Une époque dépourvue des deux enregistrements requis reste inéligible aux requêtes de récupération. La procédure de provisionnement existante doit enregistrer les deux rôles dans une transaction et publier un reçu authentifié couvrant le jeu complet. Une publication partielle ne devient jamais active.

### Authentification de l’enregistrement

1. Lors du provisionnement de laboratoire, utiliser les identités de G, A et de l’autorité épinglées indépendamment, selon le §1 du contrat.
2. Faire approuver le jeu complet par les deux détenteurs avec leurs clés de signature de rôle déjà épinglées. Contrôler la possession des nouvelles clés de réception par un défi scellé propre à chaque rôle et lié au digest de ce jeu.
3. Vérifier le mandat administratif existant et les deux approbations auprès de l’autorité. La transaction compare le serveur, le domaine et l’époque attendue, puis inscrit les deux clés ensemble.
4. Pour une rotation, ouvrir une nouvelle époque et conserver le reçu historique de l’ancienne. Les deux détenteurs approuvent le changement. Une clé proposée par R ou par un relais ne peut remplacer une clé enregistrée.
5. Le registre valide le reçu de l’autorité et la validité courante de l’époque avant d’encapsuler. Les détenteurs vérifient aussi leur appartenance au jeu autorisé.

Avant un retrait planifié d’époque, produire et qualifier une sauvegarde sous le nouveau jeu. Une révocation de clé invalide les remises encore ouvertes de cette époque. L’ancienne époque reste dans l’historique et ne peut redevenir active par restauration d’un détenteur.

La sérialisation de cet acte doit être typée, versionnée et intégrer les six rôles dans un ordre fixe. Le raccord proposé étend le schéma privé de l’autorité. Ce changement durable constitue l’un des objets précis de la validation demandée. Le provisionnement initial et la rotation restent sous les autorités déjà définies.

## 7. Unicité, révocation et réponses perdues

| Étape | Décision et comportement exigés |
| --- | --- |
| Défi | 30 secondes dans l’horloge de l’autorité. Une expiration exige une demande explicite neuve. |
| Approbation | Comparaison de tête et route, preuve de cible et signatures, consommation atomique du défi et retrait de l’ancienne route |
| Réservation | Un identifiant et un nonce par opération/rôle. Les demandes concurrentes obtiennent une seule réservation. |
| Préparation de remise | Une seule enveloppe chiffrée est persistée par le détenteur avant émission. Le journal durable conserve son empreinte et son état. |
| Autorisation de remise | Vérification courante de l’opération et de l’époque auprès de l’autorité. La révocation et l’admission sont ordonnées durablement. Une coupure laisse l’issue non confirmée. |
| Réception | Signature, tag et contexte exacts. Un emplacement par rôle dans R. Une seconde enveloppe différente pour ce rôle est refusée. |
| ACK | Signature de possession sur rôle, identifiant et empreinte. Un ACK identique est idempotent et ne relance pas la combinaison. |
| Activation | Deux parts valides, triplet complet relu depuis P9, tête et route actuelles, reçu final authentifié avant ouverture |
| Interruption | Consultation de l’issue par identifiant. Toute récupération de chiffré est explicite et rend les octets déjà persistés au même destinataire encore autorisé. |
| Révocation | Blocage local immédiat. Retrait distant confirmé uniquement après inscription durable. Les anciennes capacités restent révoquées. |

La fenêtre des deux remises après admission est de cinq minutes selon le contrat. Chaque échange TLS conserve son échéance monotone locale de dix secondes. Les horloges de l’autorité et de l’invité ne sont pas comparées directement. Après un redémarrage qui empêche d’établir la validité restante d’un défi ou d’une fenêtre, refuser une nouvelle admission et réconcilier les décisions durables par identifiant.

Le détenteur doit inscrire le digest de la remise dans l’ordre durable de l’autorité avant de considérer la remise autorisée à l’émission. Le raccord devra qualifier la course entre cette inscription, le retrait et l’émission. Une remise déjà admise peut avoir une issue encore inconnue après fermeture locale. L’activation exige toujours une vérification courante de l’autorité. Une part déjà divulguée à une instance compromise peut avoir été copiée.

Un reçu ancien reste consultable comme événement historique. Il ne vaut jamais confirmation de la tête actuelle. Après restauration complète des disques du serveur, la tête indépendante doit encore refuser le triplet antérieur à la révocation. En cas d’autorité injoignable, maintenir le refus.

Les deux corps lient la même récupération, la même sauvegarde et la même époque. Intervertir les rôles échoue sur les clés, signatures et champs de rôle. Mélanger deux récupérations échoue sur H(R), identifiant et nonce. Une part mensongère doit échouer à l’authentification du triplet avant publication. Ces comportements restent à qualifier avec le raccord.

## 8. Bornes de transport, mémoire et IPC

Les constantes suivantes sont présentes dans les sources examinées au commit de référence. Les tailles du contexte et des enveloppes constituent les calculs de la proposition.

| Niveau | Borne et source | Conséquence pour le raccord |
| --- | --- | --- |
| Métadonnées sérialisées d’une liaison | `META = 56`, assertion `META <= 256` | L’enveloppe circule dans la charge utile. Aucun champ d’enveloppe n’est ajouté à ces métadonnées. |
| Métadonnées natives | `EXTRA <= 256`, `size_of::<Port>() <= 256` | Conserver les assertions et mesurer les types réellement utilisés |
| Pages volatiles | Deux pages de 4 096 octets | 8 192 octets utiles, sans rôle persistant ni décision de fraîcheur |
| Cellule du transport | En-tête 64, charge utile maximale 448 | Même charge utile maximale en P9/512 et P9/4096 |
| IPC privé vers le port | Message 128, préfixe 32, fragment utile 96 | Réassemblage borné. Une requête nue demande 4 fragments, 96 + 96 + 96 + 4. |
| Remise nue | 324 octets | 4 fragments, 96 + 96 + 96 + 36 |
| TLS invité | Fragment applicatif 2 048, deux buffers de 2 070 | Fragmenter les échanges complets. Conserver le délai de 10 secondes. |
| Registre | Deux tampons de travail de 1 024, budget privé proposé de 512 | La requête et la remise peuvent être traitées successivement. Contextes crypto, pile et copies à mesurer. |
| Racines IPC | Maximum 4 par processus | Réutiliser les conversations privées existantes, une remise en transit à la fois |

Sources des bornes et assertions. [État de liaison](../user/novaauth/transport_qemu/etat.rs) · [Cellules](../user/novaauth/transport_qemu/cellule.rs) · [Port invité](../user/novaauth/transport_qemu/port_invite.rs) · [Adaptateur natif](../user/novaauth/transport_qemu/adaptateur.rs) · [TLS invité](../user/novaauth/service/src/bin/common/canal.rs)

Le calcul de quatre fragments concerne les octets de l’enveloppe seule. Le TLS, les preuves d’approbation et les messages de contrôle ajoutent leur propre trafic. Avec un encadrement applicatif borné à 16 octets, une requête représente 308 octets applicatifs, une remise 340. Un record TLS 1.3 sans padding ajoute 22 octets, soit respectivement 330 et 362, dans les buffers existants. Les handshakes et échanges multi-records doivent être mesurés séparément.

Les objets R v2 de 376 octets et M de 418 octets tiennent également dans une fenêtre applicative de 2 048 octets. Le raccord doit les transmettre dans des messages typés de longueur connue, puis les référencer par digest. Les preuves et signatures supplémentaires utilisent des messages bornés séparés si nécessaire. Refuser tout réassemblage supérieur à la limite avant allocation. La conformité statique des tailles est établie, l’exécution de ce séquencement reste non vérifiée.

La fermeture ou l’expiration efface les tampons secrets et invalide l’opération. Un fragment manquant, dupliqué ou réordonné provoque un refus de réassemblage ou d’authentification. Une erreur de quota précède la délivrance. Les demandes suivantes restent fermées jusqu’au drainage des ressources provisoires.

Aucune racine supplémentaire n’est prévue pour les deux détenteurs hôtes. Les maxima proposés sont quatre pour le registre 22 et quatre pour le signataire 23. Les autres acteurs conservent leurs budgets. Les mesures effectives du raccord et de la variante à deux clés R restent à produire. La réserve instrumentée AArch64 de 301 cadres au-dessus du seuil de 2 048 reste ouverte. [Budget approuvé](novaauth-recuperation-2-sur-2-v0.24.0.md#6-budget-proposé) · [Réserve mémoire](novaauth-horloge-signataire-v0.24.0.md)

## 9. Refus automatique et contrôles à préserver

### Contrôle ayant arrêté l’édition

L’outil `exec_command` a renvoyé un refus `CreateProcess / Rejected / unacceptable risk` avant exécution de l’édition ajoutant les helpers de requête et leur raccord hôte. La raison invoquée était l’ajout d’une enveloppe, de clés supplémentaires et d’un changement du schéma durable de l’autorité, considérés comme affectant le modèle de confiance et exigeant un arbitrage.

Ce refus appartient au contrôle automatique d’autorisation des actions. Aucun nom de test du dépôt n’a été associé à ce refus. Les helpers recherchés `REQUEST_BYTES`, `REQUEST_INFO` et `open-request` étaient absents lors de la revue. La seconde édition prévue, concernant les clés publiques de réception, n’a pas été exécutée.

La démarche proposée consiste à faire approuver le présent encodage et la persistance additionnelle, puis à soumettre une édition limitée à ce périmètre. Les politiques du contrôle automatique et les assertions historiques restent applicables.

### Contrôles existants et ajouts proposés

| Contrôle réel | Assertion conservée | Ajout après validation |
| --- | --- | --- |
| `etat.rs` | Représentation canonique de 56 octets, taille, version, réserves et invariants de séquence | Vérifier que les nouvelles charges utiles laissent cette représentation identique |
| `adaptateur.rs`, `port_invite.rs` | Métadonnées natives et état de port ≤ 256 | Mesure après raccord et saturation, sans relever le plafond |
| `mesure_memoire.py` | Pages + supplément = total, maximum du supplément natif et sérialisé ≤ 256 | Archiver la mesure pour chaque architecture |
| `transport_qemu/tests.rs` | Décodage, bornes, ordre, refus et drainage existants | Charges 292/324, fragmentation et interruptions à chaque frontière |
| `qualification_tls.py` | Messages de 1, 448, 449 et 2 048 octets, authentification et rejouement TLS | Requêtes/remises authentifiées applicatives et reprise de leur issue |
| `abi_registre.py` | Neuf contrôles, signatures historiques et 48 sélecteurs | Rejouer la suite intacte après raccord |
| Tests locaux de récupération en cours | Attentes des formats et refus déjà écrits | Nouveaux cas séparés pour requêtes 292 et contexte v2, avec refus des anciens contextes sur une opération v2 |
| Brouillon `Shares::receive` | En v1, les deux parts sont liées à la même clé destinataire | Ajouter un parcours v2 liant les deux clés approuvées à leurs rôles et au même H(R), conserver le contrôle et les tests v1 |

L’ajout doit comprendre des tests de format et des tests d’intégration distincts. Les tests historiques continuent de s’exécuter. Une incompatibilité découverte impose un diagnostic et l’arbitrage prévu par le mandat.

## 10. Plan de qualification complémentaire

Chaque recette d’intégration ci-dessous est à exécuter sur AArch64/P9-512, AArch64/P9-4096, x86_64/P9-512 et x86_64/P9-4096. Le statut initial de ces nouveaux contrôles est **non vérifié**.

| Contrôle | Oracle exigé |
| --- | --- |
| Canonicalité | 292 octets exacts, tous offsets, ordre des entiers, refus de 291/293 et de corps 121/123, versions et rôles inconnus |
| Couverture | Mutation de chaque champ signé, du tag, de l’encapsulation et du chiffré refusée, vecteurs indépendants d’interopérabilité |
| Séparation des détenteurs | Requête G refusée par A et inversement, clés publiques différentes, aucune part obtenue avec une seule garde compromise |
| Récepteurs de rôle v2 | R.rx.G et R.rx.A distinctes, part G indéchiffrable avec R.rx.A, refus d’une inversion des clés dans le contexte approuvé |
| Identité | Substitution serveur, domaine, incarnation, mesure, H(M), époque, défi, nonce ou identifiant refusée |
| Enregistrement | Requêtes non approuvées et tentative de remplacement par R refusées, deux rotations concurrentes sérialisées, époque partielle inutilisable |
| Fraîcheur | Défi expiré, ancienne époque révoquée, reçu historique et autorité indisponible entraînent le refus d’admission |
| Unicité | Deux demandes concurrentes par rôle, une réservation et une seule enveloppe durable, ACK idempotent, aucune seconde combinaison |
| Perte d’ACK | Issue retrouvée par identifiant, récupération explicite du même chiffré et aucun nouveau scellement implicite |
| Coupures | Avant/après réservation, persistance, admission, émission, ACK et activation, avec ordre durable des mutations et retraits |
| Saturation | Frontières de 96, 128, 448 et 2 048 octets, allocations refusées sans fuite, quatre racines maximum |
| Ressources et secrets | Compteurs avant/pic/après drainage, effacement observable des tampons, absence des secrets synthétiques dans les traces |
| Anti-rollback VFS | Lecture réelle avant révocation, refus après restauration, récupération à deux parts, réauthentification et lecture du dossier autorisé, zéro octet du dossier révoqué |
| Compatibilité | NovaAuth, NovaFS, IPC, neuf contrôles ABI et quatre ELF historiques selon les composants affectés |

Les échecs de format sont diagnostiqués avant déchiffrement. Les échecs cryptographiques produisent un refus public uniforme. Les journaux conservent seulement les références publiques, les états et les résultats, afin de préserver la confidentialité des parts et des clés.

Le refus VFS après restauration et la reprise d’accès légitimes restent les critères de bout en bout. Conserver les limites QEMU, la dépendance à l’autorité indépendante hors instantané, les limites d’effacement mémoire et la réserve mémoire AArch64. La procédure de laboratoire reste à distinguer d’une récupération de production qualifiée.

## 11. Modifications strictement nécessaires après arbitrage

1. Ajouter le format de requête 292, son encodeur, son décodeur strict et les vérifications cryptographiques dans le composant de récupération QEMU.
2. Ajouter l’enregistrement authentifié des clés G.req et A.req dans le schéma privé de l’autorité, avec migration transactionnelle additive et refus sûr d’une époque incomplète.
3. Si la variante du §5 est validée, ajouter R v2 et les deux récepteurs dans la même instance, couvrir les deux clés par les preuves existantes et conserver les tests v1. Le collecteur de parts doit vérifier le couple de clés approuvé et le rôle de chaque part dans un parcours v2 explicite.
4. Raccorder les requêtes aux détenteurs et leurs journaux durables. Vérifier la réservation, la décision d’émission, l’ACK et la réconciliation dans l’ordre durable.
5. Réutiliser le transport fragmenté existant et mesurer les allocations effectives. Ajouter les tests du §10 et archiver les résultats positifs et négatifs.

La proposition ne demande aucune capacité noyau supplémentaire, aucun nouveau détenteur de secret ni nouveau sélecteur. Elle modifie les messages privés et le schéma de l’autorité de laboratoire. Le passage de R v1 à R v2 doit être validé pour satisfaire les deux clés destinataires des remises. Toute nécessité découverte d’un droit ou d’une frontière de confiance supplémentaire restera soumise à un arbitrage distinct.

## 12. Vérifications réalisées et décision à rendre

La revue a contrôlé le contrat au commit complet, les sources des bornes, les offsets et les sommes de tailles. Les neuf contrôles de `novaOS/tests/abi_registre.py` ont réussi le 17 novembre 2026, avec leurs assertions inchangées. Le contrôle des tailles est documentaire. Les nouvelles recettes cryptographiques et QEMU restent non vérifiées, leur exécution dépend du raccord soumis à validation.

Les preuves de cette revue comprennent l’inventaire SHA-256 des sources consultées, les calculs de tailles, la vérification des liens et le journal ABI. Elles sont conservées dans `OS-worktrees/preuves-enveloppes-recuperation-2ae44a2/` et dans l’archive versionnée associée. [Archive de revue](validation/enveloppes-recuperation-v0.24.0.tar.gz) · [Empreinte de l’archive](validation/enveloppes-recuperation-v0.24.0.tar.gz.sha256)

**Validation ciblée sollicitée**

- Approuver le format de requête de 292 octets, ses couvertures et l’enregistrement authentifié des deux clés de réception des détenteurs.
- Approuver le contexte v2 de 376 octets avec deux clés destinataires de rôle, dans la même incarnation de récupération, pour satisfaire le point 5.
- Autoriser ensuite le raccord QEMU et les tests additionnels, en conservant les assertions historiques et les plafonds existants.
