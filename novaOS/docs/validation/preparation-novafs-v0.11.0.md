# Qualification de l’étape mémoire et portabilité

Mac Apple Silicon, macOS 27.2, Rust stable 1.99.1, QEMU 11.1.3. Les tests du noyau exécutent des invités UEFI x86_64 et AArch64. Les campagnes de fichiers exécutent deux binaires Mach-O du moteur sur macOS, ARM64 natif et x86_64 sous Rosetta. Les secteurs testés sont de 512 et 4 096 octets.

## Preuves en espace utilisateur

| Contrôle | Observation exigée par le banc sur chaque invité |
|---|---|
| ELF64 | Exécution de l’entrée RX. Validation de `.data` initialisée, lecture de zéros dans une BSS de 8 193 octets, écriture et relecture à son extrémité. En-têtes tronqués, débordements, adresses noyau ou non canoniques, entrée invalide, alignements et chevauchements contrôlés par le chargeur. Les mutations testées se trouvent dans `check_elf_headers`. |
| Droits matériels | Deux écritures, sur RX et R, produisent une faute de protection en écriture. Un branchement vers les données RW/NX donne une faute de protection en exécution. Une lecture hors segment donne une faute d’absence. |
| Pile gardée | Une récursion normale franchit l’ancienne limite de 4 Kio. Une récursion plus profonde atteint la garde inférieure et produit une faute d’écriture dans sa plage exacte. L’accès à la garde supérieure donne une faute à l’adresse attendue. |
| Tas | Extension de deux pages zéro, retrait, refus d’une borne désalignée, d’un second retrait avec borne périmée et d’une extension supérieure aux cadres disponibles. Refus de la réservation utilisateur de 1 Tio avec borne intacte, puis allocation et manipulation de 128 Kio par `Vec`, allocations et libérations individuelles par `Box`. Une lecture après démappage donne une faute d’absence à la base du tas. |
| Pénurie réelle | Le banc noyau retient les cadres libres, provoque un échec après admission partielle de l’ELF puis pendant une extension du tas. Il vérifie la borne, les cadres libres avant/après et le retour à l’état initial après collecte. |
| Isolation et fin anormale | Deux instances écrivent des valeurs différentes à la même adresse virtuelle, cèdent le CPU puis relisent leur propre valeur. Une instance meurt dans le fournisseur de pages pendant une allocation. Le banc attend l’issue de chaque processus, vérifie type et adresse des fautes, puis compare tout l’audit mémoire au repos. |
| Service et support | Le processus du profil bloc encode et décode un nœud avec le codec NovaFS. La source d’entropie absente refuse la génération. L’adaptateur IPC écrit un bloc complet de 4 Kio et attend FLUSH. L’hôte tue la VM, compare toute l’image brute de 1 Mio, puis une nouvelle VM relit et compare les 4 Kio. Répété sur les deux géométries. |

Les deux ELF liés mesurent chacun 34 200 octets. Sur x86_64 les tailles logiques RX, R et RW sont respectivement 21 349, 1 048 et 9 121 octets. Sur AArch64 elles sont 21 060, 988 et 9 121 octets. La partie initialisée RW occupe 928 octets sur chaque cible. La pile provisionnée ajoute 64 pages physiques au service. Les gardes ne consomment pas de cadre de données.

## Couverture macOS

La suite standard et la suite instrumentée incluent les essais de corruption, reprise, réservations, snapshots, clones, compression, clés, exports et scrub. Quatre tests sont marqués ignorés dans le passage standard. Le harnais relance le modèle millionnaire, les deux options de fixture et le banc séparé d’interopérabilité LZ4.

- Pour chaque architecture, le modèle différentiel utilise 100 graines, 1 000 000 d’opérations, 345 183 commits observés, 54 643 pénuries injectées et 1 000 remontages avec audit.
- Dans chaque géométrie et sur chaque architecture, le corpus P5 contrôle 342 coupures, 76 erreurs d’E/S et 6 écritures déchirées. Le corpus P6 contrôle 117 coupures et 20 erreurs d’écriture sur 39 frontières, en plus des séquences aléatoires. Les boucles de restitution P5 et P6 retrouvent le nombre d’octets initial après 32 cycles avec ou sans échec de F2.
- Le banc de reprise historique a comparé 24 coupures sur fichier et 10 sur le backend QEMU par architecture, avec deux remontages par point. Il s’agit du banc NovaFS piloté par macOS. Le banc natif NovaOS porte sur le transport du bloc brut.
- Les anciens producteurs d’images, les pools partagés et les exports ont été relus lors de la campagne `p6-historiques --support formats`. L’interopérabilité LZ4 a vérifié 256 blocs dans chaque sens par architecture. La campagne P7 a exercé les quatre couples producteur/secteur avec lecture sur l’autre architecture, deux restaurations par couple et refus de mutation par l’ancien écrivain. La CLI P8 a vérifié les rapports de scrub, la corruption et les réparations autorisées dans les quatre couples.
- La bibliothèque complète et ses fonctionnalités métier compilent en version optimisée sur `x86_64-unknown-none` et `aarch64-unknown-none-softfloat`. L’arbre des dépendances normales de ces cibles exclut `getrandom`, `tempfile`, `serde_json` et `libc`. Le code de sérialisation de base, les enveloppes cryptographiques, le protocole bloc et les textes MIT et MPL-2.0 sont identiques à la base Git. Les ISO incluent les notices du service et de ses dépendances.

[Commandes de reproduction NovaFS](../../../novaFS/docs/novafs-coeur-portable.md) · [Commandes NovaOS et contrats](../novafs-integration-v0.11.0.md)

## Résultats des commandes

| Cible | QEMU historiques | Groupes bloc (deux secteurs) | Perf. bloc | IPC courses / perf. | P8 standard | P8 instrumenté | Million différentiel (s) |
|---|---|---|---|---|---|---|---|
| x86_64 | 25 | 24 | 2/2 | réussis | 178 | 179 | 44,382 |
| aarch64 | 25 | 24 | 2/2 | réussis | 178 | 179 | 25,884 |

Chaque géométrie valide les dix groupes bloc historiques, puis le transfert persistant de 4 Kio et le refus du support en lecture seule propagé jusqu’au client. Chaque profil de performance vérifie 128 transferts. La suite historique inclut les sondes de privilèges, les appels ABI, l’IPC, la préemption et l’endurance courte. Le test ELF supplémentaire a réussi sur chaque architecture. La qualification graphique compare les captures attendues et la saisie AZERTY sur x86_64.

## Chargement, allocation et restitution

| Opération, µs | x86_64 médiane (min–max) | AArch64 médiane (min–max) |
|---|---|---|
| Chargement ELF | 205,562 (202,743–416,686) | 369,008 (338,992–476,000) |
| Création complète du processus | 406,921 (357,546–706,861) | 1123,008 (1080,000–6714,000) |
| Collecte de l’espace ELF | 165,968 (142,819–228,926) | 3095,008 (3010,000–3874,000) |
| Passage entre deux marqueurs, à vide | 29,936 (25,967–41,167) | 36,000 (32,992–45,008) |
| Extension du tas de deux pages | 338,279 (291,630–399,617) | 34,992 (34,000–49,008) |
| Retrait des deux pages | 271,097 (220,957–284,876) | 136,000 (132,000–166,992) |
| Allocation de Box, moyenne du lot de 512 | 0,113 (0,098–0,127) | 0,092 (0,092–0,115) |
| Libération de Box, moyenne du lot de 512 | 0,486 (0,471–0,567) | 0,397 (0,332–1,027) |
| Paire allocation/libération, lot de 4 096 | 0,110 (0,101–0,140) | 0,101 (0,097–0,115) |

Cinq démarrages indépendants par architecture, compteur invité converti à sa fréquence mesurée. Les délais incluent les interruptions et la charge de l’hôte. Les lots d’objets donnent un coût moyen par objet, marqueurs inclus. Le vecteur des 512 pointeurs est réservé avant la mesure. Le coût à vide est présenté séparément. Sur ARM, son ordre de grandeur limite la résolution de la mesure d’extension. Le chronométrage de la création se termine au retour dans le lanceur et peut inclure une préemption. Une seule paire de pages par démarrage ne permet pas de conclure sur une distribution de queue. La référence v0.10.0 utilisait des images plates et ne fournissait pas ces chronométrages.

**x86_64** — Les cinq sondes restituent les 63 596 cadres libres initiaux. Chaque campagne effectue 1 294 attributions avec un pic supplémentaire de 208 cadres (832 Kio). Au relevé avec deux processus, leur mémoire utilisateur (piles incluses) occupe 600 à 616 Kio. Le résident total invité à ce point est de 2160 à 2200 Kio.

**AArch64** — Les cinq sondes restituent les 63 019 cadres libres initiaux. Chaque campagne effectue 1 294 attributions avec un pic supplémentaire de 208 cadres (832 Kio). Au relevé avec deux processus, leur mémoire utilisateur (piles incluses) occupe 600 à 616 Kio. Le résident total invité à ce point est de 2072 à 2112 Kio.

Les gardes et les cadres de données effectivement rendus sont contrôlés après chaque faute. Les campagnes atteignent le même pic de cadres tandis que le point de mesure des deux processus peut tomber avant ou après leur extension du tas.

## Comparaison du transport avec la v0.10.0

| Invité / secteur | Construction | Résident repos/publié/collecté (Kio) | Pic résident (Kio) | Cadres alloués/retenus | Buddy en fin (octets) | Commutation p50/p99 (µs) | Commutations |
|---|---|---|---|---|---|---|---|
| x86_64 / 512 | v0.10.0 de référence | 1284/1824/1332 | 1824 | 135/12 | 9280 | 3,08/4,00 | 2057 |
| x86_64 / 512 | étape préparatoire | 1368/2284/1416 | 2284 | 229/12 | 9280 | 3,06/4,08 | 2056 |
| x86_64 / 4096 | v0.10.0 de référence | 1284/1824/1332 | 1824 | 135/12 | 9280 | 3,06/4,08 | 11555 |
| x86_64 / 4096 | étape préparatoire | 1368/2208/1416 | 2284 | 229/12 | 9280 | 3,08/4,10 | 11563 |
| aarch64 / 512 | v0.10.0 de référence | 1200/1740/1212 | 1740 | 135/3 | 10304 | 5,12/6,14 | 2055 |
| aarch64 / 512 | étape préparatoire | 1280/2196/1292 | 2196 | 229/3 | 10304 | 5,12/8,19 | 2056 |
| aarch64 / 4096 | v0.10.0 de référence | 1200/1740/1212 | 1740 | 135/3 | 10304 | 4,10/6,14 | 11527 |
| aarch64 / 4096 | étape préparatoire | 1280/2196/1292 | 2196 | 229/3 | 10304 | 3,07/6,14 | 11565 |

Le résident est issu de la comptabilité du noyau, y compris tables, piles, tas, IPC et cadres DMA. La RAM vidéo de 469 pages sur AArch64 est comptée à part dans le champ série `retenu`. L’augmentation inclut la pile physique du service, passée de 1 à 64 pages, son ELF, son allocateur et leurs tables. La rétention au premier cycle du transport est également présente dans la référence. Les sondes ELF hors DMA contrôlent séparément la restitution intégrale. Ces observations concernent les scénarios mesurés. Le RSS du processus QEMU n’est pas une mesure de la RAM du noyau.

Pour répéter les cinq mesures, depuis la racine, exécuter :

```sh
for a in x86_64 aarch64; do
    for i in 1 2 3 4 5; do
        make -C novaOS ARCH="$a" elf-test || exit 1
        cp "novaOS/build/serie-$a-elf-test.log" "novaOS/build/serie-$a-elf-mesure-$i.log"
    done
done
```

Le montage d’images de fichiers dans NovaOS, les mutations transactionnelles natives, le bail d’écriture entre services, le provisionnement des clés et l’entropie native restent des prérequis pour livrer la v0.11.0. Les coupures de VM qualifient le modèle de disque virtuel utilisé.
