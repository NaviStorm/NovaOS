# NovaFS v0.9.6 — snapshots et clones

## Autorité de récupération

Le contrat A de l’[audit de compatibilité](novafs-p5-audit-compatibilite.md) est retenu. Une vue est désignée par la paire identifiant du pool et numéro de vue. Le pool publie des numéros strictement croissants. La lignée de contenu conserve les identités FS et LV historiques utilisées dans les enveloppes et sommes P0. Deux clones peuvent ainsi partager la même racine sans avoir la même identité publique. La suppression d’un objet retire son descripteur. Ses descendants référencent directement leur racine.

Conserver hors du pool le reçu du dernier état que l’application veut retrouver. Son empreinte SHA-256 couvre, dans l’ordre, le domaine ASCII `NovaFS.P5.receipt.v1`, les 16 octets de l’identifiant du pool, le numéro de vue u64 LE, le propriétaire sur 16 octets, les identités FS et LV sur 16 octets chacune, la capacité en blocs u64 LE, le type sur un octet (origine 1, snapshot 2, clone 3) et le hash de sa racine de traduction. Les modifications d’une vue tierce laissent ce reçu valide. Le reçu détecte une substitution d’objet ou de contenu. Pour détecter un remplacement du pool par une sauvegarde antérieure, l’opérateur doit connaître indépendamment l’état attendu. Les sommes publiques protègent contre les corruptions accidentelles. L’administrateur de l’image peut en fabriquer de nouvelles.

Une copie du flux logique et du reçu ne contient pas le catalogue du pool qui attribue les vues. Pour assurer une restauration indépendante, utiliser l’[export complet](novafs-p5-export.md). La restauration réattribue une identité publique dans le pool de destination et vérifie le contenu.

## Création et divergence

`SharedPool::snapshot` accepte une origine ou un clone. `clone_snapshot` accepte un snapshot et le domaine auquel imputer la nouvelle garantie. Les deux opérations insèrent un descripteur qui référence la racine immuable existante. Leur publication écrit un catalogue de taille fixe et un sélecteur. Une racine déjà retenue s’enregistre en mémoire sans parcours des fichiers ni des feuilles. Le travail d’admission dépend du nombre de descripteurs, limité à 16. La libération d’une ancienne génération peut également intervenir dans cette publication.

Les écritures utilisent les B+Trees, les extents, les quotas logiques et les réservations de maintenance P3 du moteur de contenu. Une opération constitue une image privée des nouvelles pages, puis le pool attribue des pages physiques qui n’appartiennent à aucune des deux générations protégées. Il réécrit les feuilles de traduction touchées et la racine. La publication sur le périphérique appartient au pool. Une erreur avant publication abandonne aussi le plan de contenu. La déchirure du sélecteur et la perte de la réponse à F2 sont traitées suivant le [format de pool](novafs-p5-format.md).

Une page interne qui sort des deux générations du contenu perd son entrée de traduction dans la même transaction. Une page logique nulle peut être représentée par une absence de référence. Une lecture de cette adresse produit 4 Kio nuls. Les fichiers creux conservent ainsi leurs trous. Le moteur refuse toute mutation d’un snapshot. L’API publique du pool permet seulement la lecture d’une vue brute.

## Propriété et restitution

Les arêtes persistantes du catalogue et des cartes de traduction définissent la propriété physique. L’import vérifie les cibles et reconstitue un index en mémoire des parents distincts. Une autre référence à une racine existante augmente son nombre de références sans dupliquer ses descendants. Une adresse physique commune doit conserver le même rôle, la même position logique, la même lignée et la même empreinte. Les deux générations de catalogue restent protégées, même après suppression de l’origine ou du dernier snapshot. La suppression ne demande aucun nouveau bloc de données ou de traduction. La zone réservée de publication reste disponible quand les crédits du pool sont épuisés.

La prochaine publication, ou `advance` demandé explicitement, retire la génération précédente. Quand la dernière arête disparaît, le gestionnaire peut réattribuer le bloc. Ce travail en mémoire se limite aux arbres qui cessent d’être utilisés. Une vue entière couvre au plus 1 024 pages de contenu, 16 feuilles et une racine. L’import reconstruit les références après interruption, sans journal persistant de décréments ni balayage des blocs libres. Les octets d’une préparation abandonnée peuvent être présents sur le support jusqu’à leur réutilisation. Une nouvelle affectation écrit et contrôle le bloc entier avant de le rendre accessible.

## Crédits physiques et coût logique

| Mesure | Unité et sens |
| --- | --- |
| Capacité logique de la vue | De un à quatre grains de 256 blocs de 4 Kio. La somme des tailles logiques de fichiers vivants d’une vue inscriptible, trous compris, est plafonnée à sa capacité en octets lors d’une croissance. |
| Exclusif / partagé | Pages physiques de données et de traduction atteignables depuis cette vue, ventilées selon qu’une autre vue courante les atteint également. |
| Occupé | Cardinal de l’union des graphes des deux catalogues, plus les sept pages fixes du pool. Une page partagée entre plusieurs vues compte une fois. |
| Différé | Pages de l’ancien catalogue qui ne sont plus atteignables dans la tête. Après rotation du catalogue elles peuvent être récupérées. |
| Crédit d’admission | Pour chaque vue inscriptible, capacité maximale des pages de contenu et de traduction. Pour les snapshots, poids physique de chaque racine distincte de snapshot. À ces crédits s’ajoutent la zone fixe et, si le catalogue est occupé, une réserve de 164 pages. |

Deux racines de snapshots différentes peuvent partager des feuilles. Le crédit de chacune réserve alors leur poids complet. Ce choix conservateur réduit les créations admises près de la limite. Le compte *occupé* continue de dédupliquer les adresses. Après une suppression, les pages différées peuvent appartenir à une vue dont le crédit a déjà disparu de la tête. Elles restent incluses dans l’occupation jusqu’à la rotation. Un domaine peut garantir au total 1 024 pages logiques inscriptibles. La facturation des snapshots reste au niveau des crédits physiques du pool. Un clone acquiert toute sa garantie à sa création, même si son coût de copie de données est nul.

Deux générations protègent jusqu’à 82 pages chacune pour une divergence ordinaire. Le majorant comprend 64 pages de plan P3, une ancre de contenu, 16 feuilles et une racine. Le coût réel dépend des pages modifiées et des feuilles touchées. Une importation de volume est admise séparément et doit pouvoir matérialiser sa nouvelle image avant publication. La suppression et la rotation disposent de leurs emplacements fixes. Le crédit ne se soustrait pas une seconde fois à l’occupation pour calculer l’usage physique.

## Migration explicite

Un pool P5 neuf et une source distincte ouverte en lecture seule sont nécessaires pour migrer un pool P0. La migration vérifie et relie les catalogues historiques, conserve propriétaires, lignées et tailles des LV prêts, puis valide les deux générations de chaque contenu source. Elle matérialise les pages protégées et vérifie leur contenu et le reçu interne obtenu sur la destination. Tous les descripteurs deviennent accessibles par une seule publication. Avant F1, n’importe quelle copie partielle reste hors du catalogue. Après une réponse incertaine, réimporter la destination et vérifier son inventaire et ses reçus. Une destination encore vide permet de relancer la migration. La source reste disponible pour vérifier les identités, les noms et les octets avant bascule.

Les pools contenant des LV bruts ou en préparation conservent leur chemin de lecture historique. Une tentative de migration de cet ensemble est refusée avant copie. La migration d’un LV autonome vérifiable est également disponible dans la géométrie P5. Les volumes qui excèdent quatre grains conservent le lecteur historique. L’outil ouvre ses fichiers sources hors ligne sur un flux. Diriger la sortie vers **un nouveau fichier distinct** évite que le shell tronque la source.

## Cache, concurrence et limites

Les lectures ordinaires réutilisent le cache vérifié P4 de la dernière racine montée. La clé comprend la racine, la lignée et la géométrie. L’index de traduction vérifié permet d’aller directement à la page physique, dont le hash reste contrôlé sur un défaut de cache. Le montage strict, la migration et l’export relisent les pointeurs sur le support. Le changement de racine après publication rattache le cache à un nouvel état validé. L’écrivain peut restituer le montage de contenu et ses pages propres via `reclaim_cache`. La lecture suivante remonte la racine et reconstitue le cache à la demande. Une panne d’intégrité détectée ferme l’accès par le montage concerné. Un remontage strict de la source vérifie les deux générations.

Le périphérique doit être détenu par un seul écrivain. La bibliothèque ne verrouille pas deux descripteurs ouverts indépendamment sur la même image. L’emprunt exclusif du pool sérialise les publications et interdit de supprimer une vue empruntée en lecture. Le coordinateur utilise la file FIFO P4 pour des clients concurrents, avec 16 requêtes admises et au plus deux préparations privées. Une préparation est liée à l’identité et à la racine de sa vue. La publication vérifie de nouveau cette base et alloue sous l’autorité du catalogue courant.

Le profil conserve un seul disque, un maximum de 16 objets, une taille de page de 4 Kio et les deux géométries de secteur. La limite RAM est mesurée sur les charges du [rapport de validation](novafs-p5-validation.md). L’index mémoire évolue avec les pages matérialisées et protégées. Les mesures sur fichier Darwin et sur simulateur portent sur ces environnements. La durabilité sur coupure électrique dépend de la chaîne de stockage et de ses caches.
