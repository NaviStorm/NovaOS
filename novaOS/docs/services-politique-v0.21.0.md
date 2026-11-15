# Politique native des services système · version 1

Le gestionnaire conserve le catalogue installé, le graphe et la politique de reprise dans son espace utilisateur. Un client de contrôle peut désigner un identifiant installé. Seuls les exécutables, arguments et droits inscrits dans le catalogue sélectionné par le démarrage peuvent alors être transmis au mécanisme de lancement. [Déclarations distribuées](../user/services/catalog.rs) · [Moteur de politique](../user/services/policy.rs)

## Déclarations

Le format natif v1 est une table immuable de valeurs Rust `Declaration` incluse dans l’ELF du gestionnaire. Le champ `version` doit valoir 1 dans chaque entrée. Une déclaration ajoutée lors de la construction utilise le catalogue d’ELF natifs existant. Une requête `REGISTER` désigne une entrée par son ID et enregistre ses dépendances. L’opération est idempotente. À l’initialisation, le gestionnaire enregistre les déclarations automatiques et demande leur démarrage. Le profil de qualification possède des entrées supplémentaires.

| Champ | Signification et validation |
|---|---|
| `id`, `image` | Deux entiers non nuls. L’ID installé doit être unique. Plusieurs services peuvent employer le même ELF. |
| `args`, `caps` | Arguments scalaires et masque des droits à préparer. Le moteur accepte au plus quatre arguments. Le catalogue de cette distribution utilise un seul scalaire et la capacité du temporaire privé. Les droits des canaux de supervision proviennent du lanceur et les requêtes d’administration exigent leur propre canal autorisé. |
| `required`, `optional` | Listes d’identifiants locaux au catalogue, résolues avant toute action. Une entrée inconnue, une dépendance dupliquée ou un cycle, y compris optionnel, entraîne le rejet du catalogue. |
| `auto`, `restart` | Démarrage automatique ou manuel. La politique de relance vaut `Never`, `Failure` ou `Always`. Un arrêt demandé empêche la relance automatique. |
| `start_ticks`, `stop_ticks`, `heartbeat_ticks` | Durées dans l’unité monotone de `SERVICE_PROCESS/NOW`, la milliseconde. Le compteur de l’ordonnanceur à 250 Hz donne une résolution de 4 ms. Les deux premières durées sont strictement positives. Zéro désactive la surveillance périodique de présence. À l’instant exact d’échéance, une annonce tardive est refusée. |
| `retries`, `window`, `backoff` | Budget de 0 à 8 relances automatiques par fenêtre glissante et délai initial de reprise. Pour une politique de relance, la fenêtre et le délai initial sont strictement positifs. |
| `log` | Active les transitions ordinaires. Les refus de transition et les échecs terminaux restent visibles quand ce champ vaut faux. |

Les identifiants de service, les essais et les incarnations ont des portées distinctes. Le gestionnaire réserve un numéro d’essai croissant par service avant de soumettre le lancement. Un dépassement du compteur place le service en échec. Le canal de l’incarnation, l’identité de démarrage et le droit d’observation permettent à l’adaptateur d’authentifier la disponibilité. Le numéro d’essai relie le message authentifié à l’état en mémoire. Un gestionnaire redémarré obtient de nouveaux canaux pour ses nouvelles incarnations. [Contrat des identités](instances-phase1-v0.21.0.md)

## Dépendances et transitions

Les entrées sont parcourues dans un ordre topologique déterministe. Plusieurs lancements indépendants peuvent rester en vol. Une notification de création du processus laisse son service dans `STARTING`. Une annonce authentique de l’incarnation courante mène à `READY`. Le tour suivant mène à `RUNNING` et peut lancer les dépendants obligatoires. Une annonce répétée, tardive ou provenant d’un ancien essai est rejetée. Un fournisseur optionnel est demandé en priorité lorsque ses propres préconditions le permettent. Son indisponibilité laisse son consommateur démarrer ou continuer.

| Origine | Événement | Destination |
|---|---|---|
| Entrée installée | Enregistrement autorisé | `REGISTERED` |
| `REGISTERED`, `STOPPED` ou `FAILED` | Demande autorisée de démarrage | `WAITING` |
| `WAITING` | Tous les fournisseurs obligatoires sont disponibles | `STARTING` |
| `WAITING` | Échec terminal ou arrêt d’un fournisseur obligatoire | `FAILED`, avec l’ID du fournisseur |
| `STARTING` | Annonce authentifiée et reçue avant l’échéance | `READY`, puis `RUNNING` |
| `STARTING`, `READY`, `RUNNING` | Arrêt ou redémarrage demandé, perte de capacité ou de communication, délai dépassé, fournisseur obligatoire indisponible | `STOPPING` après l’arrêt des dépendants à retirer |
| Incarnation active | Fin observée | `STOPPED` en cas d’arrêt normal ou demandé, `FAILED` en cas d’échec, `WAITING` pour un consommateur suspendu par la perte temporaire d’un fournisseur |
| `STOPPED` ou `FAILED` après la fin observée | Politique de relance applicable et budget disponible | `RESTARTING`, puis `WAITING` à l’échéance |
| Tout état sans processus | Arrêt explicite | `STOPPED` |

La perte d’un fournisseur obligatoire demande l’arrêt de ses consommateurs actifs en ordre inverse. Le service en arrêt reçoit une demande coopérative, puis un arrêt forcé unique si le délai est dépassé. La relance attend la preuve de fin de l’ancienne incarnation. Les services en `FAILED` à cause d’une dépendance terminale requièrent une demande de démarrage qui réexamine la chaîne. Un arrêt explicite du fournisseur désactive aussi ses consommateurs obligatoires. L’arrêt complet prend en compte les arêtes obligatoires et optionnelles et laisse progresser les arrêts indépendants en parallèle.

La relance automatique compte chaque décision de relance dans une fenêtre glissante. Un instant antérieur d’au moins `window` ticks sort de la fenêtre. Le délai vaut `backoff × 2^n`, où `n` compte les décisions précédentes encore dans la fenêtre, avec saturation arithmétique. L’épuisement du budget laisse le service en `FAILED`. Une intervention explicite peut demander une nouvelle tentative. Cette intervention conserve l’historique de la fenêtre. La disparition du gestionnaire relève de la révocation et du rattachement des familles au niveau du noyau.

## Journal et budget mémoire

Chaque entrée de politique contient l’ID installé, le numéro d’essai, l’état précédent, le nouvel état, la cause, la valeur de sortie et l’ID de dépendance concerné. Le gestionnaire peut l’associer à son observation de l’incarnation. Une opération rejetée préserve l’état et produit un événement `Invalid`. Le journal local contient seize entrées et expose un compteur saturant des entrées anciennes évincées. Les arguments et les octets du temporaire restent hors du journal.

Le moteur occupe **2 072 octets** pour douze entrées sur les deux architectures, mesurés par `size_of::<Policy>()`. Ce chiffre comprend les masques de dépendances, budgets de reprise et entrées de journal. Les ELF, tables immuables, piles, canaux, processus invités et ressources du temporaire ont leurs propres coûts. Le moteur travaille sans allocation dynamique.

Douze tests ont réussi sur Darwin ARM64 natif et douze sous Rosetta x86_64. Ils couvrent notamment les graphes de douze sommets, les cycles et doublons, la perte d’une dépendance, les délais, l’ancien `READY` entre deux incarnations, l’arrêt inverse, les plafonds des catalogues installés et la fenêtre de relance. Les essais effectifs en VM relèvent de la qualification d’intégration. [Banc de politique](../tests/services_politique.rs)
