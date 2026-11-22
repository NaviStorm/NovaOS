# NovaAuth v0.24.0 — Sécurité R2, campagne du 22 novembre 2026

Référence initiale : `fc1ed9b4692dfe9d47099f5aa27bb129d616e7bf`.
Budget : 90 minutes actives, attente du verrou QEMU exclue. Utilitaires en pause.
Aucune fusion, aucun tag, aucune activation ordinaire.

## Matrice initiale, avant nouvelles exécutions

Les quatre colonnes invitées seront remplies à partir des résultats effectifs.
Un test du moteur sur Darwin ne vaut jamais qualification dans l'invité.

| Cas | Couverture existante applicable à examiner | Complément invité nécessaire |
|---|---|---|
| S1 auto-attribution | `admin-sonde --r2-refus`, ajout au groupe administratif refusé 5 | confirmation directe et absence de changement de privilèges |
| S2 registre persistant | confinement NXADMIN1 sans port de stockage | trois trames APPEND, refus du canal administratif et du noyau sans handle |
| S3 référence d'Alice | contexte forgé et métadonnées refusés | références exactes appartenant à une session Alice vivante |
| S4 requête directe | lecture/Prepare de Bob refusés par le serveur | Confirm/Outcome directs, sans filtrage CLI |
| S5 substitution | NXACTX01 et rejeu refusés 3 | cibles Alice/Bob, désactivation/réactivation/retrait d'appartenance |
| S6 tickets | révocation avant expiration sur les quatre profils acceptée | expiration et consommation/rejeu à rattacher au candidat R2 |
| S7 concurrence | tests hôtes et dernier administrateur séquentiel | mutations réellement concurrentes dans l'invité |
| S8 UNCERTAIN | reçu perdu après publication, consultation sans seconde mutation, quatre profils | vérifier sources et manifestes d'applicabilité |
| S9 réactivation | moteur/autorité hôtes testés ; le service invité refuse actuellement Reactivate | raccord au reçu terminal authentifié puis nouvelle authentification invitée |

## Nouveau parcours négatif séparé

`profile.py --administration-securite` conserve le raccord duplex approuvé et
sélectionne la sonde `admin-sonde --r2-securite`. Le témoin temporel accepté
`--administration-ticket-vivant` n'est pas modifié. Les deux modes sont exclusifs.
Les chemins PAGE de transport déjà qualifiés sont repris, sans changement de
bail, délai, quota ou capacité. NXEXEC01, NXADMIN1 et les noyaux ordinaires
ne sont pas modifiés.

La sonde conserve les huit réponses historiques, puis exige les refus natifs
de Confirm et Outcome directs, trois requêtes APPEND correspondant aux trois
tables d'identité, trois envois sans handle de stockage, et trois substitutions
de cible. La vue de Bob doit rester identique et non administrative. Les flux
bruts sont collectés uniquement par le banc ; Alice ne reçoit pas les sorties
ni les capacités de Bob. Un refus d'APPEND avant stockage n'est pas présenté
comme une ouverture VFS réussie suivie d'un refus d'écriture.

Les échecs, essais non exécutés et réserves seront conservés dans le bilan.

## Raccord ciblé de réactivation

Le service refusait `Reactivate` (12) malgré la présence du moteur transactionnel
et du protocole d'autorité R2. Il consulte maintenant, par `OUTCOME` existant avec
défi frais, le retrait enregistré dans le compte désactivé. La signature de
l'autorité et l'état terminal avec les sept acquittements sont vérifiés. Le
moteur compare le reçu à la génération, l'époque, la requête et l'empreinte
précédente du retrait persistant. Ce contrôle est refait à la confirmation.
Un reçu manquant ou non terminal laisse une issue indéterminée et interdit
la mutation ; aucune demande d'acquittement n'est répétée automatiquement.

Le getter interne donnant l'identifiant du retrait exige un administrateur
actif, un grant de la tête courante et un sujet désactivé. Il ne confère aucun
droit et ne remplace pas le reçu. Aucun opcode ni sélecteur supplémentaire.
La publication utilise toujours le même triplet et le même écrivain NovaFS.

La recette `--administration-reactivation` prolonge le témoin temporel : Bob
conserve son ancienne référence, attend la confirmation de la réactivation,
s'authentifie explicitement dans la même instance puis vérifie le refus de
l'ancien ticket et de l'ancienne session. Un nouveau lancement doit réussir.
Ce déclenchement utilise la conversation privée de qualification déjà approuvée,
lie les deux instances et l'opération, et ne constitue pas une authentification.
Les résultats invités restent à établir ; les huit tests du moteur sont hôtes.

### Premier essai S9 : saturation du banc

Sur x86_64/P9-512, commit `71bba986ac39258de35eb3b91db79f35420ed50d`,
la désactivation et la réactivation sont confirmées (générations 24/25).
Une nouvelle session distincte est authentifiée ; l'ancien ticket est refusé
10 et l'ancienne session 5. Le relais atteint ensuite sa borne de 128 connexions
avant le lancement positif de Bob : l'essai complet est en échec.
La correction de scénario retire uniquement les deux consultations Outcome
redondantes avec S8. Les confirmations, les reçus et tous les contrôles de
fraîcheur restent exécutés. La limite de 128 et les délais restent inchangés.
