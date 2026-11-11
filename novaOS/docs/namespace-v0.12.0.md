# Espace de noms natif de NovaOS v0.12.0

## Collecte des processus

Sur la base `02c78c8`, la sortie d'un processus choisissait immédiatement le thread idle. Celui-ci reprend toutefois sa pile à l'endroit où il avait été suspendu. Entre deux tours de sa boucle, les interruptions sont permises. Un tick arrivé avant `reap()` rendait le processeur à la file normale. Deux travailleurs continuellement prêts suffisaient ensuite à affamer le collecteur. Le journal AArch64 initial montrait bien la sortie du PID 14 avec le code 23, juste avant l'expiration du délai.

La sonde `proc-test` arme cette fenêtre et attend un tick du timer matériel. Sur l'ancien sélecteur, Ring 3 et EL0 ont tous deux donné `statut=Some(Exit(23)) observe=Some(false) fin=12 collecte=11 preemption_idle=1 file=2`. Le processus avait terminé, son observateur avait disparu et son TCB occupait toujours un slot. La sonde laisse tourner les deux travailleurs, sans création de processus ni appel à join dans le contrôleur pendant l'attente.

Le premier journal ne contenait pas la chronologie des IRQ. Le cas imposé au timer reproduit le même échec avec l'état interne du collecteur.

Le sélecteur consulte désormais la dette de collecte à chaque décision. Tant qu'il reste un TCB terminé, idle peut reprendre sa pile et vider cette dette. Le retrait du TCB, la restitution de sa pile et le dernier détachement de son espace utilisateur restent protégés par le masque IRQ sur CPU0. Le prochain tour d'idle rend la main à la file FIFO. Le délai historique reste d'une seconde. [Ordonnanceur](../kernel/src/sched/mod.rs) · [Sonde de préemption](../kernel/src/sched/process/demo.rs)

Le profil `proc-lifecycle` crée 64 familles. Le parent détient le droit WAIT sur le fils. Le contrôleur alterne mort du parent en premier, mort du fils en premier, sortie normale et faute utilisateur confinée. Il vérifie la survie de l'autre processus et son accès à sa page privée, la conservation du statut pour l'observateur vivant, puis la disparition des deux entrées. Il exerce alternativement la révocation explicite et la destruction de l'endpoint à la mort du parent. À chaque réemploi de slot, l'ancienne clé IPC est refusée. Après chaque famille, l'audit contrôle les TCB, les slabs IPC, les cadres libres et le Buddy. La terminaison forcée du banc passe par une exception utilisateur. [Cycle de vie](../kernel/src/sched/process/ipc/lifecycle.rs) · [Oracle série](../tests/cycle_processus.py)

## Objets et autorité

Le module d'espace de noms s'exécute dans le processus NovaFS, en Ring 3 ou EL0. L'application utilise la bibliothèque native et deux capacités IPC privées. Un seul moteur monté sérialise les opérations et détient le bail de stockage. Le noyau configure les canaux, réalise la commutation et protège le DMA. Le service bloc reçoit des secteurs et des barrières de persistance. [Bibliothèque](../user/storage/fileapi.rs) · [Service](../user/novafs/files/namespace.rs)

Le moteur P8 indexe un nom UTF-8 dans un parent et retrouve le numéro monotone de l'enfant. Il recoupe le lien avec le parent et le nom inscrits dans l'objet. Les ouvertures mémorisent l'ID dans l'instance de montage. READ et SYNC retrouvent les mêmes extents par cet ID, même après le renommage d'un ancêtre. Le couple d'identifiants FS/LV et le montage déterminent la portée de l'ID. La révision d'un objet varie au cours de sa vie. L'autorité du client vient du canal attribué par le lanceur et du handle local retrouvé dans la table de ce canal. [Catalogue](../../novaFS/src/fs.rs) · [Données](../../novaFS/src/fs/data.rs)

Chaque session admet 32 handles de fichiers et de répertoires au total, deux chemins fragmentés et un CWD détenu par référence d'objet et masque de droits. L'émission des handles est monotone pendant la vie du service. La fin du canal invalide les handles et le redémarrage exige de nouvelles capacités IPC. Le profil à deux applications fournit un écrivain et un lecteur sur des paires de canaux distinctes. Le service traite les canaux à tour de rôle. Un endpoint témoin appartenant à chaque application permet de détecter sa disparition et d'abandonner ses ressources privées. Le canal supplémentaire du banc sert à vérifier qu'un numéro de handle réellement ouvert chez le pair est refusé. Lors du premier essai à deux clients en EL0/512, le deuxième émetteur a expiré pendant la vérification du montage en cours. La bibliothèque utilise maintenant la même échéance de 15 000 ticks pour la soumission synchrone du message fichier et pour l'attente de sa réponse. Les deux échéances s'appliquent séparément. Le collecteur d'orphelins garde son seuil d'origine. Les droits utilisateur persistants, les ACL et la délégation de handles de répertoire entre applications restent hors de ce profil.

## Chemins et droits

Les requêtes acceptent de 1 à 1 024 octets UTF-8, avec au plus 255 octets par composant. Le parseur P8 historique admet jusqu'à 32 composants par opérande. Le service compare les octets sans transformation Unicode et distingue la casse. Le séparateur est `/`. NUL, les octets de contrôle ASCII, DEL, les composants vides et les séparateurs répétés sont rejetés. Le nom `.` désigne le répertoire parcouru. Le service refuse tout composant `..`, y compris sur une capacité de la racine. Les noms réservés sont exclus comme feuilles de mutation.

Le handle zéro désigne le CWD pour une opération relative. Un opérande absolu est accepté depuis l'autorité de montage de la session en passant zéro. Une capacité de répertoire explicite exige un opérande relatif. Chaque DIR_OPEN peut réduire les droits hérités. Les contrôles portent sur traversée (1), lecture de fichiers (2), énumération (4), création (8), suppression (16), renommage (32) et écriture de fichiers (64). LOOKUP_AT permet la consultation des métadonnées de l'objet nommé au titre de la traversée. STAT concerne un handle déjà acquis. Un renommage demande traversée, déplacement et suppression dans la source, traversée, déplacement et création dans la destination.

Le plafond sur disque est de 32 arêtes depuis la racine. Un chemin canonique peut atteindre 8 192 octets avec 32 noms de 255 octets et leurs séparateurs. La limite de 1 024 vient de l'argument de résolution. Une application peut enchaîner plusieurs DIR_OPEN, puis OPEN_AT. Aucun nom absolu cumulé n'intervient dans la lecture. GETCWD parcourt les liens parents pour l'affichage et retourne le texte par fragments. Il conserve temporairement au plus 32 noms. La pagination signale un changement de génération et exige une nouvelle lecture depuis le début.

## Trames et transactions

L'enveloppe fichier v2 reste de 128 octets, dont 88 de charge utile. Les sept opérations fichier v0.11.0 gardent leurs sélecteurs 1 à 7 et leur syntaxe. Les sélecteurs ci-dessous sont des opérations du service sur les mêmes appels IPC du noyau. Les entiers sont little-endian. La session figure à l'octet 8, le handle à 16, l'offset à 24 et le dernier argument à 32. Le statut de réponse est à l'octet 3 et la longueur utile à 4. [Contrat binaire](../abi/files.rs) · [ABI noyau](../abi/native.rs)

| Opération | Contrat |
|---|---|
| 8 PATH_FRAGMENT | Deux emplacements par session, choisis par flags 0 ou 1. L'offset zéro ouvre un argument, le total reste constant et chaque fragment doit suivre le précédent. L'opération finale consomme l'argument complet. Un chemin court peut aussi être envoyé en ligne. |
| 9 DIR_OPEN | Résout le chemin depuis la capacité de départ. Retourne un handle de répertoire portant le sous-ensemble demandé et un STAT de l'objet. |
| 10 LOOKUP_AT, 17 STAT | Métadonnées par nom ou handle. La charge de 25 octets contient ID, révision et taille sur 64 bits, puis type (1 fichier, 2 répertoire). |
| 11 OPEN_AT | Retourne un handle de fichier. Les bits lecture, écriture et création gardent les valeurs de OPEN. La publication d'un objet créé précède le succès. |
| 12 READDIR | Rend le prochain nom dans l'ordre de l'index après le nom de continuation, éventuellement vide pour commencer. L'offset permet de lire la suite d'un nom. La réponse indique génération et longueur totale. Une longueur totale nulle indique la fin de l'énumération. |
| 13 MKDIR_AT, 14 RMDIR, 15 UNLINK_AT | Création ou retrait typé d'une entrée. Un doublon, un répertoire non vide ou un objet retenu par un handle ou un CWD vivant donne une erreur. |
| 18 DIR_CLOSE | DIR_CLOSE retire la référence d'ouverture et le CWD conserve sa propre référence. |
| 16 RENAME_AT | Le handle source est à 16 et le handle de destination à 32. Les chemins occupent les deux emplacements. Le déplacement conserve l'ID et utilise une seule transaction dans le LV. |
| 19 CHDIR, 20 GETCWD | CHDIR conserve l'ID et les droits du répertoire. GETCWD renvoie sa représentation actuelle. Le client peut recommencer une lecture interrompue par un changement de génération sans perdre le CWD. |

Les erreurs de domaine incluent objet absent (20), corruption (21), publication incertaine (22), manque d'espace (23), occupation (24), opération inconnue (25), E/S (26), doublon (27), répertoire non vide (28), type répertoire attendu (29) et type fichier attendu (30). Les erreurs ABI historiques de requête, de droits, de limite de ressources et de révocation restent utilisées.

Les mutations réutilisent la préparation CoW, les réservations, les contrôles du catalogue et la publication F1/F2. Le déplacement valide l'ascendance de la destination et la hauteur restante du sous-arbre avant publication. Une destination existante distincte donne une erreur. Une demande dont la source et la destination désignent la même entrée existante suit le contrat de renommage P8. Les handles ouverts et les CWD continuent de désigner leur objet si un ancêtre change de nom. Les mouvements entre volumes sont exclus par la portée du montage. Une préparation WRITE privée bloque les mutations concurrentes. UNLINK retire l'entrée de façon atomique. Les extents encore à récupérer appartiennent à la file persistante de collecte P8. Lors de la dernière fermeture normale d'un montage inscriptible, le service draine ces lots dans des transactions de maintenance. Une coupure laisse un travail identifié pour un montage suivant. Les confirmés F2, les refus de pools P5/P6/P7 et le sauvetage explicite suivent les contrats antérieurs. [Durabilité](../../novaFS/docs/novafs-p1-durabilite.md) · [Scrub](../../novaFS/docs/novafs-p8-scrub.md)

## Oracles de validation

| Contrôles de la mission | Observation exigée |
|---|---|
| 1–5 | Une image macOS contient l'objet 8 et le motif `4e 6f 76 61`. Le client monte, crée un père et un sous-répertoire, puis crée, synchronise et relit un fichier. |
| 6–11 | READDIR rend un nom de 255 octets en trois fragments puis atteint la fin. Les deux clients utilisent des CWD distincts et un rendez-vous transmet un handle réel du pair. Son utilisation pour STAT échoue. `.` fonctionne. La traversée par `..` et le départ absolu depuis une capacité explicite sont refusés. |
| 12–18 | Un fichier ouvert est déplacé dans un autre répertoire et relu par son handle. Son unlink immédiat donne BUSY. Après CLOSE, la suppression réussit. Le CWD protège un répertoire même après DIR_CLOSE. Le banc vérifie aussi doublon, répertoire non vide, cycle et destination occupée. |
| 19–21 | Réutilisation d'un handle retraité, requête après révocation du canal, composant vide, UTF-8 invalide, droits insuffisants et octets réservés. Le lecteur reste en mesure de lire son fichier autorisé avant fermeture. Le profil de déconnexion vérifie aussi la survie du premier client après la sortie du second. |
| 22–25 | Le banc atteint les longueurs canoniques 1 023, 1 024, 1 025, 1 074, 4 096 et 8 192 octets. La demande cumulée de 10 001 s'arrête à la première arête manquante après 8 192 octets. Créer le 33ᵉ niveau donne LIMIT et laisse la génération intacte. Le test hôte compare aussi le support stable intégral avant et après le refus d'un déplacement trop profond. |
| 26–29 | EIO pendant écriture, F1 et F2. Arrêt forcé avant, pendant et après les barrières, puis après réception du succès de RENAME_AT. Le vérificateur Darwin contrôle les deux ancres, la génération, l'ID et les quatre octets lus par arêtes. Un nouveau processus QEMU monte en lecture seule et l'image entière reste identique. |
| 30 | Deux passages de la charge de 32 niveaux dans la même VM. Au second retrait, le contrôleur exige la même comptabilité mémoire qu'après le premier, une table des processus vide et zéro page IPC. |

Les valeurs de performances incluent l'émulation et la contention de l'hôte. Un arrêt forcé de QEMU laisse potentiellement en mémoire les écritures déjà acceptées par macOS. Les oracles couvrent les états autorisés par le disque virtuel et ses réponses de FLUSH. Une coupure électrique sur matériel physique demanderait un autre banc. [Images et injections](../tests/namespace.py) · [Lecteur hôte par ID](../tests/namespace_oracle.rs)

## Résultats du 11 novembre 2026

Campagne sous macOS 27.2 ARM64, Rust 1.99.1 et QEMU 11.1.3. Les invités utilisent TCG avec un vCPU, 256 Mio et un périphérique VirtIO-blk en cache writeback. Chaque architecture a exécuté les scénarios de stockage sur des secteurs logiques et physiques de 512 puis de 4 096 octets.

| Contrôle | x86_64 QEMU | AArch64 QEMU |
|---|---:|---:|
| Scénarios historiques de démarrage, ABI, processus, mémoire et IPC | 25/25 | 25/25 |
| Groupes bloc avec DMA, erreurs et révocation | 24/24 | 24/24 |
| Contrôles fichier v0.11.0 | 44/44 | 44/44 |
| ELF multipage et concurrence IPC, profils supplémentaires | 2/2 | 2/2 |
| Profils de collecte ciblés, cinq répétitions de chacun | 5 × proc-test et 5 × proc-lifecycle | 5 × proc-test et 5 × proc-lifecycle |
| Contrôles d'espace de noms 1 à 30 ci-dessus | réussis sur les deux géométries | réussis sur les deux géométries |

Les cinq répétitions de proc-lifecycle ont créé et collecté 640 processus par architecture. À la fin de chacune, 63 341 cadres étaient libres sous x86_64 et 62 773 sous AArch64, comme au début. Le groupe d'espace de noms journalise douze résultats par géométrie. Ils regroupent plusieurs assertions client et hôte. Sur chacune des quatre combinaisons, la lecture après redémarrage a retrouvé l'objet 8 et les octets `4e 6f 76 61` sous le chemin de 1 074 octets. Les handles successifs ont permis de lire le fichier au niveau 32 et à 8 192 octets.

Sur chaque combinaison invité/secteur, les trois coupures autour de F1 ont rendu l'ancien nom en G10. Les trois coupures autour de F2 ont rendu le nouveau nom en G11, tout comme la coupure après la confirmation au client. Les erreurs injectées sur l'écriture et F1 ont rendu G10. L'échec de F2 a été signalé au client comme publication incertaine et le remontage a retrouvé G11. Chaque issue a été vérifiée dans une nouvelle VM et avec le lecteur macOS de l'autre architecture. Les 28 coupures et les 12 injections d'E/S de cette campagne portent sur le renommage.

Sur Darwin ARM64 natif et x86_64 sous Rosetta, NovaFS a réussi **179 tests standards et 180 instrumentés par architecture**, incluant le nouveau test des arêtes après remontage. Les campagnes différentielles séparées ont exécuté **1 000 000 d'opérations chacune** sur 100 graines, avec 345 183 commits, 54 643 pénuries injectées et 1 000 remontages et audits. Elles ont duré 25,568 s en natif et 41,271 s sous Rosetta. La qualification inclut la récupération, les corruptions, les snapshots, les clones, la cryptographie et le scrub. L'interopérabilité P8 a également relu les images du producteur historique `a33cc66` sur les quatre combinaisons Darwin/secteur. L'oracle LZ4 a échangé 256 flux dans chaque sens et sur chaque architecture. NovaFS conserve v0.9.12. Les ajouts concernent l'accès au catalogue depuis un parent et les lectures/écritures par objet. Le format P8, les deux barrières et les 17 appels noyau conservent leurs contrats.

### Temps des opérations

Les durées sont exprimées en millisecondes. Chaque ligne donne un échantillon par opération dans le profil à 32 niveaux. La seconde session remonte le volume dans la même VM. OPEN_AT comprend la création d'un fichier. Le parcours est chronométré du premier handle aux 32 niveaux. Chaque opération dispose de deux observations par configuration. Un percentile par opération demanderait une campagne plus longue.

| Invité | Secteur | Passage | LOOKUP_AT | OPEN_AT | READDIR | MKDIR_AT | RENAME_AT | UNLINK_AT | Parcours 32 |
|---|---|---|---|---|---|---|---|---|---|
| x86_64 | 512 | initial | 31,648 | 205,982 | 0,634 | 144,258 | 183,417 | 170,477 | 29,811 |
| x86_64 | 512 | second | 32,800 | 218,079 | 0,700 | 145,810 | 202,820 | 184,744 | 30,462 |
| x86_64 | 4 096 | initial | 21,575 | 136,720 | 0,541 | 98,157 | 123,030 | 116,393 | 29,618 |
| x86_64 | 4 096 | second | 23,397 | 144,134 | 0,672 | 99,973 | 128,832 | 118,466 | 30,429 |
| AArch64 | 512 | initial | 32,974 | 216,251 | 0,472 | 156,678 | 193,643 | 193,476 | 25,679 |
| AArch64 | 512 | second | 33,090 | 218,775 | 0,455 | 153,755 | 191,939 | 181,327 | 26,273 |
| AArch64 | 4 096 | initial | 23,148 | 149,516 | 0,457 | 105,155 | 131,089 | 124,339 | 26,272 |
| AArch64 | 4 096 | second | 23,251 | 148,575 | 0,423 | 104,053 | 134,417 | 126,954 | 25,969 |

Les temps F1 et F2 en millisecondes proviennent de l'adaptateur du service lors du renommage. L'histogramme IPC aller-retour couvre l'ensemble de la session, y compris les fragments du transport bloc. Les percentiles IPC sont en microsecondes.

| Invité | Secteur | Passage | F1 (ms) | F2 (ms) | Échantillons IPC | IPC p50 (µs) | IPC p99 (µs) |
|---|---|---|---|---|---|---|---|
| x86_64 | 512 | initial | 0,604 | 0,234 | 206 411 | 114,251 | 179,538 |
| x86_64 | 512 | second | 5,815 | 0,212 | 206 923 | 114,586 | 180,065 |
| x86_64 | 4 096 | initial | 0,320 | 0,181 | 145 307 | 114,421 | 163,459 |
| x86_64 | 4 096 | second | 0,246 | 0,188 | 145 667 | 115,093 | 164,419 |
| AArch64 | 512 | initial | 0,573 | 0,228 | 206 411 | 114,688 | 180,224 |
| AArch64 | 512 | second | 0,576 | 0,209 | 206 923 | 114,688 | 180,224 |
| AArch64 | 4 096 | initial | 0,536 | 0,227 | 145 307 | 114,688 | 180,224 |
| AArch64 | 4 096 | second | 0,524 | 0,199 | 145 667 | 122,880 | 180,224 |

### Mémoire résidente dans les invités

Les quatre premières colonnes de mémoire sont en Kio. Le noyau comprend image, tables, piles noyau, tas, Buddy et pages IPC. Le service NovaFS comprend ses pages utilisateur et sa pile. Le total à F1 inclut aussi les autres processus et le DMA. Le pic total additionne le résident initial et le maximum de cadres supplémentaires simultanés depuis le lancement. Les allocations comptent les cadres de 4 Kio obtenus pendant la session. La dernière paire de colonnes permet de vérifier la restitution effective.

| Invité | Secteur | Passage | Noyau à F1 | NovaFS à F1 | Total à F1 | Pic total | Allocations | Cadres libres avant | Cadres libres après |
|---|---|---|---|---|---|---|---|---|---|
| x86_64 | 512 | initial | 2 852 | 616 | 3 844 | 3 844 | 364 | 63 341 | 63 329 |
| x86_64 | 512 | second | 2 852 | 616 | 3 844 | 3 844 | 352 | 63 329 | 63 329 |
| x86_64 | 4 096 | initial | 2 852 | 616 | 3 844 | 3 844 | 364 | 63 341 | 63 329 |
| x86_64 | 4 096 | second | 2 852 | 616 | 3 844 | 3 844 | 352 | 63 329 | 63 329 |
| AArch64 | 512 | initial | 2 736 | 600 | 3 716 | 3 716 | 361 | 62 771 | 62 768 |
| AArch64 | 512 | second | 2 736 | 600 | 3 716 | 3 716 | 358 | 62 768 | 62 768 |
| AArch64 | 4 096 | initial | 2 736 | 600 | 3 716 | 3 716 | 361 | 62 771 | 62 768 |
| AArch64 | 4 096 | second | 2 736 | 600 | 3 716 | 3 716 | 358 | 62 768 | 62 768 |

Le premier lancement laisse trois cadres de tables de pages sur les deux architectures et neuf cadres DMA supplémentaires sur x86_64. Sur AArch64, le framebuffer occupe en outre 469 cadres de RAM système (1 876 Kio) en dehors du compteur résident. Il porte la RAM totale retenue à 5 592 Kio à F1. Le framebuffer x86_64 utilise de la mémoire vidéo distincte. Le second lancement conserve le nombre de cadres libres initial, le bilan complet du Buddy et zéro page IPC après fermeture. La mesure porte sur les cadres suivis par NovaOS. La restitution a été contrôlée sur les charges décrites ici.

## Commandes de qualification

Depuis la racine du monorepo sur macOS ARM64 avec Rosetta et les outils du projet.

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test lifecycle-test block-test elf-test ipc-races file-test namespace-test || exit 1
    python3 novaFS/lab/p51-validation-suites.py "$arch" || exit 1
done
```

Le passage macOS comprend aussi ces deux contrôles de compatibilité indépendants.

```sh
python3 novaFS/lab/p8-validation.py cli --sortie /tmp/novafs-p8-interop.json
python3 novaFS/lab/p6-lz4-interop.py
```

Le banc d'espace de noms conserve les journaux et les compteurs invités sous `novaOS/build/bloc-ARCH-SECTEUR/`. Les phases peuvent être isolées depuis la racine avec `NOVA_ARCH=aarch64 python3 novaOS/tests/namespace.py --sector 512 --only recovery` après construction. Chaque essai utilise une image raw jetable.

Cette commande affiche les deux passages et les fréquences utilisées pour convertir les cycles en millisecondes.

```sh
rg 'ECHANTILLON |RAM phase=|IPC hist type=aller_retour|BLOC memoire|STOCKAGE restitution' novaOS/build/bloc-*/namespace-depth.serial
```

## Audits précédant l'arbitrage

Les observations et commandes qui suivent décrivent la base v0.11.0 antérieure à la résolution relative. L'arbitrage de livraison conserve P8 et ses 32 niveaux.

## Reprise depuis `6a5656c` — compatibilité de la profondeur

**La résolution depuis une capacité de répertoire est validée comme choix d’architecture.** Le point restant à arbitrer est la profondeur d’une arborescence persistante dans un seul LV. Un lecteur NovaFS v0.9.12 refuse le montage au-delà de **32 arêtes** depuis la racine, même si les objets, les noms et les deux générations ont des sommes correctes. La longueur canonique maximale du profil est donc **32 × (255 + 1) = 8 192 octets**, séparateurs compris. Un chemin canonique de 10 001 octets exige au moins 40 arêtes. Le découpage en requêtes de 1 024 octets préserve le budget IPC et laisse intact le contrôle du montage. [Contrat du format, catalogue et admission](../../novaFS/docs/novafs-disk-format.md) · [Vérification des objets vivants](../../novaFS/src/fs/check.rs)

### Origine des limites

| Couche | Contrôle observé | Conséquence |
|---|---|---|
| API publique et résolution | `components` refuse une entrée de plus de 1 024 octets et plus de 32 composants. `resolve` commence à l’objet racine. Le curseur de lecture mémorise facultativement un chemin déjà accepté. | La limite de 1 024 porte sur l’argument. Les appels successifs actuels recommencent à la racine. [Résolution](../../novaFS/src/fs.rs) · [Lecture et cache du chemin](../../novaFS/src/fs/data.rs) |
| Index et sérialisation | La clé de nom contient le parent u64, la longueur u16 et 1 à 255 octets UTF-8. Sa valeur est l’ID enfant u64. L’enregistrement d’objet porte le parent, le nom, le type et la révision. Le compteur `next_oid` interdit le recyclage d’un numéro au sein de la lignée validée. | Le moteur peut rechercher un enfant depuis un parent. La limite de niveaux internes du B+Tree concerne l’index. Le chemin canonique n’occupe aucun champ sur disque. [Records](../../novaFS/src/fs/record.rs) · [Comparateur et codec B+Tree](../../novaFS/src/tree.rs) |
| Écriture et montage | La création contrôle la profondeur du parent. Le renommage vérifie aussi les descendants lorsqu’il augmente la profondeur. Le montage contrôle les parents de chaque objet vivant dans chacune des deux générations admises. | Un écrivain qui produirait un objet de profondeur 33 créerait un volume refusé par les lecteurs actuels. Les mêmes champs binaires peuvent encoder cette chaîne, mais sa publication changerait le contrat de compatibilité. [Mutations](../../novaFS/src/fs.rs) · [Contrôle des générations](../../novaFS/src/engine.rs) |
| Adaptateur natif et IPC | Le cœur `no_std` partage les fonctions précédentes avec Darwin. Le `Device` transporte des blocs de 4 Kio. Les messages fichier v2 ont 128 octets dont 88 de charge utile, et OPEN limite actuellement son argument à une trame. | La fragmentation d’une requête pourra franchir 88 octets. La géométrie de secteur et les tampons de transport n’interviennent pas dans le calcul de profondeur. [Adaptateur](../user/novafs/device.rs) · [Trame fichier](../abi/files.rs) · [Client](../user/storage/fileapi.rs) |
| Tests historiques | Le scénario hôte V02 crée 32 répertoires, exige le rejet du suivant et exerce une entrée de 1 024 octets. | Le maintien des tests et de l’interopérabilité couvre un rejet observable. [V02](../../novaFS/tests/clients.rs) |

### Résultats de la nouvelle campagne

1. **Objet renommé de 820 à 1 074 octets.** Sur ARM64 natif et x86_64 sous Rosetta, en secteurs de 512 puis de 4 096 octets, la sonde mémorise l’ID du fichier avant le renommage. Après publication et remontage RO, elle suit cinq arêtes dans l’index des noms, vérifie chaque lien inverse et retrouve le même ID. La lecture de l’extent avec son checksum rend les quatre octets `4e 6f 76 61`. Aucun renommage inverse ni reconstruction du chemin complet n’intervient dans cette lecture. La sonde exerce les primitives internes du moteur avec le profil de production. La délégation IPC d’une capacité de répertoire reste à implémenter.
2. **Longueurs canoniques.** Pour chaque couple hôte/secteur, la CLI de production crée puis vérifie après remontage des images aux longueurs 1 023, 1 024, 1 025, 1 074, 4 096 et 8 192 octets. La lecture par chemin absolu retourne `Nova` dans les deux premiers cas puis `Invalid("chemin")` dans les quatre autres. Pour le témoin de 10 001 octets, le générateur expérimental assemble 40 composants, publie et vérifie le catalogue. Les deux lecteurs de production refusent ensuite ce volume au contrôle de profondeur.
3. **Écrivain, lecteur et reprise.** Le test déplace `/s`, contenant le fichier `f`, sous une chaîne de répertoires nommés `x`. Sous 30 répertoires, le fichier atteint la profondeur 32 et le renommage réussit. Sous 31 répertoires, l’opérande de destination fait 64 octets et compte 32 composants. L’écrivain de production refuse cette mutation avec `Limit("profondeur après renommage")`. Un générateur temporaire autorisant 40 niveaux publie la même mutation. Il relit `Nova` à la génération 36. Le vérificateur v0.9.12 refuse le montage normal avec `Corrupt("cycle ou profondeur du catalogue")`. Le sauvetage RO du prédécesseur explicitement choisi retrouve `/s/f` en G35. Une rétrogradation automatique vers cette ancienne génération ferait disparaître un renommage confirmé par le générateur.

| Invité de production | Secteur | 32 arêtes, G35 | 33 arêtes, G36 | 40 arêtes, chemin de 10 001 octets |
|---|---:|---|---|---|
| x86_64 | 512 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| x86_64 | 4 096 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| AArch64 | 512 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| AArch64 | 4 096 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |

Le bloc de commandes de reproduction ci-dessous a été exécuté depuis une extraction neuve de la base. Le banc compare chaque image octet pour octet après chacune des 16 VM, y compris lors d’une tentative RW refusée. Les images à 33 et 40 arêtes sont des contre-exemples fabriqués dans une copie temporaire du code. Le seul paramètre changé dans le générateur est `MAX_DEPTH`. Toutes les admissions QEMU ci-dessus utilisent le noyau et le service de la base. [Reproducteur](../tests/profondeur_p8.py)

### Régressions rejouées depuis la base de reprise

| Architecture | Scénarios historiques QEMU | Groupes bloc | Contrôles fichier v0.11.0 | ELF / courses IPC | macOS clients / coupures |
|---|---|---:|---:|---|---|
| x86_64 | 25/25 au premier passage | 24/24 | 44/44 | réussis | 9/9 et 3/3 sous Rosetta |
| AArch64 | Premier passage interrompu au scénario processus, puis 25/25 au rejeu complet | 24/24 | 44/44 | réussis | 9/9 et 3/3 en natif |

Lors du premier passage AArch64, le PID 14 avait annoncé sa sortie et le noyau a paniqué sur l’assertion `orphelin non collecté` de `proc-test`. Le test exige la collecte dans un délai de `HZ` ticks. Le même binaire et le même test ont réussi au rejeu. La cause de cet échec intermittent reste ouverte. Les contrôles bloc et fichier couvrent les deux géométries, les coupures et les erreurs de FLUSH. Les suites P8 complètes, instrumentées et millionnaires n’ont pas été relancées. Le moteur de production et ses codecs n’ont reçu aucune modification. [Assertion du noyau](../kernel/src/sched/process/demo.rs) · [Banc QEMU](../tests/demarrage.py)

### Décision à prendre avant implémentation

- **Conserver P8 en v0.12.0 (recommandé).** Autoriser les requêtes de 1 024 octets depuis une capacité pour atteindre tout objet admis, y compris les fichiers dont le chemin canonique fait 1 074, 4 096 ou 8 192 octets. Garder la borne persistante de 32 arêtes et qualifier le refus au-delà. Dans ce périmètre, un parcours répété de 100 ou 1 000 composants peut mesurer des appels successifs, mais ne démontre pas une arborescence de 100 ou 1 000 niveaux distincts.
- **Exiger plus de 32 niveaux distincts sur un même volume.** Autoriser une évolution explicite et détectable du profil de stockage. Elle devra conserver la lecture des images P8 et permettre aux anciens lecteurs de refuser le nouveau profil avant mutation. Remplacer la borne fixe par une détection de cycles et des budgets d’inspection vérifiables à l’admission, au renommage et à la récupération. Les modifications de format et toute migration demandent une décision séparée.

Dans les deux cas, la base d’une requête sera un handle lié à un montage, à une session authentifiée par son canal et à ses droits effectifs. L’ID sur disque sert à la recherche interne. LOOKUP_AT et OPEN_AT devront vérifier la traversée à chaque étape. Une capacité ouverte restera attachée à son objet si le renommage reste dans son périmètre. Un déplacement qui ferait sortir un objet ouvert ou le répertoire courant de ce périmètre devra être refusé avant publication. Toute remontée par `..` devra respecter la racine de confinement de la capacité, ou être refusée. CHDIR conservera l’identité en session et GETCWD pourra produire le texte par fragments. Le service utilisera les transactions existantes pour MKDIR, RMDIR, UNLINK et RENAME. Ces contrats portent sur l’implémentation à reprendre après arbitrage.

Les latences, les allocations et les pics mémoire des futures opérations `*_AT` restent à mesurer. Aucune mesure de montage négatif n’est présentée comme la performance d’une opération de répertoire native.

### Reproduire le contrôle de compatibilité

Depuis la racine de la livraison documentaire, sur macOS ARM64 avec Rosetta et les outils de développement du projet :

```sh
set -eu
base=6a5656cbc7dab65320aa417f4298b7e39c412d67
lab=$(mktemp -d)
git archive "$base" novaFS | tar -x -C "$lab"
python3 - "$lab" <<'PY'
from pathlib import Path
import sys
doc = Path("novaOS/docs/namespace-v0.12.0.md").read_text()
probe = doc.split("#### identite_p8.rs\n\n```rust\n", 1)[1].split("\n```", 1)[0]
with (Path(sys.argv[1]) / "novaFS/src/fs/tests.rs").open("a") as tests:
    tests.write("\n" + probe + "\n")
PY
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" "$lab/novaFS/lab/run" test --offline --lib \
        fs::tests::acces_par_identite_apres_renommage -- --exact
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run build --offline --bin novafs
    make -C novaOS ARCH="$arch" kernel
done
python3 - "$lab" <<'PY'
from pathlib import Path
import sys
f = Path(sys.argv[1]) / "novaFS/src/fs/record.rs"
old = "const MAX_DEPTH: usize = 32;"
s = f.read_text()
assert s.count(old) == 1
f.write_text(s.replace(old, "const MAX_DEPTH: usize = 40;"))
PY
NOVAFS_TARGET=aarch64-apple-darwin "$lab/novaFS/lab/run" build --offline --bin novafs
for arch in aarch64 x86_64; do
    python3 novaOS/tests/profondeur_p8.py "$arch" "$lab/novaFS/target/aarch64-apple-darwin/release/novafs"
done
```

<details>
<summary>Sonde de lecture par identité dans la copie temporaire du moteur</summary>

#### identite_p8.rs

```rust

#[test]
fn acces_par_identite_apres_renommage() -> Result<()> {
    use crate::device::FileImage;
    for secteur in [512, 4096] {
        let mut fs = Engine::format(FileImage::create(256, secteur)?, IDS)?;
        let noms = ["a".to_owned(), "b".repeat(250), "c".repeat(250), "d".repeat(250), "e".repeat(64)];
        let mut parent = String::new();
        for nom in &noms[..4] {
            parent.push('/');
            parent.push_str(nom);
            fs.mkdir(&parent)?;
        }
        let ancien = format!("{parent}/{}", noms[4]);
        assert_eq!(ancien.len(), 820);
        fs.create(&ancien)?;
        fs.write(&ancien, 0, b"Nova")?;
        let id = fs.stat(&ancien)?.id;
        let nouveau_nom = "z".repeat(255);
        fs.rename("/a", &format!("/{nouveau_nom}"))?;
        let fs = Engine::mount(fs.unmount()?, IDS, Mode::ReadOnly)?;
        fs.verify()?;
        assert_eq!(ancien.len() - 1 + nouveau_nom.len(), 1074);
        let mut objet = live(&fs, ROOT)?;
        for nom in core::iter::once(&nouveau_nom).chain(&noms[1..]) {
            assert_eq!(objet.stat.kind, Kind::Directory);
            let parent = objet.stat.id;
            let valeur = get(&fs, Family::Names, &name_key(parent, nom))?.expect("entrée présente");
            objet = live(&fs, number(&valeur)?)?;
            assert_eq!((objet.parent, &objet.name), (parent, nom));
        }
        assert_eq!((objet.stat.id, objet.stat.kind, objet.stat.size), (id, Kind::File, 4));
        let (debut, extent) = data::covering(&fs, id, 0)?.expect("extent présent");
        assert_eq!(debut, 0);
        let page = data::checked_page(&fs, extent.reference(0), 0, objet.stat.size)?;
        assert_eq!(&page[..4], b"Nova");
    }
    Ok(())
}
```

</details>

## Audit initial sur la base `24609a5`

**Décision de livraison.** Arrêt au contrôle d’intégrité de l’espace de noms sur la base `24609a52e64a5626527827babc912ba7731d70de`. Un renommage valide fait disparaître l’accès à un fichier par le moteur public, alors que son entrée reste énumérable et que le volume passe la vérification intégrale. Un renommage inverse restitue le même objet et les mêmes octets. Le choix à trancher porte sur l’accès aux arbres déjà publiés par NovaFS. NovaOS reste en v0.11.0 (MIT) et NovaFS en v0.9.12 (MPL-2.0).

## Témoin sur un volume autonome

Le fichier se trouve sous cinq composants UTF-8 ASCII. Son chemin commence par `/a`, puis contient trois noms de 250 octets et un nom de fichier de 64 octets. Le moteur n’utilise ni lien ni alias dans cet exemple. Le laboratoire suit le même fichier sur trois générations.

| Génération | Opération confirmée | Observation après montage |
|---|---|---|
| 7 | Écriture des quatre octets `4e 6f 76 61` dans l’objet 6 | Le chemin de 820 octets permet de lire `Nova`. |
| 8 | Renommage de `/a` à la racine, avec un nom de 255 octets | Le parent est accessible par un chemin de 1 009 octets. Sa liste contient le fichier. L’ancien chemin renvoie `NotFound`. Le nouveau chemin de 1 074 octets renvoie `Invalid("chemin")` pour `stat` et pour la lecture. Même sans le premier séparateur, il compte 1 073 octets. |
| 9 | Renommage inverse, dans une transaction distincte | Une nouvelle ouverture retrouve l’objet 6 et les quatre octets exacts. |

La limite de 1 024 octets est à l’entrée de `components`. Les opérations fichier publiques reconstruisent leur parcours depuis la racine. `rename` vérifie ses deux arguments, le cycle et la profondeur finale. Il examine la hauteur du sous-arbre si la destination est plus profonde. Le changement de longueur du préfixe à profondeur constante échappe à ce contrôle. Le vérificateur compare les index directs et inverses et remonte les parents jusqu’à la racine. Ces invariants restent vrais en G8. Le volume conserve ses données et ses deux ancres. C’est l’accessibilité logique d’un fichier publié qui régresse. [Résolution et renommage](../../novaFS/src/fs.rs) · [Lecture par chemin](../../novaFS/src/fs/data.rs) · [Vérification du catalogue](../../novaFS/src/fs/check.rs)

Un contrôle limité aux nouvelles requêtes NovaOS pourrait empêcher la création du témoin côté invité. Il laisserait le service incapable de lire le fichier sur un volume G8 importé de macOS, y compris si le client fournit deux chemins relatifs courts. Aujourd’hui, le moteur public part de la racine même en l’absence d’un séparateur initial. Fragmenter davantage les trames IPC ne change pas ce point.

## Résultats exécutés

| Hôte macOS | Secteurs | Contrôle à 1 024 octets | Cas à 1 026 et 1 074 octets | Contrôle après renommage inverse |
|---|---|---|---|---|
| ARM64 natif | 512 et 4 096 | Objet 6, lecture exacte | Nom listé, `stat` absolu et lecture même sans slash initial refusés | Objet 6, lecture exacte |
| x86_64 sous Rosetta | 512 et 4 096 | Objet 6, lecture exacte | Mêmes résultats | Objet 6, lecture exacte |

À 1 025 octets, l’entrée absolue est refusée mais la forme sans séparateur initial, longue de 1 024 octets, permet encore de lire le fichier depuis la racine. Ce cas frontière ne résout pas G8. Les seize combinaisons utilisent un fichier image temporaire, un démontage et un nouveau montage RO. Le moteur relit les deux générations. Un contrôle distinct par la CLI de production a extrait `Nova` depuis le slot de G7 et a reproduit le refus sur le slot de G8 dans les quatre configurations.

| Invité QEMU | Secteur | Écritures disque / FLUSH terminés à chaque arrêt G8 et G9 | Nouvelle VM RO après G8 | Nouvelle VM RO après G9 |
|---|---:|---:|---|---|
| x86_64 | 512 | 40 / 3 | Entrée listée, lecture refusée | Objet 6, `Nova` |
| x86_64 | 4 096 | 5 / 3 | Identique | Identique |
| AArch64 | 512 | 40 / 3 | Identique | Identique |
| AArch64 | 4 096 | 5 / 3 | Identique | Identique |

La sonde appelle le moteur dans le service Ring 3 ou EL0 d’une copie temporaire de v0.11.0. Le service bloc, le transport VirtIO et NovaFS proviennent de la base. L’hôte prépare G7. Une VM renomme le répertoire et s’arrête sur un marqueur **après** publication et contrôle. Le banc relève les compteurs via QMP, tue QEMU, vérifie l’image sur macOS et redémarre en RO. Il répète ce cycle pour le renommage inverse. Les trois FLUSH comprennent celui du montage RW et les barrières F1/F2. Dans les quatre couples, la sonde a tourné dans quatre VM successives et chaque remontage RO a laissé l’image identique octet pour octet. La CLI Rosetta a contrôlé les images de l’invité AArch64 et la CLI ARM64 celles de l’invité x86_64.

Les binaires de livraison v0.11.0, sans sonde, ont aussi monté en RO les quatre images G8 et renvoyé leur identifiant de volume via MOUNT et INFO. L’appel public RENAME, les neuf opérations de v0.12.0, les deux répertoires courants simultanés, leurs latences, la mémoire et les fuites de cette future interface restent à qualifier. Les compteurs QMP décrivent des arrêts de VM après les barrières, pas une panne électrique du support physique.

Les suites NovaFS ciblées ont réussi sur ARM64 natif et sous Rosetta, avec neuf tests du contrôleur de clients et trois tests de coupures par architecture. Le contrôleur macOS exerce deux racines privées et des handles mais résout lui aussi les chemins en reconstruisant une chaîne depuis la racine. Le code de NovaFS étant identique à la base, ces essais ciblés constituent la campagne hôte de cet audit. [Banc clients](../../novaFS/tests/clients.rs) · [Banc de coupures](../../novaFS/tests/crash.rs)

### Non-régression sur les binaires de livraison v0.11.0

| Invité | Scénarios historiques | Groupes bloc (deux géométries) | ELF et courses IPC | Contrôles du banc fichier (deux géométries) |
|---|---:|---:|---|---:|
| x86_64 | 25/25 | 24/24 | Les deux suites réussies | 44/44 |
| AArch64 | 25/25 | 24/24 | Les deux suites réussies | 44/44 |

Chaque banc fichier a vérifié 20 coupures, huit erreurs d’E/S avec remontage, l’admission des supports, les refus de pools et la restitution après deux sessions. Le chronométrage et la vérification des ressources de ces suites portent sur v0.11.0. Aucun résultat de ce tableau ne qualifie une opération publique de répertoire.

## Résultat de l’audit de l’interface

- **Processus et volumes.** Le service NovaFS monte un LV autonome. La couche de résolution peut vivre dans ce même processus utilisateur et déléguer les opérations persistantes au moteur. Une instance de montage et un seul écrivain doivent servir les différentes sessions. Les attaches de futurs montages pourront imposer RO et leurs propres droits. Un déplacement entre deux montages doit rendre une erreur explicite. [Intégration et périmètre v0.11.0](validation/novafs-natif-v0.11.0.md) · [Cœur portable](../../novaFS/docs/novafs-coeur-portable.md)
- **Catalogue et identités.** Le format contient déjà la racine (objet 1), les parents, les types, l’index `(parent, nom) → objet`, les révisions et un compteur d’objets croissant avec détection de débordement. `Stat.revision` change lors de certaines mutations. L’identité d’une ouverture doit comprendre l’incarnation du montage et l’objet. Une révision sert au contrôle de concurrence. Un numéro communiqué au client ne confère aucun droit. [Records P0](../../novaFS/src/fs/record.rs) · [Index et tri des noms](../../novaFS/src/tree.rs)
- **Canaux et sessions.** Le lanceur v0.11.0 donne à un seul client une paire privée de capacités IPC. Ses handles de fichier désignent actuellement un chemin dans une table locale de 32 entrées. Le service doit attribuer un état de session, ses droits, son répertoire courant et une table de handles à chaque paire privée. Il doit vérifier l’autorité de traversée sur chaque répertoire, y compris après `..`. Un identifiant dans le payload ne constitue pas une preuve d’identité du client. Un handle fermé ou d’une session révoquée doit rester caduc. [Service actuel](../user/novafs/files.rs) · [Lanceur](../kernel/src/sched/process/ipc/mod.rs) · [Sécurité VFS](../../novaFS/docs/vfs-security.md)
- **Transport et noms.** Les 17 appels système existants suffisent au transport. Un payload IPC de 128 octets laisse 88 octets dans la trame fichier v2. Un chemin, deux opérandes de renommage ou une réponse READDIR peuvent nécessiter plusieurs fragments bornés et propres à la session. Le contrat VFS retient UTF-8 exact, casse significative, absence de normalisation implicite, 255 octets par composant, 32 composants et 1 024 octets par chemin présenté. Rejeter NUL, les composants vides et le séparateur final sauf pour la racine. `.` et `..` sont réservés. Le parcours se fait de gauche à droite avec contrôle d’existence, de type et de droits avant le composant suivant. `..` reste à la racine de confinement quand elle est atteinte. La politique des caractères de contrôle de l’OPEN v2 devra être rendue explicite pour les noms P8 préexistants. [Trame fichier](../abi/files.rs) · [ABI native](../abi/native.rs) · [IPC](ipc-abi.md) · [Contrat des chemins](../../novaFS/docs/vfs-architecture.md)
- **Mutation et récupération.** MKDIR peut réutiliser `Engine::mkdir`. RMDIR et UNLINK doivent contrôler le type puis appeler `Engine::unlink` sous le même écrivain. Un répertoire non vide est refusé. La collecte reprend par curseur pour les fichiers qui détenaient des extents. RENAME réutilise une seule transaction, refuse une destination distincte déjà occupée et un déplacement cyclique. L’index des noms, les parents et les révisions sont publiés ensemble après F1 et F2. L’essai ne met en cause aucune des deux barrières. [Publication](../../novaFS/src/engine.rs) · [Réserve de maintenance](../../novaFS/docs/novafs-p3-espace.md) · [Récupération](../../novaFS/docs/novafs-p1-durabilite.md)
- **Consultation et concurrence.** LOOKUP résout un objet sous la base autorisée. STAT rend type, identité, taille et révision. READDIR utilise le curseur de nom du moteur et doit détecter une révision du répertoire modifiée entre deux pages. CHDIR doit conserver une identité de répertoire dans la session. GETCWD reconstruit le chemin visible depuis la racine de cette session. Pour la première implémentation, refuser par OCCUPÉ la suppression ou le déplacement d’un répertoire courant, d’un objet ouvert ou d’un de leurs ancêtres concernés. Appliquer cette règle à toutes les sessions sous le même écrivain avant publication. [Primitives existantes](../../novaFS/src/fs.rs) · [Règles de concurrence](../../novaFS/docs/vfs-architecture.md)
- **Compatibilité.** Les pools, snapshots, clones, unités comprimées, attestations et clés conservent leurs contrats historiques. Le service v0.11.0 expose uniquement le contenu du LV autonome. Cette expérience porte sur ce profil. [Partage CoW](../../novaFS/docs/novafs-p5-partage.md) · [Compression](../../novaFS/docs/novafs-p6-compression.md) · [Vérification P7](../../novaFS/docs/novafs-p7-verification.md) · [Scrub P8](../../novaFS/docs/novafs-p8-scrub.md)

## Arbitrage initial (option B autorisée lors de la reprise)

**Option A — borner le chemin canonique de tout objet à 1 024 octets.** Contrôler aussi les descendants au sein de la transaction, y compris lors d’un changement de nom à profondeur constante. Refuser avant F1 si la nouvelle longueur dépasse la borne ou si le budget d’inspection s’épuise. Appliquer la règle aux écrivains macOS et NovaOS. Définir une admission explicite pour les images historiques comme G8 et un accès de récupération RO. L’admission doit distinguer une limite d’accessibilité et une corruption de blocs.

**Option B — borner le chemin fourni à chaque requête et permettre la résolution depuis un répertoire identifié.** Conserver les entrées P8 existantes et ajouter au cœur portable un parcours depuis un objet répertoire validé dans l’instance de montage. Le service conserve l’autorité et les identités en privé. Un client peut atteindre l’objet 6 de G8 en plusieurs requêtes courtes. GETCWD peut rendre par fragments un chemin canonique allant jusqu’à 8 192 octets (32 noms de 255 octets et leurs séparateurs). Il faut distinguer cette borne de la longueur admissible d’une requête d’entrée.

**Recommandation soumise à validation — option B.** Elle permet de monter et de parcourir en lecture seule une image G8 déjà publiée, sans la renommer ni la migrer. Le format sait représenter les cinq objets sous la racine. L’arbitrage concerne le contrat de résolution, les opérations privées du moteur et GETCWD. Dans les deux options, conserver les identités lors du renommage, exercer la révocation sur deux clients, vérifier les refus avant publication, puis reprendre la matrice fonctionnelle et les mesures v0.12.0.

## Reproduction

Depuis la racine du monorepo, les trois blocs sources ci-dessous sont des sondes de qualification pour images jetables. Les commandes extraient la base Git dans un répertoire temporaire. Seule cette copie reçoit la sonde Ring 3 / EL0.

```sh
set -eu
lab=$(mktemp -d)
git archive 24609a52e64a5626527827babc912ba7731d70de novaOS novaFS | tar -x -C "$lab"
python3 - "$lab" <<'PYTHON'
import re, sys
from pathlib import Path
lab = Path(sys.argv[1])
doc = Path("novaOS/docs/namespace-v0.12.0.md").read_text()
for name, lang in (("temoin.rs", "rust"), ("sonde.rs", "rust"), ("qemu-audit.py", "python")):
    block = re.search(r"^#### " + re.escape(name) + r"\n\n`{3}" + lang + r"\n(.*?)^`{3}$",
                      doc, re.M | re.S)
    (lab / name).write_text(block.group(1))
f = lab / "novaOS/user/novafs/files.rs"
s = f.read_text()
line = "            w::set(a, 8, self.id);"
assert s.count(line) == 1
f.write_text(s.replace(line, "            sonde(self.fs.as_mut().unwrap(), self.writable)?;\n"+line)
             + "\n" + (lab / "sonde.rs").read_text())
PYTHON
for arch in aarch64 x86_64; do
    target="$arch-apple-darwin"
    NOVAFS_TARGET="$target" "$lab/novaFS/lab/run" build --lib --bin novafs --offline
    release="$lab/novaFS/target/$target/release"
    rustup run stable rustc --edition=2024 -C opt-level=2 --target "$target" \
        -L "dependency=$release/deps" --extern "novafs=$release/libnovafs.rlib" \
        "$lab/temoin.rs" -o "$lab/temoin-$arch"
    for secteur in 512 4096; do
        for longueur in 205 206 207 255; do
            "$lab/temoin-$arch" "$secteur" "$longueur" "$lab/$arch-$secteur-$longueur.raw"
        done
    done
done
for arch in x86_64 aarch64; do
    make -C "$lab/novaOS" ARCH="$arch" kernel
    case "$arch" in x86_64) hote=aarch64 ;; aarch64) hote=x86_64 ;; esac
    cli="$lab/novaFS/target/$hote-apple-darwin/release/novafs"
    for secteur in 512 4096; do
        python3 "$lab/qemu-audit.py" "$lab" "$arch" "$secteur" "$cli"
    done
done
```

Pour rejouer les suites v0.11.0 sur les sources de livraison, depuis la racine :

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test block-test elf-test ipc-races file-test || exit 1
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --test clients --test crash || exit 1
done
```

<details>
<summary>Sources des sondes de reproduction</summary>

#### temoin.rs

```rust
use novafs::{Engine, Error, Ids, Kind, Mode, device::FileImage};
use std::{fs::File, io::{Seek, SeekFrom}};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let secteur: u32 = args[1].parse()?;
    let cible: usize = args[2].parse()?;
    let ids = Ids { fs: [1; 16], lv: [2; 16] };
    let mut fs = Engine::format(FileImage::create(256, secteur)?, ids)?;
    let mut parent = String::from("/a");
    fs.mkdir(&parent)?;
    for c in ["b", "c", "d"] {
        parent += &format!("/{}", c.repeat(250));
        fs.mkdir(&parent)?;
    }
    let nom = "e".repeat(64);
    let ancien = format!("{parent}/{nom}");
    fs.create(&ancien)?;
    fs.write(&ancien, 0, b"Nova")?;
    let objet = fs.stat(&ancien)?.id;
    let destination = format!("/{}", "z".repeat(cible));
    fs.rename("/a", &destination)?;
    assert_eq!(fs.generation(), 8);
    let mut support = fs.unmount()?.into_file();
    support.seek(SeekFrom::Start(0))?;
    let mut copie = File::create_new(&args[3])?;
    assert_eq!(std::io::copy(&mut support, &mut copie)?, 256 * 4096);
    copie.sync_all()?;
    drop(copie);
    let ro = Engine::mount(FileImage::from_file(File::open(&args[3])?, 256, secteur)?, ids, Mode::ReadOnly)?;
    ro.verify()?;
    let nouveau_parent = format!("{destination}{}", &parent[2..]);
    let nouveau = format!("{nouveau_parent}/{nom}");
    assert_eq!(ro.stat(&nouveau_parent)?.kind, Kind::Directory);
    assert_eq!(ro.list(&nouveau_parent, None, 2)?, [nom]);
    assert!(matches!(ro.stat(&ancien), Err(Error::NotFound)));
    if nouveau.len() <= 1024 {
        assert_eq!(ro.stat(&nouveau)?.id, objet);
        assert_eq!(ro.read(&nouveau, 0, 4)?, b"Nova");
    } else {
        assert!(matches!(ro.stat(&nouveau), Err(Error::Invalid("chemin"))));
        if nouveau.len() == 1025 {
            assert_eq!(ro.read(&nouveau[1..], 0, 4)?, b"Nova");
        } else {
            assert!(matches!(ro.read(&nouveau[1..], 0, 4), Err(Error::Invalid("chemin"))));
        }
    }
    drop(ro);
    let mut rw = Engine::mount(FileImage::from_file(support, 256, secteur)?, ids, Mode::ReadWrite)?;
    rw.rename(&destination, "/a")?;
    let final_ro = Engine::mount(rw.unmount()?, ids, Mode::ReadOnly)?;
    final_ro.verify()?;
    assert_eq!(final_ro.stat(&ancien)?.id, objet);
    assert_eq!(final_ro.read(&ancien, 0, 4)?, b"Nova");
    println!("secteur={secteur} chemin={} objet={objet} générations=7,8,{} retour=Nova", nouveau.len(), final_ro.generation());
    Ok(())
}
```

#### sonde.rs

```rust
fn sonde(fs: &mut Engine<&IpcDevice<'_>>, ecriture: bool) -> rt::Result<()> {
    let ancien_parent = alloc::format!("/a/{}/{}/{}", "b".repeat(250), "c".repeat(250), "d".repeat(250));
    let dest = alloc::format!("/{}", "z".repeat(255));
    let parent = alloc::format!("{dest}{}", &ancien_parent[2..]);
    let nom = "e".repeat(64);
    let ancien = alloc::format!("{ancien_parent}/{nom}");
    let nouveau = alloc::format!("{parent}/{nom}");
    let court = |fs: &Engine<&IpcDevice<'_>>| -> rt::Result<()> {
        shape(fs.stat(&ancien).map_err(error)?.id == 6 && fs.read(&ancien, 0, 4).map_err(error)? == b"Nova")
    };
    let long = |fs: &Engine<&IpcDevice<'_>>| -> rt::Result<()> {
        shape(fs.list(&parent, None, 2).map_err(error)? == [nom.as_str()]
            && matches!(fs.stat(&ancien), Err(Error::NotFound))
            && matches!(fs.stat(&nouveau), Err(Error::Invalid("chemin")))
            && matches!(fs.read(&nouveau, 0, 4), Err(Error::Invalid("chemin"))))
    };
    let marque: &[u8] = match (fs.generation(), ecriture) {
        (7, true) => {
            court(fs)?;
            fs.rename("/a", &dest).map_err(error)?;
            long(fs)?;
            b"AUDIT G8 rename confirme"
        }
        (8, false) => { long(fs)?; b"AUDIT G8 entree listee lecture refusee" }
        (8, true) => {
            long(fs)?;
            fs.rename(&dest, "/a").map_err(error)?;
            court(fs)?;
            b"AUDIT G9 retour confirme"
        }
        (9, false) => { court(fs)?; b"AUDIT G9 objet 6 Nova" }
        _ => return Err(w::CORRUPT),
    };
    fs.verify().map_err(error)?;
    if ecriture { crate::qualification::stop(marque); }
    rt::value(rt::call(rt::DEBUG_WRITE, marque.as_ptr() as u64, marque.len() as u64, 0))?;
    Ok(())
}
```

#### qemu-audit.py

```python
from pathlib import Path
import re
import subprocess
import sys
import tempfile
base = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(base / "novaOS/tests"))
from bloc import Bench
arch = sys.argv[2]
secteur = int(sys.argv[3])
cli = str(Path(sys.argv[4]).resolve())
def outil(cmd, *args, data):
    return subprocess.run([cli, cmd, "256", str(secteur), *args], input=data,
                          capture_output=True, check=True).stdout
parent = "/a"
cmds = ["mkdir /a"]
for ch in "bcd":
    parent += "/" + ch * 250
    cmds.append("mkdir " + parent)
ancien = parent + "/" + "e" * 64
cmds += ["create " + ancien, "write " + ancien + " 0 4e6f7661"]
def oracle(image, attendu):
    contenu = image.read_bytes()
    ancres = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)",
                        outil("inspect", data=contenu).decode())
    assert len(ancres) == 2
    slot, gen, fs, lv = max(ancres, key=lambda x: int(x[1]))
    assert int(gen) == attendu
    outil("verify", fs, lv, data=contenu)
    if attendu in (7, 9):
        assert outil("salvage", fs, lv, slot, ancien, data=contenu) == b"Nova"
    return contenu
with tempfile.TemporaryDirectory(prefix="nova-noms-") as tmp:
    b = Bench(Path(tmp), arch, secteur)
    b.image.write_bytes(outil("script", data=("\n".join(cmds)+"\n").encode()))
    oracle(b.image, 7)
    for gen, mutation, lecture in [(8, "AUDIT G8 rename confirme", "AUDIT G8 entree listee lecture refusee"),
                                  (9, "AUDIT G9 retour confirme", "AUDIT G9 objet 6 Nova")]:
        b.run("files-inject", f"g{gen}-mutation", cut=mutation)
        avant = oracle(b.image, gen)
        s = b.run("files-info", f"g{gen}-reprise", readonly=True)
        assert lecture in s and "BLOC session terminee ok=true" in s, s[-1800:]
        assert b.image.read_bytes() == avant
        print(arch, secteur, gen, lecture, flush=True)
```

</details>
