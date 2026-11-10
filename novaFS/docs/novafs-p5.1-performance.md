# NovaFS P5.1 · performances du pool partagé

## Origine du surcoût

Le témoin historique alterne 128 écritures de 16 Kio avec la relecture immédiate des mêmes octets, sur quatre plages et un seul client FIFO. Le chiffre de 109,37 Mio/s en P5, contre 140,23 en P4.1, décrit ce chemin particulier sur ARM64 en secteurs de 4 096 octets. Le rejeu des deux commits durant P5.1 donne respectivement 31,04 et 40,45 Mio/s. L'écart de débit absolu entre sessions impose de comparer les versions dans une même campagne. Le comptage des blocs, lui, se reproduit exactement.

| Pour 128 écritures de 16 Kio | P4.1 | P5 |
| --- | ---: | ---: |
| Données écrites | 512 | 508 |
| B+Trees, manifestes et ancrages internes du FS | 640 | 640 |
| Feuilles et racines de traduction du pool | 0 | 256 |
| Catalogues et sélecteurs publiés | 0 | 256 |
| Total de blocs de 4 Kio | 1 152 | 1 660 |
| Amplification physique | 2,250× | 3,242× |
| Synchronisations du périphérique | 256 | 256 |

La première charge applicative est nulle. P5 représente ses quatre pages par des trous. À chaque transaction, les mises à jour logiques du moteur de contenu sont regroupées en mémoire. Le pool construit une feuille de traduction, sa racine, un catalogue et un sélecteur pour le cas mesuré. Ce sont quatre pages écrites supplémentaires par mutation, y compris sur une origine seule. F1 stabilise les nouvelles pages et le catalogue, F2 confirme le sélecteur. Les flushs de l'overlay du FS interne portent sur sa mémoire privée. Sur un fichier plus vaste, modifier plusieurs feuilles ou scinder un arbre peut augmenter le coût.

Avant modification du moteur, les sondes du pool et du FS ont été appliquées à des **copies temporaires** des sources. Pour 16 lectures de 16 Kio déjà chaudes sur une origine ARM64/4 096, le profil initial a compté 1 032 allocations, 67 contrôles de métadonnées et aucune lecture du périphérique. Il consommait environ 71 µs CPU par appel. Les recherches de chemin, le décodage des objets et la recherche d'extent se répétaient pour un contenu immuable. Lors de la première passe de lecture suivant le lot de 16 écritures, le profil a relevé quatre lectures physiques de données par appel, auxquelles s'ajoutent les défauts de métadonnées. Le pool vérifiait l'empreinte physique et le FS vérifiait l'empreinte logique. Ces octets venaient déjà d'être générés, relus et contrôlés pendant la publication. Les lectures ordinaires montées n'accèdent pas au catalogue sur disque à chaque traduction.

Pour la mutation de 16 Kio du profil initial (jeu distinct du témoin), les 14 pages physiques moyennes se décomposent en quatre données, cinq métadonnées FS, deux feuilles, une racine de traduction et deux pages de publication. Les 25,19 lectures et 239,19 allocations moyennes incluent préparation FS, lecture des anciennes cartes, construction CoW et contrôle des nouvelles pages. Les durées murales de F1 et F2 incluent l'attente du backend. Les portées CPU de résolution, hachage, préparation et publication s'emboîtent et ne peuvent être additionnées. Les compteurs d'E/S désignent les appels achevés au fichier image, sans visibilité sur les octets finalement écrits par le SSD.

[Profils instrumentés](validation/v0.9.7/profil/) · [Rejeu non instrumenté des versions historiques](validation/v0.9.7/mixte/references/)

## Modifications en mémoire

- Le montage P5 mémorise un seul chemin validé, son objet et le dernier extent, avec au plus 1 024 octets de chemin et 16 empreintes. L'identité comprend la racine, la lignée FS/LV et la géométrie. Une mutation abandonne le curseur. Deux vues qui divergent ne peuvent donc pas utiliser le même état. La résolution d'un chemin différent reste soumise à toutes les règles du FS. Chaque lecture de données passe par le contrôle du cache de pages et la vérification du suffixe de fichier.
- L'index du graphe utilise les positions des enfants validés à l'import ou à la publication. Un masque de 64 bits par entrée en mémoire permet de trouver l'indice compact d'un enfant par comptage des bits précédents. La recherche P5 faisait plusieurs recherches dans le dictionnaire des blocs à chaque étape du parcours binaire. Les deux positions du chemin sont maintenant calculées directement. L'audit et l'export relisent les pointeurs persistants. Le masque ne décide ni du partage, ni d'une réservation, ni d'une libération.
- Le plan FS conserve jusqu'à 17 références de données qu'il calcule déjà pour l'écriture. Après le succès de F2, le pool admet les pages correspondantes de son overlay privé dans le cache propre P4 du nouveau montage. La clé contient le LBA logique, la naissance et l'empreinte du FS. Le graphe a vérifié les octets effectivement relus sur le périphérique avant F1. Une page nulle suit la convention persistante de trou. Le cache borné peut refuser une entrée et se restitue complètement. Une erreur de publication interdit de servir le montage incertain. La vérification des pages nulles n'est effectuée qu'une fois à l'écriture.

Modifier en place un bloc apparemment exclusif compromettrait la génération précédente protégée, même sans snapshot nommé. Omettre la relecture des nouvelles pages, F1 ou F2 réduirait la couverture d'intégrité ou changerait le contrat de reprise. Ces optimisations ont été écartées. Aucun compteur en mémoire ne remplace une preuve de propriété pour le réemploi des adresses. Les algorithmes de réservation, la transition des catalogues, le coordinateur et les structures disque P5 conservent leurs règles.

[Cas de frontière de l'index](../src/shared_graph.rs) · [Tests du cache de publication](../tests/p51_publication.rs) · [Tests de chemin, de divergence et de corruption](../tests/p51_curseur.rs)

## Protocole comparatif

Le banc compile le même client release contre P4.1 (`b865c58`), P5 (`ae508ce`) et les sources P5.1. Cibles Darwin ARM64 native et x86_64 sous Rosetta sur le même Mac, macOS 27.2 (26B123), Rust 1.99.1. Les mesures utilisent des images temporaires de 32 Mio, des secteurs logiques de 512 ou 4 096 octets, un fichier de 512 Kio et un seul client. Les snapshots appartiennent aux deux versions du pool partagé. Trois snapshots de générations distinctes partagent les mêmes données de fichier. Sur le clone, une première mutation sépare les pages partagées, la deuxième modifie à nouveau ces plages. Le banc contrôle les octets du fichier final et de tous ses pairs.

Les six exécutables chronométrés ont été recompilés et ont reproduit leurs empreintes SHA-256 bit pour bit. Le banc a vérifié les trajectoires, les tailles et les compteurs lors de l'import des relevés. Les campagnes alternent les versions, ordres de secteurs, régimes de cache et positions aléatoires. Chaque cellule de lecture comprend quatre passes de huit opérations sur des plages de 4, 16 ou 64 Kio. Chaque catégorie d'écriture comprend huit opérations par campagne. À froid, la restitution du cache et une lecture vide remontent le contenu avant de lancer huit lectures de données. P4.1 garde le moteur monté lors de la restitution. P5 et P5.1 abandonnent ce montage et le revérifient lors de la lecture vide. La table de pages propres de données (CLOCK) commence vide, mais les relectures de montage peuvent réchauffer APFS ou le contrôleur différemment selon la version. Leur coût reste hors chronomètre. L'état du cache évolue pendant la passe. À chaud, les positions mesurées sont lues pendant le préchauffage. Le panneau F_NOCACHE désactive le cache du fichier hôte via Darwin et applique la préparation à froid. APFS, le contrôleur et le SSD conservent leurs propres comportements. Aucun scénario de ce banc ne simule une coupure électrique physique.

Les durées couvrent l'appel public et sa demande de ticket. L'admission est sans concurrence dans ces mesures. Les tests de concurrence qualifient la discipline FIFO séparément. Le débit est le total des octets utiles divisé par la somme des durées. Les p50/p95/p99 sont les rangs supérieurs sur les durées individuelles. Avec 64 lectures ou 16 mutations par cellule sur deux campagnes, le p99 de cellule coïncide avec le maximum observé. Les quantiles agrégés à partir de plusieurs cellules décrivent une autre population. Les mesures CPU du thread, l'E/S murale et les pauses d'ordonnancement ne sont pas interchangeables.

## Profil CPU et lectures évitées

Les mêmes sondes ont été appliquées aux copies des deux versions. Pour 16 lectures de 16 Kio sur ARM64/4 096 après 16 mutations, les lectures de l'image passent de **67 à 3**. Il reste trois pages de métadonnées et les 64 relectures de données disparaissent. Le lot passe de 68,88 à 49,81 allocations par appel. Le temps CPU instrumenté passe de 409,78 à 143,49 µs sur ARM64 et de 700,19 à 145,94 µs sous Rosetta. Sur un second parcours des plages déjà chaudes, le nombre de vérifications de métadonnées passe de 67 à 19 sur le lot. Les temps CPU par lecture passent de 157,03 à 97,63 µs sur ARM64 et de 262,62 à 147,75 µs sous Rosetta. Un test distinct relit deux sous-plages du même extent chaud et vérifie qu'il ne reste que l'allocation du tampon de résultat.

Sur le même lot de 16 écritures, P5.1 compare 144 pages à zéro au lieu de 288 et admet les données propres 16 fois après F2. Le coût de cette admission sur le lot ARM64 est de quatre allocations supplémentaires par écriture de 16 Kio en moyenne (239,19 → 243,19). Le CPU instrumenté d'écriture passe de 3 100 à 3 138 µs par appel. Ces durées dépendent des sondes imbriquées, particulièrement des appels d'horloge sous Rosetta. Seules les comparaisons à instrumentation identique décrivent leur variation. Les mesures non instrumentées ci-dessous qualifient le coût visible par le client.

[Contrôles, réservations, traduction et publication par phase](validation/v0.9.7/profil/README.md) · [Traces appariées P5 et P5.1](validation/v0.9.7/profil/comparaison/)

## Témoin historique de 16 Kio

Les triplets suivent l'ordre **P4.1 / P5 / P5.1**. Chaque quantile porte sur 128 opérations. L'origine reste sans snapshot pendant ces mesures. Le client d'origine et sa séquence de données sont identiques aux trois passages.

| Cible / secteur | Lecture Mio/s | p95 lecture ms | p99 lecture ms | Écriture Mio/s | p95 écriture ms | p99 écriture ms |
| --- | --- | --- | --- | --- | --- | --- |
| ARM64 / 512 | 42,01 / 34,64 / 74,24 | 0,57 / 0,79 / 0,41 | 0,70 / 0,86 / 0,45 | 1,69 / 1,52 / 1,62 | 10,36 / 11,96 / 10,94 | 10,57 / 12,30 / 11,83 |
| ARM64 / 4096 | 40,45 / 31,04 / 66,23 | 0,56 / 0,78 / 0,36 | 1,03 / 0,90 / 0,46 | 1,69 / 1,50 / 1,54 | 9,88 / 11,54 / 11,26 | 11,34 / 12,40 / 12,08 |
| Rosetta / 512 | 29,32 / 26,59 / 48,07 | 0,85 / 0,82 / 0,49 | 0,93 / 1,03 / 0,59 | 1,87 / 1,65 / 1,39 | 10,50 / 12,17 / 13,30 | 13,36 / 13,58 / 15,65 |
| Rosetta / 4096 | 25,29 / 23,83 / 52,76 | 0,95 / 0,81 / 0,45 | 1,20 / 1,02 / 0,80 | 1,52 / 1,64 / 1,47 | 13,13 / 11,47 / 12,00 | 13,87 / 12,11 / 12,92 |

Sur ce rejeu, la lecture P5.1 dépasse 95 % de P4.1 dans les quatre couples cible/secteur. Les 66,23 Mio/s en ARM64/4 096 représentent 164 % des **40,45 Mio/s contemporains** et 47 % des **140,23 Mio/s de l'ancienne session**. Le seuil absolu de 133,22 Mio/s issu de l'ancienne session dépasse le résultat de ce premier rejeu. La matrice ci-dessous examine les lectures indépendantes d'une publication.

Sous Rosetta, le témoin écrit moins vite en P5.1 lors de ce passage. En secteurs de 512 octets, le temps moyen total passe de 9,50 à 11,24 ms, dont 7,18 à 8,51 ms dans les flushs et 2,31 à 2,73 ms dans le reste de l'appel. En secteurs de 4 Kio, les flushs passent de 6,95 à 8,28 ms et le reste de 2,57 à 2,35 ms. Le temps dans les flushs explique respectivement 76 % et plus de 100 % du surcoût mural observé. Ces variations sur un passage indépendant doivent être rapprochées des répétitions appariées et du profil CPU.

L'amplification P5.1 reste exactement **3,2421875×** sur chaque cible et chaque secteur. Les 512 pages de traduction et de publication sont toujours présentes. Le facteur 2,25× n'est pas atteint. Le coût documenté est celui de cette trace et de la granularité actuelle du format. Les opérations de création de snapshot et de clone restent à deux pages de publication et deux synchronisations chacune sur le témoin. Les distributions de création, suppression, remontage, vérification, export et restauration sont également conservées dans les relevés.

[Échantillons et distributions du témoin P5.1](validation/v0.9.7/mixte/p51/)

### Répétitions en ordres inversés

Le même exécutable par architecture et par version a rejoué deux fois le client historique. Les deux valeurs de chaque colonne sont données dans l'ordre des répétitions. Les rapports utilisent dans chaque cas le P4.1 ou le P5 de la même répétition.

| Cible / secteur | Lecture P5.1 Mio/s | Débit lecture P5.1/P4.1 | Débit écriture P5.1/P5 | Δ moyen F1+F2 µs | Δ moyen hors flush µs |
| --- | --- | --- | --- | --- | --- |
| ARM64 / 512 | 70,56 / 90,74 | 1,798 / 1,044 | 0,941 / 1,054 | 170 / -935 | 453 / 393 |
| ARM64 / 4096 | 167,45 / 97,44 | 2,388 / 1,674 | 0,657 / 0,992 | 5757 / -78 | -565 / 159 |
| Rosetta / 512 | 80,59 / 72,63 | 1,576 / 1,765 | 1,055 / 0,933 | -466 / 462 | -103 / 287 |
| Rosetta / 4096 | 85,93 / 70,89 | 2,167 / 1,316 | 0,977 / 1,067 | 628 / -570 | -375 / -134 |

L'amélioration de la relecture est présente dans les huit comparaisons appariées face à P4.1 et à P5. Le résultat absolu ARM64/4 096 varie de 97,44 à 167,45 Mio/s dans ces deux passages. Les variations des écritures Rosetta changent de signe selon la répétition. Le débit regroupé des 256 écritures par version et par géométrie vaut 0,992× P5 en secteurs de 512 octets et 1,021× P5 en secteurs de 4 096 octets. La baisse de 34,3 % du débit d'écriture ARM64/4 096 au premier passage coïncide avec 5,76 ms de plus dans les flushs et 0,56 ms de moins en dehors. Au second passage le ratio de débit est de 0,992. En ARM64/512, le coût hors flush augmente dans les deux répétitions (+0,45 puis +0,39 ms) alors qu'il diminuait dans le premier rejeu (1,87 → 1,70 ms). Dans les panneaux chauds de la matrice à 16 Kio sur l'origine ARM64/512, l'écart CPU moyen est de l'ordre de +0,063 ms. L'admission des pages au cache représente du travail supplémentaire en publication, et le temps mural hors flush inclut aussi les autres E/S et les attentes. Les résultats qualifient le gain de lecture sur cette charge sans établir d'accélération des écritures.

[Deux passages, quantiles et temps de synchronisation](validation/v0.9.7/mixte/apparie/synthese.json) · [Sources, empreintes et ordre effectif](validation/v0.9.7/mixte/apparie/provenance.json)

## Matrice de lectures et d'écritures indépendantes

La matrice conserve **62 208 appels sur 72 exécutions**. Pour l'origine, chacun des 12 cas d'un régime combine secteur, ordre et taille. Le tableau indique le nombre de cas qui atteignent 95 % du débit P4.1 **dans chacune des deux répétitions**. La colonne suivante applique le seuil au débit obtenu en regroupant les octets et les durées des deux répétitions. Les trois dernières colonnes donnent la médiane des ratios appariés P5.1/P5 sur les quatre topologies. Un ratio de débit supérieur à 1 et un ratio de latence inférieur à 1 indiquent une amélioration.

| Cible / cache | ≥ 95 % P4.1 dans les deux répétitions | ≥ 95 % après regroupement | Débit P5.1/P5 | p95 P5.1/P5 | p99 P5.1/P5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 / chaud | 12/12 | 12/12 | 2,498 | 0,555 | 0,542 |
| ARM64 / froid | 6/12 | 7/12 | 1,383 | 0,759 | 0,762 |
| ARM64 / F_NOCACHE | 1/12 | 3/12 | 1,175 | 0,970 | 0,979 |
| Rosetta / chaud | 12/12 | 12/12 | 2,329 | 0,578 | 0,556 |
| Rosetta / froid | 6/12 | 8/12 | 1,372 | 0,799 | 0,829 |
| Rosetta / F_NOCACHE | 0/12 | 0/12 | 0,998 | 1,030 | 1,045 |

À chaud, les 24 conditions d'origine dépassent le seuil dans les deux répétitions. À froid, les charges de 64 Kio perdent une partie de ce gain. Sous Rosetta, en secteurs de 4 096 octets, la première campagne séquentielle de 64 Kio F_NOCACHE compte 16,375 lectures de bloc par appel pour chaque version, dont zéro lecture de carte. Les temps CPU P4.1/P5/P5.1 sont de 417/616/582 µs et les temps mesurés dans les appels de lecture du fichier sont de 635/933/894 µs. À volume de lecture identique, la double intégrité FS/pool conserve un coût CPU. Les délais des E/S hôte contribuent aussi à l'écart.

Pour illustrer la différence de population avec le témoin mixte, l'origine ARM64/4 096 en lecture séquentielle de 16 Kio donne **298,61 / 278,22 / 1 282,88 Mio/s** à chaud en P4.1/P5/P5.1. Les mêmes panneaux donnent **160,65 / 118,97 / 182,06** à froid et **54,03 / 44,48 / 49,49** sous F_NOCACHE. Le débit à chaud est servi en mémoire. Les 64 appels par cellule permettent de décrire le maximum rencontré, avec une précision limitée pour estimer un p99 futur.

### Ensemble des topologies

Chaque ligne de lecture regroupe les 2 304 requêtes d'une cible et d'une topologie par version, toutes tailles et tous régimes inclus. Les valeurs sont **P5 → P5.1**. Le débit agrégé pondère les requêtes par leurs octets et leur temps. Une pointe d'E/S peut donc faire baisser ce débit malgré la baisse du p50.

| Cible | Topologie | Lecture Mio/s | p50 ms | p95 ms | p99 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 | Origine | 76,35 → 98,73 | 0,110 → 0,070 | 1,739 → 1,338 | 2,613 → 2,034 |
| ARM64 | Un snapshot | 87,19 → 96,16 | 0,101 → 0,066 | 1,323 → 1,400 | 1,930 → 2,054 |
| ARM64 | Trois snapshots | 73,38 → 69,87 | 0,103 → 0,068 | 1,735 → 1,531 | 2,624 → 2,904 |
| ARM64 | Clone | 77,50 → 93,61 | 0,109 → 0,068 | 1,557 → 1,347 | 2,403 → 2,224 |
| Rosetta | Origine | 72,04 → 64,05 | 0,157 → 0,103 | 1,514 → 1,665 | 2,315 → 2,615 |
| Rosetta | Un snapshot | 65,75 → 77,83 | 0,142 → 0,098 | 1,625 → 1,643 | 2,728 → 2,424 |
| Rosetta | Trois snapshots | 60,73 → 72,48 | 0,150 → 0,102 | 1,702 → 1,777 | 2,349 → 2,440 |
| Rosetta | Clone | 71,51 → 72,73 | 0,153 → 0,101 | 1,596 → 1,635 | 2,224 → 2,466 |

À chaud, les ratios de débit P5.1 de chaque vue relativement à l'origine seule (un snapshot / trois snapshots / clone) ont pour médianes **1,012 / 1,017 / 1,039** sur ARM64 et **0,996 / 0,996 / 0,998** sous Rosetta. Ces ratios comparent les mêmes conditions dans chaque répétition. Sous F_NOCACHE, les trois générations donnent **0,816** et **0,915** du débit de l'origine. Les mesures ne garantissent donc pas un coût négligeable de toutes les topologies en lecture froide.

### Coût des mutations et valeurs extrêmes

Le débit d'écriture agrégé ci-dessous regroupe les mêmes distributions de tailles, secteurs et ordres entre versions. La première modification des plages sur le fichier de 512 Kio écrit en moyenne 3,938 à 4,000 fois les octets applicatifs à 16 Kio en P5.1. La réécriture des mêmes plages atteint 3,922 à 3,938 fois. Dans le panneau séquentiel chaud ARM64/512, quatre données et six pages FS sont accompagnées de 3,75 pages de traduction et de deux pages de publication par écriture. La géométrie des arbres de cette charge diffère de celle du témoin historique à 3,242×. Les 9 216 paires de mutations P5/P5.1 ont été rapprochées à architecture, secteur, topologie, régime, répétition et position identiques. Les quatre compteurs de blocs écrits (données, métadonnées FS, cartes, publication) sont identiques dans chaque paire. Chaque mutation a soumis deux flushs.

| Cible | Opération | N/version | Écriture Mio/s | p95 ms | p99 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 | Première écriture exclusive | 576 | 2,72 → 2,88 | 14,32 → 13,23 | 15,84 → 14,97 |
| ARM64 | Première divergence | 1 728 | 2,75 → 2,65 | 13,72 → 13,63 | 15,88 → 17,38 |
| ARM64 | Réécriture | 2 304 | 2,63 → 2,70 | 13,73 → 13,35 | 16,60 → 14,80 |
| Rosetta | Première écriture exclusive | 576 | 2,70 → 2,48 | 13,72 → 14,11 | 15,87 → 25,28 |
| Rosetta | Première divergence | 1 728 | 2,58 → 2,42 | 14,17 → 13,91 | 16,43 → 15,75 |
| Rosetta | Réécriture | 2 304 | 2,42 → 2,34 | 13,69 → 13,78 | 16,96 → 16,31 |

Pour repérer les retraits reproductibles, le critère descriptif est une hausse d'au moins 10 % du **temps mural moyen dans les deux répétitions** face à P5. Aucune cellule de lecture n'atteint ce critère. Les quatre divergences ARM64 concernées ont 99,0 % du temps additionnel dans les E/S, les sept réécritures 91,8 %. Sous Rosetta, une divergence et trois réécritures franchissent ce seuil, avec respectivement 99,5 % et 92,6 % du temps additionnel dans les E/S. Sur la réécriture aléatoire froide ARM64/512 de 64 Kio, le delta moyen est de 1 633 µs, dont 1 002 dans les flushs, 310 dans les lectures/écritures du fichier et 321 hors E/S. Ce dernier poste inclut le temps de transfert au cache propre et les 16 allocations supplémentaires observées. Il inclut également les temps d'ordonnancement non isolés. Les essais établissent la localisation des délais, sans établir la cause matérielle de chaque attente.

Les maxima ont été conservés. La lecture P5.1 la plus lente a pris 53,614 ms dont 53,424 ms dans le fichier (ARM64/512, F_NOCACHE, trois snapshots). La réécriture la plus lente a pris 262,935 ms, dont 195,900 ms dans F1+F2 et 66,390 ms en lectures/écritures (ARM64/4 096, F_NOCACHE). Sous Rosetta/4 096 à froid, une écriture exclusive de 16 Kio a pris 213,784 ms dont 212,424 ms dans F1+F2. Le banc établit les gains à chaud et les lectures évitées. Il ne fournit pas de borne sur les délais du stockage.

[Lecture de la matrice](validation/v0.9.7/mesures/README.md) · [Provenance des six exécutables](validation/v0.9.7/mesures/campagnes.json) · [62 208 échantillons](validation/v0.9.7/mesures/echantillons.csv.gz) · [Distributions par panneau et répétition](validation/v0.9.7/mesures/distributions.csv.gz) · [Ratios appariés](validation/v0.9.7/mesures/comparaisons.csv.gz)

## Budget mémoire

Le tas inclut les allocations Rust arrondies par Darwin, le moteur et les tableaux de contrôle du client de mesure. La pile est la réservation du worker. Une admission peut garder les pages privées de l'overlay jusqu'à F2, puis stocker leurs octets dans le cache propre borné. Le nouveau masque en mémoire représente huit octets de champ par allocation vérifiée. Le pic du processus inclut aussi le runtime et l'environnement Rosetta.

| Cible | Tas P5.1 (octets) | Pile worker (octets) | Tas + pile P5 → P5.1 (Mio) | RSS maximal P5.1 (Mio) |
| --- | ---: | ---: | ---: | ---: |
| ARM64 | 2 024 816 | 2 109 440 | 3,825 → 3,943 | 6,469 |
| Rosetta | 2 024 480 | 2 097 152 | 3,813 → 3,931 | 25,820 |

Les charges de référence restent sous le budget de **7 340 032 octets de tas et pile de travail**. La pile principale réservée par le lanceur Darwin du témoin mixte compte 8 372 224 octets séparément. Le plafond ne s'applique ni à ce lanceur, ni au RSS du processus hôte, ni au cache APFS. La restitution a été testée sur les cycles décrits ci-dessous.

## Récupération, compatibilité et restitution

| Mesure effectivement exécutée | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Tests Cargo standard / toutes sondes | 123 / 124 | 123 / 124 |
| Tests optionnels rejoués explicitement | 3 | 3 |
| Opérations différentielles / temps | 1 000 000 / 34,594 s | 1 000 000 / 40,603 s |
| Publications / pénuries injectées / remontages audités | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |
| Coupures P5 / erreurs E/S / sélecteurs déchirés | 684 / 152 / 12 | 684 / 152 / 12 |
| Ordres de suppression avec réattribution (24 par secteur) | 48 | 48 |
| Cycles de restitution, dont la moitié en échec F2 | 128 | 128 |
| Tas vivant avant et après chacune des quatre séries, octets | 182 960 | 167 872 |
| Arrêts de processus sur fichier / arrêts de VM P1 | 24 / 10 | 24 / 10 |

Les suites standard et toutes sondes ont des tests en commun. Le million différentiel alterne les géométries selon la graine. Les 684 coupures et 152 erreurs du partage portent sur les deux géométries ensemble, avec six mutations par géométrie. L'oracle vérifie les survivants après disparition de l'origine, la divergence des clones, les références, la réattribution et la saturation. Les deux montages après chaque arrêt P1 concordent. L'invité du test de coupure VM est x86_64 Q35 sous TCG dans les deux campagnes. Les tests ciblés couvrent les positions vides et extrêmes du masque, un import et deux racines divergentes, la corruption d'une page hors cache, les accès par chemin et par lignée, le succès ou l'échec de F2, et la restitution du cache.

Les images FS de sept révisions ont subi 112 lectures interversions et 56 lectures avec les binaires courants. Les anciens catalogues de volumes ont subi 80 vérifications de pool et 40 contrôles d'espace. Entre P5 et P5.1, huit reprises en écriture ont été relues par le producteur d'origine, en plus de 16 lectures croisées et 16 restaurations autonomes après destruction du pool source. Huit autres restaurations entre les deux exécutables P5.1 ont vérifié l'export natif actuel. L'ancien écrivain refuse les images partagées. Aucune perte de données confirmées n'a été observée sur ces scénarios.

[Validation ARM64](validation/v0.9.7/validation-aarch64.json) · [Validation Rosetta](validation/v0.9.7/validation-x86_64.json) · [Échanges et restauration](validation/v0.9.7/validation-compatibilite.json)

## Reproduction

Depuis la racine du monorepo, sur macOS Apple Silicon avec Rosetta et les deux cibles Rust Darwin installées. Les dépendances Cargo doivent être disponibles pour le mode hors ligne. Les essais de coupure VM utilisent QEMU x86_64 et son firmware EDK II. Pour comparer les chronomètres, exécuter les commandes successivement. La mesure RSS utilise `/usr/bin/time -l` et requiert l'accès aux compteurs Darwin.

```sh
set -eu
sortie=$(mktemp -d)
for arch in aarch64 x86_64; do
    python3 novaFS/lab/p51-validation-suites.py "$arch" > "$sortie/suites-$arch.json"
    NOVAFS_TARGET="$arch-apple-darwin" python3 novaFS/lab/test-persistence.py --support tous
done
python3 novaFS/lab/p51-validation.py --corpus historiques > "$sortie/historiques.json"
python3 novaFS/lab/p51-validation.py --corpus partage > "$sortie/partage.json"
python3 novaFS/lab/p5-echange.py
python3 novaFS/lab/p5-export.py
python3 novaFS/lab/test-latency-report.py
for ref in b865c58 ae508ce travail; do
    python3 novaFS/lab/p51profil.py --ref "$ref" --sortie "$sortie/profil"
done
python3 novaFS/lab/p51-matrice.py --campagnes 2 --sortie "$sortie/matrice"
python3 novaFS/lab/p51-mixte.py --sortie "$sortie/mixte-apparie"
mkdir "$sortie/p5"
git archive ae508ce novaFS | tar -x -C "$sortie/p5"
GIT_DIR="$(git rev-parse --absolute-git-dir)" python3 "$sortie/p5/novaFS/lab/p5-mesures.py" --sortie "$sortie/mixte-references"
python3 novaFS/lab/p5-mesures.py --phase P5 --sortie "$sortie/mixte-p51"
```

Dans le lanceur historique, l'option `--phase P5` désigne les sources locales du pool partagé. Les répertoires de sortie distincts identifient la révision réellement mesurée.
