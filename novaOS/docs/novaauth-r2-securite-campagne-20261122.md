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
