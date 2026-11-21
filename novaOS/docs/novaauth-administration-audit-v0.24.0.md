# NovaAuth v0.24.0 — Audit de l’administration native

Conception du 21 novembre 2026, base officielle `5c7f1bfa313a3859ccc0830660438e76d598acb1`, branche `codex/novaauth-administration-0240-conception`. Aucun mécanisme ni commande implémenté par ce mandat. Les utilitaires parallèles restent **en pause jusqu’à autorisation MOA/MOE explicite** ; conserver leurs branches ne les autorise pas à reprendre.

## Inventaire vérifié

« Présent » désigne une interface dans les sources, pas une API IPC publique ni une commande distribuée. « QEMU » désigne les seuls profils qualifiés indiqués par le [rapport global](novaauth-qualification-globale-v0.24.0.md). Les tests hôtes ne qualifient pas à eux seuls le chemin invité.

| Besoin | Interface et comportement constatés | Portée et lacune |
|---|---|---|
| Enrôlement initial | [lib.rs](../user/novaauth/src/lib.rs) : `Enrollment`, `Registry::enroll`, génération 0 seulement ; `Change::Enroll` produit le premier admin | Port de provisionnement privé, pas commande ordinaire ; aucune réinitialisation sur base corrompue ou fichiers manquants. |
| Création | `Change::Create`, `Registry::mutate` ; nouveau compte `admin=false` ; identifiant dérivé du domaine, génération, requête et type | Bibliothèque et parcours de qualification ; adaptation administrative publique absente. Référence de facteur non nulle, unique, pas mot de passe. |
| Renommage | `Change::Rename` conserve l’identifiant et incrémente la révision ; nom unique parmi comptes non supprimés | Ne change ni propriétaire ni droits. L’interface accepte aussi un compte désactivé ; aucun changement de chemin de fichier n’en découle. |
| Désactivation / suppression | `Disable`, `Delete` ; refus si `u.admin` ; suppression logique `Deleted`, retrait des appartenances ; référence du facteur conservée dans la tombe | Tous les admins sont actuellement protégés, protection plus forte que le seul dernier admin. Pas de suppression physique des fichiers, ni d’effacement de la tombe. Pas de variante de réactivation. |
| Groupes | `AddGroup`, `AddMember`, `RemoveMember` ; révision du titulaire ; admissions de groupe révisées avec `politiques-lab` | Pas de `DeleteGroup`, renommage de groupe, remplacement global des groupes ni propriété POSIX primaire/secondaire garantie. |
| Consultation | `inspect(Grant, subject)` : soi ou admin ; `groups(Grant)` : groupes du titulaire seulement | Pas de recherche publique par nom, énumération paginée, consultation des groupes d’autrui ou annuaire public stable. Ne pas sérialiser directement `User` et ses champs internes. |
| Autorisation d’administration | `mutate` exige `Grant` sur la tête courante et utilisateur `admin`; `Enroll` y est interdit | `Grant` opaque en Rust ; `grant_attested` et `resolve_verified_factor` sont des points de confiance privés, jamais des requêtes acceptant une identité déclarée par le client. Pas de promotion administrative exposée. |
| Permissions | [policy.rs](../user/novaauth/src/policy.rs) : `set_permission`, `stored_permission`, `permission`, `verify_image`; `Origin::Direct/Group` et révisions distinctes | `politiques-lab` : titulaire + installation + objet + origine. `set_permission` exige le propriétaire, pas simplement un admin. `bind_folder` / `provision_install` restent privés. Droits READ/WRITE, pas correspondance Unix implicite. |
| Authentification | [service/lib.rs](../user/novaauth/service/src/lib.rs) : `Factor`, `Possession`, `Verifier::begin/finish`, `Binding`, `Route` | Défis Ed25519, client et clé de laboratoire ; pas de login administratif de production. Argon2id et saisie protégée sont des propositions. |
| Sessions | [sessions.rs](../user/novaauth/service/src/sessions.rs) : `open`, `inspect`, `close`, `disconnect` ; 16 baux bornés ; facteurs et route revérifiés | Bibliothèque et raccords QEMU. Une mutation du registre ne démontre pas à elle seule une notification immédiate de tous les effecteurs ordinaires. |
| Anti-rollback et issue | `Authority::current/outcome/decide`, `Registry::outcome` ; tête domaine/volume/objets/génération/empreinte/requête | Autorité indépendante, signataire et reprise qualifiés QEMU ; indisponibilité refuse l’activation. `None` sur outcome n’est pas une preuve durable d’annulation d’une requête en vol. |
| Persistance | `publish_state` prépare trois objets et les relit, puis `Authority::decide` sélectionne la tête | Atomicité de visibilité du triplet par décision, pas écriture physique simultanée de trois fichiers. NovaFS reste unique écrivain ; aucune exposition d’Engine au client administratif. |
| Récupération | [recuperation.rs](../user/novaauth/src/recuperation.rs), [recuperation_store.rs](../user/novaauth/src/recuperation_store.rs), [registre-incarnation.rs](../user/novaauth/service/src/bin/registre-incarnation.rs) | Récupération 2-sur-2 et incarnation approuvée en QEMU ; pas récupération administrative de production. Vérification exacte contre la tête indépendante avant activation. |
| Révocation des lancements | [admission_rpc.py](../user/novaauth/autorite_qemu/admission_rpc.py), profils EXEC et autorité indépendante | Admissions ordonnées, drainage, RETRAIT_EN_COURS/RETIREE qualifiés QEMU. Une capacité déjà détenue exige aussi le contrôle de l’effecteur ; le reçu historique n’autorise rien aujourd’hui. |
| Journal | [journal_access.rs](../user/services/journal_access.rs), [journal du shell](../../components/shell/journal.rs) | Service existant avec droits séparés émission/lecture/maintenance ; traces de qualification disponibles. Le raccord durable de chaque mutation administrative à l’audit reste à concevoir et qualifier. |
| Commandes | Recherche dans `components/` et `components/shell/` : pas de commande intégrée id/whoami/groups/passwd/useradd/usermod/userdel/groupadd/groupdel/permissions | Les propositions ARC-027 et les branches séparées ne sont pas une livraison. Réutiliser le shell et les services, pas inventer une implémentation déjà présente. |

Les bornes constatées incluent 64 enregistrements utilisateurs et 64 groupes (tombes incluses pour les utilisateurs), slots de 16 Kio par objet/génération, noms de 1 à 32 octets `[a-z0-9-]`. Il ne faut pas annoncer Unicode, recyclage de tombes ou capacité illimitée. Atteindre la borne doit refuser sans publication ; toute évolution exige étude du format applicatif, sans modifier P9.

## Séparation des profils

Le [Cargo.toml du registre](../user/novaauth/Cargo.toml) expose `politiques-lab`, `distribution-lab`, `recuperation-lab`. Les binaires invités utilisent aussi des profils `*-guest` et des exports de qualification. Une fonctionnalité en bibliothèque ne prouve ni son routage ni ses capacités dans la distribution ordinaire. Les 48 sélecteurs, le noyau ordinaire, les services ordinaires et NovaFS P9 ne changent pas ici.

Acquis réutilisables comme références : 24 scénarios globaux + huit antérieurs, recettes VFS/2-sur-2, neuf contrôles ABI et quatre applications ELF historiques, selon le rapport global et ses manifestes. Ce mandat ne les réexécute pas et ne qualifie pas les futures commandes.

Réserves maintenues : **301 cadres AArch64 instrumentés**, plafond 2048 inchangé ; **reproductibilité entre environnements**, concernant aussi les noyaux ordinaires. Identité SHA des quatre applications historiques conservées et compatibilité observée ne prouvent pas leur recompilation reproductible. Les reconstructions appariées des noyaux étaient identiques entre base et candidat, sans reproduction garantie de tous les anciens contextes.

## Lacunes bloquant une implémentation ordinaire

1. Route administrative authentifiée liée à la vraie instance de commande et à la session de nsh, sans export de Grant ni clé privée.
2. Résolution contrôlée nom → identité stable, vues minimales, pagination, consultation des groupes et résultat d’opération autorisé.
3. Préparation/confirmation bornée et liaison immuable au contenu de mutation, à la tête attendue et aux cibles stables.
4. Notification et acquittement des effecteurs VFS, sessions, enfants et délégations ; recensement des admissions en vol et conditions de clôture.
5. Suppression de groupe, réactivation et gestion des facteurs absentes ; aucune variante ne sera annoncée avant son arbitrage et sa qualification.
6. Autorité et récupération de production, transport indépendant de NovaFS, garde des clés, service de connexion, audit durable.
7. Budget IPC complet de la nouvelle route. Le registre et le signataire atteignent déjà quatre racines dans les profils qualifiés : aucune nouvelle paire possédée par ces acteurs ne peut être ajoutée implicitement.

[Contrats proposés](novaauth-administration-contrat-v0.24.0.md) · [Recettes, étapes et arbitrages](novaauth-administration-qualification-v0.24.0.md).
