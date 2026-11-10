# Profil de lecture NovaFS P4.1

La feature Cargo `latency-probes` active les sondes du laboratoire. Le profil utilise une image temporaire, un seul thread de lecture, le cache NovaFS de 256 pages et le cache de fichiers macOS dans son mode ordinaire. Les deux cibles ont tourné successivement sur le Mac également utilisé pour les bancs transactionnels. Les campagnes de comparaison de latence disposent de leur propre créneau. La calibration précède la création du fichier. Le moteur vérifie les données lues et la totalité de l’image en fin d’essai.

| Sonde | Intervalle mesuré |
| --- | --- |
| `lecture_moteur_64k` | Appel à `Engine::read` pour une requête de 64 Kio. Le banc vérifie ensuite tous les octets rendus et libère le vecteur. |
| `recherche_cache` | Prise du verrou, contrôle de santé et de référence, recherche, copie du bloc sur succès et libération du verrou. |
| `defaut_donnees` | Après l’absence du bloc en cache, lecture de l’image, checksum de la page de données et admission jusqu’au retour du bloc validé. |
| `defaut_metadonnees` | Après un défaut sur un nœud, lecture de l’image, décodage et validation du nœud, puis admission dans le cache. |
| `eviction_clock` | Choix d’une victime dans le cache plein par CLOCK, déchaînage, remplacement des 4 Kio et insertion dans le seau. |
| `parcours_extent` | Recherche de l’extent couvrant l’offset, parcours des nœuds, décodage des références et validation des bornes. |
| `lecture_image` | Appel à `FileImage::read` portant sur 4 Kio, avec vérification d’adresse et lecture positionnelle sur le fichier temporaire. |
| `allocation_system`, `liberation_system` | Appels à `System.alloc` et `System.dealloc` du thread, y compris ceux des décodeurs et du tampon de sortie. Les compteurs du tas et `malloc_size` sont en dehors de ces intervalles. |
| `sonde_vide` | 8 192 paires d’horodatages sans travail applicatif pour quantifier le plancher du dispositif. |

Le lecteur commence par un parcours de 4 Mio depuis un cache NovaFS vide, puis effectue huit parcours sur ces mêmes 4 Mio. Il purge ensuite le cache et précharge 512 Kio avant 64 parcours du petit ensemble chaud. Chaque phase vérifie que le nombre de recherches égale les hits plus les défauts, que chaque défaut correspond à une lecture d’image, que le nombre d’évictions et d’allocations correspond aux compteurs indépendants et que l’ensemble chaud n’occasionne aucune E/S.

Les temps muraux proviennent d’`Instant`. Les temps CPU additionnent le travail du thread en mode utilisateur et noyau comptabilisé par Darwin. Le banc utilise `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` en natif et les deux `time_value_t` de Mach `THREAD_BASIC_INFO` sous Rosetta. Une lecture de fichier peut être servie par le cache macOS. L’écart mur–CPU peut aussi provenir de la désactivation temporaire du thread ou de la résolution des horloges. L’allocation, l’E/S et l’éviction sont imbriquées dans leurs appelants. Additionner toutes les colonnes compterait plusieurs fois les mêmes durées. Les sondes intérieures ajoutent du travail aux sondes extérieures. Les campagnes de référence se compilent sans cette feature.

Les histogrammes de chaque thread ont une taille fixe. Le champ `etat_sondes_tls_octets` comptabilise cet état hors du tas Rust. Chaque statistique publie son effectif, sa somme, son maximum exact et la borne supérieure du rang plafond pour p50, p95 et p99. Les 16 premières classes représentent chacune une nanoseconde, les suivantes partagent chaque puissance de deux en huit classes. L’écart entre la borne et l’échantillon correspondant est au plus de 12,5 %. La valeur `null` signale une population vide. Les mesures diagnostiques incluent des coûts de sonde. La sortie de calibration permet aussi d’interpréter les quantiles proches de la résolution de l’horloge CPU.

## Étalonnage de l’horloge CPU

Le premier essai du profil a révélé que, sur ce Mac, la valeur brute de `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` du binaire x86_64 traduit sous-estimait le temps CPU d’un facteur voisin de 41,67. Un essai de calcul a confronté quatre lectures indépendantes de l’interface de mesure. Sous Rosetta, les deux variantes `clock_gettime_nsec_np` et `clock_gettime` ont donné 2,692 ms, Mach `thread_info` a donné 112,165 ms et `getrusage(RUSAGE_SELF)` 112,172 ms. En ARM64 natif, les quatre valeurs étaient entre 161,747 et 161,750 ms. Corriger la mesure par multiplication aurait imposé une hypothèse sur la machine. Le chemin x86_64 interroge directement les secondes et microsecondes du thread. Il utilise le port emprunté au pthread appelant et un tableau fixe de dix entiers sur la pile. Ces appels n’emploient pas l’allocateur Rust.

Les bancs valident leur horloge avant de lancer leurs threads. Un calcul consomme au moins 120 ms de CPU dans le processus monothread. La valeur du thread doit rester à moins de 5 % de la valeur du processus mesurée par `getrusage`. Une pause d’au moins 120 ms doit ensuite coûter moins de 12 ms de CPU selon les deux sources. Le JSON publie également la valeur brute de l’ancienne horloge. Les valeurs suivantes proviennent des deux exécutions finales.

| Cible, phase | Mur (ms) | CPU processus (ms) | CPU thread retenu (ms) | Ancienne horloge, valeur brute divisée par 10⁶ |
| --- | ---: | ---: | ---: | ---: |
| ARM64, calcul | 120,062 | 120,036 | 120,034 | 120,034 |
| ARM64, pause | 124,194 | 0,011 | 0,009 | 0,010 |
| x86_64 Rosetta, calcul | 121,147 | 120,016 | 120,009 | 2,880 |
| x86_64 Rosetta, pause | 120,574 | 0,033 | 0,027 | 0,001 |

Sur 8 192 intervalles vides par cible, les bornes p50/p99 du mur sont de 175/255 ns en ARM64 et de 1 023/1 279 ns sous Rosetta. Le p50 du CPU d’une sonde vide traduite est nul car Mach exprime les deux temps cumulatifs en microsecondes. Les distributions de l’allocateur et du cache proches de ces bornes renseignent autant sur le coût de mesure que sur l’opération elle-même.

## Résultats du parcours répété de 4 Mio

Les 512 lectures de 64 Kio de chaque cible ont déclenché 8 336 défauts et 8 336 évictions. CLOCK a examiné 16 883 descripteurs. Les valeurs p99 du tableau sont des bornes supérieures, en microsecondes.

| Périmètre | Échantillons par cible | p99 mural ARM64 | p99 mural Rosetta |
| --- | ---: | ---: | ---: |
| Lecture du moteur 64 Kio | 512 | 327,679 | 983,039 |
| Recherche cache | 11 776 | 0,479 | 1,919 |
| Défaut de données, E/S et admission comprises | 8 192 | 13,311 | 40,959 |
| Défaut de métadonnées, décodage compris | 144 | 30,719 | 73,727 |
| Remplacement CLOCK | 8 336 | 0,575 | 3,839 |
| Parcours de l’extent | 1 024 | 53,247 | 262,143 |
| Lecture d’image | 8 336 | 4,607 | 6,143 |
| Allocation System | 63 264 | 0,319 | 1,535 |
| Libération System | 63 264 | 0,319 | 1,535 |

En cumul, le moteur passe 142,157 ms muraux et 141,937 ms CPU dans les appels de lecture natifs, contre 348,410 ms muraux et 341,791 ms CPU sous Rosetta. Le petit ensemble chaud totalise 11 776 touches, zéro défaut, zéro lecture d’image et zéro éviction dans les deux exécutions. La somme des tailles des objets de sondes locaux au thread est de 80 017 octets, en plus de la mémoire du moteur. Ces résultats d’attribution ne permettent pas d’assigner une cause à une requête historique individuelle. Les fichiers bruts détaillent aussi le premier parcours, les distributions CPU et les maxima exacts. [Profil ARM64](validation/v0.9.5/sondes-aarch64.jsonl) · [Profil Rosetta](validation/v0.9.5/sondes-x86_64.jsonl)

## Reproduction

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run run --offline --features latency-probes --example profile-read -- 4096
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run run --offline --features latency-probes --example profile-read -- 512
```

## Témoin de lecture avec l’allocateur System

Le témoin utilise les archives `.rlib` et les cinq images identifiées par la campagne comparative. Son lecteur utilise directement `FileImage` et l’allocateur Rust `System`. Le seul chronométrage par requête porte sur la lecture complète de 64 Kio, le contrôle de chaque octet et la libération du résultat. Le calcul de débit chronomètre la boucle de 512 requêtes. Le banc contrôle le cache en dehors de cette boucle. Les tailles de pile du thread principal et du lecteur proviennent de `pthread_get_stacksize_np` sur chacun des deux threads. Le système hôte garde le cache des fichiers actif et le lecteur le préchauffe comme dans la condition `buffered` de la comparaison principale.

Chaque moteur passe une fois à chacune des trois positions sur trois campagnes appariées. Le témoin permet de vérifier si la conclusion sur le p99 et le débit de lecture persiste quand les compteurs de l’allocateur et les sondes d’E/S et de CPU sont tous retirés. L’écart par rapport à la campagne instrumentée intègre leurs coûts cumulés et la variabilité entre créneaux. L’empreinte du tas se mesure dans la campagne instrumentée.

Après `measure-latency.py preparer`, conserver son répertoire de travail pour cette commande. Le témoin vérifie les empreintes des images et des binaires précédents, puis enregistre les empreintes des bibliothèques réutilisées et des nouveaux lecteurs.

```sh
python3 novaFS/lab/temoin.py --travail "$tmp/banc" --sortie "$tmp/resultats"
```

| Cible et cache NovaFS | p99 P3 / P4 / P4.1 (ms) | Débit P3 / P4 / P4.1 (Mio/s) |
| --- | ---: | ---: |
| ARM64, froid 32 Mio | 0,678 / 0,449 / 0,458 | 105,483 / 209,927 / 209,003 |
| ARM64, chaud 512 Kio | 0,542 / 0,470 / 0,468 | 135,458 / 294,457 / 293,396 |
| Rosetta, froid 32 Mio | 1,107 / 0,726 / 0,710 | 65,060 / 134,241 / 134,425 |
| Rosetta, chaud 512 Kio | 0,900 / 0,768 / 0,768 | 83,029 / 183,452 / 183,825 |

Chaque valeur p99 repose sur 7 680 requêtes, avec 92 160 chronométrages sur 18 exécutions. Dans chacune des trois campagnes et des quatre conditions, P4 obtient un p99 inférieur à P3. Par rapport à P4, P4.1 varie dans les deux sens selon la campagne. Les deux architectures ont réservé exactement 2 097 152 octets pour le thread principal. La pile effective du lecteur a été de 2 109 440 octets sur ARM64, soit 12 288 de plus que la demande Rust, et de 2 097 152 sous Rosetta. Ces tailles étaient identiques dans toutes les répétitions d’une même architecture. [Distributions, empreintes et piles par processus](validation/v0.9.5/temoin.json) · [Requêtes individuelles dans leur ordre d’exécution](validation/v0.9.5/temoin.jsonl.gz)
