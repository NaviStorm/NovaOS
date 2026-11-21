# NovaAuth v0.24.0 — Contrat proposé des commandes administratives

**Spécification soumise à MOA/MOE, non implémentée.** Base et preuves dans l’[audit](novaauth-administration-audit-v0.24.0.md). Aucune nouvelle capacité publique ou extension ABI n’est autorisée par ce texte.

## Commandes et effets

Les noms sont des façades proposées vers les interfaces existantes. Aucun UID/GID POSIX n’est inventé. `SUJET` et `GROUPE` sont résolus par le registre ; `--subject-id ID` et `--group-id ID` permettent une cible stable explicite. ID désigne 16 octets affichés en 32 chiffres hexadécimaux. Nom et ID simultanés sont refusés. Les opérations ci-dessous sont mutuellement exclusives par invocation.

| Commande proposée | Syntaxe métier minimale | Droits / effet et limite |
|---|---|---|
| `whoami` | `whoami [--id]` | Session authentifiée ; identité propre uniquement, aucune mutation. |
| `id` | `id [--subject-id ID ou SUJET]` | Soi ; annuaire administratif pour autrui. Affiche ID stable, nom, état et appartenances autorisées ; jamais justificatifs ou données privées. |
| `groups` | `groups [--subject-id ID ou SUJET]` | Même frontière de lecture ; consultation d’autrui et noms de groupes exigent une nouvelle vue minimale côté service. |
| `useradd` | `useradd --factor-reference REF --group GROUPE NOM` | Mutation administrative ; référence publique d’un facteur préalablement attesté, non fournie comme simple preuve de possession. Création non administrateur, aucun home, dossier ou droit ajouté implicitement. |
| `usermod` | `usermod --rename NOM SUJET` ; `--disable SUJET` ; `--add-group GROUPE SUJET` ; `--remove-group GROUPE SUJET` | Une seule mutation par requête. Aucun `--admin`, `--enable` ou remplacement global de groupes dans ce premier lot. |
| `userdel` | `userdel SUJET` | Suppression logique irréversible du compte et révocation ; tombe durable, aucune suppression ni réattribution des fichiers. Pas d’option récursive, purge ou transfert. |
| `groupadd` | `groupadd NOM` | Création administrative d’un groupe à ID neuf ; aucune attribution automatique de droits. |
| `groupdel` | `groupdel GROUPE` | **Option future bloquée** : proposer refus si membres, règles ou références actives ; tombes et non-réutilisation à définir, `DeleteGroup` absent aujourd’hui. Aucun effacement en cascade. |
| `permissions` | `permissions show --installation ID --object ID --scope stored\|instance\|session [--instance ID] [--session ID]` | Consultation du titulaire sur l’installation attestée et l’objet VFS autorisé. Les vues effectives nécessitent leurs effecteurs ; jamais déduites seulement de la politique persistante. |
| `permissions` | `permissions grant\|revoke --installation ID --object ID --rights read\|write --origin direct\|group:ID --scope stored\|instance\|session [--instance ID] [--session ID]` | Délégation par propriétaire autorisé, pas privilège administratif général. `stored` vise la politique ; `instance/session` exigent le raccord effectif et ne modifient pas tacitement la politique. `revoke --rights write` conserve la lecture indépendante. Comme `set_permission` remplace actuellement le masque de la règle, l’adaptateur doit calculer l’union pour grant ou la soustraction pour revoke sur la même tête puis publier sous contrôle de génération ; ne pas remplacer tout le masque par zéro. Résolution VFS de l’objet requise ; un numéro déclaré ne prouve aucun droit. |
| `passwd` | Réservée, syntaxe non promise | Facteur de possession existant ; pas de mot de passe ni saisie protégée qualifiés. Définir rotation/révocation des facteurs et éventuel Argon2id avant distribution de cette commande. |

Les variantes non réalisées ne figurent pas dans l’aide comme prises en charge. Pour permissions, les filtres instance/session ne sont valides qu’avec leur scope ; ils ciblent une instance attestée, jamais un PID seul. Une demande d’octroi à un groupe ne crée pas ce groupe et ne contourne pas l’ACL ni la surprotection.

## Contrat commun de consultation et d’erreur

Chaque commande distribuée devra avoir `--help`, `--version` et une page `man`, y compris l’aide de ses opérations. Ces consultations sont locales, sans authentification métier, IPC administratif, création de tmp ou mutation. Option inconnue, ambiguë, répétée incompatible ou combinaison métier invalide : refus avant toute préparation. `--` termine l’analyse des options ; les noms invalides sont ensuite refusés par le registre. Pas d’analyse des options métier dans nsh. Modes interactif, script et `nsh -c` utilisent le même contrat. Les commandes internes `false` et `exit` gardent leurs effets et statuts.

Règle demandée pour cette conception : locale exactement `fr` → français ; autres valeurs et absence → anglais. **Écart à arbitrer avant réalisation** : [le sélecteur ARC-026 actuel](../../components/libraries/documentation_locale.rs) accepte aussi `fr-CA`, `fr-FR` et `fr-Zzzz` enregistrés, avec casse indifférente. Le présent mandat ne modifie pas ce sélecteur. La MOA/MOE doit confirmer le test littéral pour ces nouvelles commandes ou harmoniser avec le contrat antérieur ; ne pas changer silencieusement les 37 commandes existantes. Les statuts et identifiants de protocole ne sont pas traduits.

Proposition de codes CLI, distincte des codes ABI et soumise à validation : 0 résultat demandé complet et confirmé ; 2 syntaxe/entrée invalide ; 3 refus d’accès, cible non visible ou confirmation invalide ; 4 conflit de génération/nom/appartenance ; 5 borne atteinte ; 6 corruption/E/S ou incohérence d’état ; 7 session ou autorisation expirée/révoquée ; 20 issue inconnue ou effets de révocation encore non acquittés ; 21 service/autorité indisponible avant toute admission certaine ; 22 annulation volontaire avant émission. Après émission, une panne ne retourne pas 21 si l’issue peut être UNCERTAIN : retourner 20 et la référence d’opération. Ne pas confondre ces numéros proposés avec `Fault` ou les statuts natifs historiques.

stdout contient les résultats autorisés ; stderr les diagnostics sans secrets. Les mutations affichent identifiant de requête, cible stable, génération et état effectif. Une option commune proposée `--outcome REQUETE` consulte uniquement l’issue autorisée de la commande d’origine ; elle exclut tous les paramètres métier. Même identité ou autorité de récupération explicitement habilitée, jamais accès par la seule connaissance d’un identifiant. Un résultat historique indique sa date/génération et ne vaut pas autorisation actuelle.

## Capacités et chaîne d’autorisation

Les étiquettes suivantes décrivent des **portées logiques à concevoir**, pas des nouveaux sélecteurs ou bits ABI existants : `identity.self.read`, `identity.directory.read`, `identity.user.mutate`, `identity.group.mutate`, `identity.membership.mutate`, `identity.operation.read`, `permission.owner.manage`, `audit.emit`, `audit.read`. Aucun droit administratif n’implique lecture des fichiers, installation, arrêt arbitraire d’un processus, gestion des clés ou récupération de production.

1. nsh lance un ELF attesté avec sa session ; l’identité ne vient ni d’une variable, ni d’argv, ni du nom du programme.
2. Le service vérifie route cliente non substituable, incarnation, session, principal actif, tête fraîche et portée avant de construire le Grant privé. Pour les mutations administratives, le bit `admin` actuel reste une condition nécessaire ; une portée déléguée ne peut promouvoir un non-admin. La consultation de soi et la gestion de ses propres dossiers restent accessibles à leur titulaire autorisé sans exiger ce bit.
3. Le client transmet une opération bornée, non un Grant, un masque de privilèges ou un objet Engine. Le service filtre les vues retournées ; pas de sérialisation brute des utilisateurs ni des facteurs.
4. NovaAuth décide ; NovaFS écrit uniquement le triplet via le port transactionnel existant ; l’autorité indépendante décide durablement ; le VFS applique les droits aux opérations réelles.
5. Le service n’obtient aucun droit de dossier du seul fait d’administrer l’identité. Les délégations sont limitées à l’objet, l’origine, la session et l’instance ; elles ne peuvent élargir l’autorité du délégant.

Une réutilisation des transports IPC existants est préférée. Il manque le contrat public de routage, pas un nouveau registre. Budget à démontrer avant raccord : état de base / admission / attente / annulation / drainage par acteur ; quatre racines maximum, cinquième demande LIMIT sans fuite. Le registre et le signataire ont déjà un pic de quatre : utiliser des feuilles explicitement autorisées ou un canal existant validé, sans supposer qu’une racine additionnelle est gratuite. Taille des requêtes, files, délais et allocation de canaux seront figés après la mesure ; aucun budget de production fictif n’est annoncé.

## Confirmation et publication

Toute mutation sensible utilise une préparation sans effet métier : cible résolue, opération canonique, principal, session, incarnation, tête attendue et identifiant unique. Le service retourne un résumé minimal et une empreinte ; une préparation ne réserve pas indéfiniment le verrou d’écriture. Proposition : interactive, confirmation explicite du résumé ; en script, `--prepare` puis nouvelle invocation avec les mêmes arguments et `--confirm EMPREINTE --request ID`. Pas de confirmation implicite par pipe ou simple `--yes`. Cette interface et la durée de confirmation restent à arbitrer ; le reçu n’est pas une capacité transférable. Pour les deux invocations d’un script, la préparation est liée à l’instance stable de nsh, à sa session et à l’incarnation du service ; la seconde instance ELF doit être attestée et prouver sa filiation autorisée à cette même session. Le reçu seul ne suffit pas et une autre session ne peut le reprendre. Ce routage fait partie de l’API manquante, pas d’une propriété déjà qualifiée.

À l’admission, revérifier tête et droits. Une confirmation périmée ou une cible renommée/recréée ne doit jamais viser une autre identité. Une mutation concurrente retourne conflit, sans recalcul ni réémission automatique. L’identifiant est créé avant envoi, lié au contenu et conservé dans le résultat de préparation ; après perte de réponse, `--outcome` retrouve la même décision. Réutiliser l’identifiant avec un autre contenu est refusé. `outcome=None`, délai dépassé ou autorité inaccessible ne prouvent pas une non-admission : conserver UNKNOWN/UNCERTAIN tant qu’une preuve durable ne tranche pas.

Préparation du triplet → relecture d’intégrité → décision indépendante durable → visibilité du triplet → propagation des révocations → audit et acquittement final. Une création est confirmée seulement après décision durable vérifiée. Pour un retrait, distinguer état persisté et effets appliqués : le code 0 de l’opération complète attend la frontière définie ci-dessous. Une panne après décision ne rejoue jamais la mutation. La récupération utilise les mécanismes existants ; elle ne reconstitue pas arbitrairement un triplet absent.

## Dernier administrateur

**État actuel conservé : Disable/Delete refusent tout compte admin**, et aucune commande de promotion/dégradation n’est proposée dans le premier lot. Un groupe nommé admin ne confère aucun privilège. Ni l’utilisateur, ni l’ELF, ni un administrateur ordinaire ne peuvent s’auto-attribuer une nouvelle autorité de récupération.

Si un futur mandat introduit plusieurs administrateurs ou la gestion de leurs facteurs, l’invariant durable doit compter les administrateurs actifs effectivement authentifiables et les autorités valides, pas simplement les noms ou un cache. Suppression, désactivation, retrait de rôle, révocation du dernier facteur utilisable et suppression d’un groupe portant ce rôle doivent préserver au moins un administrateur utilisable. Deux retraits concurrents sont vérifiés dans le même ordre durable : au plus un admis si le second laisserait zéro administrateur.

Une exception exige une **procédure de récupération indépendante, authentifiée et validée par MOA/MOE**, liée à l’état courant, aux identités stables et au plan de transition. Sans cette procédure, refus ferme, même avec confirmation CLI. Une demande de récupération ou une approbation perdue ne constitue pas son achèvement. Le dispositif QEMU 2-sur-2 ne vaut pas procédure de production déjà approuvée et ne valide jamais un ancien état ni des droits révoqués. Définir séparément la continuité administrative et la disponibilité du système ; ne pas rouvrir l’enrôlement initial.

## Cycle de vie et données

- **Renommage** : même ID, propriétaires, règles et tâches attachés à cet ID ; aucun déplacement ou renommage automatique des fichiers.
- **Désactivation** : identité et données conservées, nouvelles authentifications/admissions interdites, retrait des délégations et sessions selon la barrière effective. Aucun rétablissement automatique ; Enable est absent et nécessite un contrat distinct.
- **Révocation ciblée** : compte actif conservé ; retirer uniquement l’origine et les opérations visées. La lecture directe survit au retrait d’une écriture de groupe. Les handles et sous-délégations perdent ce droit à leur prochain contrôle effectif.
- **Suppression du compte** : état terminal Deleted et tombe d’identité non réutilisable. Ne pas confondre la suppression logique définitive avec l’effacement des octets d’historique. Retirer les appartenances, empêcher les facteurs/sessions anciens ; un nouveau compte de même nom reçoit un ID neuf et aucun héritage.
- **Données privées** : fichiers, ACL, cache et propriétaires restent attachés à l’ancien ID ; aucune suppression ni réattribution automatique. Le compte recréé, le nouvel admin et la commande userdel ne peuvent pas les lire de ce seul fait. Les données restent conservées et protégées, sans propriétaire actif pour les exploiter.
- **Transfert ou purge ultérieurs** : procédure indépendante à arbitrer (autorité, consentement, conservation, surprotection et audit), jamais chown/chmod implicite ni option de userdel. Tant qu’elle manque, conserver les données et refuser la réattribution.
- **Tâches et services** : définitions de tâches liées à l’ID supprimé conservées mais inéligibles au lancement ; tâches déjà admises soumises au retrait. Le scheduler crontab et les services ordinaires non raccordés restent à qualifier, pas déclarés couverts par les témoins de laboratoire.

## Révocation : points effectifs et limites

| État exposé | Point de linéarisation / preuve | Ce qu’il ne permet pas d’affirmer |
|---|---|---|
| LOCAL_BLOQUE | Barrière locale atomique au registre/signataire et aux voies d’admission concernées : aucune nouvelle demande/signature depuis ces voies | Une enveloppe déjà transmise peut encore être admise à distance ; ce n’est pas un retrait distant acquitté. |
| RETRAIT_DEMANDE | Envoi avec identifiant stable ; suivi des opérations en vol | Une coupure ou un délai ne prouve aucune décision distante. |
| RETRAIT_EN_COURS | Autorité a durablement fermé les nouvelles admissions concernées ; reçu authentifié et ordre durable vérifiés | Les admissions antérieures peuvent encore produire un effet ; B reste inactive pour un retrait d’incarnation. |
| EFFETS_LOCAUX_ACQUITTES | Chaque effecteur concerné a installé le retrait d’origine/époque ; anciennes capacités et sous-délégations refusées pour les nouvelles opérations | Ne pas confondre invalidation logique et restitution physique, ni généraliser à un effecteur inaccessible. |
| RETIREE / ACHEVEE | Issues de toutes admissions antérieures établies, barrières effecteurs requises acquittées et état terminal enregistré durablement | Pas de réussite si une création ou mutation peut encore arriver, ou si un effet requis reste inconnu. |

Pour les lancements, conserver exactement la barrière existante : chaque admission est achevée avec enfant identifié/collecté selon son contrat, ou annulée irréversiblement ; aucun lancement ne peut apparaître après RETIREE. L’activation d’une nouvelle incarnation exige RETIREE. Pour le VFS, le point local est la vérification de capacité, ACL, surprotection et origine par l’effecteur à l’admission de chaque opération. Une mutation déjà engagée peut avoir été durablement publiée : la réconcilier par identifiant, ne pas promettre son annulation rétroactive. Une lecture déjà autorisée peut avoir livré des octets : ils ne peuvent pas être rappelés.

L’API doit préciser, pour chaque opération, si l’acquittement exige attente, terminaison normale ou annulation effectivement supportée. **Aucun nouveau droit d’arrêt général n’est proposé.** Une instance injoignable ne vaut ni arrêt ni annulation. Les cas où la sécurité ne peut pas être démontrée restent EN_COURS/UNCERTAIN, même au prix d’indisponibilité. La fermeture de session interdit définitivement la réutilisation de ses anciens tickets ; une nouvelle session explicite a un ID neuf et ne répète pas les anciennes opérations.

Le retrait de droit ou d’appartenance doit être rattaché à l’ordre durable de l’autorité et à une tête fraîche. Restaurer P9 seul ou tous les disques locaux ne réactive ni la politique ni les handles. Un ancien reçu reste une preuve historique consultable, pas une autorisation de dossier. L’indisponibilité de l’autorité ne devient jamais une permission.

## Audit et sécurité résiduelle

Préparer un événement lié à la même requête et décision : principal authentifié, cibles stables, opération, origine, génération, état local/distant et issue connue/incertaine. Livraison idempotente au journal, accès séparé ; aucun facteur, clé, contenu privé ou défi utilisable. Le journal existant ne suffit pas à prouver cette atomicité : protocole d’événement durable à définir. Si une attribution/enrôlement ne peut être audité, refuser avant décision. Un retrait d’urgence doit fermer l’admission malgré une panne du journal, en conservant un état de diagnostic durable et sans annoncer une réouverture sûre ; modalités à arbitrer.

Risques majeurs : substitution de principal ou d’objet, faux administrateur par simple nom, lecture excessive d’annuaire, privilèges hérités d’un nom réutilisé, suppression du dernier facteur admin, confusion stockage/effectivité, réémission UNCERTAIN, données orphelines abandonnées ou exposées, saturation des canaux, fuite de secrets dans les confirmations, restauration complète incluant l’autorité de laboratoire. Les scénarios et décisions correspondants sont dans le [plan de qualification](novaauth-administration-qualification-v0.24.0.md).
