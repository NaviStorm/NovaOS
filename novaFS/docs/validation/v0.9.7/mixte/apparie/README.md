# Rejeu apparié du témoin mixte de 16 Kio

Chaque répétition traite les architectures ARM64 native et x86_64 sous Rosetta, les secteurs de 512 et 4 096 octets et les trois versions du moteur. Les six exécutables sont compilés avant la première mesure et réutilisés. Le client Rust, son allocateur de mesure et le lanceur historique doivent être identiques aux fichiers du commit P5 `ae508ce`. Chaque passage crée ses propres images temporaires.

Le client alterne 128 écritures et 128 relectures vérifiées de 16 Kio à quatre offsets. Sur P5 et P5.1, le même processus poursuit la campagne avec 128 créations/suppressions de snapshots, clones et écritures CoW, ainsi que 16 exports, restaurations et vérifications. Les versions tournent successivement sur le même hôte. À géométrie identique, la deuxième répétition inverse l’ordre relatif des trois versions. L’ordre de départ tourne entre les géométries. Le cache système conserve sa politique macOS ordinaire entre les processus.

Les `r*-*.csv.gz` conservent les échantillons du client et les `r*-*.json` leurs distributions. `provenance.json` consigne les empreintes des sources et binaires, les conditions et les dates UTC dans l’ordre effectif. `synthese.json` rapporte par couple de versions le débit, les temps moyens dans les flushs et en dehors, les écarts et les queues de distribution. Le temps hors flush inclut le calcul, les autres E/S et les attentes du thread. Ces durées seules ne permettent pas de l’attribuer au CPU.

Depuis la racine du dépôt sur macOS avec Rosetta et les deux cibles Rust Darwin installées, la commande suivante recrée les 24 passages.

```sh
python3 novaFS/lab/p51-mixte.py --sortie /tmp/novafs-p51-mixte
```

## Résultats des deux passages

Chaque ratio est le débit P5.1 divisé par le débit P5 pour les 128 requêtes du passage. Les écarts de temps sont les différences P5.1 moins P5, en millisecondes par écriture.

| Architecture | Secteur | Rép. | Ratio lecture | Ratio écriture | Δ F1+F2 (ms) | Δ hors flush (ms) |
|---|---:|---:|---:|---:|---:|---:|
| ARM64 | 512 | 1 | 1,576 | 0,941 | 0,170 | 0,453 |
| ARM64 | 512 | 2 | 1,396 | 1,054 | -0,935 | 0,393 |
| ARM64 | 4096 | 1 | 3,290 | 0,657 | 5,757 | -0,565 |
| ARM64 | 4096 | 2 | 1,743 | 0,992 | -0,078 | 0,159 |
| Rosetta | 512 | 1 | 2,145 | 1,055 | -0,466 | -0,103 |
| Rosetta | 512 | 2 | 1,662 | 0,933 | 0,462 | 0,287 |
| Rosetta | 4096 | 1 | 2,436 | 0,977 | 0,628 | -0,375 |
| Rosetta | 4096 | 2 | 2,173 | 1,067 | -0,570 | -0,134 |

Les 24 passages ont conservé 128 lectures vérifiées et 128 écritures chacun. Chaque série de 128 écritures a produit 256 synchronisations et 1 152 blocs écrits pour P4.1 ou 1 660 pour P5 et P5.1. Au total, les 24 paires CSV/JSON contiennent 17 408 échantillons, opérations annexes comprises. La plus grande somme tas et pile de travail est de 2 352 240 octets.

Le débit de relecture augmente dans les huit paires P5.1/P5. Le signe de la différence de débit d’écriture change entre les deux répétitions pour Rosetta sur les deux géométries et pour ARM64/512. ARM64/4 096 présente une baisse de 34,3 % au premier passage, avec 5,757 ms supplémentaires dans F1+F2 et 0,565 ms de moins hors flush. La baisse se limite à 0,8 % au passage inverse. Sur ARM64/512, le temps hors flush augmente de 0,453 puis 0,393 ms. Le premier témoin historique, exécuté séparément, avait mesuré 1,871 puis 1,698 ms hors flush sur cette même géométrie. La variabilité entre campagnes limite l’attribution au moteur.
