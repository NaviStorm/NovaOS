# NovaFS P5 — qualification des snapshots et clones

## Périmètre

Les deux exécutables Darwin sont construits en profil release avec Rust stable 1.99.1. Les essais ont tourné sur le même Mac Apple Silicon sous macOS 27.2 (26B123), en ARM64 natif et en x86_64 sous Rosetta. L’échange croisé contrôle le type CPU inscrit dans les deux Mach-O. Les tests de panne créent des supports temporaires et comparent des états attendus hors du système de fichiers.

## Récupération et intégrité

Les valeurs de cette matrice s’entendent **par architecture**. Les campagnes de partage, de migration, d’export et de coupures couvrent les secteurs de 512 et 4 096 octets. Certains témoins unitaires ciblent une seule géométrie. Les numéros correspondent à la liste d’acceptation.

| Critères | Essai et oracle |
| --- | --- |
| 1, 2 | Suites P0 à P4.1 compilées avec P5, sondes désactivées puis activées. Campagne différentielle distincte de 100 graines, un million d’opérations historiques. Les suites par défaut et avec sondes ont des tests communs. |
| 3, 4 | Sur `b865c58`, le témoin identifie la page 265 puis observe l’effacement des 256 blocs réattribués. Sur le pool P5 de 512 blocs, le snapshot conserve la page sentinelle après retrait de l’origine, rotation, réattribution à un autre volume, 40 écritures et remontage. Les deux binaires compilés contre le commit P4.1 refusent l’import et le formatage des quatre images P5, avec huit contrôles sans écriture ni flush et empreintes inchangées. |
| 5 à 7 | L’origine et deux clones divergent. Les quatre objets sont ensuite supprimés dans les 24 permutations, avec réattribution, écriture et remontage entre chaque suppression. Tous les survivants rendent leur contenu attendu. Un deuxième snapshot conserve une autre génération. Les reçus de deux clones, d’un autre pool et d’une version antérieure sont distingués. |
| 8, 13 | Un pool créé avec exactement le crédit de l’origine, du snapshot et d’un clone refuse une garantie supplémentaire. Le clone admis peut encore diverger, l’origine et le snapshot peuvent être supprimés, puis un nouvel objet créé. Un autre essai sature les blocs du clone et le quota du domaine et vérifie les reçus au refus. |
| 9 à 11 | Six mutations incluent création du snapshot, création du clone, modification du clone, suppression de l’origine, suppression du snapshot et rotation libérant le prédécesseur. La trace couvre 114 frontières par géométrie. Perte du cache, persistance de sa dernière écriture, persistance intégrale en ordre inverse donnent **684 coupures**. **152 EIO** visent toutes les écritures, F0 de stabilisation, F1 et F2. Douze écritures partielles du sélecteur laissent deux générations liées. |
| 12 | Alias de rôles, de LBA et de lignées, cycle, compteurs et identités incohérents, forgés avec sommes recalculées. Les contrôles refusent aussi une substitution de page de données partagée au remontage. La combinaison des quatre positions du sélecteur est exercée sur quatre rotations et deux géométries. |
| 14 à 16 | Export et restauration vers un nouveau pool après disparition du pool source. Contrôle du contenu, des trous et des noms. Troncatures, modifications d’en-tête, de pages et de terminaison, y compris avec un condensat recalculé, et échec réel du producteur du flux. La destination est contrôlée avant et après chaque refus. |
| 17, 18 | Échange bidirectionnel des images historiques, rejet des versions et options inconnues. Pour P5, quatre images de pool comparées intégralement, huit lectures par l’architecture opposée et contrôle des images après lecture. Le banc d’export échange aussi les flux entre architectures et restaure sans accès à la source. |
| 19 | Sur chaque géométrie, 32 cycles de montage, dérivation, divergence, export, destruction et restauration, puis 32 cycles avec F2 en échec. Le compteur de l’allocateur du processus retrouve sa valeur initiale après chaque cycle. La portée est limitée aux chemins exercés. |
| 20 | Deux graines déterministes `519c04287dab33a1` et `f3a014176825914d`, 120 opérations chacune dans chaque géométrie. Les 480 décisions comportent 120 coupures. L’oracle conserve les octets attendus de chaque vue, contrôle toutes les vues après chaque décision et exige l’état choisi par le modèle de persistance. |

Un échec avant acceptation du sélecteur conserve l’ancien état. Une F2 déjà effective impose le nouvel état. Entre ces deux instants, le test fixe l’issue selon les écritures que le simulateur a réellement rendues stables. Après chaque reprise, une rotation et une seconde ouverture doivent rendre les mêmes données. Les essais sur la migration publient ou récupèrent les deux origines ensemble. Les erreurs survenant pendant la restauration d’un export ne peuvent pas le rendre complet. Ces résultats supposent la persistance des écritures acquittées et le confinement des écritures aux adresses demandées.

[Témoin historique P0](novafs-p5-preuve-p0.md) · [Scénarios de partage](../tests/p5_partage.rs) · [Test de restitution](../tests/p5_restitution.rs) · [Migration et réattribution](../tests/migration_p5.rs) · [Contrôles adverses du graphe](../src/shared_graph.rs) · [Tests d’export](../tests/p5_export.rs)

## Résultats par exécutable

| Exécution | Suite standard | Toutes les sondes | Différentiel | Coupures P5 / EIO / sélecteurs déchirés | Cycles de restitution |
| --- | ---: | ---: | --- | --- | ---: |
| ARM64 natif | 120 | 121 | 1 000 000 en 24,532 s | 684 / 152 / 12 | 128 |
| x86_64 sous Rosetta | 120 | 121 | 1 000 000 en 45,790 s | 684 / 152 / 12 | 128 |

Le test de collecte supprime un fichier de 192 Kio du clone, en récupère les pages par lots et vérifie après remontage le fichier complet du snapshot et le trou du nouveau fichier du clone. La campagne différentielle a observé 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités sur chaque architecture. Les deux autres tests optionnels ont généré un catalogue historique de 16 volumes et exercé les extents contigus. La suite a également exercé trois tests du coordinateur P5 sur les tickets, le contrôle des plans préparés après mutation du catalogue et les lectures simultanées. Sur chaque architecture, la migration atomique d’un pool à deux volumes a subi 96 interruptions par géométrie. Le banc historique P1 a exécuté sur chaque architecture 24 arrêts du processus écrivain et 10 arrêts d’une VM QEMU x86_64, avec deux remontages après chaque coupure. Les nombres P1 et P5 couvrent des protocoles et supports distincts.

Les quatre images P5 de 32 Mio ont la même empreinte SHA-256 `bcd0ef65d84ff95250a11efe63750abcff9ee14ca012b2315ab824486d9014ad`. Les quatre producteurs d’exports (architecture et géométrie) ont chacun été restaurés deux fois dans l’architecture opposée. Les images FS P0 à P4.1 donnent l’empreinte historique `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. Les quatre références de catalogues P2 et suivants ont chacune passé 16 contrôles d’intégrité et huit contrôles d’espace entre architectures. L’ancien modèle de données reste lisible sans migration.

L’exécution complète a fait apparaître une régression du rapport JSON historique lorsque le descripteur d’entrée refusait les lectures. La sonde de choix de format retournait avant les diagnostics des deux slots. Après correction, le test P2 `p2_check_pool_json_et_image_identique` a de nouveau contrôlé la présence du domaine, des volumes et des erreurs attendues sur les deux architectures.

[Relevés de qualification](validation/v0.9.6/qualification.json) · [Transferts des archives](validation/v0.9.6/exports-croises.json)

## Charge de performance

Le banc compile son client contre le commit P4.1 `b865c58`, puis contre P5. Il utilise deux fichiers images macOS temporaires neufs, un pour chaque version, à taille et géométrie identiques. Le pool de 32 Mio contient une origine de 1 Mio, avec quatre plages de 16 Kio. Les 128 itérations alternent l’écriture de 16 Kio puis la lecture et la comparaison de la même plage. Les deux versions admettent un ticket FIFO, préparent puis publient l’écriture. La lecture utilise également un ticket. Les essais mesurent des appels synchrones sur un client. Les données lues viennent de la dernière écriture sur la même plage. Le cache du système hôte reste actif. Les relevés historiques P4.1 à 64 Kio utilisent une charge différente.

La seconde charge, propre à P5, crée un snapshot et un clone, écrit 16 Kio dans le clone, vérifie la vue intacte et supprime les deux objets. Un cycle sur huit exporte le clone de 64 Kio logiques, le restaure dans un pool neuf et vérifie les octets. Le chronomètre de restauration exclut le formatage de la destination. Les débits d’export et de restauration portent sur les octets **logiques** du fichier. Une ouverture en lecture seule mesure le contrôle intégral du pool. Le temps de reprise opérationnelle inclut une autre ouverture et une rotation durable, avec la stabilisation du support.

Les échantillons rapportent la durée murale de chaque appel, les octets utiles, les blocs écrits, les durées des flushs et leur nombre. Chaque quantile correspond au rang supérieur sur la population de l’opération. Les 16 essais d’export, de restauration et de réouverture donnent un p99 égal au maximum. L’amplification divise les octets écrits au périphérique virtuel par le nombre d’octets applicatifs. Le compteur de tas mesure les blocs Rust engagés, arrondis par malloc. La pile de travail est la réservation effective du worker Darwin. La pile principale du lanceur et le RSS maximal du processus sont indiqués séparément. Le RSS est celui de l’exécutable mesuré par `/usr/bin/time -l`, indépendamment de Cargo.

## Mesures sur fichiers temporaires

Chaque case montre P4.1 → P5. Les p99 portent sur 128 appels par case et par version. Les fenêtres de mesure ont été exécutées successivement. Les deux géométries ont inversé l’ordre des versions.

| Exécution / secteur | Lecture Mio/s | Écriture Mio/s | p99 lecture ms | p99 écriture ms |
| --- | ---: | ---: | ---: | ---: |
| ARM64 / 512 | 46,36 → 63,49 | 1,73 → 1,74 | 0,601 → 0,508 | 10,138 → 10,906 |
| ARM64 / 4096 | 140,23 → 109,37 | 1,86 → 1,86 | 0,492 → 0,193 | 9,497 → 9,699 |
| Rosetta / 512 | 34,10 → 42,82 | 1,67 → 1,65 | 0,981 → 0,818 | 10,732 → 11,052 |
| Rosetta / 4096 | 37,70 → 39,10 | 1,69 → 1,62 | 0,975 → 0,700 | 11,259 → 10,750 |

Les tableaux suivants regroupent les échantillons P5 des deux géométries avant le calcul des rangs, sans moyenner leurs percentiles. Les colonnes de latence donnent **p50 / p95 / p99 / max** en millisecondes.

| Opération | N par cible | ARM64 ms | Rosetta ms | Débit ARM64 / Rosetta Mio/s |
| --- | ---: | ---: | ---: | ---: |
| Création snapshot | 256 | 8,048 / 8,426 / 9,699 / 151,578 | 8,120 / 8,795 / 10,456 / 150,090 | — |
| Création clone | 256 | 8,051 / 8,431 / 9,636 / 136,712 | 8,124 / 8,877 / 12,370 / 151,110 | — |
| Suppression snapshot | 256 | 7,936 / 8,629 / 9,065 / 10,370 | 8,067 / 8,782 / 9,638 / 81,540 | — |
| Suppression clone | 256 | 7,891 / 8,423 / 9,713 / 80,431 | 8,096 / 8,534 / 8,956 / 150,186 | — |
| Écriture indépendante 16 Kio | 256 | 8,640 / 10,759 / 11,403 / 11,675 | 9,421 / 12,139 / 13,603 / 115,774 | 1,73 / 1,51 |
| Lecture 16 Kio | 256 | 0,145 / 0,454 / 0,505 / 0,613 | 0,253 / 0,643 / 0,770 / 0,903 | 80,34 / 40,88 |
| Écriture 16 Kio | 256 | 8,492 / 10,168 / 10,882 / 10,932 | 9,294 / 10,665 / 11,019 / 11,312 | 1,80 / 1,64 |
| Vérification intégrale | 32 | 7,827 / 9,321 / 9,489 / 9,489 | 11,766 / 13,738 / 13,910 / 13,910 | — |
| Remontage et rotation durable | 32 | 16,676 / 20,675 / 20,818 / 20,818 | 19,820 / 24,010 / 25,422 / 25,422 | — |
| Export 64 Kio logiques | 32 | 4,846 / 8,807 / 9,098 / 9,098 | 8,102 / 10,419 / 10,452 / 10,452 | 10,56 / 7,45 |
| Restauration 64 Kio logiques | 32 | 14,826 / 17,448 / 18,804 / 18,804 | 18,569 / 20,397 / 20,555 / 20,555 | 4,11 / 3,34 |
| F1 pendant l’écriture | 256 | 3,840 / 4,644 / 4,765 / 6,442 | 4,192 / 4,612 / 5,032 / 5,550 | — |
| F2 pendant l’écriture | 256 | 4,114 / 4,244 / 4,300 / 4,311 | 3,791 / 4,207 / 4,378 / 6,150 | — |

La création d’un snapshot et celle d’un clone ont écrit **deux blocs de 4 Kio et deux flushs** par opération, pour chacun des 512 appels observés par architecture (256 de chaque type). Les écritures applicatives de 16 Kio ont soumis 2,250 fois leur taille au PD en P4.1.
En P5, la traduction CoW et le catalogue portent ce facteur à **3,242×** pour l’origine et **3,250×** pour la première écriture indépendante du clone.

En ARM64 et secteurs de 4 096 octets, le débit des lectures a baissé de 140,23 à 109,37 Mio/s (−22,0 %), alors que le p99 a diminué. Le débit et la queue de lecture varient selon la fenêtre et le secteur. Les comparaisons ne démontrent pas une accélération générale. Le p99 d’écriture peut augmenter même lorsque le débit moyen est voisin. Les lectures portent sur des données récentes et le banc ne qualifie ni les lectures froides sur toute la capacité ni une file chargée par plusieurs producteurs.

| Mémoire maximale observée sur les deux géométries, octets | Tas Rust | Pile de travail | Tas et pile de travail | RSS processus |
| --- | ---: | ---: | ---: | ---: |
| aarch64 P4.1 | 195 024 | 2 109 440 | 2 304 464 | 3 440 640 |
| aarch64 P5 | 242 608 | 2 109 440 | 2 352 048 | 3 817 472 |
| x86_64 P4.1 | 194 752 | 2 097 152 | 2 291 904 | 5 742 592 |
| x86_64 P5 | 242 416 | 2 097 152 | 2 339 568 | 6 373 376 |

La charge de référence respecte les 7 Mio (7 340 032 octets) pour le tas Rust et la pile de travail. La pile réservée au thread principal du lanceur macOS ajoute 8 372 224 octets si l’on comptabilise toutes les réservations du processus. L’allocation physique du cache APFS, l’endurance du SSD et la perte d’alimentation du Mac n’ont pas été mesurées.

[Échantillons et distributions par architecture, version et secteur](validation/v0.9.6/mesures) · [Client du banc](../lab/p5_mesures.rs) · [Compilation, exécution et calculs](../lab/p5-mesures.py)

## Reproduction

Depuis la racine du monorepo sur macOS Apple Silicon avec Rosetta, Rust stable et les deux cibles Darwin. Le test P1 de coupure VM utilise aussi QEMU x86_64 et EDK II.

```sh
set -eu
tmp=$(mktemp -d)
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --all-features
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    NOVAFS_POOL_FIXTURE="$tmp/pool-$arch.img" novaFS/lab/run test --offline --test volumes --test cout_extents -- --ignored --nocapture
    novaFS/lab/run test --offline --test migration_p5 --test p5_partage --test p5_restitution --test p5_export -- --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
done
unset NOVAFS_TARGET
python3 novaFS/lab/p5-echange.py
python3 novaFS/lab/p5-export.py
python3 novaFS/lab/test-latency-report.py
python3 novaFS/lab/p5-mesures.py --sortie "$tmp/mesures"
for ref in d0ba7cb 38c8699 6f15350 1510eca 0a89d23 b865c58; do
    mkdir "$tmp/$ref"
    git archive "$ref" novaFS | tar -x -C "$tmp/$ref"
    CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py --reference "$tmp/$ref/novaFS/lab/run"
    case "$ref" in
        6f15350|1510eca|0a89d23|b865c58) python3 novaFS/lab/test-pool-interop.py --reference "$tmp/$ref/novaFS/lab/run";;
    esac
done
```
