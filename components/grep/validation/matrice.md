# Matrice GNU grep 3.12 sur Darwin ARM64

Chaque graphie a été invoquée avec son propre témoin. « Identique » exige le même statut, les mêmes octets sur stdout et les mêmes octets sur stderr. Les noms abrégés dans les globes et les valeurs d’options illustrent les arguments effectivement testés. Chaque JSON fournit argv, l’empreinte de stdin et les deux résultats hexadécimaux. Un résultat porte sur le témoin et sa locale.

| Options et alias exercés | Autres arguments du témoin | C | en_US.UTF-8 |
|---|---|---|---|
| `-G` `--basic-regexp` | `a\{2\}` | Identique | Identique |
| `-E` `--extended-regexp` | `warning\|error` | Identique | Identique |
| `-F` `--fixed-strings` | `a.b` | Identique | Identique |
| `-P` `--perl-regexp` | `(?<=a)b` | Identique | Identique |
| `-i` `-y` `--ignore-case` | `WARNING` | Identique | Identique |
| `--no-ignore-case` | `Warning` | Identique | Identique |
| `-v` `--invert-match` | `erreur` | Identique | Identique |
| `-w` `--word-regexp` | `x` | Identique | Identique |
| `-x` `--line-regexp` | `erreur` | Identique | Identique |
| `-e` `--regexp` | `erreur -e start` | Identique | Identique |
| `-f` `--file` | `motifs journal.txt` | Identique | Identique |
| `-n` `--line-number` | `erreur journal.txt` | Identique | Identique |
| `-c` `--count` | `erreur` | Identique | Identique |
| `-l` `--files-with-matches` | `erreur autre journal.txt` | Identique | Identique |
| `-L` `--files-without-match` | `erreur autre journal.txt` | Identique | Identique |
| `-o` `--only-matching` | `ab*` | Identique | Identique |
| `-q` `--quiet` `--silent` | `erreur absent journal.txt` | Identique | Identique |
| `-s` `--no-messages` | `x absent` | Identique | Identique |
| `-A` `--after-context` | `1 erreur` | Identique | Identique |
| `-B` `--before-context` | `2 erreur` | Identique | Identique |
| `-C` `--context` | `1 erreur` | Identique | Identique |
| `-1` | `erreur` | Identique | Identique |
| `--group-separator=SEP` | `-C0 erreur` | Identique | Identique |
| `--no-group-separator` | `-C0 erreur` | Identique | Identique |
| `-r` `--recursive` | `TODO sources/` | Identique | Identique |
| `-R` `--dereference-recursive` | `TODO sources/` | Identique | Identique |
| `--include=*.rs` | `-r TODO sources/` | Identique | Identique |
| `--exclude=*.rs` | `-r TODO sources/` | Identique | Identique |
| `--exclude-dir=ignore` | `-r TODO sources/` | Identique | Identique |
| `--exclude-from=exclusions` | `-r TODO sources/` | Identique | Identique |
| `-d` `--directories` | `skip TODO sources/` | Identique | Identique |
| `-D` `--devices` | `skip TODO fifo` | Identique | Identique |
| `-H` `--with-filename` | `erreur journal.txt` | Identique | Identique |
| `-h` `--no-filename` | `erreur journal.txt journal.txt` | Identique | Identique |
| `--label=flux` | `-H erreur` | Identique | Identique |
| `--color=always` `--colour=always` | `erreur` | Identique | Identique |
| `-a` `--text` `--binary-files=text` | `x` | Identique | Identique |
| `-I` `--binary-files=without-match` | `x` | Identique | Identique |
| `--binary-files=binary` | `x` | Identique | Identique |
| `-z` `--null-data` | `abc` | Identique | Identique |
| `-Z` `--null` | `-l erreur journal.txt` | Identique | Identique |
| `-b` `--byte-offset` | `-o erreur` | Identique | Identique |
| `-T` `--initial-tab` | `-Hn erreur journal.txt` | Identique | Identique |
| `-m` `--max-count` | `1 erreur` | Identique | Identique |
| `--line-buffered` | `erreur` | Identique | Identique |
| `-U` `--binary` | `x` | Identique | Identique |
| `--` | `-n` | Identique | Identique |
| `--help` | — | 0/1 identique (texte ARC-026) | 0/1 identique (texte ARC-026) |
| `-V` `--version` | — | 0/2 identique (texte ARC-026) | 0/2 identique (texte ARC-026) |

## Témoins complémentaires

Les combinaisons, refus de permission, erreurs de regex et de syntaxe, EOF, FIFO, SIGPIPE, vidage avant EOF, position de stdin régulier après -m, contextes, UTF-8 malformé et pics mémoire ont des témoins distincts. Le corpus généré varie les quatre dialectes, les options et les octets d’entrée avec une graine enregistrée.

- C · 1663 comparaisons, 1659 identiques. Écarts observés : `--help`, `-V`, `--version`, `invalide-couleur`.
- en_US.UTF-8 · 1663 comparaisons, 1659 identiques. Écarts observés : `--help`, `-V`, `--version`, `invalide-couleur`.
