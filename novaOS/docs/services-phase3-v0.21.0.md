# Supervision des services système · NovaOS v0.21.0, phase 3

## Démarrage et temporaire privé

Le profil P9 ordinaire lance automatiquement trois services en espace utilisateur. Le profil `cli` exerce le même lanceur (mode 41) que le démarrage sur la partition système GPT. Le noyau admet les processus, limite leurs droits et observe leur fin. Un ELF distinct, le gestionnaire, décide des dépendances, des délais et des relances. Les repères de construction restent NovaOS 0.20.0, SDK Rust 0.2.0 et NovaFS 0.9.13 P9. [Lanceur de stockage](../kernel/src/sched/process/storage.rs) · [Chemin du disque système](../kernel/src/hardware/block/qualification.rs) · [Gestionnaire](../user/service_manager.rs)

| Étape | Composants | Condition pour progresser |
|---|---|---|
| Amorçage minimal sans temporaire | Chargeur, noyau, HAL, BootID et courtier de capacités | Le noyau établit l’isolation et les droits de la session bloc. Le BootID doit être qualifié pour les nouvelles instances durables. |
| Fournisseurs sans temporaire | Pilote VirtIO-blk utilisateur, relais bloc, serveur de fichiers NovaFS | Les trois ELF reçoivent leurs canaux de démarrage ensemble. Les opérations suivent la chaîne NovaFS → relais → pilote. NovaFS monte P9, vérifie le registre privé et traite les demandes de création. Il reste l’unique écrivain. |
| Clients exigeant un temporaire dès l’entrée | Gestionnaire et lanceur de session `nsh` | Le courtier prépare chaque bail lors de l’admission du groupe. La barrière avant l’entrée en espace utilisateur attend l’acquittement de NovaFS. Une création refusée empêche l’entrée. |
| Services supervisés exigeant un temporaire dès l’entrée | Horloge UTC (1), contrôle du temporaire (2), information de stockage (9) | Chaque principal obtient son propre bail P9. Chaque ELF énumère sa racine vide avant son annonce. Le gestionnaire peut soumettre plusieurs lancements indépendants avant de recevoir la première annonce. |
| Attribution après initialisation | Travailleur du service 2 | Le principal déjà entré en espace utilisateur peut lancer un travailleur ordinaire privé du droit, puis un autre avec délégation explicite de sa propre vue. Aucun service principal de ce catalogue ne demande de rattachement tardif. |

Les fournisseurs du deuxième rang peuvent donc créer le premier temporaire avant toute instruction du gestionnaire. Leur démarrage ne dépend ni d’un service supervisé ni de l’horloge 1. Le profil historique P8 démarre son ancienne chaîne, sans écriture de temporaire. [Contrat du temporaire](contrat-tmp-prive-v0.21.0.md) · [Barrière d’entrée](../kernel/src/sched/process/tmp.rs)

## Service existant et clients

Le service d’information de stockage sans volume traitait déjà les messages de huit octets `INFO` et `FIN`. Son état de session et sa réponse native de douze octets sont maintenant partagés entre l’ELF historique et l’ELF supervisé. Le service 9 accepte l’enveloppe sur le canal de son incarnation. Après l’annonce authentifiée, le gestionnaire lui adresse un véritable `INFO` et attend la réponse à son tag, le statut de succès et l’identité `NVFS` avant d’admettre `READY`. Chaque requête `QUERY` d’un client envoie un nouvel `INFO` au processus. Pendant l’arrêt de ce service, les couches bloc et NovaFS continuent à servir le volume. [Service existant](../user/storage/service.rs) · [Traitement INFO/FIN](../user/storage/info_session.rs)

Le service 1 répond avec l’heure UTC native. Le service 2 vérifie la vue temporaire et les règles de délégation aux enfants. Les services supplémentaires 3 à 8 appartiennent exclusivement au catalogue des essais d’incidents. Le profil de maintenance utilise les trois entrées de production et un droit d’administration explicite. [Catalogue](../user/services/catalog.rs) · [Boucle des services](../user/services/node.rs)

## Autorités, identités et arrêt

Le quadruplet de mots de 64 bits encode le BootID de 128 bits, l’InstanceID et la génération. Le numéro d’essai, l’ID stable du catalogue, le PID et l’autorité de lancement sont distincts. La racine de confiance est le catalogue incorporé au gestionnaire et l’admission des ELF par le noyau. L’autorité enregistrée vaut 1 pour la politique de démarrage et 2 pour une décision reçue sur le port d’exploitation. Ces valeurs désignent des autorités de session. L’identité d’un utilisateur humain relève d’un autre jalon.

Le gestionnaire possède deux paires IPC, l’une pour les consultations et l’autre pour les requêtes d’exploitation. Chaque incarnation possède en outre les racines de ses deux canaux privés, limités à la direction nécessaire après câblage. Les délégations excluent `GRANT` et `MANAGE`. Le gestionnaire compare le canal réel de réception, le service, l’essai et les 32 octets d’identité avec le droit de suivi courant. Un tag corrèle une réponse. La fin du propriétaire révoque ses dérivations. Le nouveau processus reçoit deux nouveaux canaux et un nouveau droit de suivi. Le gestionnaire vérifie les anciens handles en réception et en notification avant de permettre la relance. Il possède sa propre vue temporaire et aucun canal général sur les fichiers publics. [Mécanismes du noyau](../kernel/src/sched/process/ipc/service.rs) · [Révocation IPC](ipc-security.md)

Les déclarations versionnées, les neuf états, les arêtes obligatoires et optionnelles, l’ordre inverse d’arrêt et les trois politiques de relance sont définis dans la spécification de politique. Le catalogue de production admet deux relances automatiques par fenêtre glissante de 12 s, à partir d’un délai de 100 ms. Le démarrage a une échéance de 5 s, la demande d’arrêt de 800 ms et le contrôle de présence de 3 s. [Déclarations et transitions](services-politique-v0.21.0.md)

La sortie de session demande l’arrêt des dépendants avant leurs fournisseurs. Un refus d’arrêt atteint son échéance puis entraîne le retrait du processus. La mort du gestionnaire révoque les ports clients et entraîne l’arrêt de ses familles par le noyau. La fermeture globale par le lanceur initial attend les destructions d’instances et les acquittements de purge P9. Une erreur ou une expiration de cette attente remonte au client. La reprise au démarrage suivant applique les règles déjà qualifiées de vérification des marqueurs. [Protocole du temporaire](contrat-tmp-prive-v0.21.0.md)

## Contrôle

La trame de contrôle v1 contient seize entiers little-endian de 64 bits. La requête place la version, l’opération, le service et l’argument dans les quatre premiers mots, laisse les suivants à zéro et emploie un tag non nul. La réponse conserve le tag. Le mot 1 porte le statut natif. Les mots 2 à 9 d’une réponse d’état portent l’ID, l’état, l’essai, le PID et l’identité complète. Le canal de consultation autorise `LIST`, `STATUS`, `DEP` et `QUERY`. Le canal d’exploitation permet aussi `REGISTER`, `START`, `STOP`, `RESTART`, `SHUTDOWN` et la lecture du journal. `INJECT` exige le profil de qualification. [Trames](../user/services/protocol.rs) · [Répartiteur utilisateur](../user/services/control.rs)

Dans une session P9 ordinaire, `@services list`, `@services status 9`, `@services info 9` et `@services dep ID INDEX` utilisent le port de consultation. L’index de dépendance commence à zéro. La réponse DEP donne l’ID du fournisseur au mot 10 et son caractère obligatoire (1) ou optionnel (0) au mot 11. Le rôle `services-admin` permet aussi `@services stop 9`, `@services start 9` et `@services restart 9`. Une demande de démarrage ou d’arrêt est asynchrone. Consulter l’état jusqu’à `RUNNING` (5) ou `STOPPED` (7). L’entrée interactive `@services` est provisoire. [Interface de nsh](../../components/shell/services.rs)

Le rejet d’une trame avant authentification évite toute transition. Le banc compte les trois tentatives périmées dans le statut du service. Les demandes de transition invalides traitées par le moteur produisent un événement `Invalid`.

Le journal en mémoire du gestionnaire conserve 32 événements détaillés, numérotés par curseur croissant. Il indique identité, tentative, ancien et nouvel états, cause, sortie et dépendance. Un curseur évincé est refusé. Aucune donnée du temporaire n’est enregistrée. L’ABI native v1 ajoute le seul sélecteur `instances::SERVICE_PROCESS` (`0x1007b`) pour lancer un ELF admis, observer sa famille, l’arrêter, lire l’heure monotone de supervision ou demander le drainage. Le noyau réserve les quatre premières opérations au gestionnaire et la dernière au lanceur initial de cette session. [Registre ABI](registre-abi.md) · [Mécanismes](../abi/service_process.rs)

## Reproduire une cellule

Depuis le worktree de phase 3, affecter `Q` à un répertoire de preuves neuf et durable dans ce worktree. Configurer le répertoire temporaire selon les consignes du dépôt. Le banc conserve le noyau utilisé, les images P9 comprimées et les journaux par démarrage.

```sh
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR"
Q="$PWD/novaOS/build/supervision-rejeu"
mkdir -p "$Q"
make -C novaOS ARCH=aarch64 kernel
env NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --lib --bin novafs
lib="$PWD/novaFS/target/aarch64-apple-darwin/release"
rustup run stable rustc --edition=2024 --target aarch64-apple-darwin \
  -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
  novaOS/tests/tmp_prive_oracle.rs -o "$Q/lecteur"
python3 -B novaOS/tests/services.py --arch aarch64 --vcpus 2 --secteur 4096 \
  --oracle "$Q/lecteur" --preuves "$Q/aarch64-2-4096"
```

Le même banc accepte `x86_64`, 1, 2 ou 4 vCPU et des secteurs de 512 ou 4 096 octets. Le profil ordinaire est lancé sur un volume VirtIO jetable. L’arrêt forcé du processus QEMU exerce la récupération des temporaires après perte de la VM. La coupure électrique de l’hôte, le matériel physique et UTM graphique gardent un périmètre de qualification distinct.

## Qualification du 15 novembre 2026

La campagne QEMU 11.1.3 utilise TCG, VirtIO-blk sur image brute, Q35 x86_64 et Cortex-A72 AArch64. Chaque cellule traite le même volume P9 depuis sa création jusqu’à la dernière relecture. Les douze cellules réunissent **52 démarrages**. Les douze coupures par SIGKILL de QEMU interviennent après une vraie réponse INFO dans une session ordinaire. La relecture indépendante trouve alors les cinq paires marqueur/racine du gestionnaire, de nsh et des trois services. Les trois autres parcours par cellule couvrent l’exploitation des services, la perte du gestionnaire et le redémarrage de la console. Avec deux vCPU s’ajoute le contrôle interactif de maintenance. Chaque arrêt normal requiert l’acquittement de tous les vCPU attendus. [Banc invité](../user/service_control_test.rs) · [Pilote QEMU](../tests/services.py) · [Oracle P9](../tests/tmp_prive_oracle.rs)

| Critère bloquant | Preuve obtenue sur les deux architectures |
|---|---|
| Service existant utilisable | Dans le profil ordinaire, les trois services atteignent RUNNING et nsh interroge le vrai service de stockage 9. Dans le profil de maintenance, une nouvelle commande INFO réussit après STOP, STOPPED, START et RUNNING. Le banc natif compare les 32 octets d’identité, le PID, la tentative, la provenance du démarrage et le retrait des deux anciens canaux. Il teste aussi RESTART et une troisième incarnation. |
| Amorçage du temporaire | Le montage P9 autorise les admissions. Les services démarrent avec des racines vides. Le service 2 vérifie le refus au travailleur ordinaire et l’accès du travailleur explicitement délégué. Après perte de la VM, les cinq marqueurs et les OID associés sont relus. Après reprise et drainage, seul le sceau demeure, le catalogue public conserve son empreinte. |
| Rejet du READY retardé | Le service 8 envoie sa première annonce puis subit une vraie faute avant l’admission READY. Le gestionnaire conserve la trame et son canal réel. Pendant le second démarrage, il rejoue cette trame, puis éprouve séparément une identité actuelle sur le vieux canal et une ancienne identité sur le canal actuel. Trois refus sont exigés. Le service 7 reste WAITING, sans PID et sans admission. Seule l’annonce authentifiée de la deuxième incarnation débloque 7, qui répond ensuite à une requête différée. |

| Cas fonctionnels du mandat | Mécanismes exercés |
|---|---|
| 1–8 · enregistrement, graphe, parallélisme, disponibilité | Le client invité enregistre les entrées manuelles, refuse un ID inconnu, un catalogue cyclique et un fournisseur absent. Le journal doit contenir les trois lancements indépendants avant la première admission READY. La consultation distingue la dépendance obligatoire 8 de l’optionnelle 2. Sur un vCPU les lancements sont simultanément en cours, sur SMP les processus peuvent aussi s’exécuter sur des cœurs différents. |
| 9–16 · échéances, arrêts, fautes et propagation | Le témoin 4 omet son annonce, le 5 subit des fautes jusqu’à épuiser ses deux relances et le 6 ignore l’arrêt coopératif. Le client vérifie trois tentatives retirées pour 5. Le fournisseur 1 et son dépendant 3 sont redémarrés après observation des transitions réelles : le consommateur est arrêté avant le début de l’arrêt du fournisseur. La perte de canal, la sortie volontaire et le silence sont injectés séparément. |
| 17–24 · droits, isolement, réemploi, arrêt groupé et reprise | Le port de consultation refuse START et INJECT. Le droit de suivi du noyau est refusé au client. Les services tentent une duplication et un accès public à partir du temporaire. Le noyau et le gestionnaire retirent les anciennes autorités avant une nouvelle admission. SHUTDOWN engage plusieurs arrêts, la faute du gestionnaire réveille son client avec un port révoqué. Le drainage, la perte de la VM et la reprise sont contrôlés sur la même image. |

### Mémoire

Le tableau donne le pic de cadres supplémentaires, puis le nombre cumulé d’allocations de cadres pendant le parcours complet de supervision. Les deux nombres de chaque cellule correspondent aux secteurs de 512 et 4 096 octets. Un cadre vaut 4 Kio. Le périmètre comprend le noyau, les services, le pilote, le client et leurs travailleurs.

| Architecture | vCPU | Démarrages | Pic de cadres 512 / 4 096 | Allocations 512 / 4 096 |
|---|---:|---:|---:|---:|
| aarch64 | 1 | 8 | 1212 / 1212 | 2447 / 2431 |
| aarch64 | 2 | 10 | 1208 / 1208 | 2443 / 2427 |
| aarch64 | 4 | 8 | 1208 / 1208 | 2620 / 2562 |
| x86_64 | 1 | 8 | 1216 / 1216 | 2451 / 2451 |
| x86_64 | 2 | 10 | 1205 / 1205 | 2424 / 2424 |
| x86_64 | 4 | 8 | 1205 / 1205 | 2704 / 2688 |

Le pic observé de pages privées et de pile du processus nsh atteint 460 Kio sur AArch64 et 476 Kio sur x86_64. Les quatre cellules de scripts rejouées avec le PID annoncé par le noyau ont chacune fourni 40 échantillons. Pour vi, les pics observés sont respectivement de 380 et 384 Kio.

Le moteur de politique occupe 2 072 octets. Les mesures avant/après collecte vérifient l’égalité des compteurs de processus, IPC, piles, pages utilisateur, métadonnées actives et occupation du buddy. Sur les douze parcours de supervision et les douze pertes du gestionnaire, l’écart de cadres libres correspond exactement aux cadres des tables ou du domaine DMA qui restent attachés à la plateforme. Les tableaux par démarrage conservent ces deux écarts individuellement.

Une première coupure juste après une lecture par `cat` avait laissé le temporaire de ce sixième processus en cours de purge. L’assertion de cinq racines complètes s’applique désormais avant le lancement de tout ELF supplémentaire. Les huit images qui l’avaient mise en défaut ont été conservées, puis redémarrées sur des copies distinctes (six x86_64 et deux AArch64). Dans les huit cas, le registre ne contient plus que le sceau après arrêt propre, le catalogue public de 37 entrées garde son SHA-256 et le fichier sentinelle garde ses octets.

### Régressions et corrections observées

- Le banc inchangé de NovaShell compare la sortie multilignes des scripts avec Bash. Un passage x86_64 a révélé une ligne de diagnostic du service 9 entre deux lignes de la réponse. Les services publient désormais leurs transitions sur le canal de contrôle et le noyau réserve les traces de sortie de leurs familles aux profils de sonde. La preuve brute de l’échec antérieur est conservée. Les régressions finales P8, VFS P9, nsh, scripts et vi couvrent **44 démarrages** complémentaires sur deux architectures et deux géométries.
- Le rapport historique des scripts mesurait toujours le PID 4, désormais occupé par le pilote. Le banc résout le PID du client dans le même journal que ses échantillons. Les résultats corrigés de chaque géométrie figurent dans les preuves de la supervision.
- Les douze essais de politique ont réussi pour ARM64 Darwin natif et pour x86_64 sous Rosetta. Les essais QEMU d’instances et du courtier temporaire ont couvert 1, 2 et 4 vCPU. Les deux architectures ont réussi la collecte de 64 familles et 128 processus, les essais de l’ABI utilisateur et de l’IPC ainsi que les parcours BootID, dont la révocation de l’identité antérieure et le refus en l’absence de source qualifiée. Sur le banc du courtier, les cinq positions d’interruption distinguent annulation, résultat inconnu et résultat confirmé selon le contrat précédent.
- Dans les premiers essais, l’admission des services prenait le mutex du bloc sous la garde de l’ordonnanceur et l’attente d’une requête différée employait un droit de thread absent du gestionnaire. Le relevé des cadres DMA précède désormais cette garde. Le gestionnaire attend sur le canal IPC de l’incarnation avec échéance. Le client de qualification attend lui aussi sur IPC entre consultations. Les droits publiés restent inchangés pour les clients historiques.
- Le premier candidat dépassait le budget de 2 048 cadres du banc IPC au repos sur ARM. Trois nouveaux ELF emportaient chacun plus de 700 Kio de sections pour le débogueur. Le retrait des symboles des quatre ELF embarqués a économisé 2 218 112 octets sur ARM et 2 241 320 sur x86. Les huit comparaisons avant/après ont confirmé l’identité des segments chargeables, permissions, alignements et points d’entrée. Le banc IPC final retient 1 696 cadres au repos sur ARM et 1 257 sur x86, sous le plafond de 2 048 sur chaque cible.
- Le contrôle du registre et les neuf groupes de tests statiques ABI ont réussi. Les 46 entrées antérieures conservent leur définition. Le seul ajout porte le total à 47. L’archive distribuée de nova-system 0.2.0 conserve le SHA-256 `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Le moteur et les formats P8/P9 restent identiques à la base. Les campagnes longues du moteur et du DMA, la distribution UTM et les quatre anciens ELF du SDK relèvent des qualifications antérieures et n’ont pas été rejoués pour cette phase.

### Archives et limites

Le gestionnaire supporte jusqu’à douze entrées de catalogue embarquées. Son journal volatil disparaît avec lui. Si le gestionnaire tombe, le noyau retire ses familles et le redémarrage du système recrée l’ensemble. Le remplacement à chaud du gestionnaire, la supervision du fournisseur de stockage initial, le chargement dynamique de déclarations, la connexion distante et la recette complète S07 du serveur restent hors de cette qualification. La politique de contrôle protège des autorités IPC locales et suppose l’intégrité du noyau, du catalogue et des ELF embarqués.

| Cible | SHA-256 |
|---|---|
| Noyau aarch64 | `decdfaaf2e1bdff81f74f6ac140fee1f67402a03269b94f9f79f8feca38112ff` |
| Noyau x86_64 | `8cb274f66b2bc640596e6413d9ca70b21023473d4ab682adeaa77bd4f2115582` |
| [Supervision, noyaux, disques et régressions ciblées](validation/services-phase3-v0.21.0-supervision.tar.gz) | `b2981cc5ae0883d8ce2704d16660bab977717a4a9fef187f6ab48f6c4bf422b0` |
| [Régressions CLI, scripts et vi](validation/services-phase3-v0.21.0-regressions-cli.tar.gz) | `fa41f299037a1dd03642a7233df252aa0ca4681804ece201778934a678f36a5c` |
