# Coûts internes des lectures et écritures de 16 Kio

Les fichiers `ae508ce-*.jsonl` à la racine de ce dossier conservent la première mesure P5 effectuée avant les modifications du moteur. `confirmation/` contient un second passage P5, étendu à la charge historique de 128 opérations. `b865c58-*.jsonl` rejoue cette dernière sur P4.1. `comparaison/` rapproche P5 et P5.1 avec les mêmes sondes, incluant le comptage des tests de pages nulles. Les deux géométries figurent dans chaque fichier.

## Mesures

- `ecriture_historique_16k` reprend les 128 itérations de la campagne P5, quatre offsets alternés, une écriture et sa relecture par ticket FIFO. Seule l’écriture entre dans les compteurs de cette ligne. Les lectures de contrôle vérifient les 16 Kio entiers.
- Les trois autres charges mesurent respectivement 16 réécritures, 16 lectures des mêmes plages après la série d’écritures et un second passage sur ces lectures. Le fichier fait 256 Kio et les 16 offsets sont distincts. Chaque configuration repart d’une image neuve. Les vues partagées restent vivantes et leurs données sont vérifiées à la fin.
- `lectures` et `ecritures` comptent les appels réussis de 4 Kio adressés au fichier image. Le rôle vient des signatures des structures et de pages de données connues dans cette fixture. Le cache de macOS et le contrôleur du disque agissent en aval. Les deux sommes de temps de synchronisation suivent l’ordre F1 puis F2 pour chaque transaction.
- `allocations` est le nombre de demandes non nulles adressées à l’allocateur global Rust. `cpu_ns` mesure le temps du thread et `mur_ns` le temps écoulé. La première ligne donne l’étalonnage de l’horloge. La réservation de la pile du thread principal du banc est rapportée séparément de la pile de travail mesurée par la matrice.
- Les sondes sont compilées dans une copie temporaire des sources. Les temps sont inclusifs. Une recherche de métadonnées peut aussi chronométrer un parcours d’extent, un accès au cache et un appel de l’allocateur. Les durées des appels récursifs de `PoolPin` et `PoolRelease` se recouvrent également. Les compteurs non appelés dans un lot sont omis. Les percentiles individuels d’une sonde sont des bornes supérieures d’histogramme. Les latences applicatives de référence sont mesurées séparément dans la matrice sans ces sondes.
- Les sondes `Fs*` chronomètrent le moteur de contenu, dont l’ancrage et le manifeste P0. `PrepareContent` inclut la préparation de cette vue privée. `PoolTranslate` suit la traduction de bloc par l’index validé en mémoire. `PoolReadMap`, `PoolPatch`, `PoolPin` et `PoolRelease` suivent les feuilles, racines et références de partage. `PoolCommit` couvre la publication du catalogue et du sélecteur, avec les deux synchronisations. `FsReserve` suit les réservations du moteur et `Quota` l’admission de descripteurs dans le pool. Une écriture sur un volume déjà admis consomme son crédit existant.
- `PoolZero` compte chaque test d’une page d’overlay contre 4 Kio de zéros. Sur P5 les pages non nulles subissaient un test dans le pool et un second dans `store_data`. P5.1 conserve le test de `store_data`. `CachePublished` mesure l’admission dans le cache propre après F2 en P5.1. L’ancien moteur n’a pas cette étape.

## Affectation des écritures du cycle historique

| 128 écritures de 16 Kio | Données | B+Trees FS | Manifestes FS | Ancrages FS | Feuilles de traduction | Racines de traduction | Catalogues | Sélecteurs | Total / 512 blocs demandés |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| P4.1 | 512 | 384 | 128 | 128 | 0 | 0 | 0 | 0 | 1 152 / 512 = 2,25× |
| P5 | 508 | 384 | 128 | 128 | 128 | 128 | 128 | 128 | 1 660 / 512 = 3,2421875× |
| P5.1 | 508 | 384 | 128 | 128 | 128 | 128 | 128 | 128 | 1 660 / 512 = 3,2421875× |

La première écriture P5 contient quatre pages nulles, représentées par des trous. Pour les 127 autres écritures, les quatre blocs supplémentaires publient la traduction et le nouvel état du pool. Le partage par des snapshots n’est pas nécessaire pour déclencher ces publications. Chacune des deux architectures et des deux géométries a reproduit ces nombres avec 256 synchronisations.

## Effets observés sur la lecture

Volume exclusif, secteurs de 4 096 octets, 16 requêtes de 16 Kio par cellule. Les colonnes CPU donnent les moyennes du thread avec instrumentation.

| Opération | CPU ARM64 P5 → P5.1 (µs) | CPU Rosetta P5 → P5.1 (µs) | Allocations Rust par requête P5 → P5.1 | Lectures image sur les 16 requêtes P5 → P5.1 |
|---|---:|---:|---:|---:|
| Lecture après les écritures | 409,78 → 143,49 | 700,19 → 145,94 | 68,875 → 49,8125 | 67 → 3 |
| Second passage sur ces plages | 157,03 → 97,63 | 262,62 → 147,75 | 64,5 → 48,5 | 0 → 0 |
| Réécriture des 16 plages | 3 100,42 → 3 138,17 | 4 032,12 → 2 113,31 | 239,1875 → 243,1875 | 403 → 403 |

Au second passage, le cache contient déjà les données sur les deux versions. Le curseur évite de résoudre le même fichier à chaque requête. Les contrôles d’identité de métadonnées passent de 67 à 19. Il reste 19 recherches d’extents pour parcourir ces 16 plages distinctes. Sur une relecture située dans le même extent vérifié, le test dédié mesure une allocation pour le tampon retourné. Chaque lecture garde le contrôle de sa page de données selon les règles du cache P4.

Après les écritures, les 64 pages de données restent dans le cache en P5.1. Les trois lectures image restantes concernent des pages de B+Tree. Cette admission mobilise quatre allocations supplémentaires par réécriture de 16 Kio. Pour ce lot, les tests des pages nulles passent de 288 à 144. Les 16 réécritures émettent toujours 224 blocs et 32 synchronisations. Le coût CPU d’écriture instrumenté ARM64 croît de 1,2 % sur ce lot. Les mesures chronométrées sans sondes et leurs distributions sont publiées dans le rapport principal.

Sur l’ensemble des quatre topologies et des deux géométries, les deux architectures présentent les mêmes décomptes P5 et P5.1 pour les écritures de chaque rôle et les synchronisations. Lors de la première relecture de chaque lot, les appels image pour les seules données passent de 64 à zéro dans les 16 combinaisons architecture/secteur/topologie.

## Reproduction sur macOS

Depuis la racine du dépôt, les cibles Rust `aarch64-apple-darwin` et `x86_64-apple-darwin` et Rosetta permettent les deux passages successifs. Chaque exécutable Mach-O est vérifié avant exécution.

```sh
python3 novaFS/lab/p51profil.py --ref b865c58 --sortie /tmp/novafs-profils
python3 novaFS/lab/p51profil.py --ref ae508ce --sortie /tmp/novafs-profils
python3 novaFS/lab/p51profil.py --ref travail --sortie /tmp/novafs-profils
```
