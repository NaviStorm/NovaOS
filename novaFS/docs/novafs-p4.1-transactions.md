# NovaFS P4.1 — attente FIFO et durées transactionnelles

La lecture séquentielle historique appelle directement `Engine::read`. Elle ne passe pas par le coordinateur, ne prépare aucune transaction et n’envoie aucun flush. Les mesures ci-dessous portent sur les écritures qui empruntent effectivement ces chemins.

## Charge et horloges

Chaque série utilise une image ordinaire temporaire neuve et deux volumes. Un ticket remplace les 4 096 octets d’un fichier existant. Le banc alterne les volumes et compare un seul client à huit clients concurrents. Pour chaque lot, il admet tous les tickets dans un ordre connu, lance les threads en ordre inverse et leur fait franchir un rendez-vous avant leur première tentative. Les trois campagnes de chaque architecture inversent l’ordre des conditions d’une campagne à l’autre. Elles utilisent les secteurs de 512 et 4 096 octets. Le cache de fichiers macOS reste actif.

Chaque ligne du JSON représente un ticket. Les durées sont en nanosecondes. Le temps mur utilise l’horloge monotone de Rust. Le temps CPU Darwin mesure le thread appelant. Leur différence comprend notamment la désactivation du thread, les attentes d’E/S et la concurrence pour un verrou. Les lectures d’horloge successives ont elles-mêmes un coût.

| Champ | Intervalle mesuré |
| --- | --- |
| `attente_prepare`, `attente_commit` | De l’entrée de la première tentative à l’entrée de la tentative acceptée pour cette phase. Zéro lorsque le premier appel réussit. Chaque retour `Busy` conserve le rang. Le client demande une pause de 50 µs avant de réessayer. La durée inclut les appels refusés, cette pause et le délai réel de réveil. |
| `appels_busy_prepare`, `appels_busy_commit` | Somme des temps **dans** les appels qui rendent `Busy` pour cette phase. Ces temps sont déjà inclus dans l’attente. Les compteurs donnent le nombre exact de refus. |
| `prepare` | Appel accepté à `WriteRequest::prepare`, avec ses verrous locaux, la lecture des blocs et la construction du plan CoW. |
| `commit` | Appel accepté à `WriteRequest::commit`, avec la phase exclusive, les écritures des dépendances et de l’ancre, F1, F2, puis la mise à jour de l’état du moteur et le retrait du ticket. |
| `f1`, `f2` | Temps de chacun des deux appels réels à `FileImage::flush` sur le thread de publication. Inclus dans `commit`. La durée restante du commit se calcule par ticket avant agrégation. |
| `ticket`, `rang_f2` | Rang d’admission et numéro observé au retour réussi de F2 sous l’exclusion de publication du pool. Le banc vérifie leur égalité, y compris entre les deux volumes. |

Les temps de préparation et d’attente peuvent se chevaucher entre clients. La première fenêtre d’attente commence au premier appel du client, après le rendez-vous. Le temps de création des threads et l’attente préalable d’un ticket dont le client ne se présente pas ne sont pas comptabilisés. Une fois un appel accepté, le délai local de prise des verrous reste inclus dans sa durée. Les p50, p95, p99 et maxima sont calculés au rang supérieur sur les 128 écritures de chaque condition, hors formatage, remontage, restitution JSON et contrôle de contenu. Les données brutes permettent aussi d’agréger les tickets avant calcul d’un quantile.

## Contrôles

Chaque ticket doit respecter sa génération attendue sur son volume, produire une ou plusieurs écritures de dépendances puis F1, écrire un seul bloc entre F1 et F2 et terminer sans écriture supplémentaire. Le compteur global de F2 détecte un dépassement de rang même si deux candidats appartiennent à des volumes différents. Chaque condition vérifie les deux volumes et relit les données après réimport. Avec huit clients actifs, une série doit enregistrer de la contention et une phase encore refusée par `Busy` après 15 secondes fait échouer le banc. Le client unique doit réussir sa première tentative à chaque phase. Un détenteur inactif en tête peut toujours retarder les suivants. Les temps de flush représentent la réponse de macOS sur l’image.

## Mesures sur macOS

Le banc a exécuté 1 536 transactions de 4 Kio par architecture, en 12 séries. Les tableaux regroupent les échantillons bruts des deux géométries et des trois campagnes, soit **768 transactions par nombre de clients et architecture**. Chaque JSON conserve aussi les quatre quantiles pour chacune des séries de 128. L’ordre global de F2 correspond au rang des 3 072 admissions. Les réimports et vérifications ont abouti après chaque série.

| Cible | Clients | Retours Busy préparation / publication | p99 attente préparation / publication (ms) | p99 préparation / commit (ms) | p99 F1 / F2 (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 natif | 1 | 0 / 0 | 0,000 / 0,000 | 0,689 / 10,511 | 5,974 / 6,092 |
| ARM64 natif | 8 | 270 675 / 56 648 | 58,164 / 10,240 | 0,342 / 10,369 | 6,027 / 6,119 |
| x86_64 Rosetta | 1 | 0 / 0 | 0,000 / 0,000 | 1,185 / 12,019 | 6,744 / 6,166 |
| x86_64 Rosetta | 8 | 276 879 / 59 244 | 63,155 / 10,767 | 0,310 / 11,729 | 6,391 / 6,089 |

À huit clients, les temps muraux des phases acceptées ci-dessous incluent les verrous du moteur et les E/S. La colonne CPU donne le p99 du temps consommé par le thread. Chaque quantile est calculé sur sa propre distribution.

| Cible | Phase | p50 mur (ms) | p95 mur (ms) | p99 mur (ms) | Max mur (ms) | p99 CPU (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 natif | Préparation | 0,119 | 0,138 | 0,342 | 8,309 | 0,316 |
| ARM64 natif | Publication | 8,035 | 8,595 | 10,369 | 12,345 | 0,208 |
| ARM64 natif | F1 | 3,958 | 4,351 | 6,027 | 6,175 | 0,088 |
| ARM64 natif | F2 | 4,110 | 4,251 | 6,119 | 6,235 | 0,061 |
| x86_64 Rosetta | Préparation | 0,179 | 0,217 | 0,310 | 0,736 | 0,245 |
| x86_64 Rosetta | Publication | 8,260 | 9,929 | 11,729 | 14,797 | 0,223 |
| x86_64 Rosetta | F1 | 3,945 | 4,978 | 6,391 | 9,996 | 0,079 |
| x86_64 Rosetta | F2 | 4,159 | 5,174 | 6,089 | 8,810 | 0,085 |

Sur la condition Rosetta mono-client, une publication a atteint **161,150 ms** malgré une médiane de 7,934 ms. Le Mac exécutait les diagnostics du projet sur un créneau distinct des comparaisons de lecture. Le temps mur d’un flush dépend aussi des autres activités de macOS. Les distributions décrivent ce service dans les conditions consignées, sans garantir une latence maximale. Le banc destine au laboratoire ses huit threads hôtes et les tableaux d’échantillons. Les mesures du budget moteur de 7 Mio utilisent les charges de référence dédiées.

- ARM64 natif a exécuté les douze séries entre 2026-11-10T20:53:31Z et 2026-11-10T20:53:46Z (14,447 s). La charge hôte moyenne sur une minute est passée de 23,98 à 19,99. L’étalonnage en charge a mesuré 120,021 ms de CPU du thread et 120,022 ms via `getrusage`.
- x86_64 Rosetta a exécuté les douze séries entre 2026-11-10T20:53:46Z et 2026-11-10T20:54:01Z (15,486 s). La charge hôte moyenne sur une minute est passée de 19,99 à 16,94. L’étalonnage en charge a mesuré 120,048 ms de CPU du thread et 120,056 ms via `getrusage`.

Chaque architecture utilise l’horloge CPU qualifiée au démarrage. Sous Rosetta, `THREAD_BASIC_INFO` fournit des microsecondes. Une opération courte peut donc afficher zéro. Les fichiers contiennent les durées et les refus par ticket, les étalonnages, les bornes temporelles et les charges moyennes. [Données ARM64](validation/v0.9.5/transactions-aarch64.json) · [Données x86_64](validation/v0.9.5/transactions-x86_64.json)

## Contrôle apparié de la charge historique

Le premier rejeu complet P4.1 partageait le Mac avec les autres validations. Son débit séquentiel en écriture sur ARM64, 3,595 Mio/s, était inférieur aux 5,918 Mio/s de la campagne P4 antérieure. Pour contrôler cet écart, le laboratoire exécute successivement les cinq séries mixtes de la campagne d’origine avec le même harnais greffé sur les moteurs P4 et P4.1. Il lance les versions dans l’ordre P4 puis P4.1 sur ARM64 (secteurs de 4 096 octets) et dans l’ordre inverse sous Rosetta (512 octets). Chaque exécutable formate son image de 96 Mio, la préchauffe comme le banc historique, effectue cinq fois 512 écritures et 512 lectures séquentielles de 64 Kio puis 256 lectures et 256 écritures aléatoires de 4 Kio, contrôle les données et démonte l’image vérifiée. La campagne conserve les caches hôtes et ne lance aucun autre banc NovaFS pendant les quatre mesures. Les 4 096 petits fichiers de la suite sont hors de ce contrôle.


Chaque version totalise 2 560 écritures séquentielles et 1 280 écritures aléatoires par architecture. Les quantiles ci-dessous proviennent des opérations individuelles regroupées sur les cinq séries. Les allocations Rust, les blocs lus et les blocs écrits sont identiques entre les deux versions pour chaque phase et architecture. Les comptages des barrières confirment 3 840 F1 et autant de F2 dans chaque passage, aucune dans les deux phases de lecture. [Échantillons et chronométrages des barrières](validation/v0.9.5/rejeu-mixte-apparie.json)

| Cible | Phase | Débit P4 → P4.1 (Mio/s) | p50 P4 → P4.1 (ms) | p95 P4 → P4.1 (ms) | p99 P4 → P4.1 (ms) | Maximum P4 → P4.1 (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 natif | Écriture séquentielle 64 Kio | 5,852 → 6,122 | 10,646 → 9,700 | 12,409 → 12,733 | 13,861 → 14,164 | 156,443 → 203,763 |
| ARM64 natif | Lecture séquentielle 64 Kio | 190,679 → 196,134 | 0,314 → 0,308 | 0,481 → 0,440 | 0,775 → 0,586 | 1,843 → 1,724 |
| ARM64 natif | Écriture aléatoire 4 Kio | 0,431 → 0,484 | 8,625 → 8,137 | 10,672 → 10,608 | 12,356 → 11,326 | 214,698 → 12,772 |
| ARM64 natif | Lecture aléatoire 4 Kio | 51,540 → 54,051 | 0,068 → 0,067 | 0,103 → 0,102 | 0,122 → 0,120 | 2,438 → 0,139 |
| x86_64 Rosetta | Écriture séquentielle 64 Kio | 6,733 → 5,424 | 9,234 → 10,566 | 12,698 → 15,432 | 14,236 → 27,472 | 27,379 → 203,320 |
| x86_64 Rosetta | Lecture séquentielle 64 Kio | 129,492 → 128,316 | 0,472 → 0,473 | 0,667 → 0,673 | 0,976 → 0,974 | 2,418 → 2,141 |
| x86_64 Rosetta | Écriture aléatoire 4 Kio | 0,415 → 0,393 | 8,027 → 10,054 | 11,281 → 13,170 | 14,030 → 15,698 | 247,217 → 29,510 |
| x86_64 Rosetta | Lecture aléatoire 4 Kio | 35,515 → 34,668 | 0,102 → 0,107 | 0,157 → 0,159 | 0,164 → 0,174 | 0,222 → 0,202 |

| Cible | Phase | F1 par écriture P4 → P4.1 (ms) | F2 par écriture P4 → P4.1 (ms) | Hors barrières par écriture P4 → P4.1 (ms) |
| --- | --- | ---: | ---: | ---: |
| ARM64 natif | Écriture séquentielle 64 Kio | 4,034 → 4,033 | 3,882 → 3,743 | 2,764 → 2,434 |
| ARM64 natif | Écriture aléatoire 4 Kio | 3,950 → 3,501 | 3,875 → 3,320 | 1,240 → 1,243 |
| x86_64 Rosetta | Écriture séquentielle 64 Kio | 3,431 → 4,382 | 3,058 → 3,924 | 2,794 → 3,218 |
| x86_64 Rosetta | Écriture aléatoire 4 Kio | 3,872 → 4,181 | 3,573 → 3,642 | 1,970 → 2,107 |

Le déficit de débit séquentiel observé en écriture lors du premier rejeu ARM64 ne se reproduit pas dans cette paire. La moyenne initiale F1 + F2 du banc chargé atteignait 15,591 ms par transaction, contre 7,776 ms pour P4.1 dans ce contrôle. L’écriture ARM64 gagne ici 4,6 % en débit mais son p99 augmente de 0,303 ms. Sous Rosetta, le débit d’écriture séquentielle **baisse de 19,4 %** et son p99 monte de 14,236 à 27,472 ms. Sur les 2,241 ms supplémentaires par transaction, environ 1,817 ms se situent dans F1 + F2 et 0,424 ms dans le reste de l’opération. Le temps hors barrières inclut les écritures de blocs, le travail CPU et les attentes de l’hôte. La succession d’un seul passage par version et architecture permet d’observer ces écarts mais ne suffit pas à les attribuer au moteur ni à établir l’équivalence des débits d’écriture.

### Contre-épreuve Rosetta

Une seule contre-épreuve a été décidée après la première paire, en inversant l’ordre et en conservant les deux résultats. Le harnais horodate les bornes de chaque série en UTC, en dehors des mesures des opérations. Image, opérations, versions, barrières et répertoires temporaires suivent le même protocole. [Échantillons et horodatages du passage inverse](validation/v0.9.5/rejeu-mixte-rosetta-inverse.json)

| Ordre d’exécution sous Rosetta | Débit écriture séquentielle P4 → P4.1 (Mio/s) | p99 P4 → P4.1 (ms) | F1 + F2 moyen P4 → P4.1 (ms) | Hors barrières moyen P4 → P4.1 (ms) |
| --- | ---: | ---: | ---: | ---: |
| P4.1 puis P4 | 6,733 → 5,424 | 14,236 → 27,472 | 6,489 → 8,306 | 2,794 → 3,218 |
| P4 puis P4.1 | 5,611 → 6,130 | 15,629 → 17,464 | 8,010 → 7,362 | 3,130 → 2,835 |

Le débit séquentiel d’écriture varie de −19,44 % dans la première paire à +9,25 % dans la seconde. Le p99 d’écriture reste défavorable à P4.1 dans les deux paires. Il passe de 14,236 à 27,472 ms puis de 15,629 à 17,464 ms. En réunissant les 5 120 opérations de chaque version et en divisant les 320 Mio par la somme des durées, P4 atteint 6,121 Mio/s (p99 14,740 ms) et P4.1 atteint 5,755 Mio/s (p99 19,876 ms). L’inversion de la direction du débit empêche de conclure à une baisse systématique de 19,44 %. Cette contre-épreuve unique ne garantit pas non plus l’équivalence de la queue d’écriture sous Rosetta. Les barrières, nombres de blocs et allocations concordent dans les quatre passages.

## Reproduction

Depuis la racine du dépôt, les binaires ARM64 et x86_64 doivent être exécutés successivement. Les mesures utilisent le mode release. L’option `--smoke` limite chaque condition à huit transactions et à une campagne.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run run --offline --example txn-latency > /tmp/transactions-arm64.json
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run run --offline --example txn-latency > /tmp/transactions-x86_64.json
python3 novaFS/lab/rejeu-mixte.py --sortie /tmp/novafs-rejeu-mixte.json
python3 novaFS/lab/rejeu-mixte.py --arch x86_64 --inverse --horodater --sortie /tmp/novafs-rejeu-mixte-inverse.json
```
