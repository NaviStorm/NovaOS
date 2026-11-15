# Cache privé persistant des applications — v0.21.0, phase 4

## Autorités et durée de vie

Le lanceur décide si une instance peut démarrer, et le gestionnaire des temporaires lui attribue son `tmp` sur P9. L’ouverture du cache fait l’objet d’une décision distincte. Sur le profil ordinaire, une application sans identité utilisateur attestée peut exercer son `tmp` et reçoit `DENIED` pour le cache. Elle peut poursuivre son travail. Aucun compte par défaut n’est provisionné.

L’autorité utilisateur doit attester un identifiant opaque de 128 bits. L’autorité d’installation doit attester un second identifiant de 128 bits, stable pour la lignée installée. Le courtier associe la paire au processus admis et écrase les champs de routage avant transmission à NovaFS. Le nom du programme, son chemin, une chaîne cliente, le PID et le BootID ne prouvent aucune de ces identités. La qualification embarque sa propre table de provisionnement dans les profils de test. La production attend un fournisseur d’attestations. [Courtier](../kernel/src/sched/process/cache.rs)

| Événement | Preuve exigée pour ouvrir le cache |
|---|---|
| Nouvelle instance, même installation | Attribution du processus au même utilisateur attesté et à la même installation. Délivrance d’une capacité propre à cette nouvelle instance. |
| Mise à jour | Confirmation de continuité par l’autorité d’installation et attribution au même utilisateur. |
| Réinstallation | Nouvelle vérification explicite des deux identités. En l’absence de cette preuve, refus et conservation de l’ancien cache sans attribution. |
| Restauration | Vérification externe de l’origine du volume, de la continuité de l’installation et du titulaire. Sur le profil de restauration non attestée, même une intention de purge valide reste intacte. |

Une restauration intégrale peut reproduire des octets, des identifiants et des sceaux parfaitement cohérents. Le service ne peut déduire la fraîcheur d’une telle image depuis ses seuls blocs. Un futur fournisseur devra attester l’origine du montage et documenter comment il conserve la continuité hors de l’image restaurée. Le responsable de l’installation et l’autorité utilisateur devront définir la rotation et la révocation de leurs attestations avant d’ouvrir le cache à des applications ordinaires.

## Supervision, stockage et capacités

Le gestionnaire de services lance le service 10 après la disponibilité du service 9. NovaFS, son chemin bloc et la création des temporaires fonctionnent en amont. Tant que la production ne dispose pas d’une autorité d’enrôlement, sa récupération laisse intacts les objets du magasin. Le service 10 démarre sans temporaire, demande la récupération à l’écrivain P9 et annonce ensuite `READY`. Le gestionnaire peut l’arrêter et le relancer. La perte du service invalide les vues qu’il supervisait. [Catalogue](../user/services/catalog.rs) · [Service](../user/services/cache_actor.rs)

Le courtier utilise le sélecteur additif `cache::CONTROL` du registre ABI v1. Les appels historiques aux fichiers et aux temporaires reposent sur d’autres autorités. Les étendre pour transporter librement des clés utilisateur ou installation donnerait à leurs clients un pouvoir d’attribution. Le courtier vérifie au contraire la provenance au point d’entrée, associe chaque opération à un ticket privé et réutilise le réveil IPC de l’unique écrivain NovaFS. Une vue ouverte fournit un espace de noms privé de fichiers plats. Le protocole accepte des noms UTF-8 de 1 à 48 octets et des fragments de données d’au plus 64 octets. Le backend rejette les séparateurs, les caractères de contrôle, les composants `.` et `..` et les opérations sur un objet du mauvais type. Les deux types d’objets natifs de P9 sont fichier et répertoire. [Registre](registre-abi.md) · [Contrat filaire](../abi/cache.rs) · [Interface côté NovaFS](../user/novafs/files/cache.rs)

La paire attestée désigne un répertoire réservé. L’OID P9 de ce répertoire distingue chaque incarnation du cache. Deux processus autorisés pour la même paire obtiennent des jetons distincts pour ce même OID. Le noyau vérifie l’instance appelante, le BootID, le numéro de capacité et la génération. Retirer un processus révoque sa vue. La purge révoque toutes les vues de sa portée. L’API expose exclusivement des opérations relatives à la capacité. Les données de cache sont reconstructibles. Les documents utilisateur restent dans le catalogue public.

### Appel d'une application

L'appelant présente au sélecteur `CONTROL` l'opération en premier argument, zéro en deuxième argument, et en troisième argument l'adresse de sa trame privée de 224 octets. Pour `OPEN`, initialiser toute la trame à zéro. En cas de succès, conserver les 32 octets de la génération, de la capacité et du BootID. Pour `READ` et `WRITE`, recopier ces octets dans une trame neuve, remplir les longueurs, le nom, l'offset et, pour une écriture, le fragment de données. Les offsets et les codes exacts proviennent du module partagé. Un client doit vérifier le statut avant d'exploiter les octets de réponse. Une opération de cache ne confère pas un handle VFS général.

L'autorisation de consultation du gestionnaire permet d'interroger l'état du service sans provoquer d'éviction. La maintenance périodique est interne au service. Une demande explicite d'entretien passe par le port d'administration. [Protocole de supervision](../user/services/protocol.rs) · [Exemple d'utilisation par un client isolé](../user/services/cache_actor.rs)

## Purge et récupération

Pendant le prêt d’un canal VFS à une commande de nsh, l’écrivain continue de servir le temporaire et le cache, par lots bornés. La restitution du prêt remet le canal public au lanceur.

À l’admission de la purge, le courtier interdit toute ouverture conflictuelle, retire les vues et annule les opérations encore en file. P9 termine les opérations déjà remises avant d’effectuer le retrait demandé. Un acquittement confirmé conserve son résultat. Une interruption après remise d’une écriture dont le résultat reste indéterminé produit `UNCERTAIN`. Le client doit traiter cette incertitude sans répétition automatique. Une nouvelle ouverture après purge résout un autre OID. Une écriture liée à l’ancienne génération ne peut ainsi cibler le répertoire recréé.

Sur un volume enrôlé, l’écrivain vérifie le sceau du magasin, le marqueur de la paire et le type des descendants avant de publier l’intention de purge. Il renomme le marqueur actif en tombe avec la publication CoW habituelle de NovaFS. La reprise peut poursuivre un retrait interrompu avec cette tombe et la génération correspondante. Une provenance incohérente entraîne la quarantaine. Les sceaux structurels relient les identifiants FS/LV, le répertoire, le marqueur et le propriétaire. Ils n’authentifient pas un opérateur disposant d’un accès brut en écriture au volume. [Magasin P9](../user/novafs/files/cache_store.rs)

Pour les purges d'utilisateur ou du système, le registre conserve aussi une intention durable avec la portée et la borne des générations admises. Le service valide les marqueurs avant de publier cette intention. Une incohérence découverte après sa publication conserve le gel de la portée incertaine. Une incarnation créée ultérieurement est exclue de cette portée. Un premier `WRITE` sur un nouveau nom prépare une entrée inaccessible au client puis publie par renommage après l'écriture des octets. Une coupure de VM se traduit ainsi par un nom absent ou un fichier complet pour le fragment confirmé. Chaque appel `WRITE` a sa propre confirmation. Un fichier constitué de plusieurs appels n'est pas une transaction globale.

## Quotas et pression

Les quatre paramètres de la politique sont le plafond logique d’une paire, celui de l’utilisateur, le budget logique global et le nombre de blocs physiques réservés. La facturation comprend une page par incarnation, une page par nom et la longueur logique du fichier. Un fichier creux reste facturé sur sa longueur logique. Le service contrôle aussi le solde physique après préparation de l’écriture CoW. La politique embarquée se calcule depuis la capacité du volume et sa réserve de maintenance. Le service 10 peut fournir une politique explicite lors de sa récupération.

Les blocs du moteur font 4 096 octets sur les deux géométries de secteur. Le profil réserve en plus du budget de maintenance P9 la place d'un plan CoW de 64 blocs. Il attribue au budget logique global la moitié de la capacité restante après ces réserves, au plafond d'un utilisateur la moitié du budget global et à celui d'une paire le quart du budget global. Une politique explicite doit conserver l'ordre des plafonds et une réserve physique positive. L'admission des nouvelles données est suspendue si un marqueur empêche d'établir une comptabilité sûre.

Lorsque le solde physique devient inférieur à la réserve augmentée du budget de préparation CoW, le service de maintenance interroge P9 et reçoit une proposition de victime. Le courtier gèle et révoque la portée correspondante. P9 vérifie à nouveau la pression et la génération avant de retirer les données. Les purges de l’utilisateur et du système requièrent une autorité administrative distincte. Le profil ordinaire de `nsh` ne la reçoit pas.

## Qualification

Le banc démarre le code invité sur un disque VirtIO brut P9. Il conserve le noyau, les ISO, les séries et les images comprimées de chaque étape. Un lecteur hôte remonte en lecture seule, vérifie le moteur P9, compare les OID et les octets des caches et contrôle le catalogue public. L’interruption par SIGKILL exerce l’arrêt de QEMU sur un hôte qui conserve son alimentation. [Banc invité](../user/cache_client_test.rs) · [Pilote QEMU](../tests/cache_prive.py) · [Oracle indépendant](../tests/tmp_prive_oracle.rs)
