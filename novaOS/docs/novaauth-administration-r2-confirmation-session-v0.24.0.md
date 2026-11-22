# NovaAuth R2 — Préparation et confirmation dans la même session

Raccord en cours sur `codex/novaauth-administration-0240-phase1`, profil QEMU
exclusivement. Ce document ne déclare pas les mutations qualifiées.

## Frontière de confiance conservée

Le canal NXADMIN1 reste celui de l'ELF attesté. Le vérificateur relève auprès
du noyau l'installation, le titulaire, la session et l'instance du shell. Il
vérifie le bail et les portées à chaque demande. Le registre vérifie encore
la tête indépendante, le rôle, l'époque et la portée persistante de l'image.
La possession du canal ou d'un identifiant de préparation n'accorde aucun droit.

Pour une mutation seulement, le vérificateur complète NXACTX01 par une trame
privée NXABND02 avant NXACMD01. Cette trame ne peut pas être présentée par un
ELF : le décodeur frontal n'accepte que NXACMD01. Elle emprunte la conversation
existante vérificateur/registre ; aucun endpoint, handle ou droit supplémentaire.

| Octets | Contenu |
| --- | --- |
| 0–7 | ASCII `NXABND02` |
| 8–39 | Identité de session inspectée par le vérificateur |
| 40–71 | Identité de l'instance `nsh` issue de sa route privée |
| 72–127 | Zéros obligatoires |

Une liaison exige un contexte présent, ne peut être remplacée et est consommée
avec la requête suivante. Une autre requête sur la conversation efface le
contexte. Aucun changement de NXEXEC01, des 48 sélecteurs ou du noyau ordinaire.

## Préparation, publication et issue

L'identifiant de préparation et sa confirmation sont déterministes pour la
même incarnation, session, instance, référence de facteur, époque, tête, image,
portée et requête canonique. Le registre prépare le plan avec son moteur actuel,
sans écrire. Deux intentions au maximum sont retenues ; chaque entrée mesure au
plus 512 octets, assertion de compilation. Une nouvelle intention de la même
session remplace la précédente, sans mutation et en invalidant sa confirmation.

La confirmation exige ces mêmes liaisons, une tête identique et une nouvelle
vérification des droits. L'intention est consommée avant toute écriture. Une
seule mutation peut être soumise à la fois. NovaFS prépare le triplet entier ;
l'autorité décide sa publication ; le registre vérifie le reçu, relit la tête
et les objets P9 avant de retourner COMPLETE. Aucun second gestionnaire de
stockage n'est introduit.

Une erreur après consommation conserve la mutation en UNCERTAIN. La reprise
consulte le reçu du même identifiant puis relit le triplet ; elle ne rappelle
jamais la fonction de soumission. En absence de preuve suffisante, les nouvelles
opérations restent refusées. Un échec avant l'envoi de la décision peut donc
nécessiter la récupération du registre ; sa résolution automatique n'est pas
annoncée disponible.

## Limites explicites du raccord en cours

- Seuls le renommage et la création de groupe sont raccordés à la publication.
- La préparation d'une désactivation utilise le contrôle transactionnel existant
  interdisant la désactivation d'un administrateur. Aucun retrait n'est publié
  tant que ses effecteurs ne sont pas raccordés.
- Création d'utilisateur, appartenance, désactivation et réactivation restent
  refusées. Aucune référence de facteur reçue en argument n'est auto-approuvée.
- Deux issues achevées sont conservées en mémoire. Leur consultation exige aussi
  l'historique authentifié de l'autorité et le même principal. La première façade
  exige conservativement les deux portées Users et Groups pour cette consultation.
- Un reçu UPDATE isolé ne prouve pas les acquittements d'une éventuelle révocation.
  Après redémarrage ou éviction de l'issue locale, la consultation reste UNCERTAIN.
  La récupération durable des issues administratives après redémarrage reste à
  terminer ; aucun succès n'est déduit du seul changement de génération.
- Le coût du plan transitoire, la mémoire résidente et la restitution doivent être
  mesurés en invité. L'assertion sur les intentions ne remplace pas ces mesures.

## Contrôles à exécuter

Deux ELF successifs dans une même session préparent puis confirment une création
de groupe ou un renommage. Vérifier le statut natif, le reçu, la génération et le
triplet relu. Vérifier les confirmations étrangères, périmées, substituées et
concurrentes, les droits de Bob, le refus transactionnel du dernier administrateur,
la perte d'une réponse et la consultation sans seconde mutation. Rejouer les
consultations et les refus du canal R1. Les quatre profils QEMU sont requis.

Les autres scénarios R2, les limites de production et les réserves historiques
restent ouverts. Aucune fusion, aucun tag ni reprise des utilitaires.
