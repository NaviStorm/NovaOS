# NovaFS P1 v0.9.1 — mesures sur images macOS

## Méthode

Les deux exécutables de release ont tourné successivement sur le même Mac Apple Silicon sous macOS 27.2, avec Rust 1.99.1. ARM64 utilise des secteurs logiques de 4 096 octets et x86_64 sous Rosetta des secteurs de 512 octets, comme dans la campagne P0. Les quatre combinaisons architecture–secteur sont testées séparément pour la récupération et la restitution du tas.

La campagne complète utilise le même banc que P0. L’image principale est un fichier ordinaire temporaire de 96 Mio. Chaque série écrit puis lit 32 Mio par opérations de 64 Kio et effectue 256 lectures et 256 écritures de 4 Kio à des adresses pseudo-aléatoires. Les lectures contrôlent les octets. Chaque écriture mesurée est une transaction avec F1 et F2. Les caches macOS restent actifs. Les opérations de formatage et de montage sont hors des quatre phases de débit ci-dessous. Le banc mesure également les petits fichiers, la contention, la récupération du cache, la fragmentation et une image creuse de 8 Gio.

Le débit est le nombre total d’octets utiles divisé par le temps cumulé des cinq séries. Chaque quantile indiqué est le plus élevé des cinq quantiles par série. Les temps mesurés incluent le moteur et les appels au système hôte. La charge générale du Mac n’a pas été isolée. Les valeurs décrivent ces exécutions sur fichiers temporaires, sans mesure de coupure électrique. [Référence P0](novafs-prototype-macos.md) · [Banc](../lab/novafs-bench.rs)

## Débit et latence

| Architecture | Opération | P0 (Mio/s) | P1 (Mio/s) | P1 p50 / p95 / p99 (ms) | P0 p99 (ms) |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM64 | Lecture séquentielle 64 Kio | 101,896 | 101,726 | 0,631 / 0,949 / 1,382 | 1,548 |
| ARM64 | Écriture séquentielle 64 Kio | 4,767 | 5,326 | 11,711 / 13,452 / 34,602 | 35,112 |
| ARM64 | Lecture aléatoire 4 Kio | 53,469 | 50,190 | 0,075 / 0,106 / 0,117 | 0,100 |
| ARM64 | Écriture aléatoire 4 Kio | 0,377 | 0,409 | 9,599 / 11,327 / 17,376 | 15,895 |
| x86_64 | Lecture séquentielle 64 Kio | 67,556 | 63,103 | 0,983 / 1,187 / 1,590 | 2,119 |
| x86_64 | Écriture séquentielle 64 Kio | 4,888 | 5,154 | 12,340 / 13,681 / 19,458 | 22,696 |
| x86_64 | Lecture aléatoire 4 Kio | 33,651 | 30,386 | 0,140 / 0,171 / 0,192 | 0,165 |
| x86_64 | Écriture aléatoire 4 Kio | 0,335 | 0,390 | 10,396 / 12,351 / 13,716 | 158,487 |

Chaque cible a enregistré 5 120 synchronisations pour 2 560 transactions séquentielles et 2 560 synchronisations pour 1 280 transactions aléatoires. Les quatre phases de lecture ont enregistré zéro synchronisation. Le coût moyen d’une synchronisation est le temps cumulé dans `FileImage::flush` divisé par le nombre d’appels.

| Architecture | Écritures | P0 (ms/flush) | P1 (ms/flush) | Temps cumulé P1 en flush (s) |
| --- | --- | ---: | ---: | ---: |
| ARM64 | Séquentielles | 4,908 | 4,572 | 23,408 |
| ARM64 | Aléatoires | 4,343 | 4,321 | 11,062 |
| x86_64 | Séquentielles | 4,459 | 4,718 | 24,157 |
| x86_64 | Aléatoires | 4,720 | 4,328 | 11,080 |

La campagne a duré 167,05 s sur ARM64 et 187,60 s sur x86_64. Les deux campagnes ont atteint 324 pages avant refus sur l’image de pression, puis réemployé 157 anciennes adresses de données, comme en P0. Les temps de lecture fluctuent jusqu’à −9,7 % par rapport à P0. Une seule campagne par cible et version, sur un Mac également utilisé pour le développement, ne permet pas d’attribuer cette différence au moteur. [Journal complet ARM64](validation/v0.9.1/bench-aarch64.txt) · [Journal complet x86_64](validation/v0.9.1/bench-x86_64.txt)

## Mémoire et restitution

| Architecture | Pic du tas P1 (octets) | Tas + réservations des piles P0 (octets) | Tas + réservations des piles P1 (octets) | RSS max P0 (octets) | RSS max P1 (octets) |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 | 1 188 512 | 3 809 920 | 3 809 952 | 5 783 552 | 5 881 856 |
| x86_64 | 1 188 176 | 3 809 584 | 3 809 616 | 14 790 656 | 14 798 848 |

L’allocateur du banc compte chaque allocation Rust vivante selon sa capacité effective Darwin (`malloc_size`), instrumentation comprise. La réservation des piles des deux threads de travail totalise 2 621 440 octets. Le pic du tas augmente de 32 octets sur chaque architecture par rapport à P0. Le total tas + piles de travail tient dans les 7 340 032 octets visés. Cette comptabilité exclut le noyau macOS, ses caches et les allocations internes des bibliothèques natives. Le nombre de pages physiquement résidentes du seul moteur n’est pas disponible.

La RSS maximale du processus x86_64 dépasse le plafond moteur de 7 458 816 octets. Elle inclut aussi le code, les bibliothèques et l’environnement Rosetta. `time -l` ne permet pas d’attribuer chacune de ces pages. L’empreinte maximale comptée séparément par macOS vaut 4 587 976 octets sur ARM64 et 13 727 184 sur x86_64.

Après une chauffe, chaque cycle du mode `reprise` formate un fichier temporaire, effectue deux transactions, relit et vérifie en lecture seule puis remonte en écriture. Le banc compte quatre synchronisations pour les deux transactions, zéro pour le montage RO et une pour le montage RW. Dans le même cycle, une seconde instance sur support simulé provoque un échec de F2, perd les écritures volatiles, remonte et vérifie le reçu et les octets précédents. Après destruction des moteurs, des supports et de leurs caches, le compteur du tas vivant doit revenir exactement à la valeur initiale **à chaque cycle**. [Relevés des quatre campagnes](validation/v0.9.1/reprise.txt)

Les 64 cycles ont réussi dans chacun des quatre couples architecture/secteur. Sur ARM64, les 960 octets de la ligne de base ont été retrouvés après chaque cycle, avec un pic de 120 832 octets. Sur x86_64, la ligne de base était de 864 octets et le pic de 110 896 octets. Ces 256 contrôles portent sur les allocations Rust des chemins exercés.

## Reproduction

Depuis la racine du dépôt, avec les deux cibles Darwin, les dépendances Cargo déjà disponibles et Rosetta pour l’exécutable x86_64.

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run build --offline --bin novafs-bench
done
for arch in aarch64 x86_64; do
    bin="novaFS/target/$arch-apple-darwin/release/novafs-bench"
    case "$arch" in aarch64) sector=4096 ;; x86_64) sector=512 ;; esac
    /usr/bin/time -l "$bin" "$sector" complet || exit 1
    for size in 512 4096; do "$bin" "$size" reprise || exit 1; done
done
```

Si `CARGO_TARGET_DIR` est défini, utiliser ce répertoire à la place de `novaFS/target`. La RSS est celle de l’exécutable de mesure lancé directement. Les deux campagnes livrées ont utilisé des copies figées des exécutables. Les empreintes SHA-256 enregistrées sont `7b0b76c6fc8ce17c53da65a9204ffebeacef748ccbde9c3f904d8d004d8e66bd` sur ARM64 et `2beec8f147c5466856ec7d1516d382c2cf3edfc7f63d11a72fe314f114f4e88b` sur x86_64. Les binaires Mach-O contiennent chacun la seule architecture indiquée. Sur ce Mac, `sysctl.proc_translated` renvoie 0 sous ARM64 et 1 sous x86_64.
