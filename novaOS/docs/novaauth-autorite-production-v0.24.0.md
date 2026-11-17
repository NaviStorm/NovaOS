# NovaAuth v0.24.0 — Reprise de l’autorité persistante

Base d’intégration du lanceur ELF : `a3efdbb70d652f2687e7bbf31e27872fbe05c3a1`. Le premier lot porte sur la conservation des révocations après restauration. La MOA/MOE a retenu une autorité indépendante du serveur, hébergeable en infrastructure privée. Le profil QEMU utilisera un signataire invité isolé, accessible exclusivement au registre. La garde des clés de production reste à qualifier.

## Point de raccord existant

Le registre relit les trois objets P9, vérifie leur contenu préparé puis appelle `Authority::decide(previous, next)`. La tête lie le domaine, les identifiants du volume, la référence de récupération, les trois objets, la génération, l’identifiant de demande et le digest. À l’ouverture, `current()` désigne la tête autorisée. Après une perte de réponse, `outcome(request)` permet de retrouver une décision enregistrée. Le provisionnement dispose de deux opérations distinctes, `ensure_unused()` et `bind_new(genesis)`. [Interfaces et publication](../user/novaauth/src/lib.rs)

Le fournisseur de laboratoire `Ledger<D>` conserve ses décisions en chaîne sur un périphérique séparé de P9. La qualification a démontré qu’une restauration simultanée de ces deux supports pouvait réactiver un droit. Le banc TPM d’UTM utilise un émulateur et teste l’interface du périphérique. Une qualification de l’autorité de production devra exercer le stockage durable de la décision sur la cible retenue. [Fournisseur du banc](../user/novaauth/src/lab.rs) · [Résultats de restauration](novaauth-v0.24.0-phase2.md) · [Banc TPM](../tests/controle_tpm_utm.py)

## Cible retenue et variante ultérieure

| Cible | Condition d’exploitation | Preuve exigée avant activation |
| --- | --- | --- |
| Première cible approuvée — autorité indépendante, canal authentifié | Définir l’identité du domaine et de l’autorité, les écrivains autorisés, la disponibilité au démarrage et la reprise de l’autorité elle-même. | Restaurer complètement le serveur après une révocation acquittée. L’autorité doit désigner la tête plus récente et empêcher la réactivation des droits anciens. Éprouver aussi la perte d’un nœud et une partition si l’autorité est répliquée. |
| Variante ultérieure pour une installation autonome — ancrage matériel local lié au domaine | Identifier le matériel disponible sur les deux architectures, les opérations durables qu’il fournit et la procédure de transfert en cas de panne. | Sur matériel réel, prouver la liaison de la tête complète ou de son digest à l’ancre, l’exclusion des mises à jour concurrentes et la récupération du reçu après coupure. Mesurer latence et endurance. |

Dans les deux options, une demande ayant perdu sa réponse reste `UNCERTAIN` tant que le résultat durable est inconnu. Une indisponibilité, une divergence ou une tête ambiguë impose le refus de l’accès dépendant de l’autorité. La récupération de la connectivité doit permettre la consultation du résultat par le même identifiant. Toute rétention limitée des reçus nécessite une règle de réconciliation qui empêche une répétition après expiration.

## Recette de production à réaliser

| Situation | Observation requise |
| --- | --- |
| Deux écrivains proposent deux têtes différentes depuis la même tête complète | Une seule décision durable. Le perdant reçoit un conflit. Les reçus et la tête concordent après redémarrage. |
| Réponse perdue après la décision | Une seule mutation. La consultation du reçu retrouve le domaine, la demande et le digest exacts, même si d’autres publications ont suivi. |
| Ancien P9, ou restauration de toute la machine cliente après une révocation | Refus de l’état ancien. À génération égale, une différence de digest entraîne également un refus. |
| Autorité indisponible, support remplacé ou domaine déjà enrôlé dont les trois bases sont absentes | Refus explicite et diagnostic indépendant. Aucun nouvel enrôlement automatique. |
| Perte du facteur administratif | Soumettre le choix des gardiens, du seuil et de la présence requise. Qualifier la rotation du facteur, le retrait des sessions antérieures et le journal. Maintenir la protection des originaux et le contrôle du propriétaire sur la déclassification. |

La MOA/MOE approuve l’option B pour le stockage et l’ordre des décisions distantes sous conditions. La topologie de transport et les mesures d’isolation restent à valider avant développement. [Raccord du registre isolé](novaauth-raccord-registre-v0.24.0.md) · [Révocation et autorité distante](novaauth-revocation-distante-v0.24.0.md) · [Comparaison des transports et témoins rejoués](novaauth-transport-autorite-v0.24.0.md)

Après validation de ces contrats, qualifier chaque modèle de panne sur son support réel. Les commandes de gestion des comptes et groupes dépendront de l’authentification du demandeur, du contrôle de son autorité administrative et d’un reçu de mutation consultable en cas d’interruption. La connexion de production, la récupération et ces commandes restent ouvertes. [Architecture de récupération](novaauth-v0.24.0-phase1.md) · [Contrats des commandes et recettes](novaauth-recette-v0.24.0.md)
