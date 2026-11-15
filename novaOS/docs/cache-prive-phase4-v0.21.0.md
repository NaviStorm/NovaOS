# Cache privé persistant des applications — v0.21.0, phase 4

## Autorités et durée de vie

Le lanceur décide si une instance peut démarrer, et le gestionnaire des temporaires lui attribue son `tmp` sur P9. L’ouverture du cache fait l’objet d’une décision distincte. Sur le profil ordinaire, une application sans identité utilisateur attestée peut exercer son `tmp` et reçoit `DENIED` pour le cache. Elle peut poursuivre son travail. Aucun compte par défaut n’est provisionné.

L’autorité utilisateur doit attester un identifiant opaque de 128 bits. L’autorité d’installation doit attester un second identifiant de 128 bits, stable pour la lignée installée. Le courtier associe la paire au processus admis et écrase les champs de routage avant transmission à NovaFS. Le nom du programme, son chemin, une chaîne cliente, le PID et le BootID ne prouvent aucune de ces identités. La qualification embarque sa propre table de provisionnement dans les profils de test. La production attend un fournisseur d’attestations. [Courtier](../kernel/src/sched/process/cache.rs)

| Événement | Preuve exigée pour ouvrir le cache |
|---|---|
| Nouvelle instance, même installation | Attribution du processus au même utilisateur attesté et à la même installation. Délivrance d’une capacité propre à cette nouvelle instance. |
| Mise à jour | Confirmation de continuité par l’autorité d’installation et attribution au même utilisateur. |
| Réinstallation | Nouvelle vérification explicite des deux identités. En l’absence de cette preuve, refus et conservation de l’ancien cache sans attribution. |
| Restauration | Vérification externe de l’origine du volume, de la continuité de l’installation et du titulaire. Sur le profil de restauration non attestée, même une intention de purge valide reste intacte. |

Une restauration intégrale peut reproduire des octets, des identifiants et des sceaux parfaitement cohérents. Le service ne peut déduire la fraîcheur d’une telle image depuis ses seuls blocs. Un futur fournisseur devra attester l’origine du montage et documenter comment il conserve la continuité hors de l’image restaurée. Le responsable de l’installation et l’autorité utilisateur devront définir la rotation et la révocation de leurs attestations avant d’ouvrir le cache à des applications ordinaires.

## Supervision, stockage et capacités

Le gestionnaire de services lance le service 10 après la disponibilité du service 9. NovaFS, son chemin bloc et la création des temporaires fonctionnent en amont. Tant que la production ne dispose pas d’une autorité d’enrôlement, sa récupération laisse intacts les objets du magasin. Le service 10 démarre sans temporaire, demande la récupération à l’écrivain P9 et annonce ensuite `READY`. Le gestionnaire peut l’arrêter et le relancer. La perte du service invalide les vues qu’il supervisait. [Catalogue](../user/services/catalog.rs) · [Service](../user/services/cache_actor.rs)

Le courtier utilise le sélecteur additif `cache::CONTROL` du registre ABI v1. Les appels historiques aux fichiers et aux temporaires reposent sur d’autres autorités. Les étendre pour transporter librement des clés utilisateur ou installation donnerait à leurs clients un pouvoir d’attribution. Le courtier vérifie au contraire la provenance au point d’entrée, associe chaque opération à un ticket privé et réutilise le réveil IPC de l’unique écrivain NovaFS. Une vue ouverte fournit un espace de noms privé de fichiers plats. Le protocole accepte des noms UTF-8 de 1 à 48 octets et des fragments de données d’au plus 64 octets. Le backend rejette les séparateurs, les caractères de contrôle, les composants `.` et `..` et les opérations sur un objet du mauvais type. Les deux types d’objets natifs de P9 sont fichier et répertoire. [Registre](registre-abi.md) · [Contrat filaire](../abi/cache.rs) · [Interface côté NovaFS](../user/novafs/files/cache.rs)

La paire attestée désigne un répertoire réservé. L’OID P9 de ce répertoire distingue chaque incarnation du cache. Deux processus autorisés pour la même paire obtiennent des jetons distincts pour ce même OID. Le noyau vérifie l’instance appelante, le BootID, le numéro de capacité et la génération. Retirer un processus révoque sa vue. La purge révoque toutes les vues de sa portée. L’API expose exclusivement des opérations relatives à la capacité. Les données de cache sont reconstructibles. Les documents utilisateur restent dans le catalogue public.

### Appel d'une application

L'appelant présente au sélecteur `CONTROL` l'opération en premier argument, zéro en deuxième argument, et en troisième argument l'adresse de sa trame privée de 224 octets. Pour `OPEN`, initialiser toute la trame à zéro. En cas de succès, conserver les 32 octets de la génération, de la capacité et du BootID. Pour `READ` et `WRITE`, recopier ces octets dans une trame neuve, remplir les longueurs, le nom, l'offset et, pour une écriture, le fragment de données. Les offsets et les codes exacts proviennent du module partagé. Un client doit vérifier le statut avant d'exploiter les octets de réponse. Une opération de cache ne confère pas un handle VFS général.

L'autorisation de consultation du gestionnaire permet d'interroger l'état du service sans provoquer d'éviction. La maintenance périodique est interne au service. Une demande explicite d'entretien passe par le port d'administration. [Protocole de supervision](../user/services/protocol.rs) · [Exemple d'utilisation par un client isolé](../user/services/cache_actor.rs)

## Purge et récupération

Pendant le prêt d’un canal VFS à une commande de nsh, l’écrivain continue de servir le temporaire et le cache, par lots bornés. La restitution du prêt remet le canal public au lanceur.

À l’admission de la purge, le courtier interdit toute ouverture conflictuelle, retire les vues et annule les opérations encore en file. P9 termine les opérations déjà remises avant d’effectuer le retrait demandé. Un acquittement confirmé conserve son résultat. Une interruption après remise d’une écriture dont le résultat reste indéterminé produit `UNCERTAIN`. Le client doit traiter cette incertitude sans répétition automatique. Une nouvelle ouverture après purge résout un autre OID. Une écriture liée à l’ancienne génération ne peut ainsi cibler le répertoire recréé.

Sur un volume enrôlé, l’écrivain vérifie le sceau du magasin, le marqueur de la paire et le type des descendants avant de publier l’intention de purge. Il renomme le marqueur actif en tombe avec la publication CoW habituelle de NovaFS. La reprise peut poursuivre un retrait interrompu avec cette tombe et la génération correspondante. Une provenance incohérente entraîne la quarantaine. Les sceaux structurels relient les identifiants FS/LV, le répertoire, le marqueur et le propriétaire. Ils n’authentifient pas un opérateur disposant d’un accès brut en écriture au volume. [Magasin P9](../user/novafs/files/cache_store.rs)

Pour les purges d'utilisateur ou du système, le registre conserve aussi une intention durable avec la portée et la borne des générations admises. Le service valide les marqueurs avant de publier cette intention. Une incohérence découverte après sa publication conserve le gel de la portée incertaine. Une incarnation créée ultérieurement est exclue de cette portée. Un premier `WRITE` sur un nouveau nom prépare une entrée inaccessible au client puis publie par renommage après l'écriture des octets. Une coupure de VM se traduit ainsi par un nom absent ou un fichier complet pour le fragment confirmé. Chaque appel `WRITE` a sa propre confirmation. Un fichier constitué de plusieurs appels n'est pas une transaction globale.

## Quotas et pression

Les quatre paramètres de la politique sont le plafond logique d’une paire, celui de l’utilisateur, le budget logique global et le nombre de blocs physiques réservés. La facturation comprend une page par incarnation, une page par nom et la longueur logique du fichier. Un fichier creux reste facturé sur sa longueur logique. Le service contrôle aussi le solde physique après préparation de l’écriture CoW. La politique embarquée se calcule depuis la capacité du volume et sa réserve de maintenance. Le service 10 peut fournir une politique explicite lors de sa récupération.

Les blocs du moteur font 4 096 octets sur les deux géométries de secteur. Le profil réserve en plus du budget de maintenance P9 la place d'un plan CoW de 64 blocs. Il attribue au budget logique global la moitié de la capacité restante après ces réserves, au plafond d'un utilisateur la moitié du budget global et à celui d'une paire le quart du budget global. Une politique explicite doit conserver l'ordre des plafonds et une réserve physique positive. L'admission des nouvelles données est suspendue si un marqueur empêche d'établir une comptabilité sûre.

Lorsque le solde physique devient inférieur à la réserve augmentée du budget de préparation CoW, le service de maintenance interroge P9 et reçoit une proposition de victime. Le courtier gèle et révoque la portée correspondante. P9 vérifie à nouveau la pression et la génération avant de retirer les données. Les purges de l’utilisateur et du système requièrent une autorité administrative distincte. Le profil ordinaire de `nsh` ne la reçoit pas.

## Qualification

Le banc démarre le code invité sur un disque VirtIO brut P9. Il conserve le noyau, les ISO, les séries et les images comprimées de chaque étape. Un lecteur hôte remonte en lecture seule, vérifie le moteur P9, compare les OID et les octets des caches et contrôle le catalogue public. L’interruption par SIGKILL exerce l’arrêt de QEMU sur un hôte qui conserve son alimentation. [Banc invité](../user/cache_client_test.rs) · [Pilote QEMU](../tests/cache_prive.py) · [Oracle indépendant](../tests/tmp_prive_oracle.rs)

### Résultats du 15 novembre 2026

La phase 3 est intégrée sur `origin/main` par `80a3699c76af0db9ccdc1a85964fe15576c32439`. Son arbre `b7b167aa7951f24dcb4174817f86b5e3c991c576` est identique à celui du candidat `aa2c44a81e3ce444a75db40efa91f91f68b093a4`. Les 502 membres des deux archives de qualification ont été vérifiés. Le registre de cette fusion conserve les 46 sélecteurs antérieurs et ajoute le seul sélecteur de supervision.

La qualification du cache porte sur QEMU 11.1.3 TCG, VirtIO-blk, Q35 x86_64 et Cortex-A72 AArch64. Les noyaux figés portent les SHA-256 `6160e9a28a0cc04b0a63a7f44687161ff0af70ac1f107601969eb02ee774468c` (ARM) et `9f7a19bbdb67ac5c1c93820f16b07c6a69a2c106cd6257adc3aad6f3c846f7fd` (x86). Les images de test sont fictives. Le noyau, le SDK distribué et NovaFS conservent respectivement leurs manifestes 0.20.0, 0.2.0 et 0.9.13.

| Parcours | Étendue et résultat observé |
|---|---|
| Création, accès et cycle de vie | Dans les 12 cellules ISA × 1/2/4 vCPU × 512/4096, deux instances de la même paire partagent les octets avec des capacités différentes. Le crash, la destruction et le relancement d’un acteur préservent le cache et les droits indépendants de l’autre acteur. |
| Séparation, traversée et usurpation | Dans chaque cellule, rejet d’un jeton étranger, d’une attribution fournie par le client et des noms hors de l’espace privé. Les opérations de renommage et de création de lien ne sont pas exposées. Deux applications et deux utilisateurs conservent des témoins distincts. Un client de `nsh` sans attestation a effectivement créé un fichier dans son temporaire et reçu `DENIED` à l’ouverture du cache sur les deux ISA et les deux géométries. |
| Opération en vol, retrait et recréation immédiate | Dans les 12 cellules, le banc a attendu le `NEXT` réel de l’écriture avant de faire admettre la purge par l’autre instance. L’écriture remise a été acquittée dans l’ancienne génération. Les anciens jetons ont été refusés. La réouverture a produit une nouvelle génération vide. Le fichier de l’écriture engagée était absent. Les purges de paire, d’utilisateur et de système ont été contrôlées avec et sans autorité. |
| Service et canaux VFS | Perte et redémarrage du service 10, retrait de ses anciens droits, nouvelle ouverture et lecture. Pendant un prêt VFS actif, l’acteur a écrit et relu son cache et exercé son temporaire. Les deux canaux prêtés ont ensuite été retirés et le cache est resté accessible. |
| Coupure et persistance | Une interruption de QEMU par cellule après publication de la tombe, avant le premier retrait. L’oracle a relu la tombe et les anciennes données sur les 12 disques. Le redémarrage a terminé la purge, puis seul le cache ciblé a été recréé. Les autres OID et octets ont été préservés. Les jetons du démarrage précédent ont été refusés. |
| Mise à jour, réinstallation et restauration | Les 12 mises à jour attestées ont retrouvé les témoins. Les 12 réinstallations sans preuve ont laissé le magasin intact. Les 24 restaurations non attestées couvrent un magasin actif et un magasin avec tombe. Puis quatre nouvelles VM, sur des copies de ces disques et avec attestation explicite, ont démontré la réadmission. Sans tombe les générations 93/87/85 ont survécu. Avec tombe, A est passé de 93 à 168 et a rouvert vide, B et C ont gardé leurs générations et octets. |
| Propriétaire incohérent | Deux copies P9 saines au sens du moteur ont été altérées via l’API de stockage. L’unique octet modifié se trouve dans le marqueur U1/A. Dans chaque invité, refus pour cet objet, autres témoins lisibles, catalogue du cache inchangé après l’essai. |
| Quotas et espace physique | Les deux programmes hôtes sur les deux géométries ont distingué plafond d’une paire, d’un utilisateur, plafond global et zéro bloc disponible pour les caches. En VM, le dépassement de quota est refusé. Avec deux vCPU, ARM/4096 et x86/512 ont écrit 823 296 octets dans un fichier public. Le solde est passé de 334 à 127 blocs disponibles, puis à 144 après une seule éviction. L’ancienne capacité de la victime est restée invalide, les deux autres capacités ont continué de fonctionner et tous les octets du fichier public ont été relus. |
| Injection dans P9 | 200 interruptions après des FLUSH et 64 erreurs de FLUSH, réparties sur ARM64 natif, x86_64 sous Rosetta et les deux géométries. Ont été vérifiés la première écriture, la purge ciblée, la purge administrative et la reprise. Avant publication du nom, le client reçoit un échec certain. Une défaillance à la dernière barrière de publication est incertaine. Une seconde requête sur l’écrivain défaillant est refusée avant toute E/S. |

Les **104 démarrages consacrés au cache** comprennent 96 démarrages de la matrice de base, deux essais de corruption, deux de pression et quatre réadmissions. Les contrôles complémentaires couvrent dix démarrages de supervision, seize refus du lanceur, quatre sessions publiques P9 et seize sessions historiques sur P8, dont la lecture sans progression de génération. Les deux essais de `vi` ont enregistré et relu 28 octets UTF-8, vérifiés par remontage P9 indépendant. Neuf groupes de contrôles ABI et treize tests de politique par architecture hôte ont réussi. Les quatre ELF v0.20.0 archivés ont aussi fonctionné sans recompilation sur leurs profils de sonde construits avec ces sources. L’archive du SDK 0.2.0 conserve le SHA-256 `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Le registre compte 48 sélecteurs, dont les 47 historiques inchangés.

| Mémoire des arrêts normaux de la matrice cache | AArch64 | x86_64 |
|---|---:|---:|
| Pic de cadres physiques au cours des scénarios, min–max | 941–1 339 | 939–1 337 |
| Pic de cadres du scénario fonctionnel à 1 / 2 / 4 vCPU | 1 151 / 1 147 / 1 147 | 1 156 / 1 145 / 1 145 |
| Pic des allocations dynamiques du courtier | 1 344–1 696 octets | 1 344–1 696 octets |
| Allocations du courtier après drainage | 0 octet | 0 octet |

Un cadre mesure 4 096 octets. Les mesures par cellule distinguent les cadres du système des octets réservés par les tables dynamiques du courtier. Après les arrêts normaux mesurés, les classes processus, IPC, piles, utilisateur, tas et métadonnées actives reviennent à leur état initial. Le banc vérifie séparément le résidu de plateforme DMA/tables. Une VM tuée avant arrêt ne peut pas effectuer cette mesure finale. [Mesures et cellules](validation/cache-phase4-v0.21.0.json) · [Journaux, résultats et remontages](validation/cache-phase4-v0.21.0-preuves.tar.gz)

### Reproduction d'une cellule

Depuis le worktree de phase 4, utiliser un dossier de preuves neuf par cellule. Compiler le lecteur et l’outil de corruption avec le même moteur que le laboratoire. Une exécution avec `--pression` utilise un volume distinct de la matrice ordinaire.

```sh
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
Q="$PWD/novaOS/build/rejeu-cache-phase4"
mkdir -p "$Q"
make -C novaOS ARCH=aarch64 kernel
env NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --lib --bin novafs
lib="$PWD/novaFS/target/aarch64-apple-darwin/release"
for outil in tmp_prive_oracle cache_corruption_fixture
do
    rustup run stable rustc --edition=2024 --target aarch64-apple-darwin \
        -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
        "novaOS/tests/$outil.rs" -o "$Q/$outil"
done
python3 -B novaOS/tests/cache_prive.py --arch aarch64 --vcpus 2 --secteur 4096 \
    --oracle "$Q/tmp_prive_oracle" --alterer "$Q/cache_corruption_fixture" --preuves "$Q/arm-2-4096"
novaOS/tests/cache_stockage.sh aarch64
novaOS/tests/cache_stockage.sh x86_64
```

Le dossier public contient 510 fichiers de preuves relus, plus leur index SHA-256. L’archive complète de travail conserve en outre les ISO, les noyaux et les essais de mise au point dans un répertoire durable extérieur au worktree. Les limites de ce profil comprennent le catalogue d’identités de laboratoire, le parcours des objets pour la comptabilité et la confiance dans l’écrivain du montage. La qualification sur matériel physique, une coupure électrique et l’admission de comptes réels demeurent ouvertes. La phase 4 attend l’arbitrage d’intégration MOA/MOE.
