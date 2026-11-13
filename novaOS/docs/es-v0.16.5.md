# NovaOS v0.16.5 — E/S de fichiers, montage P9 et copie par plage

Le profil livré reste un service fichier sur un volume autonome, accessible par capacités. Il n'introduit pas le VFS générique proposé dans la feuille de route. Point de comparaison : [v0.16.4](vi-v0.16.4.md), commit `202313a7b27c052a94954c5de89a170d16dfdffd`. Le format P9, son numéro v0.9.13, le protocole de publication, les quotas et le code du noyau sont inchangés. PC1 est désactivé.

## Diagnostic et montage

Les 88 octets sont la charge utile d’une trame fichier de 128 octets. Avant la lecture groupée de v0.16.4, chaque fragment provoquait un nouvel accès à l’objet, au moteur et à sa politique de date d’accès : 1 491 appels au moteur sur le témoin de 64 Kio. Les [sondes et leur contre-épreuve](vi-v0.16.4-lecture-groupee.md#diagnostic) distinguent ce défaut du rendu. v0.16.5 conserve la première fenêtre de 4 Kio de vi ; la lecture n’anticipe pas au-delà de ses droits. Une trame fichier de 88 octets et une pièce du protocole **bloc** de 96 octets sont deux coûts différents.

Sur l’image P9 dense de 16 Mio de données, le montage hôte v0.16.4 demande 34 157 pages de 4 Kio. Deux contrôles contribuent à ce coût : la validation des données de **chaque** génération, et la vérification dans les deux sens que chaque page partagée conserve sa référence et son propriétaire logique. Pour ce deuxième contrôle l’ancienne boucle redescendait l’arbre adverse pour chacun des blocs du même extent.

- [Vérification des extents](../../novaFS/src/fs/check.rs) : garder uniquement le dernier intervalle adverse couvert pendant le parcours d’un extent, pour le même objet et les mêmes vues immuables. Chaque référence (adresse, naissance, empreinte) reste comparée individuellement. Le franchissement de l’intervalle impose une nouvelle recherche.
- [Inspection des générations](../../novaFS/src/engine.rs) : deux caches de métadonnées, un par vue, chacun plafonné à 32 Kio de pages et d’index. Ils utilisent le cache CLOCK existant. Clé : famille et référence complète ; décodage et bornes vérifiés même sur succès du cache. Une page non admise faute de mémoire suit le chemin ordinaire. Les caches disparaissent à la fin de l’inspection ; ils ne dispensent d’aucune lecture de payload dans les deux générations. Aucun index proportionnel à la taille du fichier n’a été ajouté. Comme le parcours de contrôle lui-même, ce cache suppose que personne ne réécrit le support sous la vue montée ; ce n’est pas un mécanisme de détection des écritures physiques étrangères.
- Le transport existant exige 45 échanges du service vers le bloc pour une page sur secteur de 4 Kio (43 pièces et deux phases) et 64 pour huit secteurs de 512 (6 pièces et deux phases par secteur). Le relais vers le pilote reste nécessaire. Le contrôle complet d’un gros volume ne tient donc pas nécessairement dans l’attente d’une unique réponse à MOUNT. Augmenter la constante du client ne réduirait pas ce travail.

**[Contrat de progression](../abi/files.rs)** : dans la trame fichier v2, MOUNT avec flag 0 conserve la seule réponse finale. Le nouveau client propose le flag 1. Les autres champs restent nuls. Pendant le montage initial, après chaque **256 lectures de pages entières réussies**, le service peut retourner sur le même endpoint et le même tag une trame MOUNT, flag 1, compteur little-endian à +32 ; tout le reste est nul. La première valeur est 256, la suivante 512, etc. Le client valide le type, la longueur, le tag, l’opcode et la trame entière ; il ne renvoie jamais la demande. Une progression authentique par le canal ouvre une nouvelle attente bornée à 15 000 ticks. Une réponse finale réussie (flag 0) avec session non nulle est seule habilitée à autoriser les opérations. Une absence de réponse, une révocation ou une progression invalide ne donne aucune session. Le compteur exprime du **travail effectué**, pas un pourcentage ni un engagement sur la durée totale. Les anciens clients peuvent encore utiliser flag 0 ; aucun repli automatique vers un service ancien.

### Montage en lecture seule sur Darwin ARM64

Pages effectivement demandées au backend de fichier ordinaire, puis temps muraux d’un montage complet. Même image pour les trois exécutables. Les durées incluent les appels de mesure ; caches macOS non purgés.

| Fichier dense | v0.16.4 | Réemploi de l’extent seul | Avec cache de métadonnées | Temps v0.16.4 → v0.16.5 |
|---|---:|---:|---:|---:|
| 16 Mio | 34 157 | 12 737 | 8 793 | 340 → 117 ms |
| 100 Mio | 213 961 | 79 687 | 55 007 | 2,112 → 0,704 s |
| 1 Gio | 2 716 105 | 881 865 | 563 079 | 25,648 → 7,776 s |

Sur 1 Gio, le temps brut dans le backend passe de 4,214 à 0,367 s ; le reste inclut les comparaisons, SHA, décodages, allocations, parcours et instrumentation. Il ne s’agit pas d’une mesure isolée de SHA. Le cache diminue les lectures mais, sur ce Mac avec ces fichiers en cache, son coût CPU augmente légèrement le temps par rapport à l’extent seul (6,934 s à 1 Gio). L’adaptateur invité ajoute une lecture de l’ancre 0 pour identifier le volume. La géométrie autonome plafonne à 2 097 152 blocs de 4 Kio (8 Gio physiques) ; le fichier dense de 1 Gio du banc tient dans une image de 2 Gio. Une taille maximale de fichier logique n’est pas une garantie de temps de montage.

## Sauvegarde et garanties

La [copie de plage dans le service](../user/novafs/files/selected.rs) évite le trajet du suffixe non modifié service → shell → service. Les premiers octets édités empruntent le chemin WRITE/SYNC existant. Un lot du suffixe, de 1 à 65 536 octets, demande une lecture du moteur et une seule préparation/validation CoW. La copie garde F1, l’ancre alternée et F2 pour chaque transaction. Le service vérifie la révision attendue avant lecture, puis transmet la révision résultant de l’horodatage. Pour une copie de texte, il valide UTF-8 et les caractères de contrôle ; un caractère ou CRLF coupé en fin de lot est relu. Une séquence invalide au dernier lot interdit la publication, même si une partie de la candidate a été synchronisée.

Trame `COPY_RANGE=27` : session à +8, handle destination à +16, offset source à +24, offset destination à +32, révision attendue à +40, longueur à +48. Flag 0 = octets, 1 = texte d’éditeur. La réponse donne la longueur effectivement copiée à +24 et la révision source à +32. Octets réservés, bornes et additions sont contrôlés. Le mode source/destination est exclusivement celui du groupe déjà autorisé (destination SELECT_NEW, fichier source SELECT_FILE au slot 2). Le destinataire doit être le handle inscriptible de la **candidate identifiée** ; l’offset doit être sa taille actuelle. Aucun accès par adresse, aucun droit global ou partage de tampon entre processus. En cas d’incertitude de transport, la demande d’écriture n’est pas rejouée.

**Comparaison, copie et publication ne sont pas une opération atomique de l’ABI.** Le courtier vérifie le nom puis l’identité, la taille et la révision de l’objet sélectionné ; le service sérialise les opérations du seul groupe de premier plan sur le seul écrivain. PUBLISH_COPY vérifie l’identité de la candidate puis appelle le renommage P9 qui refuse un nom occupé. La garantie dépend de cette exclusivité ; elle ne s’étend pas à un écrivain étranger qui modifierait physiquement le disque, à une image ramenée à une génération plus ancienne ou à un ancien lecteur partageant le support en écriture. Aucun écrasement ni remplacement transactionnel de la destination.

## Mesures en VM

QEMU direct 11.1.3, TCG, un vCPU, 256 Mio, terminal série 80 × 24 ; copie brute jetable par VM, cache de l’hôte non purgé. Aucun délai de MOUNT n’a été augmenté. Le temps d’ouverture va de la commande au premier écran éditable **après** le montage. Le premier G balaie le texte dense (lignes de 128 octets). La sauvegarde va de la demande de nouveau nom à sa publication, avec réponse OUI immédiate ; la relecture indépendante est hors chronométrage. Durées murales observées sur un Mac utilisé aussi pour les régressions ; une série et une répétition ne définissent pas un percentile de service.

| Architecture | Taille | Premier écran v0.16.4 → v0.16.5 (ms) | Premier G v0.16.5 (s) | Sauvegarde v0.16.4 → v0.16.5 (s) |
|---|---|---|---|---|
| aarch64 | 4 Mio | 122,9 → 313,3 | 20,745 | 65,736 → 40,250 |
| aarch64 | 1 Mio | 99,0 → 94,4 | 4,864 | 16,470 → 9,236 |
| aarch64 | 64 Kio | 96,8 → 97,1 | 0,293 | 1,194 → 0,770 |
| aarch64 | 1 Kio | 100,0 → 100,6 | 0,001 | 0,301 → 0,301 |
| x86_64 | 4 Mio | 95,3 → 94,7 | 16,832 | 57,537 → 33,481 |
| x86_64 | 1 Mio | 83,3 → 92,5 | 5,729 | 14,424 → 7,983 |
| x86_64 | 64 Kio | 86,8 → 81,2 | 0,255 | 1,034 → 0,661 |
| x86_64 | 1 Kio | 85,7 → 83,6 | 0,001 | 0,261 → 0,261 |

**Contre-épreuve appariée.** Une VM neuve par architecture et par révision, le même scénario de 4 Mio seul, secteurs de 4 096 octets. IPC et bloc sont comptés sur la session entière, montage compris. Les débits sont des **débits utiles du scénario**, pas des débits bruts VirtIO.

| Architecture | Version | Écran (ms) | Premier G (s) | Lecture (Mio/s) | Sauvegarde (s) | Écriture (Mio/s) | Allers-retours IPC | IPC p50/95/99 (µs) |
|---|---|---|---|---|---|---|---|---|
| aarch64 | v0.16.4 | 117,5 | 19,26 | 0,208 | 66,08 | 0,061 | 920 450 | 147/180/213 |
| aarch64 | v0.16.5 | 134,1 | 20,52 | 0,195 | 37,15 | 0,108 | 477 785 | 147/180/246 |
| x86_64 | v0.16.4 | 99,6 | 17,38 | 0,230 | 58,99 | 0,068 | 920 450 | 123/148/180 |
| x86_64 | v0.16.5 | 94,1 | 17,39 | 0,230 | 33,42 | 0,120 | 477 785 | 123/163/229 |

Sur les deux architectures, le banc apparié passe de 15 741 à 8 031 lectures de secteur de 4 Kio (61,49 → 31,37 Mio), avec **1 511 écritures et 137 synchronisations dans les deux versions**. Les fragments bloc en lecture passent de 676 863 à 345 333 : 4 096 octets sur 43 fragments, soit 95,26 octets utiles en moyenne. Sur l’interface fichier, une fenêtre de 4 Kio se répartit sur 47 réponses (87,15 octets utiles en moyenne). Les latences par échange ne diminuent pas nécessairement ; la réduction du nombre de lectures et de passages IPC explique le gain. Dans la variante avec sondes, le courtier compte 48 128 requêtes READ après le parcours et la sauvegarde de 4 Mio (96 209 dans la série instrumentée v0.16.4 au même premier fichier). La phase de publication seule utilise 116 requêtes, dont 64 lots de suffixe. Le second G utilise l’index existant ; la navigation séquentielle intégrale reste lente.

| Architecture | Version | vi lancement / écran / sauvegarde (Kio) | Service fichiers : max observé (Kio) | nsh : max observé (Kio) | Pic des cadres du banc bloc (pages) |
|---|---|---|---|---|---|
| aarch64 | v0.16.4 | 316 / 380 / 444 | 1712 | 588 | 858 |
| aarch64 | v0.16.5 | 316 / 380 / 444 | 1716 | 460 | 827 |
| x86_64 | v0.16.4 | 320 / 384 / 448 | 1732 | 600 | 867 |
| x86_64 | v0.16.5 | 320 / 384 / 448 | 1736 | 472 | 836 |

La mémoire résidente additionne les pages privées et la pile ; le maximum observé aux jalons n’est pas le maximum instantané de tout le système. Le service finit avec une page de plus par architecture sur cette sauvegarde. Le courtier évite son tampon de retransmission ; dans le profil avec sondes, son pic de tas passe de 192 à 64 Kio. Le pic de tas de vi reste de 128 Kio avec 52 allocations sur 4 Mio. Le plafond de cache est local à l’inspection ; il ne s’ajoute pas à la mémoire de chaque éditeur. Le [relevé JSON](validation/es-v0.16.5.json) donne les durées séparées, les latences bloc et les allocations instrumentées.

### Fichiers denses et géométries

| Architecture | Secteur | Données | Démarrage au shell (s) | Commande au premier écran (ms) | Navigation / observation |
|---|---|---|---|---|---|
| aarch64 | 4096 | 16 Mio | 66,01 | 124,6 | EOF en 77,43 s |
| aarch64 | 4096 | 100 Mio | 369,92 | 132,0 | ligne 41 en 38,2 ms |
| aarch64 | 4096 | 1 Gio | > 75 | — | arrêt du banc ; 319 lots bloc observés |
| aarch64 | 512 | 16 Mio | 88,78 | 173,8 | EOF en 107,75 s |
| aarch64 | 512 | 100 Mio | 524,95 | 184,5 | ligne 41 en 46,5 ms |
| aarch64 | 512 | 1 Gio | > 75 | — | arrêt du banc ; 1828 lots bloc observés |
| x86_64 | 4096 | 16 Mio | 53,79 | 100,7 | EOF en 68,16 s |
| x86_64 | 4096 | 100 Mio | 316,00 | 116,5 | ligne 41 en 40,4 ms |
| x86_64 | 4096 | 1 Gio | > 75 | — | arrêt du banc ; 401 lots bloc observés |
| x86_64 | 512 | 16 Mio | 72,07 | 137,4 | EOF en 92,65 s |
| x86_64 | 512 | 100 Mio | 436,96 | 137,8 | ligne 41 en 41,8 ms |
| x86_64 | 512 | 1 Gio | > 75 | — | arrêt du banc ; 2284 lots bloc observés |

Le montage du service seul, sur 16 Mio et secteurs de 4 Kio, mesure **57,27 s sur AArch64 et 50,12 s sur x86_64**, 8 794 lectures de pages avec la lecture d’identification. La sonde compte 115 371 allocations cumulées depuis l’initialisation ; pic des extensions du tas de 128 Kio à la fin du montage. Avant la modification, le client livré v0.16.4 quittait avec le statut 8 avant de lancer vi sur les images denses de 16 et 100 Mio et 1 Gio. La progression lève cette expiration tant que le service effectue des lectures ; elle ne dispense pas du balayage complet. Sur 100 Mio, la navigation testée passe de la ligne 1 à la ligne 41 et franchit une fenêtre de 4 Kio : aucun parcours invité jusqu’à EOF n’est revendiqué à cette taille. À 100 Mio denses, vi passe de 316 à 380 Kio sur ARM et de 320 à 384 Kio sur x86 au premier écran (secteurs de 4 Kio).

Sur le Mac, création / montage / relecture intégrale après création : **2,373 / 0,110 / 0,053 s** pour 16 Mio ; **13,145 / 0,682 / 0,327 s** pour 100 Mio ; **144,573 / 8,466 / 3,932 s** pour 1 Gio. Les données ont été comparées octet par octet. Sur 1 Gio dense, les quatre observations invitées ont été arrêtées à 75 s alors que les lectures continuaient ; le temps au premier écran et la mémoire de vi sur ce fichier ne sont **pas mesurés**. Pour 1 Gio **creux**, le premier écran a été atteint sur les deux géométries : 316 → 380 Kio de mémoire résidente au démarrage et au premier écran sur AArch64, 320 → 384 Kio sur x86_64. Ouvrir ce préfixe texte ne valide pas que les trous sont du texte.

Les cibles de premier écran sont rapportées après montage : 1 Mio sous 100 ms dans la série ; 4 Mio sous 250 ms sur x86_64 et dans la répétition ARM, mais **313 ms** dans la série ARM. Sur les images montées, le premier écran de 16 Mio reste sous 750 ms et celui de 100 Mio sous 3 s. Si l’utilisateur part d’une VM éteinte, le montage domine et ces deux dernières durées ne représentent pas son attente. La cible de 30 s à 1 Gio dense n’est pas démontrée.

## Validation et limites

- **NovaOS, par architecture** : 25 démarrages ; 24 groupes bloc ; trois contrôles ELF, cycle de vie et courses IPC. Suites fichiers et espaces de noms sur les deux géométries, CLI/P9, nsh interactif, scripts, terminal, vi, écran, volumes et sept tests de mémoire de l’éditeur. Les scénarios incluent E/S en erreur, coupures autour des deux barrières, remontage et relecture, quotas et restitution. Les empreintes écran du démarrage ont été actualisées pour le seul chiffre du numéro de version ; les 25 scénarios ont été rejoués sur chacune des deux architectures.
- **Nouveaux contrats** : trois tests de progression sur Darwin ARM64 natif et x86_64 sous Rosetta (progression valide, refus falsification/répétition, silence/retrait, sans redemande). Dans QEMU sur les quatre couples architecture/secteur : sauvegarde validée octet par octet, UTF-8 et CRLF aux frontières des lots de 64 Kio, dernière ligne sans LF, rejet d’un suffixe invalide après un premier lot durable, aucune destination visible en échec. Dans le service réel avec clients de laboratoire : révision périmée, suppression d’un fichier épinglé refusée, renommage puis réutilisation du nom donnant un conflit avec tampon conservé, essais de mauvais handle, longueur, offset, flags et portée, et copie en mode octets. Les quatre profils sans injection et les quatre avec injection ont été relus indépendamment. Un arrêt de QEMU après F2 du premier lot, avant réponse au courtier, a également été injecté dans chacun des quatre profils : les 135 171 octets de la source ont été relus à l’identique avant et après remontage normal et le nom final est resté absent. Une candidate temporaire déjà durable peut rester sur disque si toute la VM disparaît ; ce test ne revendique ni son nettoyage automatique ni une transaction atomique multi-lot.
- **NovaFS P9** : sur chacune des deux architectures hôtes, 196 tests standards et 197 instrumentés réussis (six ignorés dans chacune des deux suites), 1 000 000 d’opérations différentielles et 2 304 injections d’écriture partielle d’ancre. Contrôles supplémentaires de migration, saturation, snapshots/clones, pool chiffré et récupération. Interopérabilité P9 vérifiée dans les deux sens sur les deux géométries (images source et alias intactes, deux restaurations par cas), huit échanges F01, compilation du cœur portable pour les deux cibles nues. Les écritures partielles dont P9 ne peut pas authentifier le résultat restent refusées au montage normal. Aucun changement de format, de propriété d’atomicité ou de politique de récupération.
- **UTM / AArch64** : 11 scénarios GICv3, 23 DMA séquentiels et 19 cas de sécurité GPT/4096 sur le moteur QEMU d’UTM ; séparément, installation et persistance dans l’application UTM sur QCOW2/512 (ci-dessous). Pas de mesure d’un disque dense de 100 Mio dans l’application ni d’essai de perte d’alimentation matérielle. Le balayage intégral du montage, les petits messages du protocole bloc et le coût du premier G restent les pistes mesurées pour une prochaine décision.

## Installation dans UTM

[Image installable AArch64](../../distribution/images/novaos-0.16.5-utm-aarch64.tar.gz), SHA-256 `3dc8847c047554e05abed22d36cde2f40cf68e8e24e00c78aa1d66bcb37d2a11`. Les 27 entrées du manifeste ont été relues. Exécuter depuis la racine du dépôt :

```sh
d=$(mktemp -d "$HOME/Documents/NovaOS-0.16.5.XXXXXXXX")
tar -xzf distribution/images/novaos-0.16.5-utm-aarch64.tar.gz -C "$d"
dist="$d/novaos-0.16.5-utm-aarch64"
(cd "$dist" && shasum -a 256 -c SHA256SUMS)
cat "$dist/LIRE-MOI.txt"
python3 novaOS/scripts/paquet_installation.py creer "$dist" "$d/machine"
```

UTILISATION : suivre la notice du paquet et la [procédure de sélection d’un disque vierge](installer-v0.15.0.md). L’application UTM 5.0.8, moteur QEMU 10.0.12, a installé le système, redémarré deux fois depuis le seul disque QCOW2/512, exécuté les scripts et relu 66 560 octets de l’original et 66 562 de la copie. Premier écran : 787 ms, puis 70 ms après le deuxième démarrage (contre 810 et 716 ms dans le banc v0.16.4). Ce sont des observations via le PTY de l’application ; elles ne mesurent ni une perte d’alimentation, ni un débit de 100 Mio dans UTM.

## Reproduire

- Construire `make -C novaOS ARCH=aarch64 kernel` et `ARCH=x86_64`. Utiliser `novaFS/lab/run build --offline --bin novafs` pour le lecteur hôte. [Protocole de vi](vi-v0.16.4.md#reproduire), [banc de comparaison](../tests/vi_performance.py) : tailles `4194304 1048576 65536 1024 --retour-fin`, dans cet ordre.
- [Générateur](../tests/vi_volume_dense.rs) : créer une image neuve, sans la réutiliser comme destination ; le générateur relit tous les octets. [Profil de montage](../tests/profil_montage.rs) : lier la même source de sonde aux bibliothèques des deux révisions ; arguments `image.raw taille_Mio secteur`. [Banc invité](../tests/vi_montage_dense.py) : `architecture image.raw taille_Mio rapport.json --secteur 4096`, ajouter `--parcourir` pour G, `--observation 75` pour l’exploration bornée. Toutes les écritures invitées utilisent une copie jetable.
- Sondes uniquement dans une copie du dépôt : copier `novaOS/user/storage/fileapi.rs` dans `components/libraries/sonde_fileapi.rs`, appliquer [vi-profil-pages](../tests/fixtures/vi-profil-pages.patch) puis [io-0165-profil](../tests/fixtures/io-0165-profil.patch) avec `patch -p1`. Reconstruire, employer `--sondes` dans les deux bancs. N’incorporer aucun de ces patches à la distribution.
- Concurrence : dans une copie distincte, appliquer dans l’ordre les fixtures [vi-conflit-source](../tests/fixtures/vi-conflit-source.patch), [vi-fenetres](../tests/fixtures/vi-fenetres.patch), [vi-fenetres-pages](../tests/fixtures/vi-fenetres-pages.patch), [io-0165-contrats](../tests/fixtures/io-0165-contrats.patch). Reconstruire, puis `python3 -B novaOS/tests/vi_pages.py aarch64 rapport.json --secteur 512 --injection --remplacement`, et répéter pour les deux architectures/géométries. Pour la coupure entre deux lots, appliquer en plus [la suspension juste après F2](../tests/fixtures/io-0165-coupure.patch) dans cette copie de laboratoire et reconstruire avant d’exécuter `python3 -B novaOS/tests/coupure_copie_plage.py aarch64 512 rapport.json` pour chacune des quatre combinaisons. Pour la progression, exécuter [le client sous transport simulé](../tests/progression_montage.rs) avec `make -C novaOS ARCH=aarch64 mount-progress-test` puis `ARCH=x86_64`.

Pour construire les deux sondes hôtes avec le moteur courant (Darwin ARM64, depuis la racine) :

```sh
t=aarch64-apple-darwin
NOVAFS_TARGET="$t" novaFS/lab/run build --offline --lib
lib="novaFS/target/$t/release"
d=$(mktemp -d "$HOME/Documents/NovaOS-0.16.5.XXXXXXXX")
for essai in vi_volume_dense profil_montage; do
    rustup run stable rustc --edition 2024 -O --target "$t" \
        --extern novafs="$lib/libnovafs.rlib" -L "dependency=$lib/deps" \
        "novaOS/tests/$essai.rs" -o "$d/$essai" || exit 1
done
"$d/vi_volume_dense" "$d/dense-16.raw" 16
"$d/profil_montage" "$d/dense-16.raw" 16 4096
```

Pour reconstruire le support AArch64 à partir du firmware privé expérimental déjà qualifié (répertoire contenant le firmware et le modèle de variables ; voir la [qualification EFI](installer-v0.15.0.md)) :

```sh
(
    set -eu
    cd novaOS
    d=$(mktemp -d "$PWD/build/es-0165.XXXXXXXX")
    t=aarch64-unknown-none-softfloat
    export RUSTC="$(rustup which --toolchain stable rustc)"
    rustup run stable cargo build --release --locked --offline --target "$t" \
        --features systeme-gpt --target-dir "$d/systeme"
    python3 scripts/esp_installation.py "$d/systeme/$t/release/novaos" "$d/esp"
    NOVA_ESP="$d/esp" rustup run stable cargo build --release --locked --offline \
        --target "$t" --features installation-gpt --target-dir "$d/install"
    python3 scripts/paquet_installation.py construire build/nsh-qualification/firmware \
        "$d/install/$t/release/novaos" "$d/paquet"
)
```

Régressions NovaOS, depuis la racine :

```sh
for a in aarch64 x86_64; do
    make -C novaOS ARCH="$a" test elf-test lifecycle-test ipc-races block-test file-test \
        namespace-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test \
        vi-test vi-ecran-test vi-memoire-test vi-volumes-test mount-progress-test || exit 1
done
```

Pour NovaFS, suivre les [commandes de régression Darwin](../../novaFS/AGENTS.md#validation) et les [injections historiques](../../novaFS/docs/novafs-p9-ancres-dechirees.md#reproduction) ; pour le P9 interarchitectures : `python3 -B novaFS/lab/p9-interop.py`.
