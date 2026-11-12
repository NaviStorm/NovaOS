# NovaOS v0.16.4 — Première campagne : isoler le coût des lectures

**Archive expérimentale.** Ce rapport et ses compteurs décrivent le témoin v0.16.3 et la variante intermédiaire à chargement intégral. La [version distribuée et les exigences révisées](vi-v0.16.4.md) font l’objet d’une campagne distincte. Dans les tableaux ci-dessous, « après » désigne uniquement cette variante intermédiaire.

Sur le volume P9 de développement à secteurs de 4 096 octets, l’ouverture de 4 Mio passe d’environ dix minutes à une demi-minute. Le gain vient du courtier fichier de NovaOS. L’éditeur, son historique, le protocole de sauvegarde et le moteur NovaFS v0.9.13 P9 restent inchangés. Les objectifs de 100 ms à 64 Kio, 1 s à 1 Mio et 3 s à 4 Mio ne sont pas atteints.

## Diagnostic

Le chronomètre hôte part à l’envoi de la commande et s’arrête à la première barre d’état du fichier. Les deux jalons intermédiaires sont le lancement de l’ELF et la demande de dimensions du terminal. Dans la version initiale, pour 64 Kio, la première phase seule prend 9,19 s sur AArch64 et 8,21 s sur x86_64, contre 7,8 et 5,3 ms pour le premier rendu (première série). Cela exclut le repositionnement du curseur comme cause principale.

1. Avant le lancement, [nsh](../../components/shell/paths.rs) ouvre un handle en lecture, vérifie la taille, réserve le tampon une fois, lit et vérifie à nouveau l’identité, la taille et la révision. Une lecture P9 peut publier une nouvelle date d’accès. Lorsque la révision change, le courtier compare une seconde fois les octets jusqu’à EOF. Cette vérification est conservée.
2. En v0.16.3, chaque tranche IPC de 88 octets fait repasser le [service fichier](../user/novafs/files.rs) par la lecture de l’objet et la politique d’horodatage du [moteur](../../novaFS/src/fs.rs). Pour un fichier de 64 Kio dont l’accès change la révision, on observe 1 491 lectures du moteur, EOF compris. Ce n’est pas une réallocation de tout le texte à chaque fragment : le nombre de passages dans le chemin objet, métadonnées et pages explique le coût.
3. Après lancement, le [dialogue privé de vi](../../components/libraries/vi_wire.rs) transfère au plus 127 octets utiles par aller-retour. Pour 64 Kio, il faut 517 demandes de contenu. L’ELF réserve la taille exacte, valide UTF-8, puis transmet le même stockage au gap buffer. Le contrôle des caractères et le calcul des lignes parcourent le texte ; aucune table de 512 lignes ni copie du texte par ligne n’est construite. Le [rendu](../../components/vi/screen.rs) reste différentiel. Avec sondes sur 64 Kio, transfert et préparation valent respectivement 36,1 et 1,0 ms sur AArch64. Les mesures de très courtes phases sur UART peuvent être regroupées par l’hôte ; elles ne constituent pas une mesure en cycles du noyau.

## Lecture groupée et intégrité

Le [client](../user/storage/fileapi.rs) permet au courtier de demander une fenêtre d’au plus 4 096 octets sur son handle déjà ouvert. Une requête READ de début contient le total et l’offset ; le service lit ce total **une seule fois**, traite normalement l’accès et renvoie le premier fragment. Les continuations utilisent le même endpoint : handle vivant avec droit de lecture, génération courante, total et offset attendu sont contrôlés. Une réponse courte termine la lecture. Une continuation sans fenêtre est refusée. Une écriture en attente empêche d’ouvrir ou d’utiliser cette fenêtre. Il n’y a pas de cache partagé entre clients. La date d’accès est traitée par le moteur une fois par lecture logique, et non pour chaque fragment de transport. Les dates absolues dépendent de l’horloge. Le profil système qualifié utilise la politique relative ; ces mesures ne caractérisent pas un volume en politique stricte. Les lectures ordinaires et les vues déléguées conservent leur contrat. Les scripts et commandes de lecture ordinaires continuent d’utiliser READ sans fenêtre.

Dans la trame fichier v2 (little-endian), READ conserve l’opcode 4. À +2, 0 désigne la lecture ordinaire, 1 le début d’une fenêtre et 2 la continuation. La longueur à +4 vaut le minimum de 88 et du nombre d’octets restant dans la fenêtre demandée. La session, le handle et l’offset restent à +8, +16 et +24 ; le total à +32 est nul pour une lecture ordinaire et compris entre 1 et 4 096 pour une fenêtre. Une réponse valide donne le nombre réellement disponible à +4 et les octets à +40.

Les messages IPC ont toujours une taille maximale de 128 octets. Le nombre de messages **entre nsh et le service** augmente légèrement à cause des fins de page. Le nombre de lectures et de transferts **entre le service et les couches de stockage** diminue beaucoup plus. Les compteurs suivants portent sur la même ouverture de 64 Kio sur les deux architectures :

| Opération mesurée | v0.16.3 | v0.16.4 |
|---|---:|---:|
| Requêtes de nsh au service, ouverture et contrôles compris | 1 495 | 1 509 |
| Dont READ, EOF compris | 1 491 | 1 505 |
| Passages demandant une lecture au moteur | 1 491 | 33 |
| Pages de 4 Kio lues par IpcDevice entre le premier STAT et CLOSE | 1 509 | 51 |
| Allers-retours service → bloc pour ces seules lectures de pages (calculés) | 67 905 | 2 295 |
| Allocations réussies du service sur cet intervalle, sondes comprises | 26 896 | 652 |
| Pic du tas privé du service pendant cet intervalle | 128 Kio | 128 Kio |
| Allocations de nsh entre résolution du chemin et source prête | 3 | 3 |
| Allocations de vi pour recevoir le texte / préparer l’éditeur / dessiner le premier écran | 1 / 1 / 26 | 1 / 1 / 26 |

À secteurs de 4 Kio, le [transport bloc](../user/storage/medium.rs) fait deux échanges de contrôle et 43 échanges de données de 96 octets au plus par lecture de secteur. Les 45 échanges par page expliquent l’amplification initiale. Le nombre de pages est mesuré ; le nombre d’échanges de cette ligne est calculé à partir du contrat et exclut les écritures de métadonnées.

Le service conserve au plus une fenêtre de contenu par session non déléguée ; la contre-lecture utilise 4 Kio de la pile déjà attribuée à nsh. Aucun quota, appel système, droit IPC ou format disque n’a changé. Le courtier conserve la source pendant l’édition ; un accès paresseux qui rouvrirait le chemin ne fournirait pas la même vue si un autre acteur changeait le fichier. Le prix de cette garantie est une copie du texte dans chacun des deux processus. À la sauvegarde, le transfert de la destination puis les écritures et synchronisations par lots de 64 Kio subsistent. Un nom occupé, une révocation ou un acquittement incertain ne déclenchent aucun remplacement automatique. [Contrat de sauvegarde](vi-v0.16.1.md)

Le second transfert par fragments et la croissance du tampon de sauvegarde sont mesurés ensemble, avant la confirmation. Le courtier grandit géométriquement au début, puis par paliers de 64 Kio ; si l’allocateur doit déplacer chaque palier, la somme des octets copiés peut croître quadratiquement. Ce cas concerne la sauvegarde, pas le chargement ; les sondes ne comptent pas directement les octets déplacés par l’allocateur. Réserver aveuglément toute la taille annoncée par un enfant ferait porter son coût au courtier. Cette version conserve la réservation progressive et le protocole de publication.

## Mesures

Les [résultats chiffrés](validation/vi-v0.16.4-groupe.json) distinguent le témoin, la variante intermédiaire, les sondes et les variantes de laboratoire. QEMU direct 11.1.3 : TCG, un vCPU, 256 Mio, VirtIO avec IOMMU, volume P9 brut autonome ; UART répondant 24 × 80. Chaque fichier contient des lignes ASCII de 128 octets, séparateur compris. Dans les essais avec sauvegarde : navigation en fin de fichier, retour en tête, insertion de X, publication sous un **nom libre**, sortie de vi, arrêt de la session puis vérification indépendante de l’intégralité de la source et de la copie. Les durées sont des observations en temps mural ; le Mac exécutait plusieurs VM. Elles ne constituent pas des percentiles ni une garantie de latence.

Durées sur volumes appariés (même géométrie, données, ordre 4 Mio → 1 Mio → 64 Kio → 1 Kio, terminal et contrôles). « Avant » est ici **remesuré** sur le commit de référence. Le grand fichier du banc historique mesurait 681,78 s et 576,56 s ; les valeurs du tableau proviennent de cette nouvelle exécution. Les différences entre séries sont conservées dans le JSON.

| Architecture | Taille | Ouverture avant → après (s) | Gain | Lecture + lancement / transfert + préparation / premier rendu après (ms) | Débit utile avant → après (Kio/s) | Navigation G avant → après (ms) | Sauvegarde complète avant → après (s) |
|---|---:|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 Kio | 0,242 → 0,099 | ×2,4 | 88,3 / 2,7 / 8,4 | 4,2 → 11,3 | 0,78 → 6,61 | 0,30 → 0,32 |
| aarch64 | 64 Kio | 10,351 → 0,545 | ×19,0 | 503,8 / 33,0 / 8,3 | 6,2 → 127,0 | 1,21 → 1,18 | 0,99 → 1,35 |
| aarch64 | 1 Mio | 168,337 → 9,004 | ×18,7 | 8491,3 / 505,1 / 8,0 | 6,1 → 120,6 | 1,20 → 1,22 | 11,64 → 12,56 |
| aarch64 | 4 Mio | 673,067 → 34,877 | ×19,3 | 32942,1 / 1929,6 / 5,5 | 6,1 → 124,3 | 1,30 → 1,39 | 46,00 → 50,20 |
| x86_64 | 1 Kio | 0,226 → 0,097 | ×2,3 | 87,3 / groupé / 9,6 | 4,6 → 11,5 | 0,95 → 0,59 | 0,27 → 0,28 |
| x86_64 | 64 Kio | 10,430 → 0,462 | ×22,6 | 428,7 / 28,4 / 4,5 | 6,2 → 149,3 | 1,22 → 1,20 | 0,91 → 0,95 |
| x86_64 | 1 Mio | 148,639 → 8,236 | ×18,0 | 7798,4 / 428,6 / 9,1 | 6,9 → 131,3 | 1,11 → 1,25 | 10,21 → 10,73 |
| x86_64 | 4 Mio | 599,161 → 29,192 | ×20,5 | 27500,0 / 1686,6 / 5,0 | 6,8 → 148,9 | 1,24 → 1,31 | 40,00 → 42,22 |

Le débit utile divise la taille du fichier par la phase lecture + lancement ; ce n’est pas le débit matériel du disque. La sauvegarde comprend le transfert au courtier, la sélection de la destination, la confirmation automatique immédiate et la publication. « groupé » signifie que deux jalons UART étaient déjà dans la même lecture hôte, en deçà de la résolution utile. Pour 4 Mio, la sauvegarde est **plus lente dans la série appariée** : +9,1 % sur AArch64 et +5,6 % sur x86_64. Dans la seconde exécution v0.16.4, elle prend 48,85 et 39,97 s. Le chemin de sauvegarde n’a pas été optimisé ; ces essais en charge ne permettent pas d’exclure une légère régression de latence ni de l’attribuer à un changement de code. Aucun FLUSH n’a été supprimé pour améliorer le chronomètre.

| Taille | Résident de vi au lancement / ouvert / lors de la sauvegarde, pile comprise |
|---|---:|
| 1 Kio | 312 / 376 / 376 Kio |
| 64 Kio | 312 / 504 / 632 Kio |
| 1 Mio | 312 / 1 464 / 3 704 Kio |
| 4 Mio | 312 / 4 536 / 12 920 Kio |

Ces relevés du noyau sont des **échantillons** du résident privé, pas un maximum continu de toute la VM. Le compteur d’extension du tas des sondes, lui, relève chaque acquisition et restitution de pages. Les segments et la pile de vi restent fixes ; à son résident de lancement on peut donc ajouter le maximum des pages supplémentaires effectivement attribuées à son tas. Sur la session complète ouverture/édition/sauvegarde/fermeture instrumentée, le **pic du résident privé de vi** vaut respectivement **376, 632, 3 704 et 12 920 Kio**, avec **38, 40, 41 et 41 allocations réussies**. Ces maxima égalent les maxima déjà échantillonnés dans le témoin non instrumenté ; à 1 et 64 Kio la mesure continue du témoin les confirme également. À 64 Kio, le pic du tas de vi à l’ouverture reste à 192 Kio. Dans la série appariée jusqu’à 4 Mio, le maximum échantillonné du service et celui de nsh pendant l’édition sont eux aussi identiques avant/après : **1 648 / 8 644 Kio** sur AArch64 et **1 668 / 8 660 Kio** sur x86_64, respectivement. La mesure de session de nsh reste à **452 Kio AArch64 / 468 Kio x86_64**. Le tampon dynamique et l’historique conservent les mêmes budgets. Le banc Darwin couvre également 16 Mio de texte en une ligne, édition et panne d’allocation ; il ne mesure pas les lectures d’un disque invité.

**16 Mio, exploration.** Sur l’image normale, le volume P9 prépeuplé ne parvient pas à l’invite : le client de montage expire (statut 8) avant le lancement de vi. Le résultat se reproduit sur les deux architectures, y compris avec un volume brut de 32 Mio limité à la lecture dans le scénario. Le témoin v0.16.3 expire également sur ce volume. Aucun temps d’ouverture de production n’en est déduit. Dans une [variante isolée](../tests/fixtures/vi-montage-exploratoire.patch), seule la réception de la même réponse MOUNT peut être réattendue, au plus sept fois après la première attente ; aucune requête n’est renvoyée, aucun quota ni contrôle du noyau n’est relâché. Sur ce banc **uniquement**, ouverture de 16 Mio en **137,79 s AArch64 / 120,55 s x86_64**, consultation de la dernière ligne, sortie sans sauvegarde et vérification des octets par le lecteur Darwin. Résident ouvert : 16 824 Kio sur les deux. Le mode « lecture seule » du banc signifie que vi ne demande pas de sauvegarde ; le volume monté conserve sa politique d’accès P9. L’essai ne qualifie pas la sauvegarde à cette taille.

Pour se rapprocher des objectifs, le prochain point à mesurer est le coût du transport bloc et des deux parcours lorsque la révision évolue. Une éventuelle distinction entre révision du contenu et mise à jour de la date demanderait un contrat de cohérence démontré : supprimer la contre-lecture sans ce contrat n’est pas une optimisation sûre.

## Témoin UTM de la lecture groupée

La variante intermédiaire, encore à chargement intégral, a rouvert 66 560 octets en 3,47 s après redémarrage de l’application UTM (48,54 s sur v0.16.3). Voir le rapport principal pour l’image finalement distribuée.

## Régressions

Sur **chacune des deux architectures** : 25 démarrages, 24 groupes bloc, trois contrôles ELF/processus/IPC, suites fichier, namespace, CLI/P9, scripts, terminal, écran, mémoire et grands volumes vi. Pour chacun des secteurs 512 et 4 096 : quatre sessions CLI (dont coupure et remontage), NT01 à NT03, 84 cas d’options/clavier et 21 mutations contrôlées par le lecteur hôte. Les profils vi ont chacun relu 16 fichiers. Sur AArch64 : 11 scénarios GICv3, 23 scénarios DMA séquentielle et matrice de sécurité GPT 4 Kio (19 entrées, dont la classification après coupure).

NovaFS sur Darwin ARM64 natif et x86_64 sous Rosetta : **196 standards et 197 instrumentés réussis**, six tests marqués ignorés dans chacune de ces exécutions ; campagne différentielle distincte de **1 000 000 opérations** par architecture. Les huit combinaisons du banc d’ancres ont exécuté 32 plages × 9 phases chacune : **2 304 injections, dont 360 refus attendus**, par architecture. Aucun changement dans le répertoire novaFS.

Les nouvelles contre-épreuves couvrent les fenêtres malformées, les offsets discontinus, l’EOF court après exactement 88 octets disponibles, le changement de handle, la répétition après consommation, la session incorrecte et l’écriture en attente. Les tests de vi comprennent un caractère UTF-8 valide réparti sur deux fenêtres, le même caractère tronqué et un fichier contenant NUL. Les seize fichiers attendus, y compris les originaux refusés, sont relus à l’octet près avant et après un nouveau démarrage, dans chacun des quatre profils architecture/secteur. Les essais historiques couvrent aussi le fichier vide, l’absence de LF final, les lignes longues, les fichiers binaires, `:w`, `:q`, `:q!`, `:wq`, les annulations et rétablissements, la concurrence, la révocation et le refus d’une allocation de 512 Mio. Les essais bloc et fichier injectent des erreurs d’écriture et de FLUSH puis contrôlent le volume remis en service. Ils n’établissent pas l’atomicité d’une ancre déchirée ; PC1 et le remplacement transactionnel restent désactivés.

## Reproduire

Depuis la racine, construire d’abord l’outil hôte en release et le noyau de chaque architecture. Chaque invocation utilise une image temporaire neuve et archive la série dans `novaOS/build`.

```sh
NOVA_ARCH=aarch64
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --release --bin novafs
make -C novaOS ARCH="$NOVA_ARCH" kernel
python3 -B novaOS/tests/vi_performance.py "$NOVA_ARCH" "novaOS/build/perf-$NOVA_ARCH.json" 4194304 1048576 65536 1024
make -C novaOS ARCH="$NOVA_ARCH" vi-test vi-ecran-test vi-memoire-test vi-volumes-test
```

Répéter avec `NOVA_ARCH=x86_64`. Pour mesurer le témoin, construire `562cf137457e4ab8d8e9038dcd957a9a4dfe43b8` dans une copie séparée, avec les deux pilotes Python `tests/{cli,vi_performance}.py` de cet audit et le même outil hôte. Pour reconstruire la variante intermédiaire, appliquer [vi-groupe-seul.patch](../tests/fixtures/vi-groupe-seul.patch) sur le commit de référence. Pour les compteurs, dans une autre copie jetable **avant la construction**, copier `novaOS/user/storage/fileapi.rs` vers `components/libraries/sonde_fileapi.rs`, appliquer [vi-profil.patch](../tests/fixtures/vi-profil.patch) puis [vi-service.patch](../tests/fixtures/vi-service.patch) avec `patch -p1`. Demander par exemple `65536 --sondes --lecture-seule` ; les marqueurs ASCII `VI-PROF` et `FS-PROF` restent sur l’UART du banc, jamais dans l’image distribuée. Une petite série suffit pour les sondes du service. La mesure continue du pic de vi utilise aussi le jalon vi-fin ; quatre fichiers dans cette session consomment douze messages du shell sur les seize autorisés.

Le [banc de trames](../tests/fixtures/vi-fenetres.patch) s’applique séparément à une copie v0.16.4 ; construire le noyau puis exécuter `NOVA_ARCH=aarch64 python3 novaOS/tests/fichiers.py --only normal` et `make -C novaOS ARCH=aarch64 cli-test`, puis répéter sur x86_64. Pour chronométrer UTM, le [pilote du port série de l’application](../tests/nsh_utm.py) prend en arguments le répertoire de distribution extrait et un **nouveau** répertoire de collecte. Exécuter un seul banc d’application UTM à la fois.
