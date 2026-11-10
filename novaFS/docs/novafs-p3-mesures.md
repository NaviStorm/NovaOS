# NovaFS P3 v0.9.3 — mémoire et performances

Mesures en release sur macOS 27.2 avec Rust 1.99.1, sur le même Mac Apple Silicon. Les exécutables Mach-O ont été identifiés avant leur lancement. ARM64 tourne nativement et x86_64 sous Rosetta. Chaque mesure lance directement le binaire sous `/usr/bin/time -l`. Le tas comprend les allocations Rust vivantes arrondies par `malloc_size`. Le fichier temporaire, les caches du système et le processus Python ne font pas partie de ce compteur.

## Réservations, annulation et remontage

Le banc prépare puis annule 64 fois la même écriture sur une image ordinaire. Il exige le retour du tas à son niveau stabilisé après chaque annulation, la conservation du reçu et zéro écriture ou flush. Il publie ensuite une écriture et vérifie ses octets, le nombre exact de blocs soumis et les deux barrières. Les valeurs suivantes utilisent le second passage après chauffe. Les coûts de plan indiquent la mémoire supplémentaire encore vivante pendant la réservation. La colonne « index publié » compte la structure et les capacités des vecteurs de métadonnées d’allocation, hors arrondis de malloc. Le coût de préparation/annulation est la moyenne des 64 cycles.

| Exécution | Secteur | Charge (octets) | Blocs données + métadonnées | Index publié (octets) | Tas du plan (octets) | Préparer et annuler (µs) | Commit avec préparation (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aarch64 | 512 | 1 | 1 + 4 | 336 | 21 024 | 103,393 | 15,779 |
| aarch64 | 512 | 65 536 | 17 + 4 | 384 | 87 264 | 405,212 | 15,552 |
| aarch64 | 4096 | 1 | 1 + 4 | 336 | 21 024 | 104,046 | 16,834 |
| aarch64 | 4096 | 65 536 | 17 + 4 | 384 | 87 264 | 401,709 | 17,214 |
| x86_64 | 512 | 1 | 1 + 4 | 336 | 20 992 | 162,401 | 8,727 |
| x86_64 | 512 | 65 536 | 17 + 4 | 384 | 87 200 | 672,539 | 8,928 |
| x86_64 | 4096 | 1 | 1 + 4 | 336 | 20 992 | 275,726 | 8,284 |
| x86_64 | 4096 | 65 536 | 17 + 4 | 384 | 87 200 | 638,247 | 9,224 |

Pour le pool, 16 tickets d’un grain sont détenus simultanément, puis tous annulés. Les 64 cycles mesurés imposent une restitution immédiate du tas stabilisé. Ensuite, le banc provisionne 16 vrais LV, formate et publie chaque FS, détruit le gestionnaire et réimporte le catalogue. Les indicateurs « pool vide » et « 16 LV remontés » sont des écarts de tas mesurés sur le même processus. Les objets tickets sur la pile sont comptés séparément du registre et des segments alloués sur le tas.

| Exécution | Secteur | Pool vide (octets) | 16 tickets sur pile (octets) | Surcroît de tas, 16 tickets¹ | Pool remonté, 16 LV (octets) | Résidu après destruction (octets) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| aarch64 | 512 | 1 536 | 384 | 576 | 5 632 | 0 |
| aarch64 | 4096 | 1 536 | 384 | 576 | 5 632 | 0 |
| x86_64 | 512 | 1 536 | 384 | 576 | 5 632 | 0 |
| x86_64 | 4096 | 1 536 | 384 | 576 | 5 632 | 0 |

¹ Différence entre le tas du pool vide et celui du premier groupe de 16 tickets. Les JSON bruts distinguent ce premier remplissage du retour après chauffe. Les contrôles de restitution portent sur les exécutions testées, sans conclusion générale sur les allocations de macOS.

## Enveloppe mémoire du processus

| Charge | Architecture | Pic du tas Rust | Pile principale | Piles de travail | Tas + toutes ces piles | RSS maximale hôte |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Réservations 512 | aarch64 | 230 384 | 2 097 152 | 2 097 152 | 4 424 688 | 3 604 480 |
| Réservations 4096 | aarch64 | 230 384 | 2 097 152 | 2 097 152 | 4 424 688 | 3 817 472 |
| Banc complet | aarch64 | 1 188 512 | 2 097 152 | 2 621 440 | 5 907 104 | 5 816 320 |
| Réservations 512 | x86_64 | 230 256 | 2 097 152 | 2 097 152 | 4 424 560 | 6 483 968 |
| Réservations 4096 | x86_64 | 230 256 | 2 097 152 | 2 097 152 | 4 424 560 | 6 234 112 |
| Banc complet | x86_64 | 1 188 176 | 2 097 152 | 2 621 440 | 5 906 768 | 15 499 264 |

Le plafond de travail est de 7 340 032 octets. Le maximum du tas instrumenté et des piles réservées atteint 5 907 104 octets. La pile principale Darwin du banc est réglée à 2 Mio et sa taille est lue sur le pthread courant. La RSS du banc complet x86_64 dépasse le plafond de 8 159 232 octets. Elle comprend également le code, les bibliothèques, la traduction et d’autres pages du processus. Le compteur Darwin ne ventile pas ce dépassement. Une pile réservée n’est pas entièrement résidente.

La campagne distincte des contrôles P2 a également tourné sur P3. La CLI standard a atteint 3 211 264 octets sur aarch64 et 4 497 408 octets sur x86_64. Les LV sains et corrompus de 8 Gio et le pool de 16 LV ont été comparés intégralement à leurs copies de départ. [Mesures sur le chemin de vérification](validation/v0.9.3/memoire-controle.json)

## Débits et synchronisations

Le banc complet reprend le protocole sur fichier temporaire de 96 Mio des versions antérieures. Les débits divisent le volume utile cumulé par la durée totale de cinq séries. La latence p99 indiquée est le maximum des cinq p99. Les coûts de flush sont moyennés sur toutes les synchronisations de la phase. Chaque transaction d’écriture doit produire deux flushs, chaque lecture zéro. Les charges du Mac et de ses caches n’ont pas été isolées. Les chiffres portent sur ces fichiers et ne qualifient pas la persistance physique.

| Architecture / secteur | Opération | Débit (Mio/s) | p99 (ms) | Nombre de flushs | Moyenne par flush (ms) |
| --- | --- | ---: | ---: | ---: | ---: |
| aarch64 / 4096 | Lecture séquentielle 64 Kio | 97,611 | 1,389 | 0 | — |
| aarch64 / 4096 | Écriture séquentielle 64 Kio | 5,333 | 23,103 | 5 120 | 4,662 |
| aarch64 / 4096 | Lecture aléatoire 4 Kio | 47,488 | 0,111 | 0 | — |
| aarch64 / 4096 | Écriture aléatoire 4 Kio | 0,393 | 193,298 | 2 560 | 4,543 |
| x86_64 / 512 | Lecture séquentielle 64 Kio | 63,622 | 1,469 | 0 | — |
| x86_64 / 512 | Écriture séquentielle 64 Kio | 5,405 | 18,268 | 5 120 | 4,215 |
| x86_64 / 512 | Lecture aléatoire 4 Kio | 30,856 | 0,177 | 0 | — |
| x86_64 / 512 | Écriture aléatoire 4 Kio | 0,380 | 151,079 | 2 560 | 4,359 |

Les journaux JSON contiennent les cinq séries, les lectures et écritures de blocs, les temps de flush et les compteurs de pression et de réutilisation. [ARM64](validation/v0.9.3/mesures-aarch64.json) · [x86_64](validation/v0.9.3/mesures-x86_64.json) · [Mesures P1 pour comparaison](novafs-p1-mesures.md)

## Reproduire depuis la racine du dépôt

```sh
python3 novaFS/lab/measure-space.py --arch aarch64
python3 novaFS/lab/measure-space.py --arch x86_64
python3 novaFS/lab/measure-check.py
```
