# Prototype macOS — coupures, volumes et échanges d’images

Campagne de laboratoire issue de la base `ab0b5c1`. Les exécutables Rust de release ont tourné sur macOS ARM64, en AArch64 natif et en x86_64 sous traduction. Les durées ci-dessous sont celles du harnais de tests, compilation exclue. [Contrat de récupération](novafs-recovery.md) · [Plan et oracles](novafs-test-plan.md)

| Exécutable de test | Scénarios | AArch64 | x86_64 |
| --- | --- | --- | --- |
| `crash` | N02, N03, N05 | 3 réussis en 2,13 s | 3 réussis en 2,83 s |
| `volumes` | P01, P02, corruption du catalogue et contre-exemples de barrières | 4 réussis en 1,98 s | 4 réussis en 3,15 s |
| `interop` | F01, vecteurs et transport de la fixture | 2 réussis en 0,01 s | 2 réussis en 0,01 s |

## Périphériques utilisés

`FileImage` crée un fichier temporaire anonyme, vérifie qu’il s’agit d’un fichier ordinaire et ne donne accès qu’aux blocs de sa géométrie. Le formatage, le démontage, l’inspection et la vérification conservent le descripteur déjà ouvert. L’interface du périphérique ne prend aucun chemin d’image ou de disque. Un flush de fichier appelle `sync_all` et propage l’erreur du système hôte. [Périphériques du laboratoire](../lab/device.rs)

`SimDevice` conserve séparément la dernière écriture volatile par bloc et l’image stable. Ses points d’arrêt sont l’entrée dans une écriture, la soumission, la complétion, la soumission d’un flush, son effet stable et sa complétion. L’injection renvoie une erreur d’E/S et interrompt le service. Le contrôleur du banc peut ensuite perdre le cache ou conserver certains octets. Ses instantanés RAM créent des branches indépendantes pour rejouer le même scénario. Les images du simulateur sont bornées à 32 768 blocs, les journaux de commandes à 8 192 événements. Ce stockage du banc est compté séparément de la mémoire du moteur.

## N02 — transactions interrompues

Chaque ligne commence par une image valide qui contient les marqueurs distincts des clients A et B. Une exécution témoin relève le manifeste et le contenu du répertoire avant et après la mutation. Le banc interrompt ensuite la même opération à chaque événement du périphérique. Il rejoue trois ordres de persistance des écritures encore volatiles : tout perdre, conserver la plus récente, tout conserver dans l’ordre inverse des soumissions. Les deux générations autorisées doivent présenter exactement le répertoire et les octets attendus. Avant publication de l’ancre, seule l’ancienne racine convient. Dès l’effet stable du second flush, la nouvelle est exigée même si la réponse n’a pas été livrée. [Tests sur les six opérations](../tests/crash.rs)

| Mutation observée | Frontières d’E/S | Coupures par architecture |
| --- | ---: | ---: |
| Modification de deux blocs de données | 27 | 81 |
| Renommage | 21 | 63 |
| Suppression logique | 24 | 72 |
| Collecte physique des extents retirés | 21 | 63 |
| Réemploi d’au moins un bloc déjà écrit dans une génération antérieure | 27 | 81 |
| Création entraînant la montée d’un niveau de la racine de l’arbre des noms | 33 | 99 |
| **Total** | **153** | **459** |

À ces coupures s’ajoute un arrêt après le retour de succès pour chacune des six mutations. Chaque image est remontée et vérifiée. Les trois ordres couvrent ces fenêtres déterministes. Ils ne constituent pas une énumération de toutes les permutations indépendantes de toutes les écritures possibles.

## N03 et N05 — secteurs déchirés et confirmation

- Juste avant F2, le banc a exploré les 256 choix d’anciens ou de nouveaux secteurs de 512 octets pour l’ancre de 4 Kio. Il a retrouvé 128 anciennes et 128 nouvelles générations, entièrement vérifiables. Dans cette enveloppe P0 les octets variables occupent les 184 premiers octets. Avec un secteur logique de 4 096 octets, les deux choix ont donné une ancienne et une nouvelle génération.
- Une déchirure **dans** le secteur, après 96 octets de la nouvelle ancre, donne une ancre illisible et un refus du montage normal. Le secours ouvert explicitement sur l’ancre survivante extrait les marqueurs, refuse les mutations et laisse l’image identique octet par octet. Son journal compte zéro écriture et zéro flush. Une injection supplémentaire endommage aussi l’ancre voisine. Dans cette configuration hors du modèle de confinement, les deux extractions échouent.
- Après l’arrêt du service entre l’écriture d’ancre et F2, le cache du périphérique peut encore présenter la nouvelle tête. Le montage RW la stabilise avant retour. Une deuxième coupure à chacune des trois frontières de ce flush a laissé la génération correspondant aux octets effectivement stables. Si ce flush échoue, le nouvel écrivain n’est pas livré.
- **F1 ignoré par le simulateur.** L’ancre seule devient stable, une dépendance manque et le vérificateur refuse le montage. **F2 ignoré.** Le moteur a reçu un succès mensonger du périphérique et a confirmé une nouvelle racine. Après perte du cache, l’ancienne paire cohérente remonte. Le reçu indépendant détecte la confirmation perdue. Ces deux expériences sont répétées pour le catalogue de pool.
- Le témoin associe identité de pool, de LV et de FS au numéro de génération et au digest du manifeste. Le test distingue égalité, prédécesseur lié, tête plus récente attestée mais inaccessible, divergence de digest, mauvaise lignée et absence de preuve de filiation. Il détecte le remplacement intégral par une ancienne image valide. Un bloc de données remis à zéro après confirmation provoque une erreur d’intégrité. Un reçu ne reconstitue pas ses données.

## P01 et P02 — deux niveaux indépendants de publication

Le gestionnaire de volumes utilisé par ces deux scénarios est une fixture réservée aux tests. Il sérialise réellement un catalogue mono-PD avec deux ancres physiques et trois bandes fixes de 64 Kio. Le moteur NovaFS ne reçoit qu’une vue bornée d’un LV. Un emprunt exclusif maintient la géométrie pendant les E/S synchrones. L’essai conserve aussi un export non rendu et vérifie le refus d’une mutation du catalogue. [Fixture du pool](../tests/support/pool.rs) · [Essais de volumes](../tests/volumes.rs)

P01 exerce les secteurs 512 et 4 096, les deux extrémités des plages, l’épuisement des grains, les exports RAW/FS et le prêt d’un LV en préparation au seul formateur. Sept catalogues malformés sont présentés avec des checksums recalculés : identifiant LV dupliqué, recouvrement, hors PD, grain de métadonnées, trou logique, longueur débordante et deuxième membre requis. L’ancienne génération retarde la réattribution. Après rotation des racines et nouveau provisionnement, chacun des 256 blocs d’un grain relu par son nouveau détenteur est nul. Le propriétaire administratif figure dans le catalogue. L’identité de l’ancien détenteur est refusée sur le nouvel export RAW, avant et après réimport. Le test atteint le quota de quatre grains d’un domaine : un second LV de deux grains est refusé après un premier de trois grains, alors que le même LV est admis pour un autre domaine ayant du crédit.

Pour P02, toutes les écritures de mise à zéro, de catalogue et d’ancrage des quatre opérations de pool ci-dessous sont interrompues successivement, puis l’image est importée avec perte du cache ou persistance de sa dernière écriture. L’essai de formatage interne du LV interrompt séparément chacune de ses frontières, avec perte du cache. Une création interrompue après réservation conserve un LV en préparation. Publier l’état prêt exige un FS cohérent aux bonnes identités. Après reprovisionnement du même identifiant de LV et réemploi du même numéro d’objet dans un nouveau FS, l’ancien profil de montage est rejeté.

| Étape | Coupures par architecture |
| --- | ---: |
| Préparation du LV, mise à zéro incluse | 1 650 |
| Formatage NovaFS du LV réservé | 21 |
| Publication de l’état prêt | 114 |
| Suppression du LV | 114 |
| Réattribution du grain, deuxième mise à zéro incluse | 1 650 |
| **Total** | **3 549** |

Ce banc couvre un seul PD, un seul administrateur de test et des E/S séquentielles terminées à l’arrêt. Il ne qualifie aucune exclusion d’écrivains distants ni aucun arrêt matériel du DMA. Les droits et le lancement des processus restent en dehors du banc de volumes.

## F01 — compatibilité croisée

Les deux exécutables Mach-O ont produit chacun une image en géométrie 512 et en géométrie 4 096. Les quatre images de 1 572 864 octets étaient identiques, d’empreinte SHA-256 `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. Huit transferts de l’image entière par pipe ont été exécutés : deux architectures productrices, deux géométries sources et deux géométries chez le lecteur de l’autre architecture. Chaque lecteur a monté en lecture seule, vérifié l’image intégralement et comparé les octets, notamment deux noms Unicode distincts, une écriture traversant une frontière de bloc et un fichier creux. Les vecteurs supplémentaires contrôlent une somme de données calculée indépendamment avec `hashlib`, l’ordre des entiers, un décalage d’origine et le rejet d’une version et de fonctionnalités inconnues. [Vecteurs Rust](../tests/interop.rs) · [Échange entre exécutables](../lab/test-interop.py)

## Reproduction

Depuis la racine du dépôt, sur un Mac avec les cibles Rust Darwin installées et les deux architectures exécutables :

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run test --test crash --test volumes --test interop -- --nocapture
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run test --test crash --test volumes --test interop -- --nocapture
python3 novaFS/lab/test-interop.py
```

La coupure du support est simulée en mémoire. Les syncs et lectures sur fichiers ordinaires temporaires exercent le chemin d’E/S de macOS. Aucune expérience de perte d’alimentation du Mac, du contrôleur ou d’un disque physique n’a été réalisée.
