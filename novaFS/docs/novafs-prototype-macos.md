# NovaFS 0.9.0 — résultats du prototype macOS

**Le moteur P0 fonctionne sur des images jetables.** Les quinze scénarios du plan ont été exécutés avec le moteur Rust sur macOS ARM64 natif et avec un exécutable macOS x86_64 sous traduction sur le même Mac. Les droits et les files de clients ainsi que le catalogue de pool ont été exercés dans le banc hôte. Les opérations de fichiers, les B+Trees, les extents, les vérifications de blocs et les commits utilisés par ces essais sont ceux du moteur livré. [Guide et commandes](../README.md) · [Format expérimental](novafs-disk-format.md) · [Plan de référence](novafs-test-plan.md)

## Périmètre et reproductibilité

Base Git `ab0b5c1`, Rust stable 1.99.1, macOS 27.2. Le paquet hôte a son propre verrou Cargo. Le commit de livraison du prototype est `8820a58`. Les deux suites de régression existantes `make ARCH=x86_64 test` et `make ARCH=aarch64 test` ont également réussi sur le noyau v0.8.0. Ces régressions QEMU ne mesurent pas le stockage.

Le formateur a utilisé des fichiers ordinaires temporaires anonymes. Les coupures ont été injectées dans un périphérique RAM distinct qui sépare le cache volatil du support stable. L’échange interarchitecture passe par des tubes entre deux exécutables Mach-O identifiés. Les essais de performances utilisent les écritures et synchronisations réelles de fichiers macOS. Aucun disque physique n’a été ouvert.

## Quinze scénarios

| Cas | Sujet | ARM64 | x86_64 | Observation |
| --- | --- | --- | --- | --- |
| V01 | Ouvertures et curseurs | Réussi | Réussi | Deux ouvertures du même fichier, lecture partielle à EOF et au bord de 4 Kio, seek hors limites, handle fermé puis réutilisation du slot. |
| V02 | Noms et confinement | Réussi | Réussi | UTF-8 invalide rejeté, deux écritures Unicode distinctes, chemins de 1 024/1 025 octets, noms de 255/256, profondeur 32/33, racine et résolution ordonnée de .. dans le modèle. |
| V03 | Mutations de namespace | Réussi | Réussi | Deux créations concurrentes du même répertoire, rename entre parents, conflit de destination, cycle, déplacement intermontages refusé et unlink d’un objet ouvert ou d’un répertoire non vide. |
| V04 | Autorité | Réussi | Réussi | Port et incarnation liés à la session. Handle étranger, voie RO, quota de 32 et absence de création au refus de la 33e ouverture. Client silencieux à l’amorçage et progression de l’autre au temps simulé. |
| V05 | Fragments et rejeu | Réussi | Réussi | 43 trames pour une écriture de 4 Kio, fragments altérés, incomplets ou hors ordre, FIFO pleine, perte puis rejeu de réponse, acquittement avec doublon encore en file et fermeture de session. |
| V06 | Concurrence | Réussi | Réussi | Threads hôte concurrents, opérations atomiques sur plages recouvrantes, copie de lecture conservée en privé, les deux ordres open via alias RO contre unlink RW, confinement après déplacement. |
| N01 | Arbres et extents | Réussi | Réussi | Scission, fusion et contraction de racine, recherches différentielles, écritures partielles sur des extents, trous initialisés à zéro, collecte puis réutilisation de mêmes adresses physiques. |
| N02 | Coupures | Réussi | Réussi | 459 coupures par architecture sur six mutations, dont split, rename, unlink et collecte. Les marqueurs durables et le manifeste attendu ont été vérifiés après reprise. |
| N03 | Ordre et déchirure | Réussi | Réussi | Complétions perdues, réordonnancement, déchirure interne au secteur, 256 masques de secteurs de 512 octets et deux choix sur 4 096. Les témoins négatifs sans F1 et sans F2 ont révélé les garanties perdues. |
| N04 | Images hostiles | Réussi | Réussi | Checksums, offsets, types, hauteurs et propriétés entre générations contrôlés, y compris des images recalculées avec OID ou bloc réattribué. Le dernier bloc corrompu d’une lecture fait échouer toute la lecture et gèle le montage. |
| N05 | Perte de fraîcheur | Réussi | Réussi | Reçu extérieur, paire valide ancienne, fork de même génération, mauvais FS/pool, ancre ou payload corrompu, refus du montage puis extraction RO. Le banc distingue perte de preuve et perte d’accès. |
| N06 | Pénuries | Réussi | Réussi | Crédit de préparation de 0 à 20 pages refusé sans publication. Succès à 21 pour le cas ciblé. Sur 224 blocs, 77 pages de données avant PLEIN, 77 lots de collecte et une écriture de 64 Kio réussie après reprise. |
| P01 | Volumes et attribution | Réussi | Réussi | Banc de catalogue avec empreintes SHA-256, identités, FS/RAW, droits du propriétaire, quotas épais, arithmétique de segments, invalidation des exports et zéro durable avant réattribution. |
| P02 | Deux niveaux de commit | Réussi | Réussi | 3 549 injections par architecture sur préparation du LV, formatage, publication de l’état prêt, suppression et réattribution. État du pool vérifié avant toute activation du FS. |
| F01 | Format croisé | Réussi | Réussi | Quatre images canoniques identiques octet par octet, huit échanges entre architectures et géométries, somme indépendante, refus de versions, de bits requis et d’endianité altérée. |

Les expériences V01–V06 utilisent des ports et des sessions attribués par le contrôleur de test macOS. Deux threads se disputent un verrou d’écrivain hôte. Les délais de bootstrap y sont simulés. P01–P02 emploient une fixture de catalogue réservée aux tests, avec ses propres ancres et sa table de volumes. Ces résultats laissent ouverts l’ordonnancement, les droits DMA et le lancement de futurs services NovaOS. [Banc clients](../tests/clients.rs) · [Rapport détaillé des pannes et des volumes](novafs-prototype-pannes.md) · [Transferts F01 exécutés](validation/v0.9.0/echange.txt)

## Propriétés et vérification indépendante

| Cible macOS | Tests courts réussis | Opérations différentielles | Têtes modifiées observées | Pénuries injectées | Remontages et audits | Durée différentielle (s) |
| --- | --- | --- | --- | --- | --- | --- |
| aarch64 | 40 | 1 000 000 | 345 183 | 54 643 | 1 000 | 24,509 |
| x86_64 | 40 | 1 000 000 | 345 183 | 54 643 | 1 000 | 39,621 |

Chaque campagne longue utilise 100 graines de 10 000 opérations. Un modèle indépendant de noms, identités et vecteurs d’octets fournit l’oracle des réponses et du contenu après remontage. Les pénuries utilisent le crédit de pages de construction du moteur. Les allocations de petite taille de la bibliothèque Rust peuvent encore faire échouer le processus en cas d’épuisement réel de la mémoire de macOS. Le banc ne provoque pas une pénurie générale du Mac.

Un corpus complémentaire de 10 000 mutations de cellules, avec SHA-256 recalculé, a entraîné 8 688 rejets et 1 312 décodages structurellement canoniques. Chaque nœud accepté a été réencodé à l’identique. La validation sémantique de la propriété des blocs s’exerce séparément sur des images complètes. Les essais de CLI passent par les véritables commandes de création, script, inspection, vérification et extraction d’un slot en lecture seule. [Propriétés](../tests/properties.rs) · [Mutations du décodeur](../tests/decoder.rs) · [Résultats ARM64](validation/v0.9.0/tests-aarch64.json) · [Résultats x86_64](validation/v0.9.0/tests-x86_64.json)

## Performances sur fichiers temporaires

Deux campagnes successives de release, image principale de 96 Mio. Chaque mesure ci-dessous agrège cinq séries sur le fichier de 32 Mio. Les écritures incluent les deux flushs de la transaction. Les lectures incluent le contrôle des octets. Les secteurs diffèrent entre les deux campagnes (ARM64 4 096, x86_64 512) et l’exécution x86_64 utilise la traduction. Ces chiffres ne comparent donc pas deux processeurs physiques.

| Opération | ARM64 (Mio/s) | x86_64 (Mio/s) | ARM64 p99 maximal par série (ms) | x86_64 p99 maximal par série (ms) |
| --- | --- | --- | --- | --- |
| Lecture séquentielle 64 Kio | 101,896 | 67,556 | 1,548 | 2,119 |
| Écriture séquentielle 64 Kio | 4,767 | 4,888 | 35,112 | 22,696 |
| Lecture aléatoire 4 Kio | 53,469 | 33,651 | 0,100 | 0,165 |
| Écriture aléatoire 4 Kio | 0,377 | 0,335 | 15,895 | 158,487 |

Le banc a aussi créé et relu 4 096 petits fichiers, comparé des lectures après purge du cache moteur à leur répétition, mesuré deux clients concurrents et réalisé une vérification intégrale. Le cache fichier de macOS reste actif. Les relevés par série donnent les distributions p50/p95/p99/max, les volumes utiles et physiques, le nombre et le temps cumulé des flushs. Après les écritures aléatoires, le fichier principal passe de 1 579 extents en fin de première série à 3 792 en fin de cinquième. Sur l’image de pression de 480 blocs, les 324 premières pages sont réparties en 324 extents. Après collecte, 157 adresses physiques de données ont été réemployées pour les 162 nouvelles pages. [Séries ARM64](validation/v0.9.0/bench-aarch64.tsv) · [Séries x86_64](validation/v0.9.0/bench-x86_64.tsv)

## Mémoire

Le compteur du binaire de mesure utilise la capacité réelle des allocations Rust vivantes arrondie par `malloc_size`, instrumentation comprise. Deux piles de travail sont bornées et comptées à hauteur de leur réservation, même lorsqu’elles ne sont pas toutes deux actives. La RSS maximale et l’empreinte du processus proviennent séparément de `/usr/bin/time -l` et englobent notamment l’exécutable, les bibliothèques et, pour x86_64, la traduction. Le cache de fichiers et la mémoire du noyau macOS échappent à l’enveloppe moteur.

| Cible | Pic du tas suivi (octets) | Piles de travail réservées | Tas + piles (octets) | RSS max du processus (octets) | RSS au-delà de 7 Mio (octets) |
| --- | --- | --- | --- | --- | --- |
| aarch64 | 1 188 480 | 2 621 440 | 3 809 920 | 5 783 552 | 0 |
| x86_64 | 1 188 144 | 2 621 440 | 3 809 584 | 14 790 656 | 7 450 624 |

Le budget proposé est de **7 340 032 octets**. Sur les charges mesurées, le total tas suivi + piles de travail reste en dessous de ce plafond. La RSS du processus x86_64 traduit dépasse ce seuil de 7 450 624 octets. Le relevé macOS ne ventile pas ce dépassement entre traduction, bibliothèques et autres pages résidentes du processus. Le rapport donne aussi l’empreinte macOS, le pic de chaque phase et la vérification d’une image creuse de 8 Gio avec deux bitmaps. Aucun de ces essais ne mesure le pic d’un futur système NovaOS complet de 128 Mio. [Mesures ARM64](validation/v0.9.0/bench-aarch64.json) · [Mesures x86_64](validation/v0.9.0/bench-x86_64.json)

## Limites de cette livraison

- Une transaction s’arrête à 64 blocs CoW neufs et 64 Kio de données utilisateur. Le tableau d’espace est limité à 4 096 intervalles. Un refus de complexité est possible sur une image très fragmentée. L’arbre d’espace est reconstruit sous ces bornes, les autres arbres réécrivent les chemins concernés. Le contrôle intégral est plus coûteux que le contrôle à la demande.
- Le cache propre à seconde chance retient au plus 256 blocs de contenu, plus leurs clés. Son éviction est explicite sous pression. La vérification intégrale relit le périphérique, y compris si une copie en cache reste lisible.
- Le montage normal exige deux ancrages admissibles. Une paire ancienne entièrement cohérente échappe à un nouveau montage privé de preuve indépendante. Le journal de fraîcheur est propre aux tests. Le sauvetage nécessite une génération structurellement lisible et ne peut restituer les octets d’un bloc endommagé.
- Les résultats de coupure portent sur le modèle d’E/S injecté. Les appels de synchronisation du fichier temporaire ont réellement été effectués. La persistance lors d’une perte d’alimentation d’un support physique demeure non qualifiée.
