# Seuil de performance Darwin · 17 novembre 2026

Les douze charges respectent individuellement le plafond Nova/GNU de 1,10. Le rapport maximal est de 1,039671 pour un nom avec `-az`. Le calcul utilise les durées entières en nanosecondes et les fractions du [relevé brut](mesures.json).

| Chemins | Option | Fin | Nova ms/appel | GNU 9.12 ms/appel | Nova/GNU | MAD Nova ms | MAD GNU ms |
|---:|---|---|---:|---:|---:|---:|---:|
| 1 | `-a` | LF | 1.235130 | 1.194484 | 1.034028 | 0.009359 | 0.013724 |
| 1 | `-az` | NUL | 1.246021 | 1.198477 | 1.039671 | 0.017471 | 0.015768 |
| 1 | `-s.txt` | LF | 1.223435 | 1.184240 | 1.033098 | 0.011628 | 0.006234 |
| 1 | `-zs.txt` | NUL | 1.216542 | 1.177102 | 1.033506 | 0.008891 | 0.009453 |
| 30 | `-a` | LF | 1.218477 | 1.189568 | 1.024302 | 0.013792 | 0.018943 |
| 30 | `-az` | NUL | 1.269786 | 1.229966 | 1.032375 | 0.026700 | 0.023344 |
| 30 | `-s.txt` | LF | 1.225648 | 1.205086 | 1.017063 | 0.026167 | 0.022740 |
| 30 | `-zs.txt` | NUL | 1.226969 | 1.197117 | 1.024936 | 0.008662 | 0.009583 |
| 512 | `-a` | LF | 1.265143 | 1.265112 | 1.000025 | 0.005807 | 0.008315 |
| 512 | `-az` | NUL | 1.270148 | 1.268685 | 1.001154 | 0.009161 | 0.015802 |
| 512 | `-s.txt` | LF | 1.282867 | 1.284800 | 0.998496 | 0.031706 | 0.028797 |
| 512 | `-zs.txt` | NUL | 1.293029 | 1.280685 | 1.009638 | 0.027635 | 0.036112 |

## Protocole

Mac ARM64, macOS 27.2.1, source publiée `36f37751dc43928925ef221de572d6226e140fae`. Créneau du 17 novembre, de 05 h 30 min 36 s à 05 h 30 min 44 s, heure de Paris. Le relevé des processus ciblés au début et à la fin montre uniquement Python. Les deux instantanés de charge figurent dans le JSON.

Le corpus est `dossier-{i%97}/document-{i:05}.txt/` pour 0 ≤ i < N. Les deux programmes reçoivent le même argv et la seule variable `LC_ALL=C`. Après quatre lancements d’échauffement par binaire et par charge, le banc alterne leur ordre sur quinze passages de seize lancements. Les temps incluent la création du processus, sa collecte et les écritures dans un même fichier binaire local. Le fichier est tronqué et repositionné hors chronométrage avant chaque série. Les temps correspondent à la médiane des quinze durées par appel. MAD désigne la médiane des écarts absolus à cette médiane. Le JSON conserve aussi les extrema et les échantillons.

Le banc compare les octets des deux canaux, contrôle tous les statuts et vérifie après chaque série la concaténation exacte des seize sorties attendues. Il recalcule le corpus et répète la capture individuelle en fin de cellule. Les chemins physiques, inodes, tailles, dates de modification et SHA-256 des deux exécutables correspondent avant et après la campagne.

Depuis `livraison-basename-0240/`, sur un créneau Mac réservé, exécuter dans un dossier de résultats neuf. Le code de sortie vaut 1 si au moins un rapport dépasse le seuil.

```sh
./mesurer-performances-basename.sh seuil ../novaOS/build/basename-seuil-rejeu
```

Le port et les trois binaires sont identiques à la livraison de référence. Les résultats QEMU et les limites du lanceur et de la console sont consignés dans la [matrice de livraison](../../MATRICE.md).
