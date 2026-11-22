# NovaAuth R2 — Conservation des événements anticipés

Base 3342217e3156df6261262f1a8474ede9d0007c59.
Correctif hôte bf66f21d5db0a429a3a4ced63f74219d736a1ec8.
Invité inchangé : be3707b90008365037cc02c31180bfa42a0d3a9e.

## Correction et tests
Events.before_ready fixe sa frontière avant de consommer « registre prêt ».
Le Serial est créé pour cet unique démarrage. Aucun événement antérieur à
cette frontière n'est récupéré. PID unique, étape unique, doublons et absence
restent contrôlés. L'opération préparée est corrélée à l'annonce de Bob ;
les routes collectées associent PID, instance et session distincts.
Les événements déjà reçus sont annotés avec cette liaison ; l'événement
d'émission suivant reste relié par son PID à sessions_temoin dans le rapport.

47 tests hôtes réussis en 2,707 s. Le nouveau test reproduit l'annonce Bob
avant registre prêt, avec une ancienne annonce de même PID exclue par la
frontière ; double consommation et absence sont refusées.
Aucune compilation ni modification invitée. ISO et noyau identiques à
l'essai précédent ; profil novaauth-sessions vérifié avant lancement.

## Unique essai x86_64/P9-4096
Échec, sans répétition. Le défaut du curseur ne se reproduit plus.
Alice PID 5 et Bob PID 11 sont authentifiés dans deux instances et sessions
distinctes, consignées dans commandes-resultats.json.
Préparation Alice : PID 21, retour 0, 134 octets stdout conformes, stderr vide.
Bob annonce la réception du déclenchement, son authentification et son ticket
vivant non consommé.

Premier échec invité observable : Bob PID 11 sort avec REFUSED (12), avant
la création de l'enfant de confirmation Alice PID 22. Aucun marqueur
« recu local revoque » ni appel de consommation Bob n'est observé.
Le code peut produire 12 après l'observation bornée ou sur un résultat
inattendu du reçu. Les traces présentes ne distinguent pas ces branches :
ne pas attribuer avec certitude cette sortie à une expiration.

Ensuite PID 22 retourne 3. Le contrôle NSHLIVE1 d'Alice décide à 77 640 ms
contre une échéance à 74 596 ms : refus 5 après expiration, puis statut shell
126. Ces deux valeurs sont monotones invitées, sans mélange avec l'horloge hôte.
Le banc s'arrête sur ce statut non conforme. Aucune nouvelle tentative.

## Matrice
- Annonce anticipée conservée : démontré.
- Préparation Alice et deux routes authentifiées distinctes : démontré.
- Émission du ticket Bob sans consommation : marqueur invité présent.
- REVOKED avant expiration, reçu RETIREE : non démontrés.
- Aucun enfant Bob visible dans la trace : observé, insuffisant pour qualifier
  la révocation car aucune consommation révoquée n'a été exercée.
- Restitution finale du parcours : non qualifiée.
La présence de deux sessions ne constitue pas à elle seule une qualification
complète de leur isolation.

## Durées et preuves
Verrou acquis 2026-11-22T15:11:28.612708+00:00, libéré 2026-11-22T15:12:49.019100+00:00 UTC.
Attente 0.000360 s ; exécution 80.362 s.
Tests hôtes 2,707 s. Intervention sous 20 minutes ; durée précise de diagnostic
non instrumentée séparément, donc non inventée.

Archive /Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/collecteur-3342217
39 fichiers vérifiés SHA-256. Empreinte manifeste :
06cfe2a67696166adf85095b6bf4d9a63d50b9e1f6351d8d3779986e40fb476d.
Artefacts et états privés préservés hors Git.
Réserves historiques inchangées. Aucun changement NovaAuth, bail ou ABI.
Aucun autre profil, aucune fusion, aucun tag ; utilitaires en pause.
