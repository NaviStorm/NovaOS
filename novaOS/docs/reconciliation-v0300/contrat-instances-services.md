# Instances d’application et services · contrat de phase 1

La branche de développement applique la décision MOA sur le principal, les travailleurs et la fin de l’instance. Ce document reprend le contrat préparatoire de v0.20.0. Le mandat de phase 1 précise également les trois sous-commandes futures de `permissions`. La recette du serveur v0.30.0 et la répartition des travaux de stockage entre jalons restent distinctes de la qualification présente. [Contrat préparatoire au commit 622d8d9](https://github.com/NaviStorm/NovaOS/blob/622d8d97e547434d62b65da00827324cf9405507/novaOS/docs/reconciliation-v0300/contrat-instances-services.md) · [Qualification de phase 1](../instances-phase1-v0.21.0.md)

## Identité et admission implémentées

| Objet | Identité, portée et autorité |
|---|---|
| Application installée | Le manifeste immuable associe un identifiant explicite à chaque commande embarquée. Renommer un ELF ou changer son indice ne confère aucun droit. Le constructeur et le gestionnaire utilisent la même déclaration. Le paquet des services système, le shell et l’application multiprocessus de référence ont leurs propres identités de catalogue. |
| Instance | Le noyau attribue un numéro strictement croissant dans le domaine supérieur à 2⁶³ pour ce démarrage. Une référence interne contient aussi le slot. Toute consultation vérifie la génération. À épuisement, le registre refuse la création. |
| Processus et threads | Le descripteur de processus porte la référence d’instance et le rôle de principal ou de travailleur. Le TCB référence le PID générationnel du processus. L’espace mémoire et l’instance sont communs à ses threads. Le recyclage d’un slot laisse les anciens droits inopérants. |
| Utilisateur | L’identité de l’utilisateur relève du futur service d’autorisation. Aucun des trois identifiants précédents ne l’atteste. |

Le lanceur remet au gestionnaire isolé un plan issu du catalogue, la sélection des flux terminal et la présence d’une paire IPC. Un refus arrête le lancement. Les flux doivent également appartenir au lanceur. Chaque canal de la paire doit pouvoir être dérivé depuis sa table avec les droits demandés. Une erreur de préparation annule la publication du processus. Les anciens indices du catalogue et l’ABI d’appel de `SPAWN` sont conservés. Le shell conserve ses délégations explicites de session. [Manifeste](../../abi/installation.rs) · [Admission et lancement](../../kernel/src/sched/process/ipc/spawn.rs)

Le gestionnaire s’exécute en EL0 ou Ring 3, dans un espace séparé pour chaque décision. Pour une application monoprocessus, il rend son verdict et se termine. Pour le profil multiprocessus, il reste associé à une seule incarnation et dort jusqu’à la restitution finale. Le noyau authentifie le gestionnaire par son rendez-vous privé, détient les références privilégiées et applique le retrait même si le principal ne coopère plus. La perte du gestionnaire lié déclenche l’arrêt d’une famille encore active. [Gestionnaire utilisateur](../../user/instance_manager.rs) · [Rendez-vous et observation](../../kernel/src/sched/process/supervision.rs)

## Transitions et ressources

| Transition | Effet |
|---|---|
| Créée → initialisée → active | Réserver une entrée, initialiser la référence, associer le principal. La garde de publication protège le TCB et l’installation des droits jusqu’au premier quantum. |
| Active → en arrêt | Sortie ou faute du principal, perte du gestionnaire lié ou arrêt forcé autorisé. Fermer le rattachement et retirer les droits des membres sous la garde de l’ordonnanceur. Un travailleur qui sort seul laisse son principal actif. |
| En arrêt → terminée | Chaque membre a une issue. Un travailleur encore actif est arrêté avec le code 125. La collecte du résultat du principal suit le droit d’attente habituel. |
| Terminée → en nettoyage → détruite | Réveiller les attentes concernées, retirer chaque TCB après sa commutation et restituer les espaces d’adressage. La dernière restitution libère l’entrée. Le dernier numéro détruit par slot reste enregistré en RAM, sans autorité, jusqu’à la destruction suivante dans ce slot. |

Le principal admis peut créer un travailleur du même programme dans sa propre incarnation. Le travailleur reçoit un argument scalaire et son propre espace. Le droit de suivi existant borne le principal à un enfant observé à la fois. Un travailleur peut exécuter ses propres threads. L’arrêt forcé exige simultanément une offre du principal, le droit local de le suivre et le numéro exact de l’incarnation. Les retraits des pilotes gardent leurs procédures de fin et de quarantaine. La collecte du résultat d’un processus et la restitution du dernier cadre du groupe sont deux événements distincts. [Registre](../../kernel/src/sched/process/instances.rs) · [Cycle des processus](../../kernel/src/sched/process.rs)

Les deux nœuds IPC initiaux délégués par le lanceur sont suivis par leurs références générationnelles. La destruction du bénéficiaire coupe leur branche, même après fermeture locale ou déplacement d’un handle. Un endpoint créé par le bénéficiaire est détruit avec lui. Les autres branches d’un propriétaire indépendant restent utilisables. La fermeture ordinaire d’un handle conserve la sémantique IPC publiée. [Architecture IPC](../ipc-architecture.md)

## Contrats réservés aux phases suivantes

| Objet | Propriétaire et contrat à réaliser | Condition préalable |
|---|---|---|
| `tmp` privé | Une racine par incarnation, accessible par une capacité initiale bornée. Les threads partagent la vue du processus. Tout partage avec un autre processus exige une délégation explicite. À l’arrêt, rendre la vue inaccessible avant la purge. | Un identifiant sur disque doit distinguer deux démarrages. Prévoir une époque fiable ou un espace de noms neuf attesté, des quotas et une purge reprenable. Isoler les orphelins de propriétaire incertain. Un numéro de cette phase, repris seul après redémarrage, ne permet ni réattribution ni suppression. |
| `cache` persistant | Un espace reconstructible par identité d’installation et d’utilisateur. À chaque lancement autorisé, ouvrir une nouvelle vue après vérification de la politique. La destruction d’une instance éteint sa vue et préserve les données partagées selon la politique de purge. | Arbitrer l’identité durable, les quotas, la concurrence entre instances, la purge, les réinstallations et les restaurations. Un nom de compte ou un chemin identique ne suffit pas. |
| Services système | Séparer les droits d’appel, de supervision et d’accès aux données d’un client. Prévoir disponibilité explicite, dépendances, budget de relance et renouvellement des canaux. | La phase 1 identifie le paquet système. L’enrôlement distinct des services, le graphe de démarrage et les relances automatiques restent à qualifier. |
| `permissions list` | Distinguer application installée, principal utilisateur, instance active, politique persistante versionnée et capacités effectives détenues. | Définir qui peut consulter chaque information et quelle source l’atteste. |
| `permissions grant`, `permissions revoke` | Identifier la politique ciblée, son autorité et le périmètre d’effet sur les délégations existantes. Retirer un droit effectif et modifier une politique durable sont des opérations distinctes. | Arbitrer l’administration et la révocation avant toute écriture persistante. Aucun droit administratif implicite pour `nsh`. |

Ces contrats réservent les décisions de sécurité nécessaires à la future intégration de NovaAuth, des vues de fichiers et des politiques persistantes. [Contrat d’autorisation du serveur](contrat-autorisations.md)
