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

## Bilan de reprise contrôlée

La qualification globale de sécurité R2 reste **partielle**. Aucun blocage Daybreak
n’a été rencontré par les outils de cette reprise ; le message rapporté par
la MOA/MOE ne vaut ni résultat de test ni autorisation de contournement. Après
ce nouveau mandat, aucune nouvelle campagne QEMU n’a été lancée. L’essai déjà
en cours a terminé et ses preuves ont été conservées. Une lecture `ps` hors
escalade a été refusée par le sandbox ; elle n’a pas été répétée. Les lanceurs
QEMU autorisés attestent la fin des descendants et la libération de leur verrou.

Q = qualifié dans le périmètre décrit, P = partiel, N = non exécuté.

| Cas | x86/512 | x86/4096 | ARM/512 | ARM/4096 | Portée et reste à faire |
|---|---|---|---|---|---|
| S1 Auto-attribution | Q | Q | Q | Q | Ajout au groupe administratif refusé 5 ; Confirm et Outcome directs refusés ; Bob non administrateur ; triplet identique. |
| S2 Registre persistant | Q | Q | Q | Q | APPEND tables 0/1/2 : refus 3 sur canal administratif et 5 sans handle ; aucun port de stockage attribué ; triplet relu identique. |
| S3 Références exactes d’Alice | P | P | P | P | Contexte forgé/META refusés ; rejeu de références exactes d’une session Alice vivante restant à exécuter. |
| S4 Requête administrative directe | Q | Q | Q | Q | Prepare/Confirm/Outcome envoyés directement au serveur ; refus 5. Contrôle positif Alice réussi. |
| S5 Substitutions | P | P | P | P | Cibles Alice/Bob et NXACTX01/rejeu refusés ; substitution de la session réelle d’Alice restant à exercer. |
| S6 Tickets expirés/révoqués/consommés | P | P | P | P | Révocation avant expiration : témoin accepté fc1ed9b. Expiration et consommation à rattacher par preuve applicable ou exécution R2. |
| S7 Concurrence/dernier administrateur | P | P | P | P | Tests hôtes et refus séquentiel invité historiques ; concurrence administrative effective invitée non exécutée dans cette reprise. |
| S8 UNCERTAIN | P | P | P | P | Quatre preuves historiques de reçu perdu vérifiées par manifeste ; revue complète d’applicabilité à la réactivation nouvelle encore à terminer. |
| S9 Désactivation/réactivation | Q | N | N | N | x86_64/512 réussi : nouvelle session, anciens ticket/session refusés, nouveau lancement ; autres profils non exécutés. |

### Exécutions nouvelles et ressources

| Profil / parcours | Source / banc | Résultat | TLS initial ms | Connexions | Exécution hôte s |
|---|---|---|---:|---:|---:|
| aarch64/4096 refus | f6399d1 / f6399d1 | réussi | 2692 | 74 | 23.419 |
| aarch64/512 refus | f6399d1 / f6399d1 | réussi | 3352 | 74 | 30.556 |
| x86_64/4096 refus | f6399d1 / f6399d1 | réussi | 2852 | 74 | 16.538 |
| x86_64/512 S9 | 71bba98 / 71bba98 | échec conservé | 2820 | 128 | 90.875 |
| x86_64/512 S9 | 71bba98 / 19e5a05 | réussi | 3104 | 127 | 50.871 |
| x86_64/512 refus | f6399d1 / f6399d1 | réussi | 2732 | 74 | 16.431 |

Les quatre parcours négatifs ont publié zéro mutation administrative : tête 23,
triplet et données privées inchangés après relecture P9. Les nouveaux droits
ne sont jamais inférés du seul statut de la CLI. Refus natifs 5 (capacité ou
opération interdite), 3 (protocole invalide), 10 (révoqué) distingués de 12.
Les requêtes rejetées peuvent avoir des traces légitimes ; le volume complet
n'est pas exigé identique car la récupération initiale écrit légitimement P9.

S9 sur x86/512 : sources `71bba986ac39258de35eb3b91db79f35420ed50d`,
banc `19e5a055fd4dd90e4b6c30bfe236f5dd6b896b56`. Après correction du scénario,
les générations 24 puis 25 sont confirmées, exactement deux mutations avec
signatures vérifiées. Après réactivation, seuls les champs `revision` et
`authentication` de Bob changent ; groupes, facteurs, droits et autres comptes
restent identiques. Nouvelle session dans la même instance, ancien ticket 10,
ancienne session 5 ; nouveau PID autorisé et statut 0. Le premier refus du
ticket précède son expiration de 3 372 ms. Le second refus après nouvelle
session vérifie la non-réactivation, sans être présenté comme un second témoin
avant expiration. Les remises 2-sur-2 ne sont pas confondues avec EXEC_TICKET.

Sur les cinq essais réussis : quatre racines maximum, cinquième refusée,
fermeture des deux sessions et drainage IPC global vérifiés. Les compteurs
signataire et registre reviennent aux valeurs initiales. Les tableaux exacts
avant/après figurent dans `matrice-resultats.json`. Les heures d'acquisition et
de libération du verrou sont conservées pour chacun des six essais.

### Non-régressions ordinaires et applicabilité

18 tests du client administratif, 8 tests du moteur, 20 tests de l'autorité et
9 contrôles ABI réussissent ; index ABI cohérent. Les quatre ELF historiques
n'ont pas été relancés dans cette reprise. Aucun fichier du noyau ordinaire,
de l'ABI ou du moteur NovaFS n'a changé depuis fc1ed9b. Les anciens NXEXEC01 et
NXADMIN1 restent inchangés. La sonde est un ELF de qualification signé distinct.

Les résultats négatifs f6399d1 restent applicables aux chemins testés : les
changements fonctionnels suivants ajoutent uniquement la consultation du reçu
et le traitement de Reactivate après l'autorisation administrative. Le refus
précoce de Bob, les lectures, le décodage et le confinement IPC restent les mêmes.
Cela ne qualifie pas par extrapolation les nouveaux chemins de réactivation.

Le manifeste historique de commandes/mutations contient 1 362 fichiers tous
revérifiés, SHA-256 `a72990ab95462664af21a69e3c50801c21502ccd8ccdad2981c68499575d8170`.
Les preuves consolidées fc1ed9b restent conservées ; aucun témoin accepté n'a
été relancé pour le seul besoin de compter davantage de succès.

### Préconditions des travaux restants

- S3/S5 : définir un témoin de présentation des références exactes d'Alice sans
  déléguer ses capacités ni donner un accès aux sorties de l'autre session.
- S6 : établir l'applicabilité des anciennes preuves d'expiration/consommation,
  sinon compléter les seules variantes manquantes dans l'invité.
- S7 : coordonner deux requêtes administratives effectivement concurrentes,
  conserver leur ordre d'admission et vérifier le dernier administrateur dans
  la transaction faisant autorité ; ne pas compter les seuls tests hôtes.
- S8 : compléter la comparaison des sources des quatre essais de reçu perdu
  avant de les déclarer pleinement applicables au nouveau candidat.
- S9 : qualifier les trois autres profils et la marge du relais (127/128 sur
  le succès x86/512), sans relever arbitrairement une borne ou un délai.

Les suppressions définitives restent absentes. Les utilitaires restent en pause.
Réserves conservées : 301 cadres instrumentés AArch64, reproductibilité entre
environnements, aucune qualification de la distribution ordinaire ou de production.

## Archives et temps

310 fichiers vérifiés dans `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/securite-fc1ed9b-20261122`.
Manifeste SHA-256 : `638396286251ae1c210cecc0f0fa1f8c21b7b85e83d46a03aa438d4207e0f349`.
Archive : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/securite-fc1ed9b-20261122.tar.gz` ; SHA-256 `a61acbbde07257befc4199154587648bd4aa04642e4ce1d421df04d16f4afa15`.

Attente cumulée du verrou : 0.002 s ; exécution QEMU : 228.69 s.
Développement, diagnostic, contrôles hôtes et consolidation jusqu’au scellement : 1736.47 s.
Aucune durée hôte n’est soustraite d’une échéance invitée. Aucun conflit de
planification n’a imposé de campagne concurrente. Les acquisitions/libérations
sont détaillées dans les fichiers de coordination et la matrice JSON.
