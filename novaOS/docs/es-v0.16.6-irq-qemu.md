# NovaOS v0.16.6 — Attente VirtIO-blk par polling et par IRQ sous QEMU direct

## Protocole de comparaison

Les quatre archives proviennent du commit `4ae44124b42ad9cf971ee8c134cb2e4145aa17c5`. Les deux pilotes soumettent les mêmes chaînes VirtIO multipages à une seule file. Le témoin polling scrute la complétion et le compteur d’IRQ, avec une limite de temps et d’itérations. Le candidat IRQ vérifie la complétion sous masque local et, si nécessaire, bloque le propriétaire de la requête sur un événement. Les deux contrôlent l’entrée utilisée, la longueur, le statut du périphérique et l’IRQ attendue avant de rendre la zone DMA. [Fixture IRQ](../tests/fixtures/io-0166-irq-attente.patch) · [Pilote multipage de référence](../tests/fixtures/io-0166-virtio-multipage.patch)

Les invités sont les profils de développement de QEMU 11.1.3, utilisant TCG, un vCPU, 256 Mio et des disques jetables RAW en writeback. Le profil AArch64 est `virt`, Cortex-A72, GICv2 et VirtIO-IOMMU. Le profil x86_64 est `q35`, CPU `max` et VT-d. Le test local insère Ω au milieu d’une source dense de 4 Mio, pour une destination de 4 194 306 octets. Le test combiné insère 4 Mio au milieu, ajoute Ω en tête et retire un octet près de la fin, pour une destination de 8 388 609 octets. Chaque essai ouvre un disque neuf, publie vers le nom libre `copie`, contrôle les deux générations et compare tous les octets des deux fichiers. [Charges et lecteur de contrôle](../tests/es_groupees_elargies.py) · [Configuration QEMU](../scripts/bloc_qemu.py)

Sur chaque architecture et taille de secteur, les mesures ordinaires comparent huit paires pour la grande insertion et trois pour l’édition locale. Des ELF instrumentés comparent trois paires par cellule. Le lancement est séquentiel et l’ordre IRQ/polling est inversé entre répétitions. La fenêtre utilisateur commence à l’envoi de `w copie` et se termine au message de succès après confirmation et F2. Le débit logique est la taille complète du fichier publié divisée par cette durée. Il inclut le travail de validation et le protocole avec `nsh`. [Ordre des essais et vérification des identités](../tests/irq_qemu.py)

La sonde TCG légère mesure séparément le CPU consommé par le thread du vCPU et le temps monotone écoulé entre les marqueurs invités de publication. Ses durées couvrent le code invité, la traduction et les tâches que QEMU exécute sur ce thread. Le temps CPU dans les autres threads QEMU, les I/O du Mac et les temps écoulés dans le terminal ont leurs propres périmètres. Le pilote mesure chaque requête de sa préparation à sa complétion vérifiée. La trace donne le type et la durée en ticks de chaque commande. Les classes de requêtes sont pondérées par leur nombre, les latences de sauvegarde par observation. Les percentiles empiriques utilisent le rang supérieur `ceil(p × n)`. [Sonde TCG](../tests/vcpu_phases_tcg.py) · [Trace individuelle du pilote](../tests/fixtures/io-0166-irq-latences.patch) · [Étalonnage du vCPU](es-v0.16.6-vcpu-qemu.md)

Deux témoins de préparation ont contrôlé indépendamment la trace individuelle du pilote par lecture TCG des compteurs cumulés de `metrics::record`, au début de l’opération suivante et à la fin de la publication. L’ordre, le type et la durée en ticks des 190 requêtes de la grande insertion en 4 096 sur AArch64, puis des 169 requêtes de l’édition locale en 512 sur x86_64, correspondent exactement. La campagne appariée utilise ensuite le plugin léger et la trace de laboratoire intégrée.

## Résultats

La campagne du 13 novembre 2026 s’est déroulée de 22 h 26 min 56 s à 22 h 49 min 00 s CET, après les régressions et avant la campagne UTM. Les **136 sauvegardes sur 136** ont publié le contenu attendu. Le lecteur indépendant a vérifié les deux ancres et retrouvé les générations 67 et 68 après chaque publication, pour 65 et 66 sur les disques initiaux.

| Architecture | Secteur | Édition | Paires | Polling p50/p95/p99 (ms) | IRQ p50/p95/p99 (ms) | Médiane Δ IRQ−polling apparié (ms) | IRQ plus rapide | Débit médian IRQ (Mio/s) |
|---|---|---|---|---|---|---|---|---|
| AArch64 | 512 | combinée | 8 | 132,7 / 134,9 / 134,9 | 133,2 / 135,2 / 135,2 | 0,5 | 3/8 | 59,9 |
| AArch64 | 512 | locale | 3 | 98,4 / 98,4 / 98,4 | 92,1 / 98,5 / 98,5 | -5,8 | 2/3 | 43,4 |
| AArch64 | 4096 | combinée | 8 | 129,6 / 135,5 / 135,5 | 131,5 / 134,3 / 134,3 | 0,6 | 2/8 | 60,4 |
| AArch64 | 4096 | locale | 3 | 92,3 / 96,8 / 96,8 | 97,1 / 99,8 / 99,8 | 2,8 | 0/3 | 41,2 |
| x86_64 | 512 | combinée | 8 | 209,4 / 217,2 / 217,2 | 213,2 / 218,4 / 218,4 | 1,1 | 3/8 | 37,3 |
| x86_64 | 512 | locale | 3 | 137,1 / 142,3 / 142,3 | 140,0 / 140,9 / 140,9 | 2,0 | 1/3 | 28,6 |
| x86_64 | 4096 | combinée | 8 | 214,4 / 217,2 / 217,2 | 214,7 / 218,9 / 218,9 | 0,0 | 3/8 | 37,2 |
| x86_64 | 4096 | locale | 3 | 141,0 / 144,1 / 144,1 | 140,1 / 141,4 / 141,4 | -1,0 | 3/3 | 28,6 |

Les 88 sauvegardes hors sonde ont toutes pris moins d’une seconde. Le maximum du polling est de 217,216 ms sur x86_64 en secteurs de 512, à la deuxième grande insertion. Celui du candidat IRQ est de 218,874 ms sur x86_64 en secteurs de 4 096, à la première grande insertion. Le maximum des 48 essais instrumentés est de 218,947 ms. Dans les cellules de trois ou huit observations, p95 et p99 reprennent le maximum. [Latences individuelles, deltas appariés et distributions](validation/es-v0.16.6-irq-qemu.json)

Pour la grande insertion, le candidat IRQ finit plus tôt dans 11 des 32 paires hors sonde. Les quatre écarts médians appariés vont de +0,030 à +1,068 ms. Le parcage n’a donc pas accéléré systématiquement la commande en RAW. Le débit médian des données multipages écrites sur l’intervalle de publication instrumenté atteint 61,45–62,63 Mio/s sur AArch64 et 38,90–40,12 Mio/s sur x86_64 avec IRQ, contre 60,57–62,46 et 39,43–39,94 Mio/s en polling.

| Architecture | Secteur | Édition | CPU de publication polling / IRQ (ms, p50) | Médiane Δ CPU apparié (ms) | kick → used polling / IRQ (ms, p50) | Immédiates / parcages / réveils IRQ, cumulés | IPC max polling / IRQ |
|---|---|---|---|---|---|---|---|
| AArch64 | 512 | combinée | 130,8 / 125,2 | -3,2 | 9,8 / 10,0 | 7 / 563 / 563 | 56 / 56 |
| AArch64 | 512 | locale | 85,9 / 85,1 | -0,8 | 8,8 / 8,0 | 7 / 500 / 500 | 43 / 43 |
| AArch64 | 4096 | combinée | 126,7 / 123,3 | -3,8 | 9,8 / 9,8 | 6 / 564 / 564 | 56 / 56 |
| AArch64 | 4096 | locale | 86,4 / 85,0 | -1,4 | 8,5 / 7,8 | 6 / 501 / 501 | 43 / 43 |
| x86_64 | 512 | combinée | 200,7 / 196,8 | -5,0 | 7,4 / 9,2 | 7 / 563 / 563 | 57 / 57 |
| x86_64 | 512 | locale | 136,3 / 133,3 | 1,3 | 6,6 / 8,0 | 8 / 499 / 499 | 43 / 43 |
| x86_64 | 4096 | combinée | 203,3 / 204,3 | 1,0 | 7,7 / 8,7 | 5 / 565 / 565 | 57 / 56 |
| x86_64 | 4096 | locale | 136,8 / 133,1 | -0,6 | 6,5 / 8,2 | 8 / 499 / 499 | 43 / 43 |

Les 24 sauvegardes IRQ instrumentées ont enregistré 54 complétions immédiates et 4 254 parcages, tous suivis d’un réveil par interruption. Les compteurs de réveils par délai, de replis, d’expirations et d’interruptions parasites sont restés à zéro. Le compteur d’IRQ physiques inclut également les complétions constatées avant le parcage. Les deux modes ont consommé 43 à 57 échanges IPC et exactement deux FLUSH par sauvegarde instrumentée.

| Architecture | Mode | Pic global de cadres de 4 Kio sur la session, min–max | Service NovaFS (Kio) | Pilote utilisateur (Kio) | nsh (Kio) | vi (Kio) |
|---|---|---|---|---|---|---|
| AArch64 | poll | 2989–9754 | 3800 | 36 | 460 | 33608 |
| AArch64 | irq | 2989–9754 | 3800 | 36 | 460 | 33608 |
| x86_64 | poll | 3000–9765 | 3828 | 36 | 476 | 33608 |
| x86_64 | irq | 3000–9765 | 3828 | 36 | 476 | 33608 |

Les maxima par processus sont les pages privées plus les pages de pile comptabilisées par le noyau au cours des observations. Le pic global est compté sur toute la session de l’invité.

Sur les ELF ordinaires de développement, `llvm-size` mesure 2 442 175 puis 2 444 495 octets de texte sur AArch64 (+2 320) et 2 601 064 puis 2 602 200 sur x86_64 (+1 136). La BSS augmente respectivement de 28 et de 44 octets. La taille des données initialisées est identique entre modes, 12 712 et 17 240 octets selon l’architecture.

| Architecture | Secteur | Édition | Mode | Lecture p50/p95/p99/max (µs) | Écriture p50/p95/p99/max (µs) | FLUSH p50/p95/p99/max (µs) |
|---|---|---|---|---|---|---|
| AArch64 | 512 | combinée | poll | 41,0 / 70,0 / 137,0 / 156,0 | 63,0 / 113,0 / 147,0 / 226,0 | 305,0 / 1475,0 / 1475,0 / 1475,0 |
| AArch64 | 512 | combinée | irq | 42,0 / 62,0 / 129,0 / 463,0 | 66,0 / 90,0 / 159,0 / 176,0 | 257,0 / 1544,0 / 1544,0 / 1544,0 |
| AArch64 | 512 | locale | poll | 39,0 / 72,0 / 113,0 / 205,0 | 63,0 / 103,0 / 944,0 / 944,0 | 717,0 / 1030,0 / 1030,0 / 1030,0 |
| AArch64 | 512 | locale | irq | 42,0 / 59,0 / 132,0 / 207,0 | 67,0 / 91,0 / 106,0 / 106,0 | 722,0 / 1088,0 / 1088,0 / 1088,0 |
| AArch64 | 4096 | combinée | poll | 40,0 / 71,0 / 121,0 / 166,0 | 61,0 / 98,0 / 170,0 / 177,0 | 225,0 / 1487,0 / 1487,0 / 1487,0 |
| AArch64 | 4096 | combinée | irq | 43,0 / 61,0 / 128,0 / 194,0 | 66,0 / 92,0 / 159,0 / 196,0 | 297,0 / 1565,0 / 1565,0 / 1565,0 |
| AArch64 | 4096 | locale | poll | 39,0 / 64,0 / 136,0 / 202,0 | 63,0 / 101,0 / 111,0 / 111,0 | 740,0 / 1240,0 / 1240,0 / 1240,0 |
| AArch64 | 4096 | locale | irq | 41,0 / 60,0 / 116,0 / 194,0 | 64,0 / 87,0 / 98,0 / 98,0 | 713,0 / 1340,0 / 1340,0 / 1340,0 |
| x86_64 | 512 | combinée | poll | 29,0 / 45,0 / 214,3 / 310,8 | 50,8 / 100,9 / 154,5 / 205,8 | 202,8 / 1496,6 / 1496,6 / 1496,6 |
| x86_64 | 512 | combinée | irq | 38,9 / 50,9 / 220,0 / 526,8 | 58,7 / 91,6 / 192,2 / 281,8 | 51,9 / 1438,9 / 1438,9 / 1438,9 |
| x86_64 | 512 | locale | poll | 30,9 / 50,8 / 95,2 / 220,5 | 49,1 / 166,4 / 344,9 / 344,9 | 435,1 / 1420,0 / 1420,0 / 1420,0 |
| x86_64 | 512 | locale | irq | 39,0 / 56,0 / 161,9 / 519,6 | 59,0 / 93,0 / 304,9 / 304,9 | 54,8 / 1512,8 / 1512,8 / 1512,8 |
| x86_64 | 4096 | combinée | poll | 29,0 / 50,7 / 243,7 / 309,5 | 51,7 / 105,5 / 128,1 / 135,3 | 246,2 / 1566,4 / 1566,4 / 1566,4 |
| x86_64 | 4096 | combinée | irq | 36,9 / 54,0 / 173,0 / 641,8 | 53,2 / 90,0 / 151,5 / 156,7 | 223,9 / 1511,4 / 1511,4 / 1511,4 |
| x86_64 | 4096 | locale | poll | 28,9 / 51,2 / 113,4 / 653,3 | 53,2 / 93,6 / 448,9 / 448,9 | 710,7 / 1147,6 / 1147,6 / 1147,6 |
| x86_64 | 4096 | locale | irq | 38,8 / 51,8 / 113,2 / 325,7 | 58,1 / 81,7 / 326,4 / 326,4 | 91,6 / 1720,5 / 1720,5 / 1720,5 |

## Reproduction

Créer quatre répertoires `poll-normal`, `poll-sonde`, `irq-normal`, `irq-sonde` et y extraire séparément `git archive 4ae44124b42ad9cf971ee8c134cb2e4145aa17c5 AGENTS.md novaOS novaFS components`. Pour chaque archive, appliquer les fixtures du laboratoire avec `patch -p1 -F0` dans cet ordre.

1. `io-0166-prototype`, `io-0166-plages`.
2. Pour les deux ELF sondés, `io-0166-prototype-sonde`, `io-0166-plages-sonde`.
3. `io-0166-charge-4m`, `io-0166-virtio-multipage`, `io-0166-invite`.
4. Pour les deux ELF sondés, `io-0166-dma-sonde`, `io-0166-vcpu`, `io-0166-irq-latences`.
5. Pour les deux candidats IRQ, `io-0166-irq-attente`. Pour le candidat IRQ instrumenté, ajouter `io-0166-irq-sonde`.

Copier le lecteur courant `novaOS/tests/vi_ipc_sauvegarde.py` dans les deux archives sondées pour valider les nouveaux compteurs. Compiler dans chaque archive avec `make -C novaOS ARCH=aarch64 kernel` et `make -C novaOS ARCH=x86_64 kernel`. Installer dans chaque archive le lecteur indépendant en `novaFS/target/aarch64-apple-darwin/release/novafs` et créer le répertoire `novaOS/build` pour les médias de démarrage et les traces. Utiliser le générateur de profils `es-groupees` de NovaFS sous Darwin. Construire le plugin selon la procédure de qualification TCG et lancer le banc depuis la racine du dépôt, dans un nouveau répertoire. [Construction des dépendances](es-v0.16.6-dma.md#reproduction) · [Compilation du plugin](es-v0.16.6-vcpu-qemu.md#reproduction)

```sh
python3 -B novaOS/tests/irq_qemu.py "$ARCHIVES" "$PLUGIN" "$GENERATEUR" "$SORTIE" --repetitions 8
```
