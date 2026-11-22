# NovaAuth v0.24.0 — Consultation des mutations R2 après interruption

Le reçu historique est une consultation privée additive, opération 53 entre le
registre isolé et le signataire QEMU, puis l'autorité indépendante. Il n'est
pas un sélecteur ABI. Les opérations 50 (publication), 51 (acquittement) et 52
(état des effecteurs) conservent leurs encodages et leurs assertions historiques.

## Motif et sécurité

Après redémarrage, le registre ne doit pas accorder une consultation d'issue à
n'importe quel demandeur ni perdre l'identité administrative de l'auteur. Le
journal durable conserve déjà la requête signée d'origine. L'opération 53 la
restitue dans une réponse authentifiée fraîche ; le registre doit encore
contrôler le principal courant, ses droits et la publication correspondante.
La réponse ne publie rien, n'acquitte aucun effecteur et ne fait pas progresser
le drainage. Elle ne constitue pas une autorisation de réactiver un compte.

## Encodage canonique

Les entiers du protocole d'autorité sont en big-endian. La requête compte 160
octets : état serveur/domaine/incarnation/génération/empreinte de 112 octets,
identifiant d'opération de 16 octets et défi frais de 32 octets. Elle est signée
par le registre avec séparation de domaine
`NovaAuth/QEMU/administration-history/v1\0`, puis transportée par le signataire
isolé. Les contrôles de portée et d'identité TLS préexistants restent actifs.

La réponse répète la requête (160), la publication historique (209), le masque
des effecteurs (1), éventuellement le reçu terminal (128 + signature 64), puis
une signature d'autorité (64) : exactement 434 ou 626 octets. Les cadres privés
existants acceptent cette taille applicative ; les métadonnées de liaison ne
changent pas. Le tampon de vérification de l'opération 53 est borné à 640 octets ;
les vérifications historiques restent sur leur tampon de 512 octets.

Le codec vérifie la signature et la requête fraîche exacte, puis le serveur,
le domaine et l'identifiant d'opération de la publication d'origine. Le reçu
terminal reste soumis à `Retired::verify`, avec le retrait persistant attendu.
Une réponse sans reçu terminal ne démontre jamais une clôture de révocation.

## Validation acquise et limites

Le vecteur public provient de l'autorité Python ; aucune clé privée n'est
archivée dans ce vecteur. Les tests Rust vérifient les réponses avec/sans reçu,
chaque altération d'octet, toutes les troncatures, la substitution de nonce et
le refus de cet encodage par l'ancienne opération 52. Les tests de l'autorité
vérifient notamment l'absence de mutation ou de progression lors de la lecture.

Ces validations sont des tests hôtes et des tests de codec. Elles ne qualifient
pas encore le raccord complet des commandes, la révocation des deux sessions,
ni la disponibilité après redémarrage dans NovaOS.
