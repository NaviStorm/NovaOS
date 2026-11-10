# Mesures comparatives P6 sur fichiers temporaires Darwin

Les deux répétitions sans sondes comparent trois exécutables ou configurations dans des ordres inversés pour chaque couple architecture et secteur. Les mêmes données et positions sont rejouées à l’intérieur d’une répétition. Une autre graine sert à la répétition suivante. Chaque processus construit six images neuves de 32 Mio (trois profils, fichiers de 4 Kio et 512 Kio) dans lesquelles il crée une origine, un snapshot et un clone. Les mesures équipées des sondes internes sont isolées des distributions de latence applicative. L’horloge CPU de chaque Mach-O est étalonnée avant le banc. F1 et F2 sont comptées dans les temps de flush. Les E/S indiquent les appels logiques de 4 Kio au fichier macOS.

**Cache.** Les deux passes de lecture suivent une restitution du montage et une lecture vide pour installer le chemin. La première réchauffe le cache des données du moteur, la deuxième relit les mêmes positions. Le cache du fichier hôte conserve sa politique ordinaire. Le banc n’isole pas le cache du SSD. Les oracles vérifient l’intégralité des octets et les trois vues après remontage. Le petit fichier de 4 Kio offre une seule position de bloc. Sur le fichier de 512 Kio, les lectures parcourent les 32 tranches de 16 Kio et l’écriture aléatoire modifie les huit premières positions de la permutation. Les lectures murales gardent les vérifications normales du chemin. Les petits fichiers conservent la même géométrie de périphérique.

## Espace à la fin de la première écriture

Les occupations du pool incluent les métadonnées, la tête et son prédécesseur. Le ratio logique/physique des données se calcule sur les pages de fichier initialement écrites, y compris leurs pages brutes. Les nombres de pages du tableau sont les minima et maxima observés dans les deux répétitions, secteurs et architectures.

| Fichier | Profil | Occupation P5.1 / P6-none / P6-lz4 (blocs) | Gain net de pool (blocs) | Ratio des données logique/physique |
| --- | --- | --- | ---: | ---: |
| 4 Kio | repetitif | 26 / 26 / 26 | 0 | 1,000× |
| 4 Kio | mixte | 26 / 26 / 26 | 0 | 1,000× |
| 4 Kio | aleatoire | 26 / 26 / 26 | 0 | 1,000× |
| 512 Kio | repetitif | 162 / 162 / 46 | 116 | 10,667× |
| 512 Kio | mixte | 162 / 162 / 99 | 63 | 1,969× |
| 512 Kio | aleatoire | 162 / 162 / 162 | 0 | 1,000× |

## Débit et amplification

Les triplets suivent l’ordre **P5.1 / P6-none / P6-lz4**. Les tableaux regroupent les deux secteurs et les deux répétitions pour les fichiers de 512 Kio. Le débit divise les octets utiles par la somme des temps individuels. Pour les écritures, l’amplification divise les octets écrits au fichier image par les octets utiles. Le retrait du montage avant les lectures aléatoires est extérieur au chronomètre.

| Cible | Profil | Écriture séquentielle 64 Kio (Mio/s) | Amplification | Écriture aléatoire 16 Kio (Mio/s) | Lecture séquentielle 16 Kio 1re passe (Mio/s) | Lecture aléatoire 16 Kio 1re passe (Mio/s) |
| --- | --- | --- | --- | --- | --- | --- |
| aarch64 | repetitif | 5,92 / 6,16 / 6,40 | 1,64 / 1,64 / 0,73 | 1,56 / 1,65 / 1,69 | 149,63 / 170,01 / 157,23 | 161,39 / 164,89 / 165,08 |
| aarch64 | mixte | 6,17 / 6,10 / 5,99 | 1,64 / 1,64 / 1,15 | 1,62 / 1,56 / 1,65 | 174,62 / 158,69 / 155,67 | 164,62 / 168,07 / 163,31 |
| aarch64 | aleatoire | 6,17 / 5,99 / 6,08 | 1,64 / 1,64 / 1,64 | 1,63 / 1,61 / 1,65 | 159,37 / 169,77 / 153,82 | 165,86 / 170,03 / 163,92 |
| x86_64 | repetitif | 5,76 / 5,39 / 5,77 | 1,64 / 1,64 / 0,73 | 1,59 / 1,53 / 1,57 | 116,64 / 114,73 / 107,41 | 120,60 / 119,07 / 115,68 |
| x86_64 | mixte | 5,74 / 5,70 / 5,51 | 1,64 / 1,64 / 1,15 | 1,59 / 1,56 / 1,58 | 115,78 / 110,59 / 110,49 | 117,32 / 117,50 / 113,81 |
| x86_64 | aleatoire | 5,67 / 5,90 / 5,43 | 1,64 / 1,64 / 1,64 | 1,57 / 1,66 / 1,55 | 116,73 / 118,31 / 114,39 | 120,46 / 118,42 / 117,91 |

## CPU du codec et opérations sur les vues

Seul le passage P6-lz4 instrumenté mesure la compression et la décompression. Le coût de décompression en écriture inclut les vérifications réelles de publication. Un essai de compression sans gain physique est compté. Les colonnes de latence et de débit principales utilisent les builds sans sondes. Les petites fenêtres d’horloge sur Rosetta limitent la précision des valeurs individuelles.

| Cible | Profil | CPU compression par écriture séquentielle de 64 Kio (µs) | CPU décompression par même écriture (µs) | CPU décompression par lecture aléatoire de 16 Kio, 1re passe (µs) | Montage P5.1 / P6-none / P6-lz4 (ms) | Contrôle supplémentaire des systèmes de fichiers des trois vues P5.1 / P6-none / P6-lz4 (ms) |
| --- | --- | ---: | ---: | ---: | --- | --- |
| aarch64 | repetitif | 45,11 | 7,71 | 1,20 | 98,36 / 97,33 / 94,29 | 84,55 / 84,86 / 85,84 |
| aarch64 | mixte | 155,20 | 5,36 | 1,16 | 96,23 / 97,23 / 95,87 | 85,13 / 85,24 / 86,01 |
| aarch64 | aleatoire | 190,29 | 0,00 | 0,00 | 93,65 / 98,37 / 94,07 | 84,84 / 85,17 / 85,26 |
| x86_64 | repetitif | 50,75 | 20,31 | 3,17 | 139,78 / 140,96 / 144,24 | 130,92 / 131,98 / 133,21 |
| x86_64 | mixte | 206,31 | 18,31 | 3,05 | 137,52 / 140,68 / 142,46 | 131,32 / 132,34 / 133,87 |
| x86_64 | aleatoire | 186,00 | 0,00 | 0,00 | 140,33 / 139,84 / 142,20 | 131,67 / 132,60 / 132,23 |

## Latences et E/S

Les p50/p95/p99 utilisent le rang supérieur empirique. Chaque distribution stockée indique son effectif. Avec moins de 100 appels par cellule, le p99 est généralement le maximum. Les distributions ci-dessous regroupent les trois profils des gros fichiers. Les groupes de réécriture partielle mélangent 1 octet, 4 et 16 Kio. Les archives séparent ces trois tailles.

| Architecture | Format | Opération | N | p50 / p95 / p99 / max (ms) | Lectures / écritures de blocs par appel | Flushs par appel |
| --- | --- | --- | ---: | --- | --- | ---: |
| aarch64 | P5.1 | ecriture_seq | 96 | 10,284 / 11,916 / 12,180 / 12,180 | 47,88 / 26,25 | 2 |
| aarch64 | P5.1 | ecriture_alea | 96 | 9,505 / 10,940 / 11,522 / 11,522 | 30,62 / 16,06 | 2 |
| aarch64 | P5.1 | partielle_clone | 36 | 9,663 / 11,204 / 11,368 / 11,368 | 29,00 / 14,83 | 2 |
| aarch64 | P5.1 | lecture_seq_froid | 384 | 0,092 / 0,141 / 0,164 / 0,195 | 4,09 / 0,00 | 0 |
| aarch64 | P5.1 | lecture_alea_chaud | 384 | 0,022 / 0,040 / 0,063 / 0,066 | 0,00 / 0,00 | 0 |
| aarch64 | P5.1 | snapshot | 12 | 8,624 / 10,682 / 10,682 / 10,682 | 0,00 / 2,00 | 2 |
| aarch64 | P5.1 | clone | 12 | 8,406 / 9,909 / 9,909 / 9,909 | 0,00 / 2,00 | 2 |
| aarch64 | P6-none | ecriture_seq | 96 | 10,211 / 11,897 / 12,361 / 12,361 | 47,88 / 26,25 | 2 |
| aarch64 | P6-none | ecriture_alea | 96 | 9,625 / 10,879 / 11,302 / 11,302 | 30,62 / 16,06 | 2 |
| aarch64 | P6-none | partielle_clone | 36 | 10,028 / 17,437 / 146,316 / 146,316 | 29,00 / 14,83 | 2 |
| aarch64 | P6-none | lecture_seq_froid | 384 | 0,089 / 0,133 / 0,157 / 0,164 | 4,09 / 0,00 | 0 |
| aarch64 | P6-none | lecture_alea_chaud | 384 | 0,021 / 0,040 / 0,061 / 0,067 | 0,00 / 0,00 | 0 |
| aarch64 | P6-none | snapshot | 12 | 8,343 / 10,038 / 10,038 / 10,038 | 0,00 / 2,00 | 2 |
| aarch64 | P6-none | clone | 12 | 8,587 / 9,846 / 9,846 / 9,846 | 0,00 / 2,00 | 2 |
| aarch64 | P6-lz4 | ecriture_seq | 96 | 10,263 / 11,685 / 13,373 / 13,373 | 40,42 / 18,79 | 2 |
| aarch64 | P6-lz4 | ecriture_alea | 96 | 9,368 / 10,537 / 10,912 / 10,912 | 28,96 / 14,40 | 2 |
| aarch64 | P6-lz4 | partielle_clone | 36 | 9,487 / 10,756 / 10,874 / 10,874 | 28,22 / 14,06 | 2 |
| aarch64 | P6-lz4 | lecture_seq_froid | 384 | 0,093 / 0,141 / 0,171 / 0,183 | 4,09 / 0,00 | 0 |
| aarch64 | P6-lz4 | lecture_alea_chaud | 384 | 0,022 / 0,040 / 0,062 / 0,064 | 0,00 / 0,00 | 0 |
| aarch64 | P6-lz4 | snapshot | 12 | 8,356 / 9,185 / 9,185 / 9,185 | 0,00 / 2,00 | 2 |
| aarch64 | P6-lz4 | clone | 12 | 8,343 / 9,811 / 9,811 / 9,811 | 0,00 / 2,00 | 2 |
| x86_64 | P5.1 | ecriture_seq | 96 | 10,629 / 12,642 / 13,396 / 13,396 | 47,88 / 26,25 | 2 |
| x86_64 | P5.1 | ecriture_alea | 96 | 9,600 / 11,331 / 12,242 / 12,242 | 30,62 / 16,06 | 2 |
| x86_64 | P5.1 | partielle_clone | 36 | 10,046 / 11,339 / 11,341 / 11,341 | 29,00 / 14,83 | 2 |
| x86_64 | P5.1 | lecture_seq_froid | 384 | 0,121 / 0,186 / 0,216 / 0,219 | 4,09 / 0,00 | 0 |
| x86_64 | P5.1 | lecture_alea_chaud | 384 | 0,031 / 0,057 / 0,085 / 0,094 | 0,00 / 0,00 | 0 |
| x86_64 | P5.1 | snapshot | 12 | 8,454 / 8,680 / 8,680 / 8,680 | 0,00 / 2,00 | 2 |
| x86_64 | P5.1 | clone | 12 | 8,369 / 10,314 / 10,314 / 10,314 | 0,00 / 2,00 | 2 |
| x86_64 | P6-none | ecriture_seq | 96 | 10,659 / 12,782 / 14,618 / 14,618 | 47,88 / 26,25 | 2 |
| x86_64 | P6-none | ecriture_alea | 96 | 9,663 / 11,154 / 11,319 / 11,319 | 30,62 / 16,06 | 2 |
| x86_64 | P6-none | partielle_clone | 36 | 10,250 / 12,289 / 13,440 / 13,440 | 29,00 / 14,83 | 2 |
| x86_64 | P6-none | lecture_seq_froid | 384 | 0,123 / 0,187 / 0,217 / 0,236 | 4,09 / 0,00 | 0 |
| x86_64 | P6-none | lecture_alea_chaud | 384 | 0,031 / 0,057 / 0,086 / 0,086 | 0,00 / 0,00 | 0 |
| x86_64 | P6-none | snapshot | 12 | 8,260 / 9,573 / 9,573 / 9,573 | 0,00 / 2,00 | 2 |
| x86_64 | P6-none | clone | 12 | 8,329 / 9,673 / 9,673 / 9,673 | 0,00 / 2,00 | 2 |
| x86_64 | P6-lz4 | ecriture_seq | 96 | 11,314 / 12,752 / 13,681 / 13,681 | 40,42 / 18,79 | 2 |
| x86_64 | P6-lz4 | ecriture_alea | 96 | 9,698 / 11,290 / 11,421 / 11,421 | 28,96 / 14,40 | 2 |
| x86_64 | P6-lz4 | partielle_clone | 36 | 10,347 / 11,719 / 11,797 / 11,797 | 28,22 / 14,06 | 2 |
| x86_64 | P6-lz4 | lecture_seq_froid | 384 | 0,131 / 0,195 / 0,223 / 0,250 | 4,09 / 0,00 | 0 |
| x86_64 | P6-lz4 | lecture_alea_chaud | 384 | 0,032 / 0,057 / 0,088 / 0,092 | 0,00 / 0,00 | 0 |
| x86_64 | P6-lz4 | snapshot | 12 | 8,383 / 9,712 / 9,712 / 9,712 | 0,00 / 2,00 | 2 |
| x86_64 | P6-lz4 | clone | 12 | 8,391 / 10,271 / 10,271 / 10,271 | 0,00 / 2,00 | 2 |

## Pointes et mémoire

| Cible | Mode | Maximum mutation (ms) | Opération / profil / secteur | F1 / F2 (ms) | CPU du thread (ms) | Tas (octets) | Tas + pile de travail (Mio) | RSS hôte (Mio) |
| --- | --- | ---: | --- | --- | ---: | ---: | ---: | ---: |
| aarch64 | P5.1 | 15,264 | ecriture_seq / mixte / 512 | 4,589 / 10,331 | 0,484 | 1757520 | 3,688 | 5,125 |
| aarch64 | P6-none | 146,316 | partielle_clone / aleatoire / 4096 | 68,748 / 76,230 | 1,581 | 1757520 | 3,688 | 5,750 |
| aarch64 | P6-lz4 | 152,625 | ecriture_seq / mixte / 4096 | 74,117 / 77,927 | 0,810 | 1757520 | 3,688 | 5,234 |
| aarch64 | P6-lz4-sondes | 12,183 | ecriture_seq / aleatoire / 512 | 5,396 / 5,072 | 1,878 | 1757536 | 3,688 | 5,516 |
| x86_64 | P5.1 | 13,396 | ecriture_seq / aleatoire / 512 | 4,964 / 5,193 | 3,387 | 1757168 | 3,676 | 10,367 |
| x86_64 | P6-none | 14,618 | ecriture_seq / repetitif / 4096 | 5,007 / 7,127 | 2,614 | 1757232 | 3,676 | 9,836 |
| x86_64 | P6-lz4 | 13,681 | ecriture_seq / aleatoire / 4096 | 5,690 / 4,909 | 3,281 | 1757232 | 3,676 | 10,402 |
| x86_64 | P6-lz4-sondes | 13,864 | ecriture_seq / mixte / 4096 | 4,982 / 6,668 | 2,364 | 1757248 | 3,676 | 11,277 |

Sur ARM64, les deux plus fortes pointes de mutation se produisent sur le secteur de 4 096 octets. Les deux synchronisations totalisent 152,044 ms sur 152,625 ms pour P6-lz4 et 144,978 ms sur 146,316 ms pour P6-none. Ces mesures localisent l’attente dans les synchronisations du fichier hôte. Leur fréquence au-delà de cette campagne reste inconnue. Sur Rosetta, les lectures séquentielles de la première passe du fichier répétitif passent de 116,64 à 107,41 Mio/s entre P5.1 et P6-lz4. Sur ARM64 pour ce même fichier, elles passent de 149,63 à 157,23 Mio/s. Les deux répétitions ne suffisent pas à extrapoler ces écarts à d’autres systèmes de stockage.

Le budget de 7 Mio porte sur les allocations Rust arrondies par Darwin et sur la réservation de pile du worker. Il inclut les oracles du banc pendant la mesure. Le RSS comprend en plus le runtime et les pages résidentes du processus macOS. La pile principale de l’hôte et les caches de stockage sont hors de ce budget. La phase sondée utilise aussi des compteurs de profilage TLS.

[Échantillons individuels](echantillons.jsonl.gz) · [Distributions par architecture, secteur, profil, taille et opération](distributions.json.gz) · [Occupations des vues et crédits réservés](espace.json) · [Provenance, horloges, horaires et mémoire](campagnes.json)

```sh
python3 novaFS/lab/p6-mesures.py --sortie /tmp/novafs-p6-mesures
```
