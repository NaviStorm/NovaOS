# NovaAuth v0.24.0 — Qualification proposée et arbitrages

Aucun test d’implémentation exécuté dans ce mandat documentaire. « Prévu » n’est ni un succès ni une qualification. Base `5c7f1bfa313a3859ccc0830660438e76d598acb1`. Voir l’[audit](novaauth-administration-audit-v0.24.0.md) et le [contrat](novaauth-administration-contrat-v0.24.0.md).

## Préparation et preuves

Banc futur : un administrateur initial C, Alice et Bob à identités stables distinctes, deux sessions Alice/Bob, groupe G, installation attestée commune, dossier privé et dossier toujours autorisé. Données exclusivement synthétiques : privé `PRIVE-ALICE\n`, autorisé `LECTURE-OK\n` (LF réel dans les fichiers). Conserver la référence binaire et son SHA-256. L’admin n’acquiert aucun accès aux dossiers du seul fait de son rôle.

Chaque essai indépendant part d’une copie neuve du volume P9 et de son état d’autorité de laboratoire ; les redémarrages d’un même parcours gardent les copies de ce parcours. Pour l’anti-rollback, ne restaurer que les disques locaux : l’autorité et les détenteurs sont hors du périmètre. Les deux VM d’une course partagent une seule autorité durable, pas deux copies. Les sessions après reprise doivent être effectivement authentifiées dans l’invité.

Archiver commit/arbre, profils de compilation, SHA ISO/ELF/P9 initial et sauvegardes, version/arguments QEMU, décisions VFS invitées, octets de sortie autorisés, stderr, statuts CLI et natifs séparés, PID/instance/session pseudonymes, identifiants de requêtes et positions durables, horodatages monotones avec unités, limites de bail, compteurs de ressources et causes de refus. Pour un refus, vérifier zéro octet de contenu privé, pas seulement un message de permission. Aucun secret réel ni part de récupération dans les traces publiques ; artefacts exacts privés conservés dans une archive locale protégée.

Le futur runner doit acquérir `.qualification-utilitaires-exclusive.lock` sous OS-worktrees, vérifier les campagnes incompatibles et libérer uniquement son propre verrou en finally. Ne jamais forcer un verrou occupé. Mesurer séparément attente, banc et diagnostic ; les campagnes des utilitaires restent interdites pendant leur pause. Une future qualification NovaAuth, autorisée séparément, pourra acquérir le verrou sans attendre la reprise des utilitaires, après vérification des autres campagnes incompatibles. Le verrou ne vaut jamais autorisation de reprendre un chantier. Aucun verrou QEMU acquis ici, aucune VM lancée.

## Matrice à exécuter après validation

| Recette | AArch64/512 | AArch64/4096 | x86_64/512 | x86_64/4096 |
|---|---|---|---|---|
| A01 — Consultations et langues | Prévu | Prévu | Prévu | Prévu |
| A02 — Analyse et modes nsh | Prévu | Prévu | Prévu | Prévu |
| A03 — Identité administrative | Prévu | Prévu | Prévu | Prévu |
| A04 — Création et persistance | Prévu | Prévu | Prévu | Prévu |
| A05 — Renommage | Prévu | Prévu | Prévu | Prévu |
| A06 — Désactivation | Prévu | Prévu | Prévu | Prévu |
| A07 — Suppression et nom réutilisé | Prévu | Prévu | Prévu | Prévu |
| A08 — Données après suppression | Prévu | Prévu | Prévu | Prévu |
| A09 — Dernier admin existant | Prévu | Prévu | Prévu | Prévu |
| A10 — Dernier admin futur et concurrence | Prévu | Prévu | Prévu | Prévu |
| A11 — Récupération indépendante | Prévu | Prévu | Prévu | Prévu |
| A12 — Appartenances et origines | Prévu | Prévu | Prévu | Prévu |
| A13 — Persistance du retrait | Prévu | Prévu | Prévu | Prévu |
| A14 — Même installation deux utilisateurs | Prévu | Prévu | Prévu | Prévu |
| A15 — Suppression de groupe proposée | Prévu | Prévu | Prévu | Prévu |
| A16 — Surprotection et objet substitué | Prévu | Prévu | Prévu | Prévu |
| A17 — Concurrence des mutations | Prévu | Prévu | Prévu | Prévu |
| A18 — Confirmation | Prévu | Prévu | Prévu | Prévu |
| A19 — Publication interrompue | Prévu | Prévu | Prévu | Prévu |
| A20 — UNCERTAIN et réponses perdues | Prévu | Prévu | Prévu | Prévu |
| A21 — Ordre distant et drainage | Prévu | Prévu | Prévu | Prévu |
| A22 — Admission acquittée, création retardée | Prévu | Prévu | Prévu | Prévu |
| A23 — VFS pendant écriture et fermeture | Prévu | Prévu | Prévu | Prévu |
| A24 — Restauration complète et VFS | Prévu | Prévu | Prévu | Prévu |
| A25 — Reprise 2-sur-2 et disponibilité | Prévu | Prévu | Prévu | Prévu |
| A26 — Anciens tickets et sessions | Prévu | Prévu | Prévu | Prévu |
| A27 — Quotas et restitution | Prévu | Prévu | Prévu | Prévu |
| A28 — Audit | Prévu | Prévu | Prévu | Prévu |
| A29 — Bornes et corruption | Prévu | Prévu | Prévu | Prévu |
| A30 — Compatibilité | Prévu | Prévu | Prévu | Prévu |

A10 et A15 nécessitent d’abord un contrat et une interface aujourd’hui absents. A01 reste conditionnée à l’arbitrage linguistique. Pour les fonctionnalités non disponibles, marquer « bloqué/non implémenté », jamais « réussi » ni un témoin hôte substitué à l’invité.

### A01 — Consultations et langues

help/version/man sans authentification ni capacité métier ; fr et autres locales selon arbitrage ; stdout/stderr et code 0 ; aucune mutation, tmp ou STOP.

### A02 — Analyse et modes nsh

Option inconnue, doublon, argument vide, nom invalide, -- ; même résultat interactif/script/nsh -c ; false=1, false --help=0, exit --help poursuit la session et exit ferme normalement.

### A03 — Identité administrative

Admin authentifié admis ; non-admin, faux nom admin, session substituée/expirée ou preuve de facteur déclarée refusés. Aucune promotion implicite.

### A04 — Création et persistance

Créer Alice/Bob non-admin avec facteurs attestés distincts ; redémarrer ; IDs, comptes et groupes présents ; aucune donnée privée créée/lue par défaut.

### A05 — Renommage

Même ID et mêmes propriétaires ; l’ancien nom ne redirige pas une confirmation déjà préparée vers un nouveau compte.

### A06 — Désactivation

Refus nouvelles sessions/admissions ; délégations retirées ; compte et données conservés ; aucun Enable implicite ; état partiel affiché tant que les effecteurs requis ne sont pas acquittés.

### A07 — Suppression et nom réutilisé

Tombe conservée, ancienne référence de facteur non réaffectée ; nouveau compte même nom mais nouvel ID ; aucun droit, groupe, cache, fichier ou tâche hérité.

### A08 — Données après suppression

SHA des fichiers synthétiques inchangés, propriétaire ancien ID, ACL/surprotection conservées ; refus au compte recréé et à l’admin sans délégation du propriétaire ; aucune purge automatique.

### A09 — Dernier admin existant

Depuis le premier administrateur, Disable/Delete du compte admin refusés sans mutation. Vérifier également les tentatives indirectes par nom, groupe ou paramètre client.

### A10 — Dernier admin futur et concurrence

Conditionné à un futur contrat multi-admin/facteurs : deux retraits simultanés ne laissent pas zéro admin utilisable ; refus du retrait du dernier facteur ; exception impossible sans procédure indépendante validée. Non exécutable sur l’API actuelle.

### A11 — Récupération indépendante

Ancienne génération, approbation absente/expirée/rejouée ou mauvais serveur refusés ; nouvelle incarnation approuvée ne vaut pas validation de l’ancien triplet ni autorité admin supplémentaire.

### A12 — Appartenances et origines

Lecture directe + écriture par G ; retirer Alice de G avec instance active ; lecture demeure, écriture et sous-délégations de groupe refusées après acquittement ; capacités indépendantes intactes.

### A13 — Persistance du retrait

Redémarrer et réauthentifier réellement Alice : lecture directe valide, écriture retirée refusée ; un simple contrôle du registre ne remplace pas l’authentification et le VFS invité.

### A14 — Même installation deux utilisateurs

Droit persistant d’Alice sur installation attestée commune ; Alice lit, Bob refusé ; aucune propagation implicite par le nom/chemin/ID de l’installation.

### A15 — Suppression de groupe proposée

Avant implémentation de DeleteGroup, commande non distribuée ; ensuite groupe vide sans références uniquement, refus des membres/règles/admissions en vol et non-réutilisation de son ID.

### A16 — Surprotection et objet substitué

Droit admin ne donne pas lecture de dossier ; faux OID, chemin changeant de cible, handle d’un autre propriétaire, ACL ou surprotection incompatible : refus.

### A17 — Concurrence des mutations

Deux créations depuis la même tête : une réussite et un conflit explicite ou sérialisation contrôlée, jamais perte du premier résultat ; seconde soumission éventuelle explicite après relecture et nouvelle confirmation.

### A18 — Confirmation

Pas de mutation en préparation ; empreinte liée à cible/arguments/tête/session ; refus d’une confirmation périmée, substituée ou réutilisée par autre session. Script sans confirmation explicite refusé.

### A19 — Publication interrompue

Injecter avant décision et après décision durable avant réponse ; triplet complet visible ou refus sécurisé, jamais mélange ; création confirmée présente au reboot.

### A20 — UNCERTAIN et réponses perdues

Perdre l’acquittement de mutation, query par même ID ; aucune seconde mutation ; réemploi ID avec contenu différent refusé ; None/timeout non interprété comme annulation.

### A21 — Ordre distant et drainage

Enveloppe signée avant fermeture locale puis retardée ; ordonner admission/retrait durable ; EN_COURS maintenu tant qu’une issue admise manque ; aucune activation B avant RETIREE.

### A22 — Admission acquittée, création retardée

Deux VM, autorité unique : A vivante non expirée ; création retardée après admission ; B bloquée ; enfant identifié/collecté ou annulation irréversible démontrée ; aucun nouveau PID A après RETIREE.

### A23 — VFS pendant écriture et fermeture

Écriture engagée lors de fermeture de session : réveil, retrait des délégations temporaires, résultat confirmé ou UNCERTAIN par ID ; pas de répétition ; test distinct du retrait de groupe.

### A24 — Restauration complète et VFS

Alice lit exactement le témoin avant instantané ; révocation durable ; restauration de tous disques locaux, autorité conservée ; refus effectif et zéro octet privé ; autorité injoignable : refus aussi.

### A25 — Reprise 2-sur-2 et disponibilité

Nouvelle incarnation approuvée, deux remises, triplet exact à la tête durable ; réauthentifier Alice, lire autorisé, refuser révoqué sans octet, puis nouvelle opération autorisée dans la même session.

### A26 — Anciens tickets et sessions

Ancien EXEC_TICKET non expiré depuis A vivante retirée refusé sans PID ; B ne peut le réutiliser ; nouveau ticket B fonctionne ; concurrence un seul enfant, reprise après reboot distincte.

### A27 — Quotas et restitution

Quatre racines max par acteur à chaque phase ; cinquième LIMIT sans allocation résiduelle ; saturation/échec allocation ; mesurer handles/endpoints/processus/threads/cadres avant-pendant-après ; plateau cache séparé de fuite.

### A28 — Audit

Identité de l’émetteur authentifiée, déduplication par ID, issue connue/incertaine ; saturation/panne/reprise ; accès lecture séparé ; aucune clé, secret ou contenu privé dans traces.

### A29 — Bornes et corruption

Noms, slots 16 Kio et limites 64 du registre ; dépassement refusé avant décision ; triplet manquant/altéré et état de même génération mais empreinte différente refusés, sans nouvel enrôlement.

### A30 — Compatibilité

Neuf contrôles ABI, 48 sélecteurs, quatre ELF historiques inchangés, pipes cat|wc et cat|head avec statuts exacts, sessions/VFS/IPC ; ordinaire sans raccord labo. Compatibilité distincte de reproductibilité.

## Réutilisation des acquis

Le [rapport global](novaauth-qualification-globale-v0.24.0.md) et sa [matrice](validation/novaauth-qualification-globale-aa489ca.json) constituent la référence datée : 24 scénarios exécutés, huit réutilisés, quatre recettes VFS et récupération, contrôles ABI et ELF, mesures mémoire prolongées. Leur applicabilité doit être décidée par le diff fonctionnel et les conditions réelles, pas par le nom de la branche. Les nouvelles routes administratives exigent de nouveaux essais ; les anciennes preuves ne qualifient pas automatiquement A03–A29 via les nouvelles commandes.

Pour chaque ligne future : réalisé / preuve antérieure réutilisée avec justification / bloqué / non exécuté. Garder les négatifs, les essais intermittents et les attentes initiales. Au premier échec, conserver les traces et diagnostiquer avant rejeu ciblé. Fixer le budget et les répétitions avec le mandat d’implémentation ; aucune augmentation des délais TLS, baux, plafonds ou attentes pour fabriquer un succès.

## Étapes de réalisation proposées, non engagées

| Étape | Périmètre et dépendances | Porte d’acceptation / risque |
|---|---|---|
| 0 | Arbitrer langue, vues/portées, confirmation, issue, audit, dernier admin et données ; démontrer budget IPC de la route | Contrats précis et absence d’élargissement implicite ; aucun développement avant validation. Risque élevé : frontière de confiance. |
| 1 | API de lecture authentifiée minimale puis whoami/id/groups ; réutiliser inspect/groups et ajouter seulement les vues manquantes | A01–A03, A16, A27, A30 ; aucun accès aux données/facteurs. Risque moyen : divulgation et substitution. |
| 2 | Préparation/confirmation/outcome, journal et mutations déjà disponibles : useradd, usermod, userdel, groupadd | A04–A09, A17–A20, A28–A29 ; dernier admin toujours protégé, suppression sans données. Risque élevé : durabilité, verrouillage administratif. |
| 3 | Permissions persistantes/effectives, notification des effecteurs, barrière de retrait et appartenance | A12–A14, A16, A21–A27, A30 ; statuts locaux/distants distincts et non-réactivation. Risque élevé : révocation incomplète. |
| 4 | Lacunes sous mandat séparé : groupdel, réactivation, gestion de facteurs et éventuel passwd, multi-admin et récupération indépendante | A10–A11, A15 et recettes dédiées ; aucune promesse d’options avant réalisation. Risque élevé : dernier facteur, identité et récupération. |
| 5 | Raccords de production, connexion/services ordinaires, tâches planifiées, audit durable et qualification de clôture | Architecture de production explicitement approuvée ; qualification matérielle/UTM séparée ; réserves examinées ; aucun tag automatique. |

Les étapes 2 et 3 peuvent être préparées en conception mais ne seront pas qualifiées comme administration complète tant que le retrait effectif ne couvre pas leurs effets. L’interface partielle ne doit pas annoncer une désactivation achevée si les anciens accès restent actifs. Les utilitaires grep/sed/awk/cut et autres restent en pause : ce découpage n’autorise pas leur reprise.

## Décisions MOA/MOE nécessaires

- D1 — Langue : test littéral `fr` du présent mandat ou contrat BCP47/IANA ARC-026 (`fr-CA` inclus) ; aucune modification existante sans décision.
- D2 — API et portées administratives : transport privé/public, vues minimales, délégations, authentification fraîche pour mutations sensibles, budgets IPC et mémoire. Aucun sélecteur supplémentaire présumé.
- D3 — Validation du protocole préparation/confirmation, durée, stockage du suivi de requête, CLI de consultation et codes de retour proposés ; définir un refus d’admission certain distinct d’une issue inconnue.
- D4 — Maintien du refus actuel pour tous les admins ; si gestion multi-admin souhaitée, définir le dernier administrateur utilisable, la perte de facteur et la procédure indépendante avant toute exception. Aucun --force ne la remplace.
- D5 — Données des comptes supprimés : conservation par défaut ; rétention, accès de récupération, éventuelle réattribution/purge et surprotection à arbitrer séparément. Pas de suppression automatique.
- D6 — Suppression des groupes, réactivation des comptes et cycle de vie des facteurs : nouveaux contrats applicatifs absents de Change ; migration éventuelle des métadonnées à évaluer sans modifier P9.
- D7 — Liste des effecteurs et garanties d’acquittement par opération, drainage et éventuelle annulation contrôlée ; aucune nouvelle autorité d’arrêt général. Une machine injoignable laisse le retrait en cours lorsque la sécurité n’est pas démontrée.
- D8 — Couplage audit/décision, rétention, saturation et retrait d’urgence ; authentification administrative et récupération de production, protection et rotation des clés.

## Réserves et limites

Conserver la réserve de 301 cadres AArch64 instrumentés et le plafond 2048. Distinguer ressources actives, cache réutilisable et mémoire restituée. La reproductibilité entre environnements reste ouverte, pour les ELF historiques et les noyaux ordinaires ; une exécution réussie ne démontre pas une compilation reproductible. Les profils QEMU, autorités et détenteurs de laboratoire ne sont pas des services de production. Aucun noyau, format P9, ABI, commande ou service modifié par cette livraison.
