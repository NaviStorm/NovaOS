# Matrice des options de GNU awk 5.4.1

Oracle GNU ARM64 du banc. Les formes courtes et longues, séparées ou attachées, figurent avec leurs arguments, sorties, diagnostics, fichiers et statuts dans [la mesure brute](lab/results/options.json). Une égalité porte sur le cas exercé. Les restrictions du langage et des services sont consignées dans [le rapport](QUALIFICATION.md).

| Option courte | Option longue | Formes exercées | Résultat du candidat Darwin |
|---|---|---:|---|
| `-f` | `--file` | 4 | Concordance des formes exercées. |
| `-F` | `--field-separator` | 4 | Concordance des formes exercées. |
| `-v` | `--assign` | 4 | Concordance des formes exercées. |
| `-b` | `--characters-as-bytes` | 2 | Traitement par octets, y compris NUL et octets invalides dans le différentiel. |
| `-c` | `--traditional` | 2 | Refus explicite du candidat (statut 2). |
| `-C` | `--copyright` | 2 | Notice MIT du moteur. |
| `-d` | `--dump-variables` | 4 | Refus explicite du candidat (statut 2). |
| `-D` | `--debug` | 4 | Refus explicite du candidat (statut 2). |
| `-e` | `--source` | 4 | Concordance des formes exercées. |
| `-E` | `--exec` | 4 | Concordance des formes exercées. |
| `-g` | `--gen-pot` | 2 | Refus explicite du candidat (statut 2). |
| `-h` | `--help` | 2 | Aide ARC-026 en français ou en anglais. |
| `-i` | `--include` | 4 | Recherche AWKPATH et inclusion unique. Les chemins par défaut GNU installés ne sont pas embarqués. |
| `-I` | `--trace` | 2 | Refus explicite du candidat (statut 2). |
| `-k` | `--csv` | 2 | Refus explicite du candidat (statut 2). |
| `-l` | `--load` | 4 | Chargement dynamique refusé avant exécution, y compris pour un module nommé. |
| `-L` | `--lint` | 4 | Refus explicite du candidat (statut 2). |
| `-M` | `--bignum` | 2 | Refus explicite du candidat (statut 2). |
| `-N` | `--use-lc-numeric` | 2 | Refus explicite du candidat (statut 2). |
| `-n` | `--non-decimal-data` | 2 | Refus explicite du candidat (statut 2). |
| `-o` | `--pretty-print` | 4 | Refus explicite du candidat (statut 2). |
| `-O` | `--optimize` | 2 | Refus explicite du candidat (statut 2). |
| `-p` | `--profile` | 4 | Refus explicite du candidat (statut 2). |
| `-P` | `--posix` | 2 | Refus explicite du candidat (statut 2). |
| `-r` | `--re-interval` | 2 | Refus explicite du candidat (statut 2). |
| `-s` | `--no-optimize` | 2 | Refus explicite du candidat (statut 2). |
| `-S` | `--sandbox` | 2 | Exécution admise. Les quatre opérations interdites produisent une erreur dans le différentiel. |
| `-t` | `--lint-old` | 2 | Refus explicite du candidat (statut 2). |
| `-V` | `--version` | 2 | Version propre du moteur en français ou en anglais. |
| `-T` | `--persist` | 4 | Refus explicite du candidat (statut 2). |
| `-Y` | `--parsedebug` | 2 | Option conditionnelle du code source GNU. Refus dans les deux exécutables testés. |
| `-Z` | `--locale` | 4 | Option conditionnelle du code source GNU. Refus dans les deux exécutables testés. |

Le séparateur `--` est accepté. Les sondes du commutateur interne `-G`, de `-W version`, des options groupées `-bh` et de l’abréviation `--vers` sont également archivées. Le candidat exige les noms longs entiers et refuse les groupes courts. Les formes de `--persist` ont un diagnostic propre à l’oracle.
