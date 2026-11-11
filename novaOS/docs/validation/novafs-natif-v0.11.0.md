# Qualification des fichiers NovaFS dans NovaOS v0.11.0

Base Git `15d00efda88f16a290857a81474f7fd7ac211e3f`. Hôte macOS ARM64 27.2, rustc 1.99.1, QEMU 11.1.3, Python 3.15.2. Les invocations du moteur côté invité proviennent de deux ELF bare-metal différents. La CLI de production côté Mac est un Mach-O ARM64. Chaque campagne utilise des fichiers images ordinaires jetables, un disque raw avec cache writeback et remontage par un nouveau processus QEMU.

## Parcours et oracle

| Contrôle | Observation exigée |
|---|---|
| Démarrage, service bloc et service fichier | Un lanceur distribue les capacités à quatre processus distincts. Le service réel monte le LV et l’application attend sa réponse. Les sorties série indiquent Ring3 ou EL0. |
| MOUNT, INFO, OPEN et READ | Image fabriquée par la CLI macOS. Lecture de `/source`, comparaison des dix octets, contrôle du compte à EOF et de l’octet suivant du tampon. Le profil manuel a aussi monté un volume vide dans les quatre configurations. Son identifiant affiché correspond aux 16 octets annoncés par la CLI hôte. |
| Création, modification et synchronisation | OPEN crée `/sortie`. WRITE transmet 4 231 octets, puis une seconde écriture de 15 octets traverse la limite de bloc à l’offset 4 091. Le client lit sa vue privée avant SYNC, puis compare le fichier entier après publication. Le dernier SYNC sur handle propre demande encore un FLUSH. |
| Redémarrage et compatibilité croisée | Après arrêt de la VM, la CLI hôte vérifie les deux générations et extrait les fichiers par le slot de tête. L’oracle Python calcule les octets attendus indépendamment du codec. Une autre VM monte en RO, compare les 4 231 octets et le fichier témoin. Le banc vérifie l’égalité intégrale de l’image avant et après cette lecture. |
| Privilèges et trames | Refus d’une écriture au moyen d’un handle de lecture, de l’ouverture RW en session RO, d’une session erronée, de la traversée par `..`, d’un champ réservé non nul et d’un offset débordant. Le test réutilise successivement un handle fermé et la capacité de requêtes retirée à la fermeture de session. |
| Fragments et fermeture | Un fragment manquant bloque SYNC. DISCARD abandonne les octets sans publication. CLOSE ordinaire refuse de perdre une préparation complète. Deux handles concurrents sur le même fichier sont exercés avant et après SYNC. Le second conserve la vue publiée, son START concurrent obtient BUSY et il voit les octets nouveaux après confirmation. |
| Pannes de client et de service | Le client termine avec une préparation privée. Le support est identique et une nouvelle VM retrouve l’ancienne génération. Un second essai arrête le service après soumission de SYNC par le client, avant publication. Le client signale l’issue indéterminée. Dans la même VM, le superviseur retire le bail et lance une nouvelle chaîne qui retrouve l’ancienne vue. |
| Pénurie et intégrité | Une image de 145 blocs ne possède plus que quatre blocs ordinaires disponibles pour la mutation, avec une réserve de maintenance de 128 blocs. La demande retourne FULL. Comparaison intégrale du support et remontage en RO. Une ancre altérée et une signature inconnue provoquent également un refus RW sans modification de l’image. |
| Formats de pools | Les images P5, P6 et P7 sont produites par la CLI de leur format. Le service refuse le montage RW. Le SHA-256 du fichier entier est identique avant et après chaque tentative. La clé administrative jetable P7 transite par un tube du banc hôte. |
| Collecte | Deux chaînes consécutives distinctes exécutent le profil de mesure dans une même VM. Le noyau compare le nombre de cadres libres, les blocs, les octets occupés et engagés du Buddy après la seconde collecte avec ceux de la première. Il vérifie aussi l’absence de processus et de pages IPC résiduels. |

Le montage initial utilise les contrôles d’intégrité complets de `Engine::mount`. Pour l’oracle hôte, `verify` précède systématiquement `salvage`. Le choix explicite d’un slot pour extraire les octets intervient après validation de la paire d’ancres. Les sorties « fraîcheur indéterminée » de la commande de sauvetage sont normales. La génération de départ et le fichier témoin sont connus du banc avant le premier démarrage.

## Coupures aux frontières de publication

Chaque ligne part d’une copie neuve de la génération 5 contenant `/sortie = ancien`. La seule transaction possible publie la génération 6. L’hôte tue QEMU, demande un contrôle intégral au moteur macOS puis démarre une nouvelle VM pour comparer les octets. Les couples dans les colonnes d’E/S comptent les écritures et FLUSH **terminés** du disque de données, relevés via QMP au point de coupure. Le premier FLUSH appartient au montage RW.

| Frontière | 512 octets · écritures / FLUSH | 4 096 octets · écritures / FLUSH | Génération observée sur les deux invités |
|---|---|---|---|
| Données CoW | 1 / 1 | 1 / 1 | 5 |
| Métadonnées CoW | 17 / 1 | 3 / 1 | 5 |
| Avant F1 | 48 / 1 | 6 / 1 | 5 |
| F1 en vol | 48 / 1 | 6 / 1 | 5 |
| Après F1 | 48 / 2 | 6 / 2 | 5 |
| Avant F2 | 56 / 2 | 7 / 2 | 6 |
| F2 en vol | 56 / 2 | 7 / 2 | 6 |
| Après F2 | 56 / 3 | 7 / 3 | 6 |
| Pendant CLOSE | 56 / 3 | 7 / 3 | 6 |
| Pendant MOUNT | 0 / 0 | 0 / 0 | 5 |

Pour les coupures « pendant F1/F2 », le backend blkdebug suspend réellement la requête `flush_to_disk`. Le banc attend le retour de `wait_break` avant le SIGKILL. Pour la fermeture, la coupure intervient dans le traitement du CLOSE du handle après l’acquittement de SYNC au client. Pour le remontage, elle intervient après la lecture de la première ancre et avant admission. Aux deux points CoW, le banc interrompt une phase de plusieurs écritures. En géométrie 512, il peut ainsi s’arrêter à l’intérieur d’un bloc NovaFS. En géométrie 4 096, il s’arrête après une page entière. Les contenus de la fixture distinguent sans ambiguïté les données et les métadonnées.

Au premier essai, une injection de lecture à usage unique n’a produit aucun IOERR dans le pilote invité. Le banc maintient désormais l’erreur sur le backend jusqu’au retour de l’application et exige le statut VirtIO 1 sur l’opération visée. L’attente QMP d’un FLUSH suspendu est déclenchée après le marqueur de soumission, car une attente trop précoce bloquait le moniteur de test.

Les injections EIO indépendantes portent sur la lecture, l’écriture et chacune des deux synchronisations. Le banc exige une erreur client, l’absence d’acquittement transactionnel et la génération autorisée au montage suivant. F1 en erreur laisse la tête précédente. F2 en erreur donne le statut 22. Le fichier hôte peut déjà contenir l’ancre complète à ce stade. La génération effectivement récupérée figure ci-dessous.

| Invité | Secteurs (octets) | Historiques | Bloc | ELF / IPC courses / IPC perf. | Coupures avec nouveau montage | EIO avec nouveau montage | Refus P5/P6/P7 |
|---|---|---|---|---|---|---|---|
| x86_64 | 512 et 4 096 | 25 | 24 | réussis | 20/20 | 8/8 | 6/6 |
| aarch64 | 512 et 4 096 | 25 | 24 | réussis | 20/20 | 8/8 | 6/6 |

Dans les quatre couples, EIO lecture, écriture et F1 ont donné le statut 26 et la génération 5. EIO F2 a donné le statut 22 et la génération 6 entière. Le retrait du bail a également été contrôlé lors des injections du banc bloc. Les suites historiques exercent l’ABI, les capacités IPC, les protections, le clavier et les interruptions. La sonde ELF vérifie le 17ᵉ appel HEAP_RESIZE et la restitution des cadres. Les mesures bloc historiques, avec 128 transferts par profil, ont réussi sur les quatre couples. Les cinq références d’écran ont été recalculées depuis les captures v0.10.0 dont les empreintes ont été vérifiées, en remplaçant uniquement le glyphe de version modifié.

## Mesures

Le compteur monotone invité est converti avec la fréquence inscrite dans chaque journal. Le tableau indique la **seconde session** en VM, après un premier montage, une série d’écritures et un retrait du bail. Les arbres et les générations à vérifier peuvent différer entre ces deux sessions. Un chronométrage est un intervalle réel incluant l’ordonnancement, les appels IPC et l’émulation. Les bancs concurrents et le cache de macOS influencent les résultats.

| Invité / secteur | MOUNT (ms) | INFO (ms) | OPEN (ms) | READ (ms) | WRITE (ms) | SYNC (ms) | CLOSE fichier (ms) |
|---|---|---|---|---|---|---|---|
| x86_64 / 512 | 482,087 | 0,289 | 18,439 | 61,112 | 82,075 | 135,481 | 0,170 |
| x86_64 / 4096 | 337,157 | 0,268 | 13,478 | 51,230 | 59,647 | 95,115 | 0,167 |
| aarch64 / 512 | 503,057 | 0,242 | 18,237 | 54,492 | 81,934 | 140,742 | 0,162 |
| aarch64 / 4096 | 332,636 | 0,241 | 12,338 | 43,094 | 57,150 | 93,781 | 0,165 |

| Invité / secteur | Lecture séq. (Kio/s) | Écriture durable séq. (Kio/s) | Lecture aléat. (Kio/s) | Écriture durable aléat. (Kio/s) | F1 (ms) | F2 (ms) | IPC p50 / p99 (µs), n |
|---|---|---|---|---|---|---|---|
| x86_64 / 512 | 67,611 | 18,992 | 112,184 | 0,374 | 0,546 | 0,301 | 122.6 / 196.2 (16092) |
| x86_64 / 4096 | 80,653 | 26,698 | 113,319 | 0,535 | 0,619 | 0,373 | 123.2 / 180.6 (11361) |
| aarch64 / 512 | 75,825 | 18,555 | 154,464 | 0,360 | 0,527 | 0,226 | 131.1 / 213.0 (16092) |
| aarch64 / 4096 | 95,880 | 27,376 | 157,654 | 0,538 | 0,362 | 0,243 | 122.9 / 196.6 (11361) |

WRITE mesure l’envoi et l’admission de 4 231 octets. SYNC mesure la reconstruction du plan, sa publication et les deux barrières. Le débit séquentiel d’écriture inclut WRITE et SYNC. La lecture séquentielle porte sur 4 231 octets. Le parcours pseudo-aléatoire fait 16 lectures de 64 octets et huit écritures de 64 octets synchronisées individuellement, dans le même fichier. Il mesure les petites requêtes du service. Le p50/p99 IPC provient des allers-retours du service NovaFS vers le relais bloc durant la seconde session. F1 et F2 sont chronométrés au niveau de l’adaptateur, avec les échanges IPC correspondants. L’échantillonnage RAM en fin de F1 s’ajoute au temps global de SYNC. Deux sessions par couple ne permettent pas d’estimer un p99 fiable pour les opérations fichier.

| Invité / secteur | Noyau à F1 | Service à F1 | Résident à F1 | Pic total | Attributions de cadres | Cadres libres avant / après | Buddy final (octets) |
|---|---|---|---|---|---|---|---|
| x86_64 / 512 | 2124 | 460 | 2880 | 2944 | 325 | 63527 / 63527 | 9280 |
| x86_64 / 4096 | 2124 | 460 | 2880 | 2944 | 325 | 63527 / 63527 | 9280 |
| aarch64 / 512 | 2004 | 448 | 2748 | 2812 | 331 | 62967 / 62967 | 10304 |
| aarch64 / 4096 | 2004 | 448 | 2748 | 2812 | 331 | 62967 / 62967 | 10304 |

Les tailles indiquées sont en Kio. Le service comprend son texte, ses données, son tas et ses pages de pile. La colonne noyau regroupe l’image liée, les tables, les piles noyau, le Buddy, l’IPC et le DMA. Le pic résident total est la RAM au repos augmentée du maximum de cadres supplémentaires engagés durant la seconde session. Le compteur d’allocations compte les attributions de cadres. Le banc ne mesure pas chaque appel malloc de l’application. La vidéo et la RSS du processus QEMU sont hors de ces totaux. Le premier bail laisse 12 cadres réutilisés en x86_64 et trois en AArch64 pour l’infrastructure de transport. Le deuxième bail retrouve exactement le nombre de cadres libres de son départ. L’égalité de l’audit à chaud est vérifiée après destruction du second groupe.

## Portée

Le service expose les fichiers d’un **LV autonome** au format de contenu P0 compris par le moteur P8. Les conteneurs de pools, les vues partagées, le déverrouillage, le scrub et les exports restent du ressort des outils NovaFS sur macOS. Le service natif applique la réserve du LV. Les quotas propres à un pool nécessitent son coordinateur. La session donne autorité sur le volume entier. Elle ne fournit pas d’ACL ni d’interface multi-client concurrente. Un chemin est limité à 88 octets, une écriture à 64 Kio.

Une VM tuée peut laisser le cache de l’hôte intact. Le succès des expériences qualifie le protocole sur le backend QEMU utilisé. Il ne mesure ni la résistance d’un SSD à une coupure électrique ni un rollback de l’image entière. Une déchirure réelle qui invaliderait une ancre imposerait un refus du montage normal. La comparaison Git avec la base couvre tout `novaFS/`, le texte MIT de NovaOS et le contrat des 17 sélecteurs publics. Les sources de NovaFS, ses structures persistantes, ses dépendances et sa licence MPL-2.0 sont identiques à la base. Les suites macOS complètes P8, instrumentées et millionnaires étaient conditionnées à une modification du cœur. Cette campagne utilise la CLI hôte pour produire, vérifier et relire les images.

## Reproduction depuis la racine

```sh
for architecture in x86_64 aarch64; do
    make -C novaOS ARCH="$architecture" test block-test elf-test ipc-races ipc-perf block-perf file-test || exit 1
done
```

Pour une seule injection, par exemple sur AArch64 avec un secteur de 512 octets, construire le noyau et la CLI, puis sélectionner `cuts`, `errors`, `admission`, `pools`, `normal` ou `perf`.

```sh
make -C novaOS ARCH=aarch64 kernel
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
NOVA_ARCH=aarch64 python3 novaOS/tests/fichiers.py --sector 512 --only cuts
```

Les journaux série des profils fichiers et bloc restent dans `novaOS/build/bloc-<architecture>-<secteur>/`. Les images de test sont jetables. Pour lancer le profil manuel, utiliser `make -C novaOS ARCH=x86_64 file-info SECTOR=512 IMAGE=/chemin/volume.raw` ou `make -C novaOS ARCH=aarch64 file-info SECTOR=4096 IMAGE=/chemin/volume.raw`.
