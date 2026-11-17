# Moteur readlink sous QEMU

Chaque cellule donne la médiane de trois passages de 4 000 chemins en millisecondes. Le temps est mesuré entre marqueurs série sur l’hôte. Modèle en mémoire avec seize noms de liens indépendants pour la lecture brute, chaînes de quatre et seize liens pour -e, composante absente pour -m. Le disque P9 est attaché en lecture seule. La croissance du tas mesure la borne système depuis l’entrée de la sonde. La mesure de RSS par processus utilisée sur Darwin est indisponible dans ce profil.

| ISA | Secteur | Brut ms | Chaîne 4 ms | Chaîne 16 ms | Absent ms | Croissance du tas Kio |
|---|---:|---:|---:|---:|---:|---:|
| AArch64 | 512 | 0.80 | 7.46 | 23.98 | 3.12 | 64 |
| AArch64 | 4096 | 0.81 | 7.23 | 22.37 | 2.96 | 64 |
| x86_64 | 512 | 0.73 | 8.51 | 25.79 | 3.29 | 64 |
| x86_64 | 4096 | 0.71 | 8.66 | 26.18 | 3.28 | 64 |

| ISA | Secteur | Brut chemins/s | Chaîne 4 chemins/s | Chaîne 16 chemins/s | Absent chemins/s |
|---|---:|---:|---:|---:|---:|
| AArch64 | 512 | 5026970 | 536025 | 166782 | 1283766 |
| AArch64 | 4096 | 4934464 | 553018 | 178821 | 1350325 |
| x86_64 | 512 | 5448968 | 470279 | 155092 | 1217471 |
| x86_64 | 4096 | 5596041 | 462067 | 152793 | 1220396 |

QEMU TCG 11.1.3, 256 Mio. La sonde moteur utilise deux vCPU, la recette ELF et le terminal ordinaire un vCPU. AArch64 virt Cortex-A72 GICv2 et x86_64 Q35 CPU max. Les journaux contiennent les temps individuels, le nombre de chemins et les géométries. Le débit en chemins/s vaut 4 000 × 10⁹ / temps en nanosecondes. Aucun binaire GNU natif NovaOS ni débit de lecture de lien P9.
