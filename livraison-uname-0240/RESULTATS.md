# Résultats de qualification de uname

Darwin ARM64 a réussi 572/572 cas. Les statuts et les deux flux sont conservés pour GNU et Nova. [Mesures macOS en regard](preuves/darwin.md)

| Invité P9 | Cas ELF conformes | Commandes nsh vérifiées | Écritures / octets / flush pendant les consultations |
|---|---:|---:|---|
| aarch64 / 512 | 501 | 20 | 0 / 0 / 0 |
| aarch64 / 4096 | 501 | 20 | 0 / 0 / 0 |
| x86_64 / 512 | 501 | 20 | 0 / 0 / 0 |
| x86_64 / 4096 | 501 | 20 | 0 / 0 / 0 |

## ELF NovaOS sous QEMU

Médianes en millisecondes. Chaque case donne le trajet complet puis, entre parenthèses, le temps d’admission et de chargement inclus dans le total. Trois passages suivent un échauffement. La fréquence et les compteurs bruts figurent dans chaque rapport SDK.

| Sélection | Appels | AArch64 / 512 | AArch64 / 4096 | x86_64 / 512 | x86_64 / 4096 |
|---|---:|---:|---:|---:|---:|
| -s | 1 | 5.625 (1.103) | 4.155 (1.012) | 2.915 (0.675) | 4.416 (0.714) |
| -s | 8 | 45.668 (9.745) | 43.543 (7.785) | 32.634 (5.724) | 32.137 (5.364) |
| -a | 1 | 6.055 (1.613) | 5.896 (1.034) | 4.418 (0.688) | 3.897 (0.680) |
| -a | 8 | 48.582 (8.722) | 45.736 (8.189) | 31.869 (5.558) | 31.901 (6.105) |
| -omrs | 1 | 5.873 (1.081) | 5.874 (1.018) | 4.224 (0.681) | 4.352 (0.782) |
| -omrs | 8 | 44.466 (8.641) | 44.148 (8.083) | 32.427 (5.626) | 31.661 (5.539) |

## Couverture par orthographe

Nombre d’exécutions réussies du cas isolé dans les journaux. Une erreur attendue constitue un test conforme. Chaque cellule QEMU recompte les argv observés par le contrôleur SDK.

| Orthographe | Darwin | AArch64 / 512 | AArch64 / 4096 | x86_64 / 512 | x86_64 / 4096 |
|---|---:|---:|---:|---:|---:|
| défaut | 1 | 1 | 1 | 1 | 1 |
| `--` | 1 | 1 | 1 | 1 | 1 |
| `-a` | 3 | 1 | 1 | 1 | 1 |
| `-A` | 3 | 1 | 1 | 1 | 1 |
| `-s` | 2 | 1 | 1 | 1 | 1 |
| `-n` | 2 | 1 | 1 | 1 | 1 |
| `-r` | 3 | 1 | 1 | 1 | 1 |
| `-v` | 2 | 1 | 1 | 1 | 1 |
| `-m` | 2 | 1 | 1 | 1 | 1 |
| `-p` | 3 | 1 | 1 | 1 | 1 |
| `-i` | 3 | 1 | 1 | 1 | 1 |
| `-o` | 3 | 1 | 1 | 1 | 1 |
| `--all` | 2 | 1 | 1 | 1 | 1 |
| `--all-labeled` | 2 | 1 | 1 | 1 | 1 |
| `--kernel-name` | 2 | 1 | 1 | 1 | 1 |
| `--sysname` | 2 | 1 | 1 | 1 | 1 |
| `--nodename` | 2 | 1 | 1 | 1 | 1 |
| `--kernel-release` | 2 | 1 | 1 | 1 | 1 |
| `--release` | 2 | 1 | 1 | 1 | 1 |
| `--kernel-version` | 2 | 1 | 1 | 1 | 1 |
| `--machine` | 2 | 1 | 1 | 1 | 1 |
| `--processor` | 2 | 1 | 1 | 1 | 1 |
| `--hardware-platform` | 2 | 1 | 1 | 1 | 1 |
| `--operating-system` | 2 | 1 | 1 | 1 | 1 |
| `--help` | 10 | 1 | 1 | 1 | 1 |
| `--version` | 10 | 1 | 1 | 1 | 1 |

Les matrices complètes incluent les combinaisons, diagnostics et flux. [Darwin](preuves/options.json) · [Résultats, séries UART et médias QEMU](preuves/qemu)
