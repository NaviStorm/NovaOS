# NovaFS P5 — audit de compatibilité avant implémentation

**Base examinée** `b865c58`, NovaFS v0.9.5. **Décision en attente** pour la v0.9.6. Une référence à un bloc partagée entre deux LV change qui a le droit d’en disposer, ce que désigne l’identité du LV et quelles données un opérateur doit conserver pour vérifier une sauvegarde.

## Risque de perte de données

Prenons un pool P0 avec un seul grain attribuable et une origine A. Un futur snapshot S retient une page de A. Si S est enregistré seulement dans le FS ou dans un fichier annexe, le catalogue extérieur reste un catalogue P0 valide. Un gestionnaire v0.9.5 peut alors suivre ce chemin lors d’une réouverture administrative.

1. `Pool::import` valide les deux catalogues. Il peut importer le pool sans monter les systèmes de fichiers.
2. `remove(A)` supprime l’entrée. Le prédécesseur protège encore le grain.
3. `advance()` fait tourner les catalogues. Le grain devient attribuable.
4. Le provisionnement d’un autre LV remet le grain entier à zéro avant F1. C’est le comportement normal qui protège les données de l’ancien propriétaire lors d’une réattribution. Il détruirait aussi la page que S croit encore référencer.

Cette chronologie ne requiert ni panne ni écriture concurrente. Le mutex de P4 protège une instance en fonctionnement. Il ne peut pas imposer la connaissance d’une référence P5 à un autre binaire après fermeture. Le témoin sur périphérique simulé exerce le cycle de vie réel du gestionnaire P0. [Import, validation du reçu, effacement et suppression](../src/pool/manager.rs) · [Adresses et validation des deux catalogues](../src/pool/catalog.rs) · [Expérience reproductible](novafs-p5-preuve-p0.md)

Le même problème existe **à l’intérieur** d’un LV. Une page vivante en G, remplacée en G+1, reçoit l’âge de retrait G. Elle reste protégée lors de la préparation de G+2. À la préparation de G+3, le seuil passe à G+1 et la page peut être réallouée. Garder l’adresse et le hash du manifeste G permet de détecter par la suite qu’une page a changé. Pour pouvoir la relire, l’allocateur doit en connaître la rétention *avant* toute réutilisation. [Remplacement d’un extent](../src/fs/data.rs) · [Préparation et retrait](../src/engine.rs) · [Progression des âges](../src/space.rs)

## Contrats qui se rencontrent au niveau du pool

| Élément audité | Propriété vérifiée en P0–P4.1 | Conséquence pour P5 |
| --- | --- | --- |
| Enveloppes, arbres et extents | Le SHA-256 d’une page de données inclut les deux identités FS/LV, son numéro de bloc logique, sa naissance et ses octets. Les nœuds portent les mêmes identités dans leur enveloppe et leurs parents conservent une référence vérifiée. Le manifeste code cinq racines et exige un suffixe nul. Un extent contient jusqu’à 16 pages. | Un descripteur neuf portant les identités du clone ne rend pas valides les nœuds et les sommes de l’origine sous ces identités. Recopier les identités publiques ferait apparaître deux instances identiques. Il faut distinguer identité publique de vue et identité stable de contenu. [Codec](../src/codec.rs) · [B+Trees](../src/tree.rs) · [Extents](../src/fs/record.rs) |
| Catalogue et admission P3 | Chaque LV possède des grains physiquement disjoints et une garantie épaisse, y compris pool plein. Les tickets et les deux générations de catalogue excluent la double attribution. Le profil accepte quatre grains par domaine et seize LV. | Les pages retenues par un snapshot et les futures divergences d’un clone doivent entrer dans l’admission. Un clone peut partager ses premières pages et exiger dès sa création un crédit physique indépendant. Un domaine qui garantit déjà quatre grains ne peut promettre quatre grains supplémentaires. [Catalogue](../src/pool/catalog.rs) · [Garanties P3](novafs-p3-pool.md) |
| Publication et reprise P1 | Le FS écrit ses dépendances, effectue F1, remplace une ancre puis effectue F2. Le gestionnaire de pool possède sa propre paire d’ancres avec sa propre publication. | Sérialiser ces deux commits laisse un point de coupure entre eux. Une coupure peut alors séparer le changement de racine de vue du changement de propriété physique. Les deux doivent relever d’une seule décision durable pour les opérations de partage. [Publication FS](../src/engine.rs) · [Publication pool](../src/pool/manager.rs) · [Hypothèses de reprise](novafs-p1-durabilite.md) |
| Contrôle P2 | Une génération de FS partitionne exactement son espace. L’audit refuse une double possession dans la vue et compare aussi les références communes aux deux générations. `check` traite l’image autonome avec les identités attendues. `check-pool` contrôle les deux catalogues puis les volumes prêts de la tête. | Il faut vérifier l’union des racines protégées, y compris celles qui ne subsistent que dans le prédécesseur ou dans une collecte en cours. Le contrôle local d’un fichier extrait ne peut certifier les références et l’allocation physique du pool absent. [Contrôle FS](../src/engine/integrity.rs) · [Contrôle des vues](../src/fs/check.rs) · [Contrôle du pool](../src/pool.rs) |
| Cache et coordinateur P4 | Le cache est propre à un moteur. Le coordinateur garde une table de traduction par LV, admet des préparations simultanées et publie dans l’ordre FIFO. | La suppression doit protéger les lectures engagées. Un plan préparé avant une création de snapshot ou une collecte doit faire revalider ses réservations physiques avant publication. Les clés de cache doivent conserver le domaine, l’adresse, la naissance et l’empreinte pertinents. [Cache](../src/cache.rs) · [Coordinateur](../src/pool/coordinator.rs) |

## Arbitrage sur le support de sauvegarde et de vérification

| Choix | Ce qu’un opérateur peut vérifier après suppression de l’origine | Conséquence pour le format |
| --- | --- | --- |
| **A — Pool P5 comme unité de récupération (recommandé)** | Avec l’image du pool et l’identifiant persistant du clone, retrouver les références et les données de la vue. Une sauvegarde autonome de ce seul clone demande un export explicite qui matérialise son contenu et que l’on vérifie ensuite. | Introduire une version extérieure de pool rejetée par P0 et une version explicite des métadonnées de partage. Donner des identités publiques distinctes aux vues. Une identité de lignée continue de vérifier le contenu après disparition de l’origine. |
| **B — Image autonome de LV exigée comme contrat d’exploitation** | Un outil doit pouvoir recevoir le flux logique complet du clone et ses propres identités publiques pour contrôler le contenu, sans avoir le pool d’origine. La vérification des comptes physiques entre LV demande toujours le pool. | Spécifier un format d’image autonome, la traduction des références et le reçu de vue qui l’accompagne. Exiger en plus que ce flux soit *déjà* un flux P0 sous les nouveaux identifiants implique de reconstruire ses métadonnées de vérification. Ce travail dépend du contenu et doit être placé sur la voie d’export si la création doit rester indépendante de la taille. |

Avec A, sauvegarder le pool et le reçu de la vue suffit pour vérifier le clone à la génération attestée. L’export autonome est une opération distincte. Avec B, un outil de sauvegarde doit pouvoir lire le clone comme un LV logique complet et conserver ce flux et son reçu de branche. La restauration de ce flux doit être possible après perte du pool, sans export spécialisé préalable. Les deux options imposent une nouvelle version extérieure du pool pour protéger les blocs partagés, et une identité de branche dans le reçu pour distinguer deux clones initialement identiques. Les images autonomes P0 existantes gardent leur lecteur et leurs identités dans les deux choix.

## Proposition technique pour A, à valider

- **Format et migration.** Distinguer les ancres extérieures du pool P5 de celles du pool P0 sur les deux slots. Une paire mixte, une version obligatoire inconnue ou une filiation incertaine interdit le montage normal. Migrer hors ligne depuis une source P0 ouverte en lecture seule vers un support distinct. Vérifier la paire source, inventorier les LV, leurs types, identités, tailles et garanties, traiter explicitement les LV RAW et en préparation. Contrôler la destination et son contenu avant la bascule explicite. La correspondance entre le reçu source et le nouveau reçu de pool doit être conservée hors image. Un ancien reçu P0 seul ne certifie pas une racine P5.
- **Racine et références.** Une ancre de pool désignerait atomiquement le catalogue des vues, les racines de traduction CoW, les réservations et la collecte. Snapshot et clone ajoutent une référence de racine dans la même transaction. Les descripteurs ciblent leur contenu indépendamment du nom de leur parent. La suppression d’un snapshot peut donc retirer son nom pendant qu’un clone conserve la racine. Compter les arêtes persistantes entrantes vers les nœuds immuables et les plages de pages. Scinder une plage à la divergence, vérifier bornes, compte nul et débordement avant toute publication. L’analyse d’intégrité doit reconstruire les arêtes depuis les racines et les confronter au registre. Un compteur valide en apparence ne prouve pas la possession.
- **Admission et facturation.** Facturer à chaque vue sa taille logique, aux clones inscriptibles leur garantie de divergence, et au pool une seule fois chaque bloc réellement occupé. Les crédits nécessaires à la rétention des snapshots et à la progression de la collecte doivent rester disponibles. Distinguer dans les comptes la capacité garantie, les pages communes/exclusives, le coût d’une écriture, les blocs encore protégés par le prédécesseur et ceux dont la collecte entraînera une libération. Refuser avant F1 une promesse que le pool ne peut tenir, sans recopier les données.
- **Suppression et reprise.** Publier le retrait du descripteur et une obligation de collecte persistante dans la même génération. Traiter en lots bornés les dernières références, avec un curseur récupérable et une réserve de maintenance protégée à l’admission. Différer le réemploi si une ancre protégée ou un lecteur engagé peut encore atteindre le bloc. Après coupure, contrôler le graphe, les générations, les curseurs, les réserves et les références avant d’accorder un écrivain. Une réponse indéterminée à F2 impose un nouvel import.
- **Mémoire et validation.** À la géométrie maximale de 8 Gio, un compteur `u32` en RAM pour chacune des 2 097 152 pages coûterait 8 Mio. Un bitmap coûte 256 Kio. Préférer un index persistant par plages/nœuds et un contrôle par fenêtres avec mémoire bornée. Mesurer le coût CPU et les relectures du contrôle ainsi que le tas, les piles et les pointes de latence. Qualifier les 22 scénarios demandés et les deux géométries avant d’attribuer une version de livraison.

## Contrôles exécutés sur la base v0.9.5

Exécutions du 10 novembre 2026 sur le même Mac Apple Silicon, en profil release. Chaque colonne correspond à un binaire Darwin de l’architecture indiquée.

| Contrôle | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Suite par défaut | 95 réussis | 95 réussis |
| Suite avec toutes les sondes activées, incluant les tests communs | 96 réussis | 96 réussis |
| Deux tests optionnels lancés séparément, extent contigu et génération d’image de pool | 2 réussis | 2 réussis |
| Campagne différentielle optionnelle, 100 graines | 1 000 000 opérations en 25,062 s | 1 000 000 opérations en 40,445 s |
| Réattribution d’un grain P0 sur support simulé | Secteurs 512 et 4 096, 256/256 blocs nuls | Secteurs 512 et 4 096, 256/256 blocs nuls |

La campagne différentielle a observé sur chaque architecture 345 183 commits, 54 643 pénuries injectées et 1 000 remontages avec audit. Les trois tests ignorés par défaut ont ainsi été exécutés explicitement. Les durées correspondent à des campagnes concurrentes sur l’hôte et ne constituent pas une comparaison de performances des architectures.

Un second contrôle a exercé les commandes P0 sur image temporaire, avec les deux géométries et les deux binaires. Pour chacune des quatre configurations, `check` a accepté la paire d’identifiants correcte et renvoyé le code 1 après substitution du seul identifiant de LV. `salvage` a extrait les quatre octets attendus depuis le slot 1 avec la paire correcte et rejeté la substitution sans émettre de données. L’empreinte de l’image est restée identique. Ces seize invocations illustrent le contrat d’identité des outils existants.

Depuis la racine du dépôt, sur macOS avec les deux cibles Rust Darwin et Rosetta installées :

```sh
set -eu
travail=$(mktemp -d)
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    export NOVAFS_TARGET="$cible"
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --all-features
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    NOVAFS_POOL_FIXTURE="$travail/$cible.img" \
        novaFS/lab/run test --offline --test cout_extents --test volumes -- --ignored
done
```

Le [témoin de réattribution](novafs-p5-preuve-p0.md) donne les commandes de reproduction sur le commit exact. Le moteur reste en v0.9.5. Les tests de snapshots, de clones, de coupure et de corruption du futur format ainsi que ses mesures de latence, de débit et de mémoire restent à réaliser après l’arbitrage.
