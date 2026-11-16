# Matrice de qualification GNU uniq 9.12

Chaque cellule contient le statut observé. Tous les statuts de cette table correspondent au statut attendu du profil. Les sorties complètes, y compris les NUL et les diagnostics, figurent en hexadécimal dans les matrices liées. Pour les erreurs de syntaxe invitées, le contrôle porte sur le statut natif et la trace contient le diagnostic observé. Pour l’aide et la version, le contrôle porte sur la langue, le contenu propre au produit et les droits.

| Invocation ou option | Mach-O ARM64 | AArch64 512 | AArch64 4096 | x86_64 512 | x86_64 4096 |
|---|---:|---:|---:|---:|---:|
| Sans option | 0 | 0 | 0 | 0 | 0 |
| `-c` | 0 | 0 | 0 | 0 | 0 |
| `--count` | 0 | 0 | 0 | 0 | 0 |
| `-d` | 0 | 0 | 0 | 0 | 0 |
| `--repeated` | 0 | 0 | 0 | 0 | 0 |
| `-D` | 0 | 0 | 0 | 0 | 0 |
| `--all-repeated` | 0 | 0 | 0 | 0 | 0 |
| `--all-repeated=none` | 0 | 0 | 0 | 0 | 0 |
| `--all-repeated=prepend` | 0 | 0 | 0 | 0 | 0 |
| `--all-repeated=separate` | 0 | 0 | 0 | 0 | 0 |
| `--group` | 0 | 0 | 0 | 0 | 0 |
| `--group=separate` | 0 | 0 | 0 | 0 | 0 |
| `--group=prepend` | 0 | 0 | 0 | 0 | 0 |
| `--group=append` | 0 | 0 | 0 | 0 | 0 |
| `--group=both` | 0 | 0 | 0 | 0 | 0 |
| `-u` | 0 | 0 | 0 | 0 | 0 |
| `--unique` | 0 | 0 | 0 | 0 | 0 |
| `-i` | 0 | 0 | 0 | 0 | 0 |
| `--ignore-case` | 0 | 0 | 0 | 0 | 0 |
| `-f 1` | 0 | 0 | 0 | 0 | 0 |
| `--skip-fields=1` | 0 | 0 | 0 | 0 | 0 |
| `-s 1` | 0 | 0 | 0 | 0 | 0 |
| `--skip-chars=1` | 0 | 0 | 0 | 0 | 0 |
| `-w 1` | 0 | 0 | 0 | 0 | 0 |
| `--check-chars=2` | 0 | 0 | 0 | 0 | 0 |
| `-z` | 0 | 0 | 0 | 0 | 0 |
| `--zero-terminated` | 0 | 0 | 0 | 0 | 0 |
| `-cd` | 0 | 0 | 0 | 0 | 0 |
| `-cu` | 0 | 0 | 0 | 0 | 0 |
| `-di` | 0 | 0 | 0 | 0 | 0 |
| `-Di` | 0 | 0 | 0 | 0 | 0 |
| `-s 1 -c` | 0 | 0 | 0 | 0 | 0 |
| `-f 1 -s 1 -w 2` | 0 | 0 | 0 | 0 | 0 |
| `-cD` | 1 | 3 | 3 | 3 | 3 |
| `--group -c` | 1 | 3 | 3 | 3 | 3 |
| `--group -u` | 1 | 3 | 3 | 3 | 3 |
| `--group -D` | 1 | 3 | 3 | 3 | 3 |
| `--group -d` | 1 | 3 | 3 | 3 | 3 |
| `-du` | 0 | 0 | 0 | 0 | 0 |
| `-Dui` | 0 | 0 | 0 | 0 | 0 |
| `-w0` | 0 | 0 | 0 | 0 | 0 |
| `-w999` | 0 | 0 | 0 | 0 | 0 |
| `-1` | 0 | 0 | 0 | 0 | 0 |
| `-Dsep` | 1 | 3 | 3 | 3 | 3 |
| `--group=bad` | 1 | 3 | 3 | 3 | 3 |
| `--all-repeated=bad` | 1 | 3 | 3 | 3 | 3 |
| `--s` | 1 | 3 | 3 | 3 | 3 |
| `--unknown` | 1 | 3 | 3 | 3 | 3 |
| `-X` | 1 | 3 | 3 | 3 | 3 |
| `-w` | 1 | 3 | 3 | 3 | 3 |
| `-sX` | 1 | 3 | 3 | 3 | 3 |
| `-f-1` | 1 | 3 | 3 | 3 | 3 |
| `-w-0` | 0 | 0 | 0 | 0 | 0 |
| `--count=x` | 1 | 3 | 3 | 3 | 3 |
| `--` | 0 | 0 | 0 | 0 | 0 |
| `--help` | 0 | 0 | 0 | 0 | 0 |
| `--version` | 0 | 0 | 0 | 0 | 0 |

## Lecture des octets et cas supplémentaires

Sur Mac, chaque entrée contient les arguments, la locale, les variables, l’entrée hexadécimale, les sorties attendue et réelle, le diagnostic, le statut et, le cas échéant, le contenu de la destination. Le témoin Madrid/Lisbon, 100 graines par locale, les syntaxes historiques, les opérandes et les erreurs d’E/S y figurent. [435 cas Mach-O](preuves/options.json) · [435 cas oracle](preuves/oracle-gnu.json)

Dans chaque invité, retrouver la ligne par commande dans la matrice. Les champs `expected_hex` et `actual_hex` décrivent la console après normalisation des fins de ligne UART. La ligne `lecture P9 sortie` compare le fichier binaire brut. Les corpus P9, la commande complète et les sorties UART brutes figurent dans les archives de session. Les consultations documentaires utilisent le profil instrumenté.

- [aarch64-512](preuves/qemu/aarch64-512/matrice.json)
- [aarch64-4096](preuves/qemu/aarch64-4096/matrice.json)
- [x86_64-512](preuves/qemu/x86_64-512/matrice.json)
- [x86_64-4096](preuves/qemu/x86_64-4096/matrice.json)

Les droits et les statuts propres à NovaOS sont détaillés dans les [références](REFERENCES.md). Les compteurs supérieurs à sept chiffres sur P9 et les variantes de longueur des lignes et des groupes figurent dans les [essais autonomes invités](preuves/mesures-natives/).
