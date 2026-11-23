# NovaAuth v0.24.0 — Proposition de réutilisation mémoire pour EXEC sélectif

État du 23 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Référence documentaire précédente `01a38bb66c4dcd62c961ecc9eebd5196463f5d70`.
La réduction de représentation a été approuvée par la MOA/MOE puis appliquée
au commit `e0b246cee517067670fc6d17f7893e2e6269578f`. Le refus automatique
antérieur est conservé comme historique ; il ne constitue plus un blocage.
Le raccord des tickets EXEC réellement dépendants de G reste distinct et incomplet.

## Budget et invariants

Avant optimisation, le raccord VFS Q2 occupait 4 082 octets mesurés.
La représentation appliquée occupe 3 994 octets sur les deux architectures. Le champ `Ticket.scope`
représente les autorisations de fichiers : il ne peut pas devenir un identifiant
de groupe. Une provenance supplémentaire doit garder le groupe et sa révision,
avec liaison aux champs existants du ticket : titulaire, session, domaine,
installation attestée et incarnation. Sa représentation et son nombre maximal
simultané devront être mesurés avant intégration ; aucune hausse de borne admise.

## Réduction proposée des seules représentations

| Structure actuelle | Octets utiles | Réservés vérifiés nuls | Représentation proposée |
|---|---|---|---|
| NXMGDAT1, 128 octets | 0..74 exclusif | 74..128 | stockage de 80 octets, reconstruction avec zéros après 80 |
| NXMGOUT1, 128 octets | 0..52 exclusif | 52..128 produits nuls | même buffer de 80 octets après exécution |
| NXMGCTX2, 128 octets | 0..120 exclusif | 120..128 | stockage de 120 octets |
| NXMGIOA1, 128 octets | 0..120 exclusif | 120..128 produits nuls | même buffer de 120 octets après exécution |

Les messages transmis restent exactement de 128 octets. Les champs d'identité,
session, objet, origine, révision, empreinte et fait ne sont jamais tronqués.
Le refus des réservés non nuls précède tout stockage compact. Les tests devront
comparer les 128 octets reconstruits, les tailles limites, les réservés invalides,
la saturation, la corrélation et les réponses perdues.

Dans `Selective`, les classes 2 et 4 possèdent actuellement deux copies de
`identity(27)` (le vérificateur), tandis que la classe 1 possède `identity(23)`
(NovaFS). Proposition : conserver deux identités physiques et trois entrées
logiques, trois classes, trois requêtes EFFECT et trois bits d'acquittement.
L'instance des classes 2 et 4 est la même dans le raccord actuel ; leurs preuves
ne deviennent pas interchangeables. La classe 4 doit toujours vérifier EXEC.
Un changement ultérieur vers deux instances différentes doit être refusé ou
faire modifier le budget, jamais être déduit de ce stockage partagé.

Gain constaté avec les types réels : 56 octets pour Exchange et
32 pour Selective, soit 88 octets. Exchange mesure 224 octets,
Option<Selective> 304, le reçu conservé 224 et Io 114. Les assertions sont
compilées dans les deux invités, en complément des mesures natives. La marge
est de 102 octets ; aucune autorisation de dépasser 4 096 octets. Le budget devra inclure
chaque ticket, admission et éventuel lecteur de pipe pouvant coexister.

## Validation ciblée approuvée

La MOA/MOE autorise cette réduction de représentation dans le seul raccord privé QEMU,
sous réserve de tests natifs ARM64 et x86_64, mesure automatique <= 4 096,
comparaison exacte des encodages, conservation des trois acquittements logiques
et qualification des chemins affectés. Aucun sélecteur, capacité, délai ou
format P9 ne change. Les contrôles historiques restent conservés.

## Autre limite distincte : reprise froide incomplète

`membership_retirement.guard_activation` refuse une nouvelle incarnation tant
qu'un retrait réservé/en cours n'a pas son issue démontrée. Le contrat approuvé
interdit qu'un effecteur redémarré acquitte au nom de son ancienne instance.
Lire une réservation signée ne prouve donc pas l'annulation d'une E/S ancienne.
Aucune nouvelle procédure de neutralisation n'est appliquée dans ce travail.
La reprise froide après RETIREE déjà qualifiée reste distincte de ce cas.

## Résultats et portée

Voir le [rapport de mémoire et drainage](novaauth-r2-memoire-drainage-v0.24.0.md).
Les trois classes 1, 2 et 4 gardent trois demandes et preuves indépendantes ;
les classes 2 et 4 partagent seulement la même instance vérifiée du vérificateur.
Le prototype de provenance EXEC supplémentaire est conservé hors du code publié :
sa taille par entrée ne démontre ni la cardinalité globale ni une provenance
effectivement utilisée pour autoriser un lancement G.
