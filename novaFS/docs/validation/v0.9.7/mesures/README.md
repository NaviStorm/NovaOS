# Lire les mesures sur fichiers Darwin

Les 72 exécutions utilisent des images temporaires de 32 Mio et le même fichier applicatif de 512 Kio. Chaque cellule désigne une architecture, une révision, une géométrie, un régime de cache, une topologie, une taille, un ordre et une opération. Les exécutables P4.1 utilisent le coordinateur de volumes P4.1 pour l'origine seule. Les deux versions P5 utilisent le coordinateur du pool partagé pour les quatre topologies. Le cas « trois » conserve trois générations distinctes grâce à des modifications du fichier auxiliaire.

| Archive | Contenu |
| --- | --- |
| [Échantillons](echantillons.csv.gz) | 62 208 opérations avec indice et offset dans le fichier. Les durées sont en nanosecondes. |
| [Distributions](distributions.csv.gz) | 5 832 lignes de statistiques par campagne et pour les deux campagnes réunies. |
| [Comparaisons](comparaisons.csv.gz) | 1 080 comparaisons entre versions. Minimum, médiane et maximum des deux ratios appariés de campagne pour chaque cellule. |
| [Exécutions et provenance](campagnes.json) | Conditions hôte, étalonnages CPU, SHA-256 des six exécutables et empreintes de leurs sources, mémoire et dates UTC par exécution. Les six premières exécutions de référence conservent leur date de fin. |

Chaque lecture rend les octets attendus par l'oracle. Chaque mutation change les données. La première passe modifie huit plages distinctes, partagées quand un snapshot ou un clone existe. La seconde passe réécrit ces mêmes plages dans la vue modifiée. Une génération de récupération peut encore protéger l'ancien bloc. Le contrôle final relit le fichier et les vues survivantes. La préparation du tampon applicatif, le contrôle des octets et la sortie CSV se font hors chronométrage.

Dans la distribution groupée d'une cellule, les 64 lectures correspondent à quatre passes de huit lectures par campagne. Les 16 divergences ou réécritures correspondent à huit mutations par campagne. Chaque quantile utilise le rang supérieur. Le p99 groupé est donc le maximum observé. Pour les 16 mutations, le p95 groupé est aussi le maximum. Les sommes des colonnes en nanosecondes et des compteurs d'E/S se divisent par `n` pour obtenir des moyennes par opération. Le débit utilise les octets applicatifs divisés par la somme des durées murales.

Les colonnes `lectures_*` et `ecritures_*` comptent des appels réussis au backend de 4 Kio. La charge utilise des données dont le préfixe se distingue des signatures des quatre familles de pages. La colonne `prepare_ns` est une partie du temps mural. Les durées des lectures, écritures et deux barrières du backend sont disjointes. Le CPU du thread recoupe ces durées. `F1_ns` et `F2_ns` sont déjà compris dans `temps_flush_ns`. Le nombre de barrières a été vérifié pour chaque échantillon.

## Effet de la topologie sur les lectures P5.1

Chaque valeur divise le débit de la topologie indiquée par celui de l'origine seule, puis prend la médiane de 24 rapports correspondant aux deux campagnes, deux secteurs, deux ordres et trois tailles. Les images et emplacements physiques peuvent différer entre topologies. Le résultat exprime le débit du chemin complet sur fichier image.

| Architecture | Cache | Un snapshot | Trois snapshots | Clone |
| --- | --- | ---: | ---: | ---: |
| ARM64 | Chaud | 1,012 | 1,017 | 1,039 |
| ARM64 | Froid | 1,016 | 1,023 | 1,010 |
| ARM64 | F_NOCACHE | 0,990 | 0,816 | 0,932 |
| Rosetta | Chaud | 0,996 | 0,996 | 0,998 |
| Rosetta | Froid | 1,010 | 1,009 | 0,985 |
| Rosetta | F_NOCACHE | 1,028 | 0,915 | 0,999 |

En régime chaud, les plages mesurées ont été relues avant chaque passe. En régime froid, les caches NovaFS ont été restitués et un accès de zéro octet a installé le montage avant chaque passe. En régime `F_NOCACHE`, le banc utilise aussi le réglage Darwin du descripteur dès la création du fichier. Les caches du contrôleur et les temps d'attente dans la pile d'E/S de l'hôte restent hors du contrôle du laboratoire.
