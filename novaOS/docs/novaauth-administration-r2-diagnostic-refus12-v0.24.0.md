# NovaAuth R2 — Diagnostic du refus 12

22 novembre 2026. Branche codex/novaauth-administration-0240-phase1.
État examiné : 4637bf586839faabe7186ad22d3d529e1ce179e7.
Invité exécuté : be3707b90008365037cc02c31180bfa42a0d3a9e ;
collecteur exécuté : bf66f21. Analyse des sources exportées de cet invité et
des preuves existantes, sans nouvelle exécution, instrumentation ou correction.

## Conclusion et portée

Le premier échec visible est la sortie du **témoin nsh Bob, PID 11**, code 12,
après émission de son ticket et avant publication du processus de confirmation
d'Alice. Le témoin agrège plusieurs causes en REFUSED : les traces actuelles
ne permettent pas de choisir la branche exacte. Aucun refus REVOKED n'est
démontré. Une cause racine complète du retard n'est pas établie.

Le défaut de diagnostic est démontré : les branches génériques du témoin
perdent le statut original et l'étape. Le défaut de synchronisation du curseur
est corrigé ; il ne bloque plus ce passage.

La prémisse « confirmation non lancée avant expiration » doit être nuancée :
Alice présente le ticket de confirmation à 57 392 ms, avant son échéance de
bail à 74 596 ms. L'enfant 22 est effectivement créé puis retourne 3.
Sa création n'est pas horodatée précisément dans la trace. Le refus NSHLIVE1
à 77 640 ms est postérieur ; il ne prouve pas que le chargement initial a été
refusé pour expiration.

## Définition et composant émetteur

novaOS/abi/native.rs définit REFUSED=12, distinct de DENIED=5, WOULD_BLOCK=7,
TIMED_OUT=8, REVOKED=10 et CLOSED=11. Le nombre seul ne définit pas sa cause.
Le contrat IPC documente notamment 12 pour un don refusé faute d'ACCEPT_CAP,
mais on ne peut appliquer cette signification à tout usage de REFUSED.

Dans l'export QEMU, components/shell/main.rs dirige le service 24 vers
administration_bob::session(), puis transmet session().err().unwrap_or(0)
à rt::exit. La ligne PROC pid=11 ... sortie=12 est le statut de sortie de Bob,
pas une ligne de décision de NovaAuth ni un code diagnostique du superviseur.
Le PC de sortie commun ne désigne pas la branche ayant construit l'erreur.

Après « ticket vivant non consomme », les chemins pertinents sont :

| Chemin | Statut transformé ou propagé | Nature |
| --- | --- | --- |
| Login::receipt : IPC_SEND NXQUERY1 | erreur native propagée, puis convertie en 12 sauf WOULD_BLOCK/REVOKED | transport, capacité ou protocole à départager |
| Login::receipt : IPC_RECEIVE(key,250) | erreur native propagée, puis même conversion | transport/attente |
| Réponse tag/kind/taille/magic incorrects | REFUSED construit dans receipt, puis 12 | protocole |
| Reçu renvoyant autre chose que WOULD_BLOCK ou REVOKED, y compris Ok | branche joker transforme en 12 | état du ticket ou invariant du témoin |
| receive_for(config(1),5) autre que TIMED_OUT | transformé en 12, message ou erreur d'origine perdus | synchronisation/contrôle |
| 300 itérations sans REVOKED | REFUSED construit localement | observation bornée non satisfaite |
| SPAWN ne renvoie pas REVOKED | REFUSED construit localement | résultat de consommation inattendu |
| DEBUG_WRITE des marqueurs, IPC_CLOSE ou login.close | propagation d'une erreur éventuelle, dont 12 | fermeture/diagnostic ; non localisable par le seul code de sortie |

Avant ce marqueur, les refus de forme/cible du déclenchement donnent DENIED ;
authenticate et ticket propagent leurs erreurs. Leur réussite précède le
marqueur observé : ils n'expliquent pas une sortie ultérieure comme échec initial.
Les chemins après SPAWN supposent d'avoir observé un reçu REVOKED ; aucun
marqueur ni NAEXEC attente owner=11 ne les étaye. L'absence d'un DEBUG seule
n'est toutefois pas une preuve absolue, les traces ayant un quota.

La lecture du reçu est locale : traffic.rs/exec_port construit NXREPLY1 depuis
exec::outcome. Elle n'appelle pas l'autorité indépendante. outcome vérifie
l'identité locale et renvoie WOULD_BLOCK pour Ready/Claimed, le statut enregistré
pour Failed et l'enfant pour Published. Elle **n'appelle pas validate** :
l'expiration naturelle du ticket ne devient pas automatiquement TIMED_OUT
dans cette consultation. La vérification d'expiration est dans la consommation.
Il est donc incorrect d'assimiler cette sortie 12 à un ticket expiré ou révoqué.

La boucle ne garantit pas une durée totale de six secondes : 300 attentes
de cinq ticks représentent six secondes d'attente minimale cumulée, auxquelles
s'ajoutent les consultations, leur délai maximal de 250 ticks et l'ordonnancement.
Le commentaire « au plus 6 s » est trop fort. Aucun délai n'est modifié ici.

## Identités et corrélation

Les identités complètes figurent dans chronologie.json et commandes-resultats.json :
Alice PID 5 et Bob PID 11 ont des instances et sessions distinctes.
L'opération préparée est 7962aabb67a457274ab286de7c4bb5c8.
Son déclenchement est retrouvé par le collecteur pour Bob ; la cible est
contrôlée dans l'invité après authentification. Alice prépare avec l'enfant 21,
puis tente la confirmation avec l'enfant 22. Bob ne lance aucun enfant visible.

La référence numérique et l'échéance de **son** ticket ne sont pas imprimées
à son émission. Les deux références NAEXEC attente owner=5 appartiennent à
Alice ; ne pas les attribuer à Bob. L'isolation complète des capacités n'est
pas démontrée à nouveau par ce seul parcours.

## Chronologie monotone invitée

NXAUTH est décodé selon auth_trace_profile.py ; les présentations noyau sont
en ticks à HZ=250, donc converties par multiplication par 4. Ces unités viennent
du temps invité ; aucune soustraction avec une horloge hôte.

| Événement | Temps invité en ms | Portée |
| --- | ---: | --- |
| Défi Alice / expiration défi | 12 448 / 13 948 | signature à 12 448 |
| Vérification Alice / fin ouverture | 13 512 / 15 660 | succès 0 |
| Échéance bail Alice | 74 596 | bail de 60 s, début déduit 14 596 |
| Ticket préparation Alice présenté | 25 372 | expiration ticket 29 308 |
| Préparation enfant 21 / déclenchement Bob | non horodatés | ordre série établi, préparation succès 0 |
| Défi Bob / expiration défi | 33 424 / 34 924 | signature à 33 428 |
| Vérification Bob / fin ouverture | 34 480 / 36 556 | succès 0 |
| Échéance bail Bob | 95 508 | début déduit 35 508 |
| Ticket Bob émis | non horodaté | marqueur, aucune consommation |
| Ticket confirmation Alice présenté | 57 392 | expiration ticket 61 320 ; marge bail 17 204 ms |
| Bob sort 12, puis enfant 22 publié et sort 3 | non horodatés | ordre série établi |
| NSHLIVE1 Alice refusé | 77 640, fin 77 644 | DENIED, dépassement bail 3 044 ms |

La fin d'ouverture Bob et la présentation du ticket de confirmation Alice
sont séparées de 20 836 ms. Ce total n'isole ni chargement, ni admission distante,
ni coût des sondes. Les événements non horodatés ne doivent pas recevoir des
instants interpolés. La trace complète permet de vérifier l'ordre des lignes.

## Pourquoi la confirmation arrive trop tard pour Bob

Le banc attend la préparation d'Alice, l'authentification de Bob et leurs routes,
puis écrit la ligne de confirmation et envoie LF au marqueur « émission ticket ».
Serial.send ne contient aucun sleep fixe. Cependant, après LF, nsh doit encore
résoudre, charger et faire admettre un **nouvel ELF usermod**. Celui-ci refait
une préparation idempotente pour valider request/digest avant d'envoyer CONFIRM.
La révocation n'est donc pas déclenchée au moment où la ligne est envoyée.

Le processus de confirmation n'est publié qu'après la sortie de Bob.
L'ordonnancement actuel n'établit aucune barrière « effecteur prêt avant
émission du ticket ». C'est une faiblesse démontrée du protocole de test ;
son coût exact et la branche 12 causale restent à mesurer.
Le refus NSHLIVE1 tardif d'Alice est certain, mais n'explique pas rétroactivement
la sortie 12 de Bob ni le retour 3 de l'enfant 22. Aucun résultat persistant
RETIREE n'est prouvé. Ne pas attribuer ce retard automatiquement à l'hôte.

## Hypothèses ouvertes et instrumentation soumise à arbitrage

Sans nouveau test, on ne peut départager :
1. observation terminée sans révocation ;
2. erreur/réponse inattendue de receipt ;
3. message/erreur inattendu sur config(1) ;
4. branche ultérieure insuffisamment tracée.

Instrumentation proposée, non implémentée : une seule ligne terminale bornée
par branche d'échec Bob, avec étape, indice d'itération, statut natif original,
nature de réponse et temps monotone. À l'émission, consigner la référence locale
du ticket et son échéance en ticks depuis la métadonnée noyau de qualification.
Relier ces données aux PID/instances/sessions déjà collectés, sans clés,
signatures, contenu privé ni nouvelle capacité de lecture de l'horloge.

Tracer aussi, aux points QEMU déjà autorisés, le début/fin de chargement et
d'admission de l'enfant de confirmation, sa publication et l'entrée CONFIRM.
Le banc hôte peut mesurer séparément l'envoi LF et les arrêts du collecteur,
mais ces durées ne doivent pas être comparées directement aux baux invités.
Aucune journalisation par itération : éviter de modifier le comportement temporel.

Correction proposée sous réserve du diagnostic : prépositionner le véritable
ELF de confirmation dans la session authentifiée d'Alice avant le ticket Bob,
avec un rendez-vous explicite de qualification. Tout contrôle de capacité,
de session et de fraîcheur doit être refait à la mutation ; aucun droit conservé
ne vaut autorisation après révocation. Ce changement éventuel dépasse la seule
correction du collecteur et doit être arbitré. Une simple augmentation des délais,
un renouvellement silencieux ou accepter 12 comme 10 sont exclus.
Si une branche de protocole est fautive, corriger seulement cette cause.

## Preuves et gouvernance

39 fichiers de la campagne précédente relus et comparés à leur manifeste.
Analyse structurée et vérification :
[chronologie](validation/novaauth-r2-refus12-20261122/chronologie.json),
[manifestes](validation/novaauth-r2-refus12-20261122/SHA256SUMS).
Archive durable :
OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/diagnostic-refus12-4637bf5.
SHA-256 du manifeste de diagnostic :
58ee1e6f19e561f0f2a8f4bb1a34bc1068af1d10e1afa3dfb3972a71652638d9.

Aucun QEMU, test de sécurité supplémentaire, changement fonctionnel ou
modification des attentes. Pas de verrou QEMU acquis, pas de campagne concurrente.
Budget de 20 minutes respecté ; durée de diagnostic non instrumentée à la seconde.
Les réserves historiques et les travaux sont préservés. Publication documentaire
R2 uniquement. Aucun tag, aucune fusion ; utilitaires en pause.
Nouvel essai soumis à arbitrage MOA/MOE.
