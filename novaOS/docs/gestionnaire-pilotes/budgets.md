# Budgets prévisionnels et admission

**Hypothèse de travail.** Une machine de laboratoire de 256 Mio, des pages de 4 Kio, un gestionnaire, un contrôleur et un groupe candidat. Trois grandeurs figurent au bilan de chaque future exécution : la réservation maximale avant admission, le pic résident observé et les ressources encore retenues après clôture. Les nombres de conception ci-dessous permettent de refuser un plan trop grand. Ils ne mesurent pas un gestionnaire en fonctionnement.

## Mémoire et métadonnées

| Réserve commune proposée | Quantité × taille | Octets |
|---|---:|---:|
| Révisions de paquets résidentes dans l’index | 64 × 128 | 8 192 |
| Fonctions ou interfaces observées dans l’instantané | 128 × 64 | 8 192 |
| Descriptions de groupe et de bail | 32 × 256 | 8 192 |
| Propriétaires de contrôleur | 8 × 256 | 2 048 |
| Références de dépendance dans le plan | 64 × 32 | 2 048 |
| Reçus d’opérations et empreintes de requête | 64 × 128 | 8 192 |
| **Sous-total des tables** | **9 pages** | **36 864** |
| Lecture bornée d’un manifeste, réutilisée en série | 4 pages | 16 384 |
| **Réserve commune** | **13 pages** | **53 248** |

Les tailles sont des enveloppes de conception, à vérifier par layout sur les deux ISA. L’entrée groupe pointe vers le détail borné du profil, des pages et des opérations. Les listes physiques de cadres, tables de traduction, objets scellés et journaux persistants sont facturés séparément au propriétaire. Les reçus acquittés peuvent être compactés après inscription de leur plancher de rejeu. Réserver dans les 64 places au moins une place de clôture pour tout groupe engagé, plus les opérations déjà soumises et leur reprise. Si ce minimum ne tient plus, refuser l’admission suivante. Une topologie excédant la vue de 128 fonctions ne peut servir à déduire un groupe isolable avant preuve de complétude.

| Poste variable | Hypothèse pour prévoir l’admission |
|---|---|
| Processus communs | 64 Kio résidents par nouvel acteur commun pour son code, ses données, pile et tables CPU. Réviser à partir des segments et des allocations. Ne compter une autorité préexistante qu’à hauteur de son surcoût mesuré |
| Famille de pilote chargée | 64 Kio provisionnés pour le texte ELF. Si deux instances ne peuvent partager des pages effectivement immuables en lecture seule, compter ces pages pour chaque instance. Une deuxième version résidente a son propre coût |
| Instance du pilote | 128 Kio privés pour données, pile et tables CPU dans l’exemple du manifeste. Ajouter les ressources du service de classe et du client du banc s’ils sont créés. Le coût d’une référence compatible sur disque se limite à l’index avant apparition du matériel |
| Dispositif | Budget réel par rôles DMA, tables privées de traduction, files et historique. Sur 3E ARM, les 22 cadres du NIC coûtent 88 Kio. Sur le Q35 double, les 26 cadres privés du NIC coûtent 104 Kio, les huit ancres communes 32 Kio et les six cadres NVMe 24 Kio. Le superviseur garde un inventaire disjoint des cadres |
| Transport et staging | Paires de contrôle en DATA. Les deux boîtes Ethernet nominales représentent 8 Kio dans le pool IPC. Pour la préparation, ajouter la taille complète des objets immuables et copies simultanées : exemple maximal d’un payload `pkg01` de 4 Mio et d’une deuxième copie privée de 4 Mio, plus enveloppes. Installer en série et distinguer cache disque et RAM épinglée |
| Faute et quarantaine | Un cadre actif qui passe en quarantaine change d’imputation. Toute rétention d’une génération précédente sur un autre groupe réduit la marge disponible. Sur ARM, une preuve de file manquante peut retenir 14 cadres NIC + 9 de contrôleur, une preuve IOMMU manquante 22 + 9. Une nouvelle instance du même groupe attend la résolution |

**Exemple calculable en régime actif.** Pour quatre nouveaux acteurs communs (gestionnaire, découverte, catalogue et lanceur), une famille et une instance ARM, réserver `52 + 4 × 64 + 64 + 128 + 88 = 588 Kio`. En ajoutant le client et le service de classe à 64 Kio chacun, les 16 cadres de stockage et de contrôle observés à côté du NIC en 3E ARM et le maximum du pool IPC de 128 pages, on obtient `588 + 128 + 64 + 512 = 1 292 Kio`. Ce périmètre comprend la totalité du pool IPC une seule fois. Le système de base, ses processus de stockage, les métadonnées supplémentaires du superviseur, le staging et les réserves de panne hors de cette liste restent à chiffrer avant admission réelle. Pour une politique avec réserve de panne, imputer les cadres supplémentaires retenus au pic simultané par identité physique. Publier les pics RSS et les cadres uniques séparément. [Ressources effectivement mesurées](../architecture-ethernet/phase3e.md) · [Modèle de capacité Ethernet](../architecture-ethernet/capacite.md)

## Comptabilité IPC

Un contrôle demande/réponse peut utiliser deux endpoints, un dans chaque sens. Dans la proposition ci-dessous, chaque paire a un créateur clairement désigné, un seul pair et zéro boîte. Le lanceur provisionne les feuilles dans les deux tables avec les droits de direction. C’est un plan pour **un groupe actif**. En présence d’acteurs existants, inventorier leur consommation totale et refuser toute paire qui dépasse leur budget, même pendant la préparation.

| Paire de contrôle | Créateur des deux racines | Autre participant |
|---|---|---|
| Découverte ↔ gestionnaire | Découverte | Gestionnaire |
| Catalogue ↔ gestionnaire | Catalogue | Gestionnaire |
| Superviseur ↔ gestionnaire | Façade de contrôle du superviseur | Gestionnaire |
| Lanceur ↔ gestionnaire | Gestionnaire | Lanceur |
| Service de classe ↔ gestionnaire | Gestionnaire | Service de classe |

Cette topologie ajoute **10 endpoints, 10 racines réparties et 0 boîte**. Le gestionnaire détient 4 racines et 6 feuilles, soit 10 handles. Découverte et catalogue créent chacun 2 racines. La façade de supervision, supposée disponible dans cet exemple, en crée 2. Une façade isolée supplémentaire ajoute son propre processus, ses pages et sa liaison interne au bilan. Une racine détenue par le noyau pour le compte d’un acteur est imputée à son véritable créateur. Le service Ethernet conserve ses 4 racines et reçoit 2 feuilles de contrôle. Le client et le pilote du montage 3E gardent respectivement 0 et 1 racine. Le lancement d’un second groupe ou d’un second service doit présenter un nouveau calcul par propriétaire. Le partage d’une paire entre autorités indépendantes serait incompatible avec son authentification. [Création des liaisons du laboratoire](../../kernel/src/sched/process/ipc/liaison_ethernet.rs)

| Limite réelle | Vérification avant tout effet |
|---|---|
| 48 sélecteurs natifs, 4 racines par acteur | Contrôle par messages DATA. Vérifier par acteur même pendant l’installation, le remplacement et la collecte. Un acteur déjà à quatre peut recevoir une feuille si sa table a une place |
| 8 boîtes globales de 4 Kio | Si six autres boîtes sont réservées, les deux liaisons Ethernet peuvent prendre les dernières. Avec huit déjà occupées, négocier DATA à l’ouverture si les deux pairs l’acceptent. Le contrôle et le retrait restent utilisables. Chaque session conserve son transport et ses huit crédits maximum |
| 64 endpoints, 256 nœuds, 32 handles par table, 32 attentes, 8 dépôts par endpoint | Le montage Ethernet existant à 5 endpoints et 2 boîtes, plus les 10 endpoints proposés, donnerait 15 endpoints hors autres utilisateurs. Vérifier les dérivations, les slots locaux, les attentes de retrait et les recomptages globaux avant de publier |
| 32 slots d’ordonnancement/processus dans le profil de référence, budget IPC de 128 pages | Réserver les processus/threads et le budget physique réellement disponibles. Rendre `SATURE` avant tout effet si la reprise d’une opération ou la clôture du groupe ne dispose plus de place. Les files d’invalidation partagées ont leurs propres bornes |

Les deux emplacements de demande du Q35 expérimental ne démontrent pas la capacité d’une file générale. Un contrôleur dont le registre de reçus ou la file est plein conserve la progression des retraits déjà engagés. L’énumération utilise des pages de résultats bornées. Un client lent relit depuis son curseur ou reprend un instantané. [Pools IPC](../../kernel/src/sched/process/ipc/mod.rs) · [Huit boîtes](../../kernel/src/sched/process/ipc/bulk.rs) · [Ordonnanceur](../../kernel/src/sched/mod.rs) · [ABI native](../../abi/syscalls.toml)

## Budgets temporels à éprouver

Cibles de travail proposées pour QEMU TCG, 1 vCPU, matériel et catalogue résidents, profil fixé avant mesure. Le chronométrage distingue temps CPU actif, temps écoulé, appels IPC, écritures matérielles et temps en attente de reçu. Rapporter séparément les démarrages froids, l’installation/signature, le chemin nominal et la quarantaine.

| Opération, début → fin | Cible p95 proposée | Dépassement |
|---|---:|---|
| Découverte d’une topologie de laboratoire ≤ 32 fonctions visibles, première lecture → instantané complet | 100 ms | Interrompre à la borne de lectures du lecteur, publier une vue incomplète et interdire la réservation dépendante |
| Activation d’un groupe déjà vérifié, demande de réservation → publication du service | 250 ms | Identifier vérification du plan, lancement, init matérielle et publication. Toute attribution partielle doit suivre le retrait |
| Retrait nominal, clôture des soumissions → reçu de restitution complet | 500 ms | Rapporter dernière étape et ressources retenues. La disponibilité du gestionnaire est mesurée séparément du délai matériel |
| Récupération du gestionnaire, processus prêt → inventaire réconcilié d’au plus 32 groupes | 1 000 ms | Signaler les groupes sans propriétaire joignable. Le temps pour obtenir une preuve manquante peut être illimité |
| Réconciliation avec preuve déjà disponible, arrivée du reçu → consommation et mise à jour du registre | 100 ms | Conserver l’opération consultable et les cadres épinglés jusqu’à consommation |

Ces cibles servent à évaluer un futur prototype. Les échéances de fonctionnement viennent du profil matériel. Aucune de ces durées n’autorise un reset, un succès de transmission ou une restitution. Prévoir au moins trois démarrages indépendants par ISA, 256 cycles nominaux par profil si le banc le permet, annoncer l’effectif, les médianes par démarrage, p95 et maxima. Mesurer recherche avec 1 puis 64 paquets, et 1 puis 32 groupes, en distinguant inventaire seul et groupe actif admis. Un refus de capacité est un résultat du test.
