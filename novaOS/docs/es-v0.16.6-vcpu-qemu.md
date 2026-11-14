# NovaOS v0.16.6 — Témoins QEMU du coût vCPU

## Périmètre

Campagne du 13 novembre 2026 sur `b79d1133dd0dbde9aabfaa88e14a3c593708f4b2`. Les sauvegardes partent de l’éditeur, demandent l’accord de `nsh` et publient vers le nom libre `copie`. Pour une source dense de 4 Mio, les scénarios sont l’insertion de Ω en tête ou au milieu, l’insertion de 4 Mio au milieu et la même insertion accompagnée d’une suppression en fin et d’une édition Unicode en tête. Les deux premiers portent la destination à 4 194 306 octets, les deux derniers à 8 388 608 et 8 388 609 octets. Les anciens ELF ont été réutilisés et identifiés par SHA-256. [Qualification CPU antérieure](es-v0.16.6-cpu.md) · [Mesures individuelles et empreintes](validation/es-v0.16.6-vcpu-qemu.json)

Le Mac M4 Max exécute trois bancs directs. Les invités AArch64 et x86_64 de QEMU 11.1.3 utilisent les images RAW en writeback et le profil de développement. Le moteur AArch64 QEMU 10.0.12 de l’application UTM 5.0.8 est lancé directement, avec un seul Cortex-A72 TCG, 256 Mio, le microprogramme système privé, VirtIO-IOMMU, GICv3, TPM CRB et un QCOW2 clairsemé. Chaque essai part d’un disque jetable en secteurs de 512 ou 4 096 octets. Les séries ont coexisté avec des régressions, la charge hôte de ces séries directes n’était pas fixée. La qualification de charge contrôlée dans l’application est documentée séparément.

## Mesure des latences

Pour chaque combinaison moteur, architecture, secteur et scénario, trois sauvegardes hors sonde mesurent le délai monotone du Mac entre l’envoi de la commande de sauvegarde et la réception du succès. Un quatrième démarrage instrumenté compte les échanges, les opérations bloc et suit les phases invitées jusqu’à l’acquittement après F2. Les ELF et les traces distinguent les deux profils. Le témoin du moteur UTM ajoute les compteurs QMP du disque et les temps utilisateur/système de l’ensemble des threads du processus QEMU. Les lecteurs P9 vérifient l’image, puis comparent la source et la destination octet par octet. Le témoin UTM direct redémarre également l’invité et rouvre la destination avant la relecture hôte. Les 16 passages TCG légers et les 16 passages de comptage d’instructions sont des essais auxiliaires distincts. [Banc RAW](../tests/es_groupees_elargies.py) · [Banc QCOW2](../tests/es_groupees_moteur_utm.py)

Les percentiles sont empiriques, calculés après tri au rang entier supérieur. Trois démarrages ne permettent pas d’estimer précisément la queue de distribution de chaque cellule. Avec trois valeurs, p95 et p99 reprennent tous deux la plus lente.

| Moteur direct, essais hors sonde | n | p50 (ms) | p95 (ms) | p99 et maximum (ms) | ≥ 1 s |
|---|---:|---:|---:|---:|---:|
| QEMU 11 AArch64, RAW | 24 | 152,565 | 511,758 | 583,624 | 0 |
| QEMU 11 x86_64, RAW | 24 | 236,983 | 586,211 | 629,793 | 0 |
| QEMU 10 d’UTM AArch64, QCOW2 | 24 | 128,397 | 378,357 | 386,306 | 0 |

Les 72 sauvegardes ont accusé réception en moins d’une seconde. La plus longue est la troisième grande insertion au milieu sur x86_64, avec des secteurs de 4 096 octets. Les 24 passages instrumentés distincts ont tous compté deux FLUSH et 41 à 57 échanges IPC par sauvegarde. Les maxima des différentes colonnes ci-dessous peuvent provenir de passages différents.

| Moteur direct, essais avec compteurs invités | n | Commande → succès, min–max (ms) | Publication → F2 acquise, min–max (ms) | Lot métadonnées + F1, max (ms) | Lot ancre + F2, max (ms) | Attente VirtIO cumulée, max (ms) | IPC, min–max |
|---|---:|---:|---:|---:|---:|---:|---:|
| QEMU 11 AArch64, RAW | 8 | 97,928–468,880 | 85,813–444,630 | 9,187 | 1,983 | 178,268 | 41–56 |
| QEMU 11 x86_64, RAW | 8 | 136,252–707,072 | 128,309–531,134 | 7,941 | 12,736 | 87,037 | 41–57 |
| QEMU 10 d’UTM AArch64, QCOW2 | 8 | 101,910–872,809 | 90,680–831,051 | 52,363 | 10,428 | 618,784 | 41–56 |

Le plus long passage instrumenté du moteur UTM direct concernait la grande insertion au milieu en secteurs de 4 096. Pendant les 872,809 ms écoulées, les threads du moteur ont consommé 344,477 ms de CPU. De la notification VirtIO à l’observation des entrées terminées, le pilote a cumulé 618,784 ms d’attente. Le backend QMP a compté 142 lectures (8,314 ms), 43 écritures (78,979 ms) et deux FLUSH (7,993 ms). Le lot F1 a pris 52,363 ms et le lot F2 10,428 ms. Les compteurs du backend, de l’invité et du moteur couvrent des intervalles qui se recoupent. Ces horloges ne permettent pas de répartir le délai restant entre ordonnancement des threads, traitement VirtIO et livraison de la notification. [Compteurs individuels](validation/es-v0.16.6-vcpu-qemu.json)

## Temps CPU et travail invité

Un plugin du laboratoire s’attache au QEMU 11.1.3 lancé directement. Il récupère dans l’ELF les adresses du marqueur et de ses compteurs avec `llvm-nm`. À la traduction, il ne place de rappel mémoire que sur les instructions du marqueur. À l’exécution d’un store réussi, il filtre sa destination pour repérer l’activation de la sauvegarde, les neuf entrées/sorties de phase et les quatre bornes des deux lots de publication. Il lit dans ce rappel synchrone les horloges Darwin `CLOCK_THREAD_CPUTIME_ID` et `CLOCK_MONOTONIC`. Les points sont gardés en mémoire puis écrits après la dernière borne. Il y a un seul vCPU. Les interruptions, la boucle d’attente invitée, le code de traduction et les opérations synchrones exécutées sur ce thread sont inclus. Les temps des autres threads QEMU ne sont pas imputés à la phase. [Sonde TCG](../tests/vcpu_phases_tcg.c) · [Vérification et regroupement des phases](../tests/vcpu_phases_tcg.py)

Une deuxième exécution ajoute une incrémentation inline à **chaque instruction invitée dispatchée**, y compris en dehors des marqueurs. Les points lisent aussi ce compteur. Cela augmente le temps CPU du thread. Pour les durées CPU on utilise le passage léger et pour le nombre d’instructions le passage compté. Le nombre d’instructions dispatchées inclut les instructions interrompues par une exception et les instructions du noyau. QEMU TCG émule le jeu d’instructions et les périphériques. Une durée sur un CPU AArch64 ou x86_64 matériel ne peut être déduite de ces valeurs. [API des plugins TCG](https://www.qemu.org/docs/master/devel/tcg-plugins.html) · [Modèle d’exécution QEMU](https://www.qemu.org/docs/master/devel/tcg-icount.html)

Le comptage à chaque instruction a fait dépasser une seconde à 12 de ses 16 sauvegardes, jusqu’à 2 023,074 ms sur x86_64 en secteurs de 4 096 pour la grande insertion combinée. Les 16 sauvegardes chronométrées par le plugin léger sont restées sous une seconde. Les latences individuelles des deux séries auxiliaires figurent dans le JSON.

| Architecture et famille | CPU thread de publication, ms | Part CPU SHA source + candidat | Instructions invitées de publication, millions | Part des instructions dans les deux SHA |
|---|---:|---:|---:|---:|
| AArch64, édition localisée | 96,33–104,79 | 44,3–49,2 % | 332,38–342,30 | 81,4–83,8 % |
| AArch64, grande insertion | 131,87–138,54 | 47,9–49,2 % | 500,64–512,88 | 81,4–83,4 % |
| x86_64, édition localisée | 144,00–155,69 | 64,9–68,5 % | 507,96–512,45 | 92,8–93,6 % |
| x86_64, grande insertion | 215,36–227,89 | 66,4–69,4 % | 765,01–768,71 | 92,7–93,2 % |

Sur les grandes insertions AArch64, le parcours et la construction des extents représentent ensemble 4,01–4,16 ms CPU. La validation du texte prend 8,21–8,50 ms. La paire de hachages représente une part importante du travail reproductible. Ces temps mesurés dans le moteur de développement ne permettent pas d’attribuer un arrêt de plusieurs secondes de l’application UTM à une opération SHA coûteuse.

Les bornes F1 couvrent l’envoi des métadonnées en attente et la barrière. Les bornes F2 couvrent l’envoi de l’ancre et la barrière. Sur les huit publications ARM64 du témoin léger, le premier intervalle dure 2,19–4,06 ms écoulées, dont 2,14–3,97 ms de CPU du vCPU. Le second dure 0,37–0,43 ms écoulées, dont 0,35–0,41 ms CPU. Le x86_64 a atteint 13,66 ms sur F1 et 6,17 ms sur F2. Les deux maxima proviennent de sauvegardes différentes. Les compteurs bloc mesurent séparément le cumul des deux FLUSH physiques et, sous QMP, le temps de service du nœud du disque. Une partie du CPU mesuré pendant ces lots peut être occupée par l’attente active et la soumission au périphérique. [Marqueurs de lots](../tests/fixtures/io-0166-prototype-sonde.patch) · [Relevés individuels](validation/es-v0.16.6-vcpu-qemu.json)

## Travail NovaFS restant

La candidate d’un nom libre possède un nouvel identifiant. Chaque page source est vérifiée, même si les octets seront supprimés. L’empreinte lie les identifiants de volume, l’adresse physique et la génération de naissance aux 4 Kio du bloc physique. Une page candidate dans un autre emplacement demande sa propre empreinte. Le contrôle des blocs partagés vérifie aussi le propriétaire et le décalage logique. Une réutilisation directe du digest ou du bloc à une autre identité changerait ces conditions. [Domaine des hachages](../../novaFS/src/codec.rs) · [Validation des références](../../novaFS/src/fs/check.rs)

Le code actuel offre deux pistes de réduction du travail. Lorsqu’une fenêtre de 1 Mio contient un caractère non ASCII, le prétest peut balayer un préfixe ASCII que la validation détaillée reparcourt ensuite par morceaux de 256 octets. Pour un suivi, une sonde peut isoler les fenêtres où ce premier scan échoue et compter réellement les octets revisités. Le parcours de la source relance une recherche depuis la racine et recule jusqu’à quinze pages à chaque frontière de fenêtre pour trouver l’extent chevauchant celle-ci. Un curseur persistant pourrait conserver ce point si les mêmes vérifications de bornes et de digest sont préservées. Le temps CPU mesuré pour le texte et les extents borne le gain atteignable en supprimant ces redondances sur les fichiers testés. [Préparation par fenêtres](../../novaFS/lab/es-groupees/preparation.rs) · [Parcours de l’index](../../novaFS/lab/es-groupees/arbre.rs)

## Reproduction

Construire les ELF et le générateur selon les fixtures de la qualification précédente. Pour chaque secteur, chaque architecture et chacun des quatre scénarios, exécuter le banc RAW trois fois hors sonde sur le profil approprié puis une fois sur le profil instrumenté. Exécuter les trois passages QCOW2 et le passage instrumenté sur des dossiers neufs. [Construction des profils](es-v0.16.6-cpu.md#reproduire-une-cellule)

```sh
python3 -B "$ARCHIVE_SANS/novaOS/tests/es_groupees_elargies.py" aarch64 512 inserer-debut "$GENERATEUR" "$SORTIE/raw-1.json" --sans-sonde
python3 -B novaOS/tests/es_groupees_moteur_utm.py "$PROFIL_UTM" "$ELF_UTM_SONDE" "$GENERATEUR" "$SORTIE/qcow2-1" 512 --cas inserer-debut --format qcow2 --sonde
```

Le plugin s’exécute sur l’hôte Darwin doté des en-têtes de QEMU 11 et GLib. Le module charge uniquement des adresses passées depuis la table de symboles du noyau de test. Les chemins de sortie doivent être neufs.

```sh
cc -O2 -Wall -Werror -fvisibility=hidden -bundle -undefined dynamic_lookup \
  $(pkg-config --cflags glib-2.0) -I"$(brew --prefix qemu)/include" \
  novaOS/tests/vcpu_phases_tcg.c -o "$SORTIE/vcpu-phases.so"
python3 -B novaOS/tests/vcpu_phases_tcg.py "$ARCHIVE_SONDE" "$SORTIE/vcpu-phases.so" "$GENERATEUR" aarch64 512 inserer-debut "$SORTIE/horloges"
python3 -B novaOS/tests/vcpu_phases_tcg.py "$ARCHIVE_SONDE" "$SORTIE/vcpu-phases.so" "$GENERATEUR" aarch64 512 inserer-debut "$SORTIE/instructions" --compter
```
