# Charge grep sur macOS ARM64

32/32 exécutions conformes au modèle indépendant.
Temps médian de deux passages par binaire, RSS maximal observé. Cache système ordinaire, ordre alterné.
Les lignes ont un identifiant unique et un corps synthétique de période 65 536. Le motif d’extraction lit stdin sur un fichier régulier.

| Lignes | Cas | Lignes sélectionnées | Livré (s) | GNU (s) | Livré (Mio RSS) | GNU (Mio RSS) |
|---:|---|---:|---:|---:|---:|---:|
| 10000000 | ere-journal | 445875 | 2.58 | 2.16 | 3.45 | 3.53 |
| 10000000 | bre-rappel | 1248905 | 2.52 | 8.30 | 3.20 | 3.22 |
| 10000000 | pcre-extraction | 43638 | 0.53 | 0.63 | 3.38 | 3.03 |
| 10000000 | pcre-absence | 0 | 0.42 | 0.44 | 3.30 | 3.03 |
| 65000000 | ere-journal | 2898142 | 17.77 | 15.51 | 3.42 | 3.52 |
| 65000000 | bre-rappel | 8118036 | 17.81 | 57.15 | 3.20 | 3.20 |
| 65000000 | pcre-extraction | 283658 | 3.58 | 4.26 | 3.39 | 3.03 |
| 65000000 | pcre-absence | 0 | 2.78 | 2.95 | 3.30 | 3.03 |
