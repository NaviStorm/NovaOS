# Préparation de v0.21.0 · instances d’application et services

**Statut.** Proposition soumise à la MOA/MOE sur la base de NovaOS v0.20.0. Le mandat inscrit trois exigences dans la stable serveur v0.30.0 : un `tmp` privé par instance, un `cache` persistant et une commande native `permissions`. La répartition entre jalons intermédiaires reste à décider. Les désignations ci-dessous expriment des rôles et des opérations à éprouver. Elles ne réservent aucun identifiant ABI.

## Ce que fournit v0.20.0

Le lanceur admet les exécutables natifs selon son profil. Un PID identifie un processus vivant. La collecte de sa terminaison exige un droit d’attente ; observer un processus exige une délégation distincte. Les threads partagent l’espace mémoire du processus. Le service de fichiers contrôle les vues et opérations déléguées. Le SDK 0.2.0 expose ces mécanismes aux applications admises. La politique durable des comptes, applications et objets reste un chantier du serveur. [ABI processus](../abi.md) · [Contrat du SDK](../contrat-public-rust-v0.20.0.md) · [Identités et autorisations du serveur](contrat-autorisations.md)

## Identifier une instance et la terminer

Le lanceur attesterait l’identité durable de l’application installée, le principal utilisateur ou service, la session applicable et une incarnation fraîche à chaque lancement. Deux lancements du même exécutable par le même compte ont des incarnations différentes. Le PID, le nom du programme ou un identifiant fourni par le client ne suffisent jamais à exercer un droit. Le redémarrage du superviseur ou la réutilisation d’un PID doivent laisser les anciens handles inopérants.

| Transition proposée | Responsable et résultat observable |
|---|---|
| Admission → lancement | Le lanceur valide provenance, principal, politique et ressources. Il prépare les vues et les droits initiaux avant le premier accès applicatif. Si une délégation obligatoire échoue, il retire celles déjà préparées et conserve une trace du refus. |
| En exécution → arrêt | À la sortie, au crash ou à l’arrêt autorisé, fermer l’admission de nouveaux usages, révoquer les délégations descendantes et résoudre les opérations en vol selon le contrat du service. Un résultat de processus reste collectable une fois par son détenteur autorisé. |
| Arrêt → restitution | Le gestionnaire ferme les vues, rend le temporaire inaccessible et libère les ressources après confirmation du retrait. Une purge interrompue reprend sur l’ancienne incarnation. Elle ne doit atteindre ni le cache persistant ni le temporaire du prochain lancement. |
| Relance | Le lanceur crée une incarnation, un temporaire et des capacités neufs. Il réévalue séparément tout accès à un cache existant. |

Un processus enfant reçoit uniquement les délégations nommées que son parent peut transmettre. La proposition de base est de créer une instance distincte par lancement applicatif. Le rattachement de plusieurs processus à une même instance et l’événement mettant fin à cette instance restent à arbitrer. La terminaison d’un seul thread ne suffit pas à déclencher la purge.

## Services et capacités initiales

Un service aurait une identité durable, une incarnation à chaque démarrage, des dépendances déclarées et un état « prêt » observable. Le superviseur attend les dépendances obligatoires et le signal de disponibilité avant de diriger les clients vers le service. En cas de panne, il ferme les anciens canaux, applique un budget de relance et expose l’échec après épuisement de ce budget. Les clients doivent obtenir de nouveaux canaux. En cas de dépendance indisponible ou cyclique, le service concerné reste indisponible et la cause est observable.

Le lanceur calculerait chaque octroi par intersection du profil déclaré, de la politique courante, des droits du principal et du plafond réellement délégable. Pour chaque vue ou endpoint, le contrat devra identifier l’émetteur, le destinataire, les opérations admises, la durée de vie et le responsable de la révocation. Les droits de **superviser un service**, de **l’appeler** et d’**agir sur les données d’un client** doivent être séparés. Le démarrage automatique ne confère aucun accès aux données privées d’une session. [Règles de délégation et de retrait](contrat-autorisations.md)

## Espaces de fichiers et commande native

| Objet | Contrat proposé | Cas qui doit échouer |
|---|---|---|
| `tmp` privé | Le lanceur remet une vue vide propre à l’incarnation. Accès borné à cette racine, quota et nettoyage à la fin de l’instance. Après une coupure ou une purge différée, isoler les restes avant d’accepter un nouveau lancement. | Une autre instance simultanée, même du même compte et du même logiciel, ne peut lister ou ouvrir le témoin sans délégation explicite. Un ancien handle doit échouer après relance. |
| `cache` persistant | Proposition de rattachement au couple principal durable × application enrôlée. Après synchronisation réussie, une relance autorisée retrouve les données, y compris après redémarrage en l’absence de purge. Revalider la politique avant ouverture. Les données doivent être reconstructibles après éviction. Définir quotas, purge et accès simultanés avant de promettre le partage entre instances. | Une autre application, un compte homonyme recréé ou une session révoquée ne récupère pas le cache. Une provenance incertaine après réinstallation entraîne le refus. Une restauration du cache ne rétablit aucun ancien droit. |
| `permissions` | Commande native proposée pour consulter les capacités initiales d’une instance dont l’observation est autorisée, puis les opérations effectives sur un objet ou une vue explicitement délégués. Le service répond selon le principal, la session, la politique et le plafond du handle. La sortie distingue les opérations consultables et le refus d’inspection. Une éventuelle mutation exigerait une opération distincte soumise à un mandat. | Connaître un chemin, être propriétaire du processus `nsh` ou exécuter la commande ne permet ni d’énumérer d’autres objets, ni de déléguer davantage, ni de révéler un nom privé ou une clé. |

Le service peut exposer ces espaces par des vues déléguées sans imposer de répertoires globaux POSIX ni de noms physiques sur disque. Les comptes, les ACL durables et la fraîcheur des retraits restent soumis au contrat de stockage. Le dossier ne choisit aucune extension de NovaFS P9. [Espace de noms livré](../namespace-v0.12.0.md) · [Dépendances de la politique durable](dependances-p9.md)

## Arbitrages avant mandat de développement

1. Choisir la première tranche de v0.21.0 et la frontière entre lanceur, superviseur et services. Fixer la durée de vie d’une instance multiprocessus et les conditions de retrait après crash.
2. Choisir l’autorité d’identité, le périmètre d’une délégation initiale et les observations permises. Examiner séparément toute extension d’ABI ou de droits.
3. Retenir le domaine du cache, les règles d’accès concurrent, les quotas, la purge du temporaire et les conditions de reprise après mise à jour, réinstallation ou restauration.
4. Fixer le périmètre de `permissions` (inspection seule proposée), la visibilité des motifs de refus et les preuves exigées avant toute fonction administrative. [Épreuves de la stable serveur](acceptation-serveur.md#instances-tmp-cache-et-permissions)
