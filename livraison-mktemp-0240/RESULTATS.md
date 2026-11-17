# Mesures et qualification du 17 novembre 2026

Base Git `425e4e7cc5c03e246efb64d3638376906344bf47`. Portage `c63151a7116f7f0bc3130520e3c8be465ef4d763`. Le SHA-256 des trois binaires est consigné dans [l’environnement](preuves/environnement.txt).

## macOS ARM64

M4 Max, Darwin natif, GNU 9.12 et Nova 0.24.0. Chaque ligne mesure 256 créations de même type par passage, sur trois passages par exécutable en alternant l’ordre. Le temps est la médiane murale du lot. La mémoire est le maximum des RSS individuelles observées avec wait4, en Kio.

| Objet | Processus | Temps Nova (ms) | Temps GNU (ms) | Nova (créations/s) | GNU (créations/s) | RSS Nova | RSS GNU |
|---|---:|---:|---:|---:|---:|---:|---:|
| Fichier | 1 | 708.3 | 696.8 | 361 | 367 | 1952 | 1872 |
| Fichier | 4 | 326.6 | 316.9 | 784 | 808 | 1952 | 1872 |
| Dossier | 1 | 724.9 | 719.8 | 353 | 356 | 1936 | 1872 |
| Dossier | 4 | 301.9 | 296.4 | 848 | 864 | 1936 | 1872 |

90/90 cas différentiels. Le test GNU mktemp.pl compte 33 cas par exécutable. Un test distinct conserve le lien symbolique occupé et son fichier témoin pour les trois modes. Les essais Linux strace/setarch et /dev/full ne sont pas disponibles sur ce Mac. Le banc ferme stdout pour vérifier la restitution du nom et le nettoyage. APFS refuse le suffixe d’octet FF sur les deux programmes.

## NovaOS sous QEMU

TCG, deux vCPU, 256 Mio de RAM, disque VirtIO P9 jetable. Chaque lot comporte quatre fichiers et quatre répertoires par demandeur. Les débits sont calculés à partir de trois durées murales, délimitées par la série. Les mesures de l’ELF comprennent l’échange de requête et réponse par P9, le lancement et la collecte des huit travailleurs. Les noms sont relus après la collecte. En concurrence, les intervalles des deux demandeurs se recouvrent à chaque passage. La sonde de collisions impose un premier nom occupé par création dans un seul travailleur par lot. Le pic de cadres de 4 Kio couvre toute la session de stockage dans la VM. La RSS isolée de mktemp y reste non mesurée.

| Invité | Secteur | ELF série (créations/s) | ELF, 2 demandeurs (créations/s) | Sonde avec collisions (créations/s) | Pic de cadres, série / duo |
|---|---:|---:|---:|---:|---:|
| aarch64 | 512 | 20.7 | 25.5 | 50.2 | 807 / 1018 |
| aarch64 | 4096 | 19.1 | 25.3 | 48.0 | 807 / 1018 |
| x86_64 | 512 | 23.3 | 23.5 | 54.4 | 811 / 1024 |
| x86_64 | 4096 | 23.2 | 27.5 | 64.8 | 811 / 1024 |

81/90 cas de la matrice Darwin sont rejoués par le véritable ELF sur chacun des quatre profils. Les absences de correspondance figurent dans la matrice. Les quatre profils vérifient aussi un chemin fragmenté et le refus de remontée. Le shell ordinaire donne 0 pour aide, version et manuel en français et en anglais selon LANG, 127 pour la commande métier et 3 pour le pipe mktemp -u | wc -l. Aucune variation de l’empreinte P9 ni des compteurs WRITE/FLUSH pendant les consultations. Le profil SDK sans ports refuse la sortie des sept invocations. GNU n’a pas d’exécutable pour l’ABI native.

Les chronométrages d’ELF de recette sont propres à la machine virtuelle. Le lanceur générique et les droits de flux du shell restent à intégrer. Le profil concurrent porte sur deux instances, chacune propriétaire de sa vue privée. Une panne par coupure secteur pendant la préparation et la comparaison des traductions gettext hors locale C restent hors qualification.

[Mesures Darwin brutes](preuves/darwin.json) · [Résultats et chronométrages des vingt démarrages](preuves/qemu.json) · [Matrice détaillée](MATRICE.md)
