# NovaFS P4 v0.9.4 — mesures avant/après

**Relecture en P4.1.** Les valeurs historiques ci-dessous sont conservées. Le [diagnostic P4.1](novafs-p4.1-latences.md) borne le p99 global d’origine et publie des distributions calculées sur les appels individuels.

## Conditions et instrumentation

Mac Apple Silicon, macOS 27.2, Rust stable 1.99.1 en release. Les mesures ARM64 sont natives et les mesures x86_64 utilisent Rosetta. Le harnais vérifie le type Mach-O. La référence a été amorcée sur une extraction de `1510eca` avant les modifications du moteur. Seuls les compteurs et les charges du laboratoire y ont été ajoutés. Chaque version utilise la même suite de mesures, avec une image ordinaire temporaire neuve par exécution. Les deux cibles du banc complet sont lancées successivement. Le Mac sert aussi aux compilations, aux profils courts et aux tests. Les caches hôtes restent actifs ; il ne s’agit pas de mesures sur machine isolée.

Le banc complet travaille sur un fichier de 96 Mio. Cinq séries transfèrent chacune 32 Mio en requêtes séquentielles de 64 Kio, puis 256 lectures et 256 écritures aléatoires de 4 Kio, dont une sur deux traverse une frontière de page. Le jeu de petits fichiers compte 4 096 objets. Les lectures contrôlent leurs octets. Pour comparer chaque cible à son historique, le banc complet emploie des secteurs de 4 096 octets sur ARM64 et de 512 octets sous Rosetta. Le profil cache, les remontages et les passes sur quatre LV tournent aussi dans l’autre géométrie. [Banc de référence](../lab/novafs-bench.rs) · [Charges complémentaires](../lab/performance.rs)

## Débits et latences

Chaque flèche donne P3 puis P4. Le débit divise tous les octets utiles par le temps total des cinq séries. Le p99 publié est le maximum des cinq p99 par série. Les lectures et les allocations sont les totaux des cinq séries ; les allocations comptent les demandes Rust réussies, y compris les allocations transitoires des décodeurs et de la restitution des octets.

| Cible | Charge | Débit (Mio/s) P3 → P4 | p99 (ms) P3 → P4 | Blocs lus P3 → P4 | Allocations P3 → P4 | Pic du tas au-dessus du départ (octets) P3 → P4 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| aarch64 | Lecture séquentielle 64 Kio | 101,477 → 153,638 | 1,572 → 11,739 | 41 837 → 41 870 | 4 467 276 → 1 405 866 | 76 288 → 1 124 864 |
| aarch64 | Écriture séquentielle 64 Kio | 5,758 → 5,918 | 15,402 → 35,767 | 173 313 → 104 818 | 7 738 804 → 5 286 924 | 116 272 → 103 216 |
| aarch64 | Lecture aléatoire 4 Kio | 50,893 → 50,206 | 0,121 → 0,238 | 3 270 → 3 271 | 229 082 → 180 762 | 14 848 → 14 848 |
| aarch64 | Écriture aléatoire 4 Kio | 0,426 → 0,428 | 12,461 → 33,773 | 16 770 → 15 411 | 928 822 → 879 934 | 43 008 → 37 040 |
| aarch64 | Lecture 16 octets / 4 096 fichiers | 0,077 → 0,079 | 0,500 → 0,235 | 57 719 → 57 719 | 21 142 030 → 21 142 033 | 12 192 → 24 480 |
| x86_64 | Lecture séquentielle 64 Kio | 63,646 → 121,378 | 1,839 → 1,899 | 41 837 → 41 870 | 4 467 276 → 1 405 866 | 76 528 → 1 125 104 |
| x86_64 | Écriture séquentielle 64 Kio | 5,445 → 6,773 | 21,456 → 16,710 | 173 313 → 104 818 | 7 738 804 → 5 286 924 | 116 064 → 102 848 |
| x86_64 | Lecture aléatoire 4 Kio | 30,689 → 34,696 | 0,226 → 0,163 | 3 270 → 3 271 | 229 082 → 180 762 | 15 088 → 15 088 |
| x86_64 | Écriture aléatoire 4 Kio | 0,398 → 0,443 | 153,000 → 135,653 | 16 770 → 15 411 | 928 822 → 879 934 | 43 248 → 37 280 |
| x86_64 | Lecture 16 octets / 4 096 fichiers | 0,047 → 0,046 | 0,367 → 0,474 | 57 719 → 57 719 | 21 142 030 → 21 142 033 | 12 432 → 24 720 |

## Deux barrières par transaction

Le périphérique chronométré mesure chaque appel réel à `FileImage::flush`. Le succès atteste la réponse du système hôte à la barrière ; la persistance après coupure électrique reste non qualifiée. F1 suit les dépendances. F2 suit le remplacement de l’ancre 0 ou 1. Une synchronisation sans écriture préalable, notamment celle d’un montage RW, reste hors de ces deux catégories. Les 2 560 transactions séquentielles et 1 280 transactions aléatoires de chaque binaire ont chacune effectué exactement un F1 et un F2. Les lectures séquentielles et aléatoires n’ont produit aucun flush. Les autres temps ci-dessous sont le reliquat de la durée de la phase rapporté aux transactions, incluant les écritures de blocs et le harnais.

| Cible | Charge | F1 moyen (ms) P3 → P4 | F2 moyen (ms) P3 → P4 | Hors barrières (ms/transaction) P3 → P4 |
| --- | --- | ---: | ---: | ---: |
| aarch64 | Écriture séquentielle 64 Kio | 4,283 → 4,561 | 4,065 → 4,355 | 2,506 → 1,645 |
| aarch64 | Écriture aléatoire 4 Kio | 4,137 → 4,334 | 3,892 → 4,210 | 1,131 → 0,585 |
| x86_64 | Écriture séquentielle 64 Kio | 4,369 → 3,620 | 4,121 → 3,541 | 2,987 → 2,066 |
| x86_64 | Écriture aléatoire 4 Kio | 4,262 → 3,723 | 4,051 → 3,621 | 1,503 → 1,480 |

Sur ARM64, la lecture séquentielle accélère et le nombre d’allocations y baisse nettement. Son pire p99 par série monte toutefois de 1,572 à 11,739 ms. La lecture aléatoire ARM64 et les petites lectures x86_64 ne montrent pas de gain de débit dans cette campagne. La direction des variations d’écriture dépend de la durée des deux synchronisations et du travail hors barrières. Les mesures ne fixent pas de seuil de gain. Une observation du temps total sur cette machine ne permet pas d’attribuer les variations de durée des synchronisations au code du cache. [Étude isolée des parcours](novafs-p4-performance.md)

## Pages validées, évictions et restitution

Le microbanc préremplit 256 pages de 4 Kio puis exécute 4 096 recherches sur des clés présentes et 4 096 admissions distinctes dans le cache plein. Chaque admission doit évincer exactement une page. Les chiffres sont les médianes des cinq séries en ns/opération. Un remplacement comprend la recherche, le choix de victime et la copie des 4 Kio ; le support et les checksums sont exclus de ce microbanc. Chaque série détruit toutes les pages et vérifie le retour du tas exactement au niveau initial. Dans la seconde charge, le moteur lit 512 Kio vérifiés deux fois de suite ; les taux et lectures du support concernent cette charge réelle.

| Cible / secteur | Succès chaud (ns) P3 → P4 | Remplacer (ns) P3 → P4 | Octets du cache plein P3 → P4 | Taux de succès à froid P3 → P4 | Taux à chaud P3 → P4 |
| --- | ---: | ---: | ---: | ---: | ---: |
| aarch64 / 512 | 113,9 → 64,7 | 200,5 → 100,4 | 1 064 960 → 1 065 472 | 67,4 % → 27,7 % | 100,0 % → 100,0 % |
| aarch64 / 4096 | 105,5 → 68,4 | 202,5 → 105,6 | 1 064 960 → 1 065 472 | 67,4 % → 27,7 % | 100,0 % → 100,0 % |
| x86_64 / 512 | 165,4 → 99,4 | 257,9 → 145,3 | 1 064 960 → 1 065 472 | 67,4 % → 27,7 % | 100,0 % → 100,0 % |
| x86_64 / 4096 | 155,7 → 120,2 | 252,5 → 157,9 | 1 064 960 → 1 065 472 | 67,4 % → 27,7 % | 100,0 % → 100,0 % |

À capacité de pages identique, le surcoût de l’index mesuré au remplissage est de 512 octets. La seconde passe de 512 Kio a produit zéro lecture du périphérique sur toutes les séries chaudes. Le taux porte sur les recherches réellement effectuées : éviter un parcours d’index peut donc réduire simultanément le nombre de recherches et le nombre de touches. Un examen intégral relit le support. Le journal brut comporte aussi les évictions, touches et absences d’une deuxième passe sur 32 Mio, dont le contenu dépasse la capacité du cache.

## Remontage, vérification et plusieurs LV

La petite image utilise un LV de 2 048 blocs et 512 Kio de contenu. Le remontage strict RO et la vérification intégrale sont chronométrés séparément, sur cinq séries. La grande image de 96 Mio contient aussi les 4 096 petits fichiers. Le temps de remontage indiqué est sa médiane des cinq mesures ; la vérification intégrale y est chronométrée une fois. Les deux contrôles relisent les blocs atteignables.

| Cible / secteur | Remontage petit (ms) P3 → P4 | Vérification petit (ms) P3 → P4 |
| --- | ---: | ---: |
| aarch64 / 512 | 6,801 → 6,691 | 6,775 → 6,572 |
| aarch64 / 4096 | 6,787 → 6,919 | 6,773 → 7,066 |
| x86_64 / 512 | 10,502 → 9,908 | 10,394 → 9,898 |
| x86_64 / 4096 | 10,383 → 10,515 | 10,318 → 10,628 |

| Cible | Remontage grande image (ms) P3 → P4 | Vérification grande image (ms) P3 → P4 |
| --- | ---: | ---: |
| aarch64 | 2 644,692 → 2 846,569 | 2 763,899 → 2 864,011 |
| x86_64 | 4 665,406 → 4 686,511 | 4 672,805 → 4 693,360 |

Le test multi-volume utilise quatre LV réels du même pool, 32 écritures transactionnelles de 4 Kio par série, cinq séries. La voie commune P3/P4 ouvre chaque LV en RW une fois pour huit écritures et inclut les quatre montages dans la durée. La voie coordinateur P4 ouvre les quatre moteurs au début de chaque série et relève à part ce montage. Quatre tickets sont admis ; le banc prépare les plans en ordre inverse par paires puis publie en FIFO. Le nombre total d’octets et de transactions est identique. Le réimport final du pool et les relectures de contrôle des quatre LV se font hors chronométrage pour le coordinateur. Sur la voie commune, les relectures de contrôle sont également hors chronométrage. Le flux des clients est simulé dans un thread hôte. Les colonnes du coordinateur donnent les deux périmètres pour rendre visible le coût de son montage. [Coordinateur](novafs-p4-coordinateur.md)

| Cible / secteur | Voie commune P3 → P4 (Mio/s) | Coordinateur P4 hors montage (Mio/s) | Coordinateur P4 montage compris (Mio/s) |
| --- | ---: | ---: | ---: |
| aarch64 / 512 | 0,324 → 0,431 | 0,315 | 0,312 |
| aarch64 / 4096 | 0,321 → 0,428 | 0,434 | 0,421 |
| x86_64 / 512 | 0,335 → 0,390 | 0,455 | 0,444 |
| x86_64 / 4096 | 0,329 → 0,418 | 0,442 | 0,434 |

## Mémoire

Les pics du tas proviennent de la capacité effectivement accordée par malloc aux allocations Rust suivies, y compris le cache. La pile principale est relevée par pthread ; les piles de travail correspondent aux réservations explicites du banc. La somme tas + piles est un majorant de travail. La RSS est relevée sur le processus entier par `/usr/bin/time -l` et inclut le code, les bibliothèques et, pour x86_64, Rosetta. Ces périmètres n’incluent pas le cache de fichiers du noyau macOS dans le tas du moteur.

| Cible | Pic tas complet (octets) P3 → P4 | Piles réservées | Tas + piles P3 → P4 | RSS complète P3 → P4 | RSS coordinateur P4 max des deux secteurs |
| --- | ---: | ---: | ---: | ---: | ---: |
| aarch64 | 1 188 560 → 1 176 016 | 4 718 592 | 5 907 152 → 5 894 608 | 6 193 152 → 5 996 544 | 3 686 400 |
| x86_64 | 1 188 224 → 1 175 520 | 4 718 592 | 5 906 816 → 5 894 112 | 12 853 248 → 18 395 136 | 6 713 344 |

Sur ces charges P4, le plus grand total mesuré du tas et des piles réservées atteint **5 894 608 octets** pour une cible de 7 340 032 octets. La RSS du processus complet x86_64 excède cette cible de 11 055 104 octets. La mesure Darwin ne permet pas de ventiler cette différence précisément. Les pics de chaque phase et le tas de départ figurent dans les journaux ; leur écart peut comprendre des pages de cache conservées au terme de la phase. Par exemple, le tas au début des séries de lecture séquentielle ARM64 vaut environ 1,07 Mo en P3 et 24,6 à 32,3 ko en P4, après les invalidations dues aux écritures précédentes. La hausse au-dessus de cette ligne de départ inclut le remplissage du cache.

## Journaux et reproduction

Les lignes TSV de chaque phase donnent, dans l’ordre de l’en-tête du banc, les durées, quantiles, lectures/écritures et flushs. La ligne `instrumentation` suivante donne la phase, la série, le nombre de F1, de F2, le temps de F1 et de F2 en ns, les allocations et le tas de départ en octets. Les profils complémentaires portent leurs unités dans les champs. Les journaux P3 initiaux ont été recueillis directement dans l’extraction avant optimisation. Chaque sous-profil P4 conserve le SHA-256 du binaire mesuré. Sur chaque architecture, le banc complet, les deux profils de base et les deux profils coordinateur ont été rejoués avec le même binaire de livraison. [P3 ARM64](validation/v0.9.4/performance-P3-aarch64.json) · [P4 ARM64](validation/v0.9.4/performance-P4-aarch64.json) · [P3 x86_64](validation/v0.9.4/performance-P3-x86_64.json) · [P4 x86_64](validation/v0.9.4/performance-P4-x86_64.json)

Depuis la racine, avec les deux cibles Rust Darwin installées, les dépendances disponibles hors ligne, Rosetta et l’accès aux compteurs de Darwin. `--charge profil` relance uniquement les charges de cache/remontage/multi-LV et `--charge coordinateur` le flux coordonné.

```sh
set -e
(
    reference=$(mktemp -d)
    trap 'rm -rf "$reference"' EXIT
    git archive 1510eca novaFS | tar -x -C "$reference"
    for arch in aarch64 x86_64; do
        python3 novaFS/lab/measure-performance.py --arch "$arch" --reference "$reference/novaFS"
        python3 novaFS/lab/measure-performance.py --arch "$arch"
    done
)
```
