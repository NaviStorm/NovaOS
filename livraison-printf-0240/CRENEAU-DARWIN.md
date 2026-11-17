# Créneau Darwin du 17 novembre 2026 · printf 0.24.0

**Plafond respecté sur les 4 charges du profil court et les 20 du profil complet.** La comparaison porte sur chaque charge et chaque profil séparément. Les exécutables de la livraison `bdef0f47135361725bc1bdb7ca680e8eda867e2b` ont été appelés directement sur macOS ARM64 27.2.1. GNU coreutils 9.12 provient de `/opt/homebrew/bin/gprintf`. Les chemins, versions et SHA-256 des deux exécutables sont enregistrés dans chaque JSON.

Le créneau réservé a duré de 05:32:56 à 05:32:59 (heure de Paris). Les deux profils se sont succédé. Locale C, trois échauffements par exécutable et par charge, ordre Nova/GNU alterné à chaque paire, même destination `/dev/null` pour les deux flux pendant le chronométrage. Les captures binaires séparées de stdout et stderr sont réalisées avant le chronométrage. Le statut de chaque processus mesuré est vérifié. Charge moyenne du Mac au début et à la fin des deux profils, sur 1, 5 et 15 minutes, respectivement 5,95 / 5,09 / 4,94. Les relevés de processus aux bornes ne signalent ni QEMU, ni compilateur Rust, ni création d’ISO.

## Charges et médianes

Les mesures incluent la création du processus et son attente. Le verdict utilise les médianes en nanosecondes entières et le test exact `10 × Nova ≤ 11 × GNU`. Les valeurs affichées sont arrondies. La dispersion est l’écart absolu médian autour de la médiane (MAD). Minima, maxima, valeurs individuelles, temps CPU utilisateur/système, pics RSS, débits et arguments figurent dans les JSON.

Pour les lignes « Texte », « Binaire » et « Décimal », chaque argument vaut respectivement 48 lettres `N`, six répétitions de `\0\xffA` ou `98765.125`. Les formats correspondants sont `%s` puis LF littéral, `%b` et `%+.9g` puis LF littéral. L’appel court est `%s` avec `x`. Le format vide est un argument de format de longueur zéro. Le lot « NUL et TAB » utilise `%b` et 30 arguments contenant chacun six répétitions de `A\0\tB`. Le format `%3$*1$.*2$f` consomme dix triplets `7`, `2`, `3.125`. L’erreur utilise `OK%s%n` et `X`.

### Profil court · 7 passages par exécutable

| Charge | Arguments | Sortie (octets) | Nova (ms) | GNU (ms) | Nova/GNU | MAD Nova / GNU (µs) |
|---|---:|---:|---:|---:|---:|---:|
| Appel court | 1 | 1 | 2,617500 | 2,677417 | 0,977621 | 59,8 / 111,1 |
| Texte | 30 | 1470 | 2,554083 | 2,449416 | 1,042731 | 108,0 / 90,3 |
| Binaire | 30 | 540 | 2,515208 | 2,290709 | 1,098004 | 86,8 / 83,6 |
| Décimal | 30 | 330 | 2,607917 | 2,455833 | 1,061928 | 146,7 / 140,7 |

### Profil complet · 25 passages par exécutable

| Charge | Arguments | Sortie (octets) | Nova (ms) | GNU (ms) | Nova/GNU | MAD Nova / GNU (µs) |
|---|---:|---:|---:|---:|---:|---:|
| Appel court | 1 | 1 | 2,622083 | 2,586292 | 1,013839 | 127,8 / 119,5 |
| Texte | 1 | 49 | 2,541166 | 2,429000 | 1,046178 | 149,8 / 101,3 |
| Binaire | 1 | 18 | 2,352834 | 2,313750 | 1,016892 | 67,8 / 91,6 |
| Décimal | 1 | 11 | 2,334042 | 2,301541 | 1,014121 | 120,8 / 93,3 |
| Texte | 4 | 196 | 2,342250 | 2,380375 | 0,983984 | 64,3 / 97,8 |
| Binaire | 4 | 72 | 2,366042 | 2,262209 | 1,045899 | 133,2 / 84,7 |
| Décimal | 4 | 44 | 2,266417 | 2,211875 | 1,024659 | 71,6 / 62,2 |
| Texte | 12 | 588 | 2,261709 | 2,258208 | 1,001550 | 105,1 / 65,1 |
| Binaire | 12 | 216 | 2,335917 | 2,253125 | 1,036745 | 82,0 / 83,9 |
| Décimal | 12 | 132 | 2,283208 | 2,264333 | 1,008336 | 72,6 / 80,5 |
| Texte | 24 | 1176 | 2,309250 | 2,263375 | 1,020268 | 72,9 / 30,1 |
| Binaire | 24 | 432 | 2,297459 | 2,284583 | 1,005636 | 78,2 / 120,2 |
| Décimal | 24 | 264 | 2,448625 | 2,348125 | 1,042800 | 125,1 / 137,7 |
| Texte | 30 | 1470 | 2,469084 | 2,328375 | 1,060432 | 89,4 / 122,0 |
| Binaire | 30 | 540 | 2,281208 | 2,262833 | 1,008120 | 60,5 / 96,5 |
| Décimal | 30 | 330 | 2,318375 | 2,297834 | 1,008939 | 60,4 / 77,1 |
| Format vide | 0 | 0 | 2,291833 | 2,210709 | 1,036696 | 87,9 / 78,2 |
| Nul et tab | 30 | 720 | 2,373625 | 2,360334 | 1,005631 | 139,8 / 93,3 |
| Étoiles et positions | 30 | 70 | 2,428375 | 2,333083 | 1,040844 | 126,4 / 120,5 |
| Erreur de format | 1 | 3 | 2,278791 | 2,267417 | 1,005016 | 76,3 / 48,5 |

## Lancement et traitement

Le format vide donne un repère de lancement et d’initialisation, 2,292 ms pour Nova et 2,211 ms pour GNU. Dans le profil complet, le lot binaire NUL/TAB prend 2,374 et 2,360 ms de bout en bout. L’écart avec le format vide est respectivement de 82 et 150 µs. Cette différence de médianes séparées inclut le passage des arguments et la sortie. La variabilité entre processus limite sa précision. Le banc QEMU apporte une mesure distincte du moteur sur 128 appels par démarrage amorti, avec un puits mémoire. [Mesures des quatre profils QEMU](preuves/cibles.json)

Le lot binaire de 30 arguments est le plus proche du plafond en profil court (1,098004). Sa médiane sur les 25 passages du profil complet donne 1,008120. Le maximum du profil complet est 1,060432 pour le texte à 30 arguments. Les octets stdout, les diagnostics bruts et les statuts sont conservés dans chaque entrée. Le lot NUL/TAB rend exactement 180 répétitions de `41 00 09 42`. L’erreur de format rend 1 et conserve le préfixe `OKX` avec un diagnostic sur stderr. Pour comparer les diagnostics, le banc harmonise le nom d’invocation.

## Rejouer

Depuis la racine du worktree, réserver le Mac pour les deux commandes de mesure. [Archive des deux JSON bruts et des tableaux terminaux](preuves/creneau-darwin.tar.xz) · [Recette fonctionnelle et limites du vrai nsh](README.md)

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/printf-0240/"
mkdir -p "$TMPDIR"
sortie=$(mktemp -d "${TMPDIR}creneau.XXXXXXXX")
./livraison-printf-0240/mesurer-performances-printf.sh --sortie "$sortie/court"
./livraison-printf-0240/mesurer-performances-printf.sh --complet --sortie "$sortie/complet"
python3 -B livraison-printf-0240/verifier-preuves.py
```
