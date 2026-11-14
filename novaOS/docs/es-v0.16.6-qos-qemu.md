# NovaOS v0.16.6 — QoS du thread vCPU sous QEMU direct

## Protocole

Le banc compare cinq politiques publiques de Darwin sur QEMU 11.1.3 en TCG. Chaque bloc contient cinq démarrages consécutifs du même invité, avec une image P9 neuve de même géométrie et la même édition. Les politiques tournent entre blocs et l'ordre de chaque rotation s'inverse entre répétitions. Chaque architecture réalise six blocs de cinq sauvegardes par édition, dont trois en secteurs de 512 octets et trois en 4 096. Le support est un fichier RAW en writeback avec deux barrières FLUSH. Le profil de développement utilise un vCPU et 256 Mio, GICv2 et VirtIO-IOMMU sur AArch64, q35 et VT-d sur x86_64. [Lanceur](../tests/qos_qemu.py) · [Topologies](../scripts/bloc_qemu.py)

L'édition locale insère Ω au milieu d'un fichier de 4 Mio et produit 4 194 306 octets. L'édition mixte insère 4 Mio au milieu, ajoute Ω en tête et supprime un octet près de la fin, pour 8 388 609 octets. La fenêtre chronométrée côté hôte commence à l'envoi de `w copie` et finit au message de succès après confirmation et F2. Après extinction de chaque VM, le lecteur indépendant vérifie les deux ancres et compare intégralement la source et la destination aux octets attendus. Les invités sont les deux ELF instrumentés du profil polling, réutilisés avec leur identité SHA-256. [Opérations et relecture](../tests/es_groupees_elargies.py) · [Construction des ELF](es-v0.16.6-irq-qemu.md#reproduction)

| Politique | Portée et méthode |
|---|---|
| ordinaire | Aucun lancement avec un plafond et aucune demande du plugin. |
| clamp-utility | Lancement par `taskpolicy -c utility`, plafond appliqué au processus QEMU. |
| clamp-background | Lancement par `taskpolicy -c background`, plafond appliqué au processus QEMU. |
| thread-background | Seul le thread exécutant le noyau invité demande `QOS_CLASS_BACKGROUND` à l'entrée de l'ELF. |
| thread-user-initiated | Même point d'entrée et même thread, avec `QOS_CLASS_USER_INITIATED`. |

Le manuel macOS de `taskpolicy` documente les trois valeurs `utility`, `background`, `maintenance` pour `-c`. L'essai de `-c default` est refusé par le programme. Le contrôle ordinaire conserve donc le lancement naturel du moteur. Le plugin mesure dans chaque profil la classe demandée de pthread et la priorité courante et de base du thread telle que la rapporte Mach. Un plafond de processus peut abaisser la priorité alors que la classe demandée par le thread reste inchangée. Le premier callback d'exécution, installé sur l'adresse d'entrée ELF, s'exécute sur le thread TCG du seul vCPU. Le banc vérifie la même identité pthread à chaque marqueur et le résultat de chaque demande explicite. [Plugin TCG](../tests/qos_vcpu_tcg.c) · [Guide Apple des classes de service](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheTaskLevel.html)

Sur cet hôte, `ioreg -arl -p IODeviceTree -n cpus -d 2` associe les propriétés `logical-cpu-id` et `cluster-type` de chaque nœud CPU. Les cœurs 0 à 3 sont E, les cœurs 4 à 15 sont P. Le plugin appelle la fonction publique `pthread_cpu_number_np` du SDK macOS à l'ouverture et à la fermeture de la publication, aux entrées des phases instrumentées et de part et d'autre des barrières F1/F2. Ces marqueurs échantillonnent le cœur courant à des étapes du programme. Ils ne mesurent ni les migrations entre marqueurs ni le temps passé sur chaque type de cœur. Le CPU de publication est la différence de `CLOCK_THREAD_CPUTIME_ID` du même thread entre début et fin. Le temps écoulé entre ces bornes utilise `CLOCK_MONOTONIC`. La sonde inclut les relevés de ces marqueurs dans les durées. [Sonde et compteurs utilisés](../tests/qos_vcpu_tcg.c)

## Résultats

Les 120 tentatives ont été exécutées le 14 novembre 2026 sur un MacBook Pro M4 Max (Mac16,5) sous macOS 27.2, en séquence entre 02:53:37 et 03:27:26 UTC, après les régressions et avant la matrice UTM. Les 120 sauvegardes ont respecté le budget IPC et attendu F2. Le lecteur indépendant a vérifié le contenu de chaque image. La valeur la plus lente atteint **1 263,272 ms** (x86_64, 4096 octets, ajout-4m-multiple, thread-background, bloc 4). Toutes les mesures figurent dans le JSON, y compris les dépassements.

| Architecture | Édition | Politique | n | Commande p50/p95/p99 (ms) | Δ p50 apparié (ms) | CPU TCG p50 (ms) | Marqueurs E/P | ≥ 1 s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| aarch64 | mixte | ordinaire | 6 | 131,3 / 144,3 / 144,3 | 0,0 | 122,3 | 0 / 456 | 0 |
| aarch64 | mixte | clamp-utility | 6 | 138,7 / 148,5 / 148,5 | 6,7 | 122,2 | 0 / 456 | 0 |
| aarch64 | mixte | clamp-background | 6 | 711,9 / 817,2 / 817,2 | 581,5 | 668,6 | 456 / 0 | 0 |
| aarch64 | mixte | thread-background | 6 | 561,6 / 703,5 / 703,5 | 431,1 | 516,4 | 456 / 0 | 0 |
| aarch64 | mixte | thread-user-initiated | 6 | 132,3 / 145,3 / 145,3 | 0,5 | 122,1 | 0 / 456 | 0 |
| aarch64 | locale | ordinaire | 6 | 95,2 / 111,1 / 111,1 | 0,0 | 81,9 | 0 / 300 | 0 |
| aarch64 | locale | clamp-utility | 6 | 92,5 / 101,0 / 101,0 | -4,6 | 80,7 | 0 / 300 | 0 |
| aarch64 | locale | clamp-background | 6 | 416,0 / 563,1 / 563,1 | 313,1 | 368,9 | 300 / 0 | 0 |
| aarch64 | locale | thread-background | 6 | 482,6 / 564,7 / 564,7 | 385,3 | 414,2 | 300 / 0 | 0 |
| aarch64 | locale | thread-user-initiated | 6 | 96,5 / 112,8 / 112,8 | 2,1 | 81,0 | 0 / 300 | 0 |
| x86_64 | mixte | ordinaire | 6 | 203,7 / 210,0 / 210,0 | 0,0 | 194,5 | 0 / 456 | 0 |
| x86_64 | mixte | clamp-utility | 6 | 205,8 / 228,2 / 228,2 | -1,5 | 193,3 | 0 / 456 | 0 |
| x86_64 | mixte | clamp-background | 6 | 973,4 / 1237,5 / 1237,5 | 765,6 | 925,7 | 456 / 0 | 1 |
| x86_64 | mixte | thread-background | 6 | 957,9 / 1263,3 / 1263,3 | 748,0 | 895,3 | 456 / 0 | 3 |
| x86_64 | mixte | thread-user-initiated | 6 | 206,6 / 221,5 / 221,5 | -0,2 | 195,4 | 0 / 456 | 0 |
| x86_64 | locale | ordinaire | 6 | 145,2 / 151,5 / 151,5 | 0,0 | 132,1 | 0 / 300 | 0 |
| x86_64 | locale | clamp-utility | 6 | 141,7 / 143,4 / 143,4 | -3,0 | 131,5 | 0 / 300 | 0 |
| x86_64 | locale | clamp-background | 6 | 719,3 / 823,0 / 823,0 | 567,9 | 635,4 | 300 / 0 | 0 |
| x86_64 | locale | thread-background | 6 | 619,4 / 760,0 / 760,0 | 467,9 | 575,0 | 300 / 0 | 0 |
| x86_64 | locale | thread-user-initiated | 6 | 142,3 / 151,5 / 151,5 | -3,4 | 131,4 | 0 / 300 | 0 |

### Effet sur le vCPU

- `ordinaire` donne 1 512 marqueurs P sur 1 512, une classe pthread demandée 21 et une priorité de base Mach 31.
- `clamp-utility` donne 1 512 marqueurs P sur 1 512, une classe pthread demandée 21 et une priorité de base Mach 20.
- `clamp-background` donne 1 512 marqueurs E sur 1 512, une classe pthread demandée 21 et une priorité de base Mach 4.
- `thread-background` donne 1 512 marqueurs E sur 1 512, une classe pthread demandée 9 et une priorité de base Mach 4.
- `thread-user-initiated` donne 1 512 marqueurs P sur 1 512, une classe pthread demandée 25 et une priorité de base Mach 31.

Les 24 comparaisons appariées isolent la politique demandée par le thread TCG. Pour la même publication P9, le rapport médian apparié entre les temps CPU en arrière-plan et en classe `user-initiated` varie de 4,19 à 5,11 selon la charge. Les fréquences CPU et le temps passé sur chaque type de cœur restent inconnus.

| Architecture | Édition | Rapport CPU apparié background / user-initiated, p50/p95/p99 |
|---|---|---|
| AArch64 | locale | 5,11 / 6,20 / 6,20 |
| AArch64 | mixte | 4,19 / 5,44 / 5,44 |
| x86_64 | locale | 4,34 / 5,51 / 5,51 |
| x86_64 | mixte | 4,57 / 5,93 / 5,93 |

Le passage du témoin ordinaire en classe explicite `user-initiated` conserve la priorité de base 31 et les marqueurs P. Pour l’édition mixte, les différences médianes appariées de la commande sont de +0,5 ms sur AArch64 et de −0,2 ms sur x86_64.

### Dépassements et contrôles

4 commandes sur 120 atteignent ou dépassent une seconde. Les dépassements individuels sont triés par durée. Les temps CPU et mur ci-dessous portent sur la seule publication.

| Ordre | Arch. / secteur | Édition / politique | Commande (ms) | CPU / mur (ms) | FLUSH physiques F1/F2 (ms) |
|---|---|---|---:|---:|---:|
| 77 | x86_64 / 4096 | ajout-4m-multiple / thread-background | 1263,272 | 1214,3 / 1240,6 | 3,625 / 0,525 |
| 78 | x86_64 / 4096 | ajout-4m-multiple / clamp-background | 1237,452 | 1204,2 / 1218,9 | 0,258 / 0,026 |
| 109 | x86_64 / 4096 | ajout-4m-multiple / thread-background | 1221,075 | 1176,8 / 1195,2 | 3,172 / 0,849 |
| 12 | x86_64 / 512 | ajout-4m-multiple / thread-background | 1024,713 | 895,3 / 996,1 | 2,291 / 0,166 |

Les deux éditions commandent respectivement 169 et 190 soumissions VirtIO, dont deux FLUSH. Avec les contrôles conservateurs, l'édition locale demande 43 à 44 échanges IPC et la mixte 56 à 57. Les latences physiques maximales de F1 et F2 atteignent 3,625 et 1,175 ms. Le lecteur observe les générations 65/66 avant utilisation et 67/68 après la session. Ces relectures ont lieu sur le Mac alimenté. Elles vérifient l’acquittement des commandes FLUSH dans la chaîne émulée et le contenu persistant après fermeture du disque. La perte d'alimentation physique reste hors de ce banc.

Le pic global de l'invité, compté sur toute la session, va de 2 990 à 9 755 cadres de 4 Kio sur AArch64 et de 3 001 à 9 766 sur x86_64. Les maxima privés par processus, pile comprise, figurent dans chaque observation.

Le profil TCG direct établit l'effet de la politique demandée sur ces charges. Un plafond au lancement porte aussi sur les autres threads QEMU, notamment les E/S. Les deux demandes limitées au vCPU fournissent le contrôle complémentaire. Cette qualification porte sur RAW et le moteur QEMU 11.1.3. L'attribution d'un dépassement dans l'application UTM nécessite les observations de sa propre campagne.

## Reproduction

Sur le Mac, utiliser l'archive instrumentée du profil `poll-sonde` et le générateur Darwin de la qualification précédente. Dans un checkout neuf de la référence du mandat, les fixtures peuvent être appliquées directement à l'archive de cette référence dans l'ordre documenté pour `poll-sonde`. L'observateur `io-0166-irq-latences` ajoute la trace des opérations bloc, le pilote conserve l'attente par polling. Le jeu d'ELF existant provient de la base `4ae44124` de cet audit IRQ. Les ajouts entre cette base et `262a335` concernent les bancs et la documentation. [Ordre des fixtures et construction](es-v0.16.6-irq-qemu.md#reproduction)

Depuis la racine, `ARCHIVE` désigne le répertoire qui contient les deux composantes de l'archive et `GENERATEUR` l'exécutable Darwin `es-groupees`. Utiliser un répertoire de sortie neuf. La campagne reconstruit un média temporaire et lance un seul invité à la fois. Laisser les autres campagnes de VM à l'arrêt pendant les chronométrages.

```sh
QEMU_PREFIX="$(brew --prefix qemu)"
cc -O2 -Wall -Wextra -Wno-unused-parameter -Werror -fvisibility=hidden \
   -I"$QEMU_PREFIX/include" $(pkg-config --cflags glib-2.0) -bundle \
   -undefined dynamic_lookup novaOS/tests/qos_vcpu_tcg.c -o "$PLUGIN"
python3 -B novaOS/tests/qos_qemu.py "$ARCHIVE" "$PLUGIN" "$GENERATEUR" "$SORTIE" --repetitions 6
python3 -B novaOS/tests/qos_qemu_bilan.py "$SORTIE" "$SORTIE/bilan.json"
```

Les distributions utilisent le rang supérieur `ceil(p × n)`. Les p95 et p99 prennent donc la valeur maximale avec trois ou six observations. Les différences appariées soustraient le témoin ordinaire de même architecture, édition, géométrie et bloc. Le fichier JSON conserve chaque latence, chaque paire et l'ordre des mesures, ainsi que les relevés de cœur, de classe, de priorité et les horloges relatives au début de publication. [Distributions et mesures individuelles](validation/es-v0.16.6-qos-qemu.json) · [Calcul du bilan](../tests/qos_qemu_bilan.py)
