# Performance de tee sur Darwin ARM64

**17 novembre 2026.** Le Mach-O corrigé passe le seuil de **1,10 × le temps médian de GNU tee 9.12** sur chacune des six charges, dans deux séries indépendantes de 24 passages. Le rapport maximal est 1,0116446701826616 en charge ambiante et 21043250/20622499 avec trois travailleurs supplémentaires. Le verdict utilise les fractions exactes des médianes en nanosecondes.

## Le cas de 8 Mio et quatre fichiers

La série de livraison comportait trois mesures Nova de 209,324, 466,77 et 80,16 ms, avec trois à quatre QEMU AArch64 actifs et une compilation observée. La médiane GNU était de 65,938 ms. Avec **les mêmes octets d’exécutable et le même banc**, le premier rejeu donne 74,862 ms contre 73,732 ms. Deux séries plus longues sur l’ancien binaire donnent 56,625 / 55,646 ms, puis 60,043 / 59,417 ms. Les données ne permettent pas d’attribuer la pointe historique à un processus précis ou à une attente disque particulière. [Mesures d’origine](../preuves/mesures.json) · [Rejeu du banc d’origine](rejeu-historique.json) · [Première série](initiale.json) · [Seconde série](controle-initial.json)

Un coût plus petit se reproduit à 1 Mio / une sortie. L’ancien Nova prend 5,468 ms contre 4,950 ms pour GNU dans la deuxième série, puis 5,619 ms contre 4,938 ms lors de l’essai à trois binaires. Le rapport atteint respectivement 1,1046676873401704 et 1,1379078270866108. Le temps processeur utilisateur de l’ancien Nova dans la première série est de 1,183 ms contre 0,762 ms pour GNU à cette taille.

## Correction et contrôle de la cause

Le seul changement de construction est `-Wl,-dead_strip` à la liaison Darwin. La bibliothèque statique Rust entraînait du code inutilisé dans l’exécutable. Le Mach-O passe de 1 576 336 à 549 472 octets, sa section de code de 632 828 à 227 452 octets et ses données de relocation chaînée de 3 560 à 1 336 octets. L’analyse des arguments, le traitement des signaux et les appels de lecture et d’écriture sont ceux de la livraison initiale.

Pour isoler le coût fixe, deux témoins ont été liés depuis la même bibliothèque Rust avec la même commande clang, en faisant varier l’option de liaison. Le SHA-256 de leur section de code correspond respectivement à celui de l’ancien binaire et à celui du binaire livré. Les empreintes des Mach-O entiers différant entre constructions, les deux confirmations ont utilisé le fichier final exact. Sur 64 exécutions de `--version` par témoin, réparties également entre les deux ordres, la médiane passe de 2,754 à 2,353 ms. Cette différence de 0,401 ms est mesurable sans copie de données. L’expérience inclut le chargement, la production de la version et la fin du processus. Elle ne décompose pas le temps interne de dyld. [Tailles, empreintes et passages](liaison.json) · [Comparaison avec GNU et le candidat](essai-liaison.json)

**Frontières des E/S.** Pour les trois binaires, un fichier de 3 079 octets produit quatre écritures de 1 024, 1 024, 1 024 et 7 octets sur stdout, mesurées par les messages d’un socket datagramme. Deux ouvertures en ajout du même fichier régulier produisent exactement la duplication intercalée de ces blocs. L’offset d’entrée partagé après un refus immédiat sur stdout confirme la première lecture de 1 024 octets. Avec une limite fichier de 1 537 octets, une erreur pendant le deuxième bloc laisse respectivement 1 537, 1 024, 1 024 et 1 024 octets dans les quatre sorties et le statut 1. Le moteur diffuse chaque lecture dans l’ordre stdout, puis opérandes. Un changement de taille de bloc aurait pu changer ces préfixes observables. [Sondes initiales](es-initiales.json) · [Sondes finales](es-finales.json)

## Mesures d’acceptation

Fichiers réguliers sur le Mac, données et destinations identiques pour les trois exécutables. Un bloc binaire déterministe de 1 Mio, de graine 27, est répété comme dans la campagne historique. Les données d’entrée ont été relues avant chronométrage. Chaque charge et chaque exécutable ont deux échauffements. Les fichiers de sortie sont réutilisés et tronqués avant chaque passage. stdout est redirigée vers le premier fichier, les trois autres sont des opérandes de tee. Le temps va de la création du processus à la collecte du statut. Aucune synchronisation fsync. Le compte des octets et le SHA-256 de **chaque** destination sont vérifiés après le chronométrage.

Chaque tour parcourt les six charges dans un ordre mélangé à graine fixée. Chaque charge utilise également les deux ordres à deux exécutables ou les six permutations à trois. Toutes les mesures après échauffement entrent dans la médiane. Q1 et Q3 sont les quartiles inclusifs. Les données brutes conservent aussi minimum, maximum, temps utilisateur et système, RSS, changements de contexte involontaires, octets vérifiés par leur taille de corpus et débit total.

### Charge ambiante

| Entrée (Mio) | Sorties | Ancien (ms) | Corrigé (ms) | GNU (ms) | Q1–Q3 corrigé (ms) | Débit corrigé (Mio/s) | Rapport exact corrigé/GNU |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 5,510 | 5,041 | 4,984 | 4,839–5,255 | 198,4 | 593098/586397 |
| 1 | 4 | 10,920 | 10,351 | 10,358 | 10,142–10,710 | 386,4 | 20702624/20716375 |
| 8 | 1 | 19,438 | 18,921 | 18,876 | 18,748–19,507 | 422,8 | 37841375/37751749 |
| 8 | 4 | 59,074 | 58,459 | 58,296 | 57,804–59,967 | 547,4 | 116917334/116591001 |
| 32 | 1 | 66,480 | 66,396 | 65,763 | 66,033–68,044 | 482,0 | 132791333/131525791 |
| 32 | 4 | 230,226 | 228,731 | 226,098 | 226,924–232,863 | 559,6 | 457461041/452195375 |

### Deux calculs et une écriture en parallèle

| Entrée (Mio) | Sorties | Ancien (ms) | Corrigé (ms) | GNU (ms) | Q1–Q3 corrigé (ms) | Débit corrigé (Mio/s) | Rapport exact corrigé/GNU |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 5,478 | 5,122 | 5,128 | 4,886–5,356 | 195,2 | 10244042/10256459 |
| 1 | 4 | 10,924 | 10,522 | 10,311 | 10,322–10,857 | 380,2 | 21043250/20622499 |
| 8 | 1 | 19,705 | 19,149 | 18,981 | 18,879–19,768 | 417,8 | 38298417/37961251 |
| 8 | 4 | 60,023 | 59,570 | 58,568 | 58,059–60,887 | 537,2 | 39713486/39045111 |
| 32 | 1 | 68,275 | 68,608 | 67,504 | 66,534–69,614 | 466,4 | 137215750/135008791 |
| 32 | 4 | 236,139 | 228,091 | 230,824 | 225,924–234,883 | 561,2 | 152060417/153882750 |

Sur les deux confirmations, les pics RSS maximaux hors échauffement sont de 2 195 456 octets pour l’ancien Nova, 2 097 152 pour le corrigé et 2 064 384 pour GNU. Les débits concernent les octets écrits sur l’ensemble des sorties. Le rapport d’acceptation est calculé séparément pour chaque ligne. Le banc renvoie un statut d’échec si un rapport du nouveau binaire dépasse 11/10.

La confirmation ambiante a vu jusqu’à deux processus Cargo et un rustc. La seconde ajoute deux travailleurs SHA-256 et un écrivain qui réutilise un fichier privé de 8 Mio, sans fsync. Cet écrivain a traité 3 092 Mio en 39,06 s. Les comptes et moyennes de charge sont échantillonnés entre les tours. Cette expérience couvre une contention contrôlée. Elle ne reconstitue pas la charge exacte des QEMU de la première livraison. Les distributions individuelles et leurs extrêmes sont conservés dans les fichiers JSON. [Série ambiante](confirmation.json) · [Série avec charge ajoutée](confirmation-charge.json) · [Activité des travailleurs](travailleurs.json)

## Non-régression et portée

Les 191 comparaisons fonctionnelles avec GNU réussissent sur le Mach-O final, dont SIGINT, pipe fermé, sortie non bloquante, plusieurs destinations, lecture refusée et écriture partielle. Les suites amont `tee.sh` et `append.sh` réussissent sur les deux outils. `short-write.sh` et `write-eagain.sh` rendent 77 faute de strace sur Darwin. Le contrôle par quota et les comparaisons à lecteur retardé couvrent localement ces familles de comportement. [Résultats fonctionnels](fonctionnel.json) · [Bancs amont](amont.txt)

Les SHA des deux ELF et des preuves initiales ont été revérifiés, ainsi que chacun des 103 fichiers de l’archive initiale. La correction ne modifie que la liaison du Mach-O. Les résultats QEMU restent ceux de la recette native antérieure. Les invités ne possèdent pas un GNU tee comparable pour établir un ratio Darwin. [Contrôle des empreintes](archive-initiale.json)

## Rejeu

Depuis la racine du worktree, avec des noms de rapports neufs. Pour rejouer le banc d’origine après la correction, extraire l’ancienne livraison dans un répertoire privé et exécuter son propre script à cet emplacement. Le chemin du comparateur GNU est `/opt/homebrew/bin/gtee`.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$(mktemp -d "$NOVAOS_TMPDIR/tee-comparaison.XXXXXXXX")/"
export PYTHONDONTWRITEBYTECODE=1
D="$PWD/livraison-tee-0240/performance-darwin"
L="$PWD/livraison-tee-0240"
BASE=12f7f8bade0457d64fcea149389a9387b39f8ee3
git archive "$BASE" livraison-tee-0240 | tar -x -C "$TMPDIR"
ANCIEN="${TMPDIR}livraison-tee-0240/bin/tee"
python3 -B "${TMPDIR}livraison-tee-0240/outils/mesures.py" court "${TMPDIR}origine.json"
python3 -B "$D/comparer.py" "${TMPDIR}initiale.json" --ancien "$ANCIEN" --graine 271
python3 -B "$D/comparer.py" "${TMPDIR}controle.json" --ancien "$ANCIEN" --graine 272
python3 -B "$D/comparer.py" "${TMPDIR}confirmation.json" --ancien "$ANCIEN" --nouveau "$L/bin/tee" --graine 274
python3 -B "$D/sous-charge.py" "${TMPDIR}travailleurs.json" "${TMPDIR}charge.json" --ancien "$ANCIEN" --nouveau "$L/bin/tee" --graine 275
python3 -B "$D/observer-es.py" "${TMPDIR}es.json" "$ANCIEN" "$L/bin/tee" /opt/homebrew/bin/gtee
"$L/tester-options-tee.sh" macOS "${TMPDIR}fonctionnel.json"
"$L/outils/amont.sh" "$NOVAOS_TMPDIR/sort-arc027-gnu" "${TMPDIR}amont"
```

Pour isoler à nouveau la liaison et le chemin sans transfert depuis les mêmes objets de bibliothèque, puis vérifier le fichier publié indépendamment des témoins reconstruits :

```sh
S=components/coreutils/tee
export CARGO_PKG_VERSION=0.20.0
rustup run stable rustc --edition 2024 --target aarch64-apple-darwin --crate-type staticlib -C panic=abort -C opt-level=3 "$S/host.rs" -o "${TMPDIR}libtee.a"
cc -arch arm64 -O2 -Wall -Wextra -Werror "$S/darwin.c" "${TMPDIR}libtee.a" -o "${TMPDIR}temoin"
cc -arch arm64 -O2 -Wall -Wextra -Werror -Wl,-dead_strip "$S/darwin.c" "${TMPDIR}libtee.a" -o "${TMPDIR}compact"
python3 -B "$D/comparer.py" "${TMPDIR}essai.json" --ancien "$ANCIEN" --nouveau "${TMPDIR}compact" --graine 273
python3 -B "$D/cout-fixe.py" "${TMPDIR}liaison.json" "${TMPDIR}temoin" "${TMPDIR}compact" "$ANCIEN" "$L/bin/tee" "${TMPDIR}libtee.a"
```
