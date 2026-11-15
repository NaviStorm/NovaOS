# Phase 2 proposée · temporaire privé d’une instance

**Portée.** Ce contrat prépare le service de répertoires temporaires. La phase 1 fournit l’identité et la révocation des instances. La création de répertoires, leur purge et la reprise après coupure devront être implémentées et qualifiées séparément. [Contrat du gestionnaire](reconciliation-v0300/contrat-instances-services.md)

## Propriétaire et ouverture

Le propriétaire est le triplet binaire complet `(BootID, InstanceID, Generation)`. Le gestionnaire fournit au service une identité authentifiée par son canal de lancement. Le service vérifie ce canal, l’identité encore active et les ressources disponibles. Un appelant qui connaît le triplet, le PID ou le chemin interne ne peut obtenir une vue de cette façon.

À chaque lancement autorisé, le service réserve une racine neuve et vide avant de publier sa capacité privée au processus. Une collision avec un nom ou un propriétaire déjà présent fait échouer l’allocation. Le service isole l’objet litigieux. Une erreur entre réservation et publication laisse un objet à examiner lors de la reprise. La politique d’admission doit préciser si l’application peut démarrer lorsqu’elle a déclaré ce temporaire indispensable.

Les threads partagent la table de capacités du processus. Un enfant d’une même instance peut utiliser la racine si le gestionnaire lui a explicitement délégué une vue bornée. Deux instances, même issues du même manifeste, reçoivent deux racines distinctes. La capacité ne permet ni de remonter hors de sa racine, ni d’énumérer les temporaires voisins ou le cache.

## Destruction et reprise

| Événement | Comportement exigé du futur service |
|---|---|
| Sortie normale, arrêt forcé ou faute du principal | Fermer les nouvelles ouvertures, retirer les capacités et leurs descendants, puis résoudre les opérations déjà engagées selon le contrat de stockage. Autoriser le réemploi après confirmation du retrait et de la fin du nettoyage. |
| Perte du gestionnaire ou arrêt pendant une purge | Rendre les vues inaccessibles. La reprise doit pouvoir répéter la purge du même objet sans agir sur une incarnation plus récente. |
| Nouveau démarrage avec BootID qualifié | Inventorier les seuls objets de l’espace temporaire réservé au service. Pour une ancienne époque, vérifier le propriétaire et la provenance de l’objet avant suppression. Une chaîne de caractères contenant un ancien BootID ne suffit pas. |
| Source de BootID indisponible, support restauré ou provenance ambiguë | Refuser l’ouverture ou la réattribution de tout ancien temporaire. Garder les objets en quarantaine. Un éventuel mode volatil doit obtenir une racine en RAM réellement neuve, séparée du support persistant. |
| Échec de retrait ou de purge | Conserver la quarantaine et comptabiliser les ressources retenues. Une nouvelle instance n’obtient jamais la vue en attente de nettoyage. |

Une restauration de la mémoire d’une VM peut reprendre le même démarrage et les mêmes capacités. La détection de clones ou de retours à un instantané nécessite un signal indépendant du contenu restauré. Le BootID seul ne permet pas de certifier cette situation. La fermeture d’une instance ne déclenche aucune purge du cache persistant.

## Recette avant activation

- Ouvrir simultanément deux instances du même manifeste, vérifier que chaque racine est vide et que la capacité de l’une ne permet aucune opération sur l’autre.
- Vérifier le partage entre threads, le refus d’un enfant sans délégation et le refus d’ouvrir par PID, nom ou triplet recopié.
- Conserver un handle et une opération en attente pendant la destruction. Vérifier le refus de réutilisation du handle, la fin explicite de l’opération et le comportement après relance.
- Interrompre successivement la réservation, la publication de la vue et la purge. Redémarrer avec une autre époque et vérifier une récupération idempotente. Injecter une erreur de purge et une identité ambiguë, puis vérifier la quarantaine et la conservation des autres données.
- Arbitrer avant implémentation le support des métadonnées de propriété, les quotas (y compris la quarantaine), l’autorité chargée de la reprise et le traitement des restaurations de VM. Toute évolution du format NovaFS P9 demande une décision séparée.
