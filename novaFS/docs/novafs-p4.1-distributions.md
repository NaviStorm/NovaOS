# NovaFS P4.1 — distributions des lectures comparées

Quantiles au rang plafond sur les durées individuelles de six campagnes. Chaque ligne conserve
une architecture, une politique de cache hôte et une température du cache NovaFS. Le débit
divise les octets utiles par la durée des boucles. Les p99 par campagne et leurs ratios appariés
figurent dans la [synthèse numérique](validation/v0.9.5/synthese.json).
Les maxima sont conservés même lorsqu’ils n’apparaissent que dans une campagne.

## ARM64 natif

| Cache hôte | Cache NovaFS | Version | n | p50 ms | p95 ms | p99 ms | max ms | Mio/s |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ordinaire | froid 32 Mio | P3 | 15360 | 0,625 | 0,677 | 0,736 | 1,581 | 99,33 |
| ordinaire | froid 32 Mio | P4 | 15360 | 0,315 | 0,428 | 0,488 | 1,098 | 197,47 |
| ordinaire | froid 32 Mio | P4.1 | 15360 | 0,314 | 0,426 | 0,478 | 1,083 | 198,80 |
| ordinaire | chaud 512 Kio | P3 | 15360 | 0,484 | 0,528 | 0,592 | 1,179 | 127,97 |
| ordinaire | chaud 512 Kio | P4 | 15360 | 0,231 | 0,408 | 0,501 | 1,413 | 274,41 |
| ordinaire | chaud 512 Kio | P4.1 | 15360 | 0,228 | 0,408 | 0,498 | 0,893 | 276,47 |
| F_NOCACHE | froid 32 Mio | P3 | 15360 | 1,031 | 1,464 | 1,831 | 4,742 | 57,54 |
| F_NOCACHE | froid 32 Mio | P4 | 15360 | 0,716 | 1,181 | 1,494 | 6,820 | 81,56 |
| F_NOCACHE | froid 32 Mio | P4.1 | 15360 | 0,720 | 1,205 | 1,664 | 12,584 | 80,24 |
| F_NOCACHE | chaud 512 Kio | P3 | 15360 | 0,489 | 0,534 | 0,597 | 1,482 | 126,72 |
| F_NOCACHE | chaud 512 Kio | P4 | 15360 | 0,230 | 0,406 | 0,497 | 0,715 | 276,31 |
| F_NOCACHE | chaud 512 Kio | P4.1 | 15360 | 0,229 | 0,408 | 0,501 | 0,625 | 274,78 |

## x86_64 sous Rosetta

| Cache hôte | Cache NovaFS | Version | n | p50 ms | p95 ms | p99 ms | max ms | Mio/s |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ordinaire | froid 32 Mio | P3 | 15360 | 0,985 | 1,060 | 1,150 | 2,576 | 62,87 |
| ordinaire | froid 32 Mio | P4 | 15360 | 0,484 | 0,668 | 0,750 | 1,086 | 128,72 |
| ordinaire | froid 32 Mio | P4.1 | 15360 | 0,485 | 0,663 | 0,740 | 1,193 | 128,84 |
| ordinaire | chaud 512 Kio | P3 | 15360 | 0,783 | 0,848 | 0,929 | 5,831 | 79,27 |
| ordinaire | chaud 512 Kio | P4 | 15360 | 0,370 | 0,667 | 0,819 | 1,053 | 173,05 |
| ordinaire | chaud 512 Kio | P4.1 | 15360 | 0,363 | 0,663 | 0,812 | 1,039 | 173,24 |
| F_NOCACHE | froid 32 Mio | P3 | 15360 | 1,415 | 1,836 | 2,170 | 6,855 | 42,53 |
| F_NOCACHE | froid 32 Mio | P4 | 15360 | 0,897 | 1,396 | 1,768 | 9,550 | 66,05 |
| F_NOCACHE | froid 32 Mio | P4.1 | 15360 | 0,897 | 1,455 | 2,221 | 40,601 | 63,20 |
| F_NOCACHE | chaud 512 Kio | P3 | 15360 | 0,786 | 0,844 | 0,902 | 2,406 | 79,11 |
| F_NOCACHE | chaud 512 Kio | P4 | 15360 | 0,374 | 0,664 | 0,814 | 1,250 | 172,65 |
| F_NOCACHE | chaud 512 Kio | P4.1 | 15360 | 0,369 | 0,668 | 0,821 | 1,355 | 172,21 |

[Protocole, interprétation et limites](novafs-p4.1-latences.md) · [Échantillons individuels](validation/v0.9.5/lectures.csv.gz)
