# Ethernet v1.0 — contrat privé exécutable

Ce contrat relie un producteur de trames et un port Ethernet. Le contrôle tient dans la charge DATA existante de 128 octets. La charge Ethernet circule séparément. Aucun identifiant d’opération de ce document n’est un sélecteur système. La définition normative de cette phase est le [codec](../../tests/ethernet_v1/wire.rs), son [interpréteur](../../tests/ethernet_v1/protocol.rs) et le [modèle Rust no_std](../../tests/ethernet_v1/lib.rs).

## Identité et autorité

Le lanceur installe une paire privée à un seul pair. Il associe à la direction de réception une identité de démarrage de 16 octets et cinq entiers u64 non nuls désignant le port, le titulaire, son incarnation, le canal et l’époque. Le démarrage doit différer de seize octets nuls. L’identifiant de ticket ajoute à cette liaison le numéro de requête TX et l’indice dans le lot. Un slot ajoute à la liaison son sens, son indice et une génération u32. Le sens est déterminé par l’opération. Seule l’époque figure dans le message. Le récepteur obtient le reste de l’identité de la paire installée par son autorité. Le renouvellement de l’incarnation, du canal ou du démarrage invalide les références précédentes.

Une application doit recevoir une délégation explicite. Les bits de droits sont TX = 1, RX = 2, observation = 4 et administration = 8. OPEN exige au moins un droit et refuse la demande entière si un bit dépasse la délégation. Une répétition avec les mêmes droits retrouve le port. Une tentative de les changer renvoie un conflit. La négociation précède OPEN. Une fois ouvert, le port conserve sa version, ses fonctionnalités et sa MAC de station unicast non nulle. Le backend et le superviseur ont leurs entrées de confiance hors protocole client.

## Représentation

Chaque entier de contrôle est little-endian, sans alignement implicite. Une trame conserve l’ordre des octets Ethernet, de la MAC destination au dernier octet de données. Le préambule et le FCS sont extérieurs au contrat. Tous les offsets ci-dessous sont en octets.

| Offset dans l’en-tête | Type | Valeur |
|---:|---|---|
| 0, 2 | 2 × u16 | Version majeure 1 et mineure 0 |
| 4, 6 | 2 × u16 | Opération et indicateur de direction (0 requête, 1 réponse, 3 erreur) |
| 8, 10 | 2 × u16 | Taille exacte, en-tête compris (32 à 128), puis nombre d’entrées (0 à 8) |
| 12, 14 | 2 × u16 | File 0 et champ réservé à zéro |
| 16, 24 | 2 × u64 | Époque de session et identifiant de corrélation, tous deux non nuls |

Un descripteur occupe 12 octets. Aux offsets 0, 2, 4, 8 et 10 figurent l’indice de slot u16 (0 à 7), la longueur u16, la génération u32 non nulle, les options u16 et le réservé u16 à zéro. Un slot apparaît une seule fois par liste. Un reçu de 12 octets contient l’indice consécutif dans le lot u16, l’état u16 et l’identifiant TX initial u64. RX peut répondre avec une liste vide. Le compteur vaut zéro pour un corps scalaire. Toute réponse restitue l’époque et la corrélation de la requête. Une enveloppe d’erreur a 40 octets, l’indicateur 3, un compteur nul, le code u8 à l’offset 32 et sept zéros.

| Code privé | Signification | Code privé | Signification |
|---:|---|---:|---|
| 1 | Délégation ou identité refusée | 7 | Même clé, contenu différent |
| 2 | Référence périmée | 8 | Session fermée |
| 3 | Version incompatible | 9 | Transition interdite |
| 4 | Fonction obligatoire indisponible | 10 | Lien déclaré bas |
| 5 | Format ou valeur invalide | 11 | Génération épuisée |
| 6 | Crédit, journal, slot ou quota saturé | 12 | Prochaine requête TX identifiable et jamais admise |

Le décodeur de trame de contrôle refuse une version, un opcode, un nombre d’entrées ou une longueur inattendus. Une enveloppe que l’interpréteur ne peut décoder échoue avant l’admission. La construction d’un message d’erreur exige une corrélation exploitable. Les [vecteurs](../../tests/ethernet_v1/vecteurs.bin) utilisent le préfixe ASCII ETH1, suivi d’enregistrements « classe u8, longueur u16 little-endian, octets ». Les classes 0, 1 et 2 indiquent respectivement une enveloppe canonique, une enveloppe à refuser et une erreur canonique. Le [générateur indépendant](../../tests/ethernet_v1/vecteurs.py) fige les 39 cas.

## Opérations

Les offsets dans cette table repartent du début du corps. Le modèle prend en charge une file TX et une file RX, MTU 1 500, lot maximal de huit. Les octets réservés des corps produits valent zéro.

| Nº et opération | Autorité | Corps de la requête | Corps de la réponse |
|---|---|---|---|
| 1 NEGOTIATE | Titulaire, avant ouverture | Deux masques u64, obligatoire puis optionnel | Deux masques u64, sélectionné puis disponible |
| 2 OPEN | Délégation du lanceur | Droits u8 et sept zéros | Droits, lot max, nombre TX, nombre RX en quatre u8, MTU u16, MAC de station (6 octets), quatre zéros |
| 3 TX | TX | 1 à 8 descripteurs et octets correspondants dans les slots privés | Un reçu par trame dans l’ordre de la demande |
| 4 QUERY | TX | Identifiant TX initial u64 | Reçus actuels de toutes les trames du lot |
| 5 RX | RX | Vide | Liste des slots RX occupés, accessible de nouveau tant qu’ils restent détenus |
| 6 RELEASE | RX | 1 à 8 descripteurs de longueur zéro | Vide après validation de la liste entière |
| 7 ARM | Titulaire ouvert | Dernière révision vue u64 | Révision u64, masque u8, sept zéros |
| 8 LINK | Observation | Vide | Porteuse u8, duplex u8, six zéros, bits/s u64 (0 si inconnu) |
| 9 STATS | Observation | Vide | Huit u64 saturants, dans l’ordre admises TX, publiées TX, complétées TX, admises RX, pertes RX faute de slot, restitutions RX, refus de capacité TX, annulations TX |
| 10 JOIN, 11 LEAVE | RX et multicast négocié | Une MAC multicast de six octets | Vide, les répétitions sont idempotentes |
| 12 CLOSE | Titulaire ouvert | Vide | Vide, la répétition de la même clôture retrouve son reçu |
| 13 ACK | TX | Dernier identifiant TX contigu à acquitter, u64 | Vide après vérification du préfixe entier |
| 14 SET_LINK | Administration du modèle | Même représentation que la réponse LINK | Vide |

Pour LINK, la porteuse est inconnue (0), basse (1) ou haute (2). Le duplex est inconnu (0), half (1) ou full (2). Une vitesse connue ou un duplex déterminé exige une porteuse haute. Le modèle débute en état inconnu et refuse une nouvelle TX à l’état bas. SET_LINK simule un changement d’état. La fonctionnalité multicast utilise le bit 0 du masque u64. Un bit obligatoire inconnu échoue. Une option inconnue reste désélectionnée. Toute extension de format exige sa propre négociation et ses vecteurs. Le champ des options des descripteurs sert de réserve.

## Propriété, reprises et retrait

| État du ticket | Preuve et action permise |
|---|---|
| 0 Admise (Admitted) | Le service a copié l’image dans son journal privé et débité un crédit. Le producteur peut réutiliser le slot CPU avec la génération suivante après preuve d’admission. |
| 1 Publication engagée (Publishing) | Le backend prépare l’exposition. Une annulation demande la preuve que l’image est restée invisible. |
| 2 Publiée (Published) | Le backend atteste la visibilité de cette seule entrée. Les autres entrées du lot peuvent être à un stade différent. |
| 3 Complétée localement (Complete) | Le backend accepte une fois la complétion correspondante et rend un crédit. Seul un oracle indépendant chez le destinataire établit la réception distante. |
| 4 Annulée (Cancelled) | Le backend atteste l’absence de publication. Le chemin d’annulation d’une session ouverte rend un seul crédit. |
| 5 Issue inconnue (Unknown) | Au retrait, une entrée engagée ou publiée sans preuve terminale garde cette issue dans le registre de retrait. Toute réémission automatique est interdite. |

La séquence TX part de 1, progresse d’une unité par lot admis et reste indépendante des corrélations de contrôle. Huit crédits couvrent les entrées en vol, deux lots non acquittés tiennent dans le journal. Avant la première mutation, le service contrôle la capacité du lot entier, la génération, la longueur (14 à 1 514), l’unicité de chaque slot et la MAC source de la session. L’image publiée est complétée par des zéros jusqu’à 60 octets. Les générations n’effectuent aucun retour à zéro.

Après perte du reçu, QUERY restitue l’état par indice. Une même soumission, avec mêmes slots, générations, ordre et octets, retrouve le lot. À clé égale, un octet différent entraîne un conflit. NotAdmitted désigne exactement le prochain identifiant encore absent du journal. Une clé périmée ne prouve aucune absence d’émission. ACK libère les images du préfixe contigu uniquement si chaque entrée est complétée ou annulée. Sous pression, le client consulte les reçus, acquitte les lots terminés et attend de nouveaux crédits. Le numéro identique reste utilisable après un refus avant admission.

En RX, le backend accepte 60 à 1 514 octets vers la station, le broadcast ou l’un des huit groupes admis par la session. Le client détient une vue en lecture jusqu’au RELEASE. Le premier retour efface les octets et libère le slot. Un deuxième retour pour la même génération réussit sans effet additionnel. Dès la réattribution, la génération précédente est périmée.

Une révision et quatre bits coalescent TX (1), RX (2), changement de lien (4) et fermeture (8). ARM compare la révision et installe l’unique attente en une opération du modèle. Un changement déjà présent revient immédiatement. Après notification, le client relit les objets concernés, puis réarme. À saturation de la révision, le modèle impose la relecture et évite une attente indéfinie. En fermeture, le signal terminal déjà dû demeure observable par le titulaire.

CLOSE ou le retrait par le superviseur ferme les voies, efface les images et slots privés, retire les adhésions, interdit tout nouvel accès et classe chaque ticket. Le modèle conserve les états de retrait pour le superviseur. La voie CPU expérimentale ne reçoit aucune adresse de bus, page DMA, IRQ ou permission de descripteur. Les résultats de séparation entre espaces et les limites du médiateur figurent dans le [rapport de qualification](phase2.md).
