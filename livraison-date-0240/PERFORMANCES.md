# Mesures de date

Apple M4 Max, macOS-27.2.1-arm64-arm-64bit-Mach-O, arm64. Locale C, TZ=UTC0. 5 mesures chronométrées après un échauffement, en alternant les deux processus.

| Lot macOS | Dates | Nova ms | GNU ms | Nova dates/s | GNU dates/s | Nova RSS Mio | GNU RSS Mio |
|---|---:|---:|---:|---:|---:|---:|---:|
| époques | 65536 | 23.89 | 88.34 | 2743202 | 741874 | 2.20 | 1.97 |
| calendrier | 65536 | 28.56 | 103.49 | 2294494 | 633248 | 2.16 | 1.97 |
| invalides | 65536 | 104.39 | 224.31 | 627810 | 292167 | 2.22 | 1.91 |

La durée couvre le démarrage du processus, la lecture, l’analyse, le formatage et la sortie redirigée vers le périphérique nul. Le RSS est le pic du processus enfant fourni par wait4, puis la médiane des passages. Les empreintes des sorties utiles ou des diagnostics et les statuts des deux outils ont été comparés avant chronométrage.

## NovaOS sous QEMU

QEMU 11.1.3, TCG, un vCPU, 256 Mio, VirtIO-blk et volume P9 jetable. AArch64 virt Cortex-A72/GICv2 et x86_64 Q35/max. Un passage de chauffe puis trois observations. Chaque lot contient 32 lignes. Le lancement utilise la délégation de la recette.

| ISA | Secteur | Lot | Dates | Nova ms, session | Nova dates/s, session | Nova ms, processus | Pages utilisateur au pic | Kio |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| aarch64 | 512 | valides | 32 | 56.43 | 567 | 15.69 | 231 | 924 |
| aarch64 | 512 | invalides | 32 | 56.02 | 571 | 8.48 | 231 | 924 |
| aarch64 | 4096 | valides | 32 | 56.56 | 566 | 15.64 | 231 | 924 |
| aarch64 | 4096 | invalides | 32 | 51.44 | 622 | 9.30 | 231 | 924 |
| x86_64 | 512 | valides | 32 | 50.02 | 640 | 10.21 | 236 | 944 |
| x86_64 | 512 | invalides | 32 | 38.11 | 840 | 7.59 | 236 | 944 |
| x86_64 | 4096 | valides | 32 | 48.27 | 663 | 10.27 | 236 | 944 |
| x86_64 | 4096 | invalides | 32 | 42.74 | 749 | 8.18 | 236 | 944 |

Le temps de session inclut nsh, UART et le retour du statut. Le temps de processus va du premier appel observé à la sortie, à partir du compteur invité. Le pic mémoire échantillonne les pages privées de code et de données aux appels système. Il exclut les tables de pages, les services, QEMU et le noyau. Les pages sont de 4 Kio. L’oracle GNU tourne sur Darwin. La campagne ne contient aucune mesure de vitesse de GNU dans NovaOS.

[Observations Darwin](preuves/mesures.json) · [Observations QEMU](preuves/)
