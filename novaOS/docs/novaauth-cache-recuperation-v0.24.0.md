# NovaOS v0.24.0 — Cache chiffré de récupération QEMU

## Périmètre et cause

Branche : `codex/novaauth-autorite-0240`. Base des travaux :
`42128c2748bbbe0b4dd7c1fba74bf0214054b580`.

Le registre téléchargeait deux fois les mêmes fragments chiffrés : une première
passe vérifiait le triplet natif, la seconde préparait les trois objets privés
dans NovaFS. Sur le témoin EXEC P9/4096, ce doublon dépassait parfois le délai
TLS de 10 secondes avant la création du moindre ticket. Ces échecs sûrs sont
conservés ; ils ne prouvaient ni une révocation distante ni une consommation
concurrente de ticket.

La décision MOA/MOE retient **huit fragments au maximum et 3 820 octets de
ciphertexts au total**, et remplace la proposition antérieure de 8 320 octets.
L'optimisation reste dans le récepteur des profils QEMU `triplet-guest`.
Aucun délai TLS, quota IPC, sélecteur ABI ou format P9 n'est changé.

## Représentation et invariants

`service/src/cipher_cache.rs` contient une structure de 3 920 octets :
3 820 octets de charge utile, huit paires offset/longueur, une empreinte de
contexte, un scellement SHA-256 local, une longueur et deux indicateurs.
Aucune allocation sur le tas n'est effectuée par le cache. Les tampons de
transport et de déchiffrement déjà nécessaires au protocole restent distincts ;
3 820 octets n'est pas la consommation totale du registre.

Le contexte est le digest complet du `RecoveryContextV2` authentifié :
opération de récupération, serveur, domaine, incarnation, défi, manifeste de
sauvegarde, génération et destinataires. Un fragment n'entre dans le cache
qu'après authentification AEAD lors de la première passe. Les données en clair,
les parts et les clés privées ne sont jamais placées dans cette structure.

Avant la deuxième passe :

1. Le triplet complet doit avoir passé son contrôle natif d'empreinte.
2. Le registre consulte de nouveau la tête authentifiée de l'autorité.
3. Le contexte et le scellement du cache sont revérifiés à chaque lecture.
4. Chaque fragment est de nouveau authentifié AEAD avant préparation P9.
5. Le surplus éventuel non mis en cache est retéléchargé selon le chemin initial.
6. La relecture complète P9, son empreinte et les contrôles de tête avant et
   après activation restent obligatoires.

Le cache est local à une seule récupération. Les erreurs remontent sans
activation ; `Drop` appelle l'effacement logique. Le chemin de succès le vide
explicitement avant la relecture P9. Les tests vérifient le nettoyage logique,
sans revendiquer un effacement physique de toutes les copies de mémoire.

## Qualification

Les résultats sont consignés dans `validation/cache-recuperation-0240.json` et
son archive de traces. Le manifeste distingue les preuves publiques des artefacts
exacts de laboratoire conservés hors des répertoires temporaires.
Les échecs et les parcours instrumentés restent séparés des succès.
Les sondes du cache sont regroupées dans les marqueurs existants pour respecter
le quota historique de 16 messages DEBUG par processus et conserver les
mesures finales de capture.

Le banc négatif `autorite_qemu/cache_recuperation_qemu.py` exerce le vrai invité
et exige le refus du VFS avec zéro octet privé. Son injection de tête utilise
l'API durable authentifiée de l'autorité de laboratoire : elle ne modifie pas
directement sa base de données. Cette injection n'est pas présentée comme une
mutation d'identité réalisée par un client invité.

## Limites conservées

La marge temporelle dépend du profil, du volume et de la charge de l'hôte.
La borne de 10 secondes reste un refus sûr, pas une garantie de disponibilité
pour toutes les tailles possibles de registre.

La réserve historique AArch64 de **301 cadres instrumentés** reste ouverte.
Ces essais QEMU ne qualifient ni le noyau ordinaire comme distribution NovaAuth,
ni une autorité ou une récupération de production, ni UTM ou le matériel réel.
Aucune fusion dans `main` et aucun tag ne sont autorisés par ce bilan.

## Résultats du cache et limites de mesure

| Contrôle | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
| --- | --- | --- | --- | --- |
| Récupération VFS complète, restauration et reprise 2-sur-2 | réussi | réussi | réussi | réussi |
| Lectures autorisées et refus du dossier révoqué sans octet privé | réussi | réussi | réussi | réussi |
| Tête durable modifiée entre les passes | refus sûr | refus sûr | refus sûr | refus sûr |
| Ciphertext altéré ou fragment substitué | refus sûr | refus sûr | refus sûr | refus sûr |
| Réponse ciphertext perdue, sans répétition automatique | refus sûr | refus sûr | refus sûr | refus sûr |
| Expiration globale TLS de 10 s | refus sûr | refus sûr | refus sûr | refus sûr |
| Borne, mauvais contexte et effacement logique du composant dans l'invité | réussi | réussi | réussi | réussi |

Les tests de mauvais contexte manipulent l'empreinte complète du contexte dans
le composant invité, avant le dialogue de récupération. Ils ne simulent pas une
nouvelle autorité ni une approbation d'une autre incarnation. Les tests d'altération
et de substitution passent, eux, par le transport réel et aboutissent à la
quarantaine du registre et au refus VFS. La récupération positive et les pertes
de reçus durables restent exercées par le parcours VFS complet.

Le comparatif fonctionnel réutilise les quatre recettes non optimisées
`qemu-vfs-*-v26-final` : mêmes statuts et mêmes contenus des lectures VFS après
reprise, vérifiés dans `comparaison-vfs-non-optimise.json`. Il ne confond pas les
empreintes de journaux, qui diffèrent, avec l'égalité des données retournées.

| Témoin EXEC, durée monotone de récupération (ms) | Sans cache | Avec cache |
| --- | ---: | ---: |
| AArch64/512 | 9 500 | 7 916 |
| AArch64/4096 | 10 680, refus sûr | 9 452 |
| x86_64/512 | 9 804 | 8 028 |
| x86_64/4096 | 10 716, refus sûr | 9 420 |

Ces observations sont des passages instrumentés isolés, pas des moyennes ni une
borne garantie. Le témoin télécharge cinq fragments au lieu de dix :
2 771 octets chiffrés sur AArch64, 2 810 sur x86_64. La marge P9/4096 demeure
faible. Les fins des essais expirés comprennent le nettoyage après échéance ;
elles n'indiquent pas un allongement du délai TLS.

Les compteurs du registre et du signataire reviennent au niveau initial lors
des parcours achevés. Lors de l'expiration globale, le signataire termine : le
registre perd le handle correspondant et les cadres IPC globaux diminuent de
44 à 40. Ce retrait est distingué d'un retour à l'identique des compteurs.
Le cache utilise la pile déjà réservée ; sa taille native est de 3 920 octets,
sans allocation sur le tas. Cela ne signifie pas que l'ensemble du processus
consomme seulement cette quantité de mémoire. Le pic exact de pile pendant les
copies temporaires et le calcul SHA-256 n’est pas mesuré isolément ; les
compteurs exposés mesurent les pages réservées, les handles et les cadres IPC.

Les neuf contrôles du registre ABI passent. Les quatre ELF historiques ont été
rejoués sur les sources du raccord avant le cache ; celui-ci ne change pas leur
chemin noyau. Les noyaux ordinaires reconstruits dans un même chemin sont
identiques octet par octet à la base sur les deux architectures. Les qualifications
hôtes, invitées et cette comparaison binaire sont enregistrées séparément.

## Diagnostics conservés

- Le premier essai négatif d'expiration avec une pause unique de 11 s exerçait
  le délai de fragment de 2 s, pas le délai global demandé. Il est conservé.
  L'essai global utilise des retards de 600 ms successifs et maintient la
  progression du transport jusqu'à l'échéance de 10 s.
- L'ajout de messages DEBUG séparés épuisait le quota historique de 16 messages
  et masquait les dernières mesures de capture. Les sondes ont été regroupées
  dans les messages existants ; le quota n'a pas été relevé.
- Une suspension volontaire de VM A pouvait dépasser les 45 s du socket TLS
  hôte pendant la récupération de B. Le banc distingue désormais le temps de
  suspension QMP explicite du temps de VM active. Aucune horloge de NovaOS ni
  de l'autorité n'est modifiée. Une première version de ce banc omettait
  `recv_into` et échouait avec `SSLWantReadError` ; le test de lecture différée
  couvre ce défaut. Ces échecs ne valent pas preuve de révocation distante.

## Poursuite des tickets

Les essais concurrents réels passent sur les quatre profils : un succès, un
refus, un enfant et une seule collecte. Les anciennes formes SPAWN restent
refusées depuis chacun des threads secondaires. Le témoin à deux VM avec ticket
non expiré passe aussi sur les quatre profils ; A est refusée pour retrait,
aucun enfant A n'est créé et le nouveau ticket B réussit.

La création retardée après admission passe sur les quatre profils : B reste
bloquée, le PID de A est enregistré durablement, puis le drainage permet
l’activation et le lancement de B.

La qualification complète des courses, annulations et rejeux après redémarrage
reste distincte de la qualification du cache. Ce document n'en prononce pas la
clôture. Les résultats détaillés de la création retardée et les essais encore
manquants doivent rester explicites dans le bilan du raccord EXEC_TICKET.
