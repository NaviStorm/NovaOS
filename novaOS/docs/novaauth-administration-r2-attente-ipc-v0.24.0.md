# NovaAuth R2 — Attente IPC bornée

Base f984908e754dc807cd0e5210cc1368f2a7b5ca68.
Correctif et sources invitées : be3707b90008365037cc02c31180bfa42a0d3a9e.

## Correction
L'appel généré par administration_live_ticket_profile.py, INITIAL, attendait
sur config(17) avec 25 000 ticks, refusés par IPC_RECEIVE (> 15 000).
Il demande désormais 15 000 ticks. Une seule attente, sans boucle ni succès
sur timeout : l'erreur est propagée par ?. Aucune durée de bail modifiée.
Les routes, instances, sessions et contrôles de cible restent distincts.
Leur isolation complète n'est pas requalifiée par ce témoin interrompu.

45 tests hôtes réussis en 2,732 s, dont quatre contrôles de borne du raccord :
valeur admise, ancien défaut 25 000, dépassement 15 001 et expression dynamique
nécessitant une revue. Ce contrôle porte sur les attentes explicites de ce
générateur, pas sur tous les appels IPC du système. Construction réussie en
22,782 s. Profil ISO novaauth-sessions et noyau embarqué vérifiés avant essai.

## Unique essai x86_64/P9-4096
Échec du banc, sans répétition. L'attente ne produit plus BAD_ARGUMENT.
Bob reçoit le déclenchement. Alice exécute usermod --disable bob --prepare :
PID 21, statut natif 0 et marqueur NXR2_0_0__FIN.

Première cause bloquante : « acteur de l’événement absent ou ambigu ».
Dans la trace conservée, l'annonce DEBUG pid=11 NAR2 Bob attente declenchement
authentification se trouve à l'octet 10 051, avant « NAEXEC registre pret puis
connexion du vrai nsh » à l'octet 10 881. Le banc crée Events après ce second
marqueur ; Events.actor ne cherche qu'après son curseur initial. Il exclut donc
l'événement déjà arrivé. Ce n'est ni une expiration du bail ni un refus IPC.
Aucune correction supplémentaire du banc n'a été appliquée après cet échec.

Ticket valide non consommé, révocation, REVOKED avant expiration, absence
d'enfant interdit et reçu RETIREE : NON VÉRIFIÉS. Drainage final R2 non qualifié.
Les ressources du signataire et du registre relevées avant/après sont stables,
les pics observés restent à quatre racines maximum ; ne pas extrapoler au
scénario non terminé. Les changements P9 de récupération sont conservés,
sans prétendre que le volume est resté identique.

## Coordination et preuves
Acquisition 2026-11-22T15:07:29.253912+00:00, libération 2026-11-22T15:08:09.533344+00:00 (UTC).
Attente 0.000340 s ; exécution 40.234 s.
Intervention sous 20 minutes (environ cinq minutes jusqu'à publication).
Le chronométrage fin du développement n'a pas été initialisé au premier appel ;
ne pas présenter une durée précise inventée. Mesures hôtes séparées des baux.

Archive : /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/attente-ipc-f984908
40 preuves vérifiées SHA-256 ; manifeste
28f419b3555c29152a496905e37a96319c7cdf5e98d9b8546a77762d3911e3e0.
Artefacts privés préservés hors Git dans artefacts-prives-administration-0240-r2/attente-ipc-f984908.
Les avertissements de toolchain restent dans le journal de construction.
Réserves historiques conservées. Aucun AArch64, autre scénario, fusion, tag
ou activation ordinaire. Utilitaires en pause. Verrou libéré.
