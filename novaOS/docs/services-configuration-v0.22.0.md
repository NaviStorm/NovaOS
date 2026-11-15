# NovaOS v0.22.0 · Administration persistante des services, phase 1

Le service `storage-info` (9) répond à une requête native par la valeur `NVFS`. Le service `cache` (10) dépend de sa disponibilité. Une modification du démarrage automatique du 9 laisse son instance active et les réglages du 10 intacts. Le tableau reprend les états observés dans les invités de qualification.

| Moment | Service 9 | Service 10 | Générations appliquée / enregistrée |
|---|---|---|---|
| Volume P9 sans configuration | `RUNNING`, automatique | `RUNNING`, automatique | 0 / 0 |
| Après `service disable 9` | `RUNNING`, automatique désactivé, redémarrage en attente | `RUNNING`, automatique, aucun changement en attente | 0 / 1 |
| Au redémarrage | `REGISTERED` | `WAITING`, cause « dépendance 9 indisponible » | 1 / 1 |
| Après `service enable 9` | `REGISTERED`, automatique réactivé, redémarrage en attente | Attend toujours 9 | 1 / 2 |
| Après arrêt forcé de QEMU puis relance | `RUNNING`, nouvelle requête `NVFS` réussie | Démarre après la disponibilité du 9 | 2 / 2 |

Dans la troisième ligne, `service start 10` laisse le fournisseur désactivé à l’état `REGISTERED`. La lecture du fichier public `/documents/exemple.txt` réussit. L’administrateur peut démarrer explicitement 9 pour la session courante. La commande `service status` relit le magasin, rapporte l’état courant, les deux générations et le changement en attente par service. Les générations numérotent la configuration du catalogue entier. Les données de configuration lues en mode de récupération s’accompagnent d’un diagnostic et d’un code de sortie d’erreur. [Parcours automatisé](../tests/services_configuration.py) · [Déclarations et protocole](configuration-services-format-v0.22.0.md)

## Amorçage et autorités

1. Le noyau initialise le transport bloc. Le pilote en espace utilisateur et le relais minimal fournissent l’accès au disque. Le service NovaFS ouvre P9 et met son espace de noms à disposition.
2. Le courtier demande à NovaFS le répertoire privé du gestionnaire. La barrière d’entrée du processus attend le bail actif avant sa première instruction utilisateur. Le pilote, le relais et le fournisseur P9 fonctionnent sans gestionnaire de services.
3. Le gestionnaire entre avec son catalogue embarqué. Il lit les réglages par sa paire IPC privée, valide toutes les déclarations et construit le graphe. Les images admises, leurs arguments, les plafonds de droits et les dépendances proviennent du catalogue de confiance.
4. Le gestionnaire démarre les services dont la politique est active et les prérequis disponibles. Le passage par `READY` conserve les contrôles d’incarnation et de canal de v0.21.0. Le service 9 rend compte du stockage déjà disponible. Son inhibition laisse le chemin de stockage, le temporaire privé et le terminal opérationnels. [Entrée du processus](../kernel/src/sched/process.rs) · [Création du bail](../kernel/src/sched/process/tmp.rs) · [Gestionnaire](../user/service_manager.rs) · [Supervision qualifiée en v0.21.0](services-phase3-v0.21.0.md)

| Délégation | Titulaire et opérations |
|---|---|
| Consultation | Le lanceur attribue au terminal un canal de consultation de l’état et des réglages. `list`, `status`, `dependencies`, `policy` et `info` l’utilisent. |
| Exploitation | Le profil de maintenance existant `services-admin` permet `start`, `stop`, `restart`. Ces opérations ont effet sur la session courante. |
| Politique persistante | Le profil explicite `services-config-admin` ajoute au contrôle d’exploitation une paire dédiée à `enable` et `disable`. La console ordinaire obtient `DENIED` pour ces mutations. |
| Publication sur P9 | Le gestionnaire possède une paire vers NovaFS, dont les racines appartiennent au serveur de fichiers. Le terminal et les services administrés n’obtiennent aucun de ces handles. |
| Amorçage critique | Le pilote, le relais et NovaFS restent hors du catalogue administrable. Le code de configuration n’attribue aucun droit sur ces composants. |

Les identifiants numériques et les noms sont des clés de sélection. Le noyau contrôle le droit sur le canal dans la table du processus. Le générateur ABI conserve les 48 sélecteurs v1. Le choix du profil de maintenance relève de l’autorité qui maîtrise l’amorçage de cette machine. NovaAuth, l’attribution de rôles à des comptes et la future commande `permissions` restent à qualifier ultérieurement. [Raccordement des capacités](../kernel/src/sched/process/ipc/service.rs) · [Contrôle par port](../user/services/control.rs)

## Persistance et reprise

NovaFS conserve deux objets internes à contenu CoW. Chacun porte les identifiants FS/LV, la génération, la longueur, l’intégrité de l’enveloppe et l’intégralité de la déclaration encodée. L’initialisation d’un emplacement passe par un objet de préparation et un renommage vers un nom libre. Une rotation d’un emplacement existant utilise une écriture CoW de 4 160 octets. Le VFS filtre les noms internes à l’énumération et refuse leur caractère de contrôle dans un chemin fourni par un client. Le moteur NovaFS et ses ancres sont ceux de P9. [Représentation des décisions](configuration-services-format-v0.22.0.md) · [Magasin P9](../user/novafs/files/config_store.rs) · [Transport privé](../user/novafs/files/configuration.rs)

Le client joint à chaque mutation la génération qu’il a lue. Le serveur vérifie cette précondition au début et avant publication. Une autre mise à jour depuis la même génération reçoit `BUSY`. La commande annonce la génération enregistrée après l’acquittement de publication P9. La perte de la réponse après admission donne `UNCERTAIN`. Une nouvelle consultation relit P9. Une nouvelle décision de l’administrateur est nécessaire avant une nouvelle écriture.

Une copie corrompue permet une récupération signalée en lecture seule si l’autre enveloppe et son contenu sont valides. Une erreur de lecture ou de transport au boot inhibe l’automatisme supervisé. Si aucune copie n’est vérifiable, l’accès minimal au volume sain et à la console permet le diagnostic. Un défaut de montage du volume P9 lui-même conserve le refus du lanceur v0.21.0. Les CRC et contrôles P9 couvrent les altérations exercées. La restauration intégrale d’un ancien disque cohérent, la réécriture par un administrateur du support, la perte électrique de l’hôte et une ancre P9 déchirée dépassent les garanties de cette phase. [Limites du modèle P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

## Essayer dans QEMU

Depuis la racine du worktree, créer une image fictive P9 dans un dossier neuf. Conserver la même image pour les trois démarrages. Le profil ordinaire s’obtient en omettant `--admin-services`.

```sh
export NOVAOS_TMPDIR="$(dirname "$PWD")/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
export NOVA_ARCH=aarch64 NOVA_VCPUS=2
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
make -C novaOS ARCH="$NOVA_ARCH" kernel
essai=$(mktemp -d "$(dirname "$PWD")/essai-services.XXXXXXXX")
python3 -B novaOS/scripts/session_cli.py demo "$essai/volume.raw" --sector 4096 --admin-services
```

Dans `nsh`, exécuter `service list`, `service dependencies cache`, `service disable 9`, `service status 9`, puis `exit`. Après l’arrêt, relancer la même commande hôte avec `run` à la place de `demo`. Exécuter `service status 10`, `cat /documents/exemple.txt`, `service enable 9` et quitter. Au troisième démarrage, vérifier `service info storage-info` et `service status cache`. La réponse native vaut 1397118542, encodage little-endian des octets ASCII `NVFS`. Le terminal accepte aussi `service --help`. [Lanceur interactif](../scripts/session_cli.py) · [Commande et codes de retour](../../components/shell/services.rs)

## Qualification

QEMU 11.1.3, TCG, 256 Mio par VM, disque VirtIO raw avec FLUSH. AArch64 utilise Cortex-A72 et GICv2, x86_64 utilise Q35. Les noyaux figés ont été construits avec Rust 1.99.1 sur macOS 27.2 ARM64. Les journaux identifient le noyau par SHA-256. La révision du lanceur enregistrée pendant les essais désigne la base v0.21.0 du worktree, avant création des trois commits de phase. Les sources exécutables figurent dans le commit `f355af962fc4bfd5e709b74fc32d5fd644a7f4a4`. Le contrôle de construction après commit et la comparaison octet par octet retrouvent les ELF archivés, SHA-256 `8df41dcc7beb2b31e1e7782a78672546aeafb764ce42a86dfe14f8835dc7d399` sur AArch64 et `e0bdc7aa62c4094ae980e34da8d663d9e9d7c3a81410fdbf15f8472aeb98b8b0` sur x86_64.

| Épreuve | AArch64 | x86_64 | Contrôle observé |
|---|---:|---:|---|
| Sessions `nsh` ordinaires et de maintenance | 24 démarrages | 24 démarrages | 4 démarrages par combinaison de 1/2/4 vCPU et 512/4096 octets. Refus en lecture seule, désactivation, redémarrage, attente de 10, réactivation, SIGKILL après acquittement, retour de 9 et 10. Stop/start/restart explicites et renouvellement d’instance. Relecture hors VM des 48 images archivées. |
| Repli ou inhibition sur configuration invalide | 6 | 6 | 2 vCPU, deux secteurs. Enveloppe invalide, droit hors catalogue avec enveloppe valide, deux emplacements invalides. Relecture indépendante des deux objets et de la sentinelle. La publication reste bloquée pendant la récupération. |
| Concurrence de deux mutations et révocation | 3 | 3 | 1/2/4 vCPU, secteur 512. Un succès et un conflit pour deux mutations avec la même précondition. Au changement suivant, après admission, révocation des deux délégations et perte de la réponse. Deux relectures indépendantes par le client retrouvent la génération 2. L’oracle hôte vérifie les deux objets après démontage. |
| Coupure pendant rotation, puis remontage | 2 | 2 | 2 vCPU, ARM/4096 et x86/512. Après six écritures supplémentaires, QMP observe la première barrière en attente. Le nombre de FLUSH achevés reste à 31. SIGKILL interrompt la génération 3 avant sa confirmation. Une VM distincte redémarre une copie, retrouve la génération 2 et reçoit `NVFS`. |
| Supervision historique v0.21.0 | 5 | 5 | 2 vCPU. Ancien READY refusé, dépendant bloqué, nouvelle incarnation admise, anciens canaux révoqués, pannes, silence et limites de relance, perte du gestionnaire, contrôles en lecture et en maintenance. |
| Compatibilité du cache et du temporaire | 1 | 1 | Profil `cache-test`, 2 vCPU. Trois propriétaires, partage autorisé, usurpation refusée, purge pendant écriture et retrait. Catalogue public identique à la relecture. |
| Admission et compatibilité P8/P9 | 4 refus + 4 démarrages P8 | 4 refus + 4 démarrages P8 | 1 vCPU. Aucun écrit bloc lors des refus du format croisé, du volume invalide ou du BootID absent. Le parcours CLI historique retrouve G43 après la session de lecture et G44 après l’arrêt forcé. |

Le test de course s’exécute avec le vrai gestionnaire, le serveur P9 et les primitives IPC du noyau. Le profil explicite 88 donne au gestionnaire de la sonde le droit de révoquer les deux délégations après admission. Dans le profil 87, la fin du principal de maintenance retire ses canaux. Le champ `ancien_cache_desactive` dans les six premiers relevés JSON de course désigne la politique de **9** en génération 1. L’oracle relit séparément le bit de chaque service et vérifie que **10** reste activé dans les deux générations. Le libellé du banc a été précisé pour les exécutions suivantes. [Concurrence et oracle](../tests/services_configuration_courses.py) · [Coupure QMP](../tests/services_configuration_coupure.py) · [Corruption et repli](../tests/services_configuration_invalide.py)

Sur ARM64 natif et x86_64 sous Rosetta, les 13 tests de politique historiques et les trois tests de configuration ont réussi sur chaque ISA. Ils couvrent les troncatures à tous les offsets de la charge, la version, les services et dépendances inconnus, le cycle, les plafonds, les politiques et l’inhibition d’un fournisseur. Le banc de stockage a exercé par ISA **28 coupures** et **56 erreurs de FLUSH** sur les deux géométries, ainsi que huit coupures après création d’un objet vide. Il a également vérifié le volume plein, une erreur de lecture injectée, le CAS périmé et le refus de P8. Les premières publications comportent six barrières, la rotation en comporte deux. Une erreur de la dernière barrière de publication peut laisser le nouvel état complet malgré l’erreur retournée. La relecture au remontage distingue les issues observables. [Banc P9](../tests/services_config_stockage.rs) · [Tests du catalogue](../tests/services_configuration.rs)

| Architecture | 1 vCPU, 512 / 4096 | 2 vCPU, 512 / 4096 | 4 vCPU, 512 / 4096 |
|---|---:|---:|---:|
| AArch64 | 1020 / 1020 | 1015 / 1015 | 1015 / 1015 |
| x86_64 | 1026 / 1026 | 1019 / 1019 | 1019 / 1019 |

Chaque valeur est le maximum des pics des deux sessions de maintenance arrêtées proprement pour la cellule, en cadres de 4 Kio supplémentaires pour la machine entière. Sur les 24 collectes, les compteurs processus, IPC, pages utilisateur, piles, heap, buddy et métadonnées actives retrouvent leurs valeurs initiales. La différence de cadres libres correspond exactement aux tables et au DMA retenus. AArch64 conserve trois cadres de tables. x86_64 conserve neuf cadres du remappeur VT-d, plus deux cadres de tables dans le profil à un vCPU. Les mesures individuelles sont conservées dans les JSON par cellule. La série interrompue par SIGKILL ne fournit pas de mesure de collecte.

### Reproduire la matrice

Préparer l’outil hôte et les deux noyaux avec les commandes de construction ci-dessus. Pour l’architecture et la taille de secteur choisies, utiliser un dossier de sortie neuf. Créer un dossier durable et y conserver une copie du noyau choisi.

```sh
preuves=$(mktemp -d "$(dirname "$PWD")/preuves-config.XXXXXXXX")
case "$NOVA_ARCH" in
  aarch64) cible=aarch64-unknown-none-softfloat ;;
  x86_64) cible=x86_64-unknown-none ;;
esac
cp "novaOS/target/$cible/release/novaos" "$preuves/novaos.elf"
export NOVA_KERNEL_ELF="$preuves/novaos.elf"
python3 -B novaOS/tests/services_configuration.py "$NOVA_ARCH" 2 4096 "$preuves/parcours"
sh novaOS/tests/services_config_stockage.sh "$NOVA_ARCH"
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --lib
lib=novaFS/target/aarch64-apple-darwin/release
rustup run stable rustc --edition=2024 --target aarch64-apple-darwin -C opt-level=2 \
  -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
  novaOS/tests/services_configuration_fixture.rs -o "$preuves/configuration-fixture"
python3 -B novaOS/tests/services_configuration_invalide.py "$NOVA_ARCH" 4096 "$preuves/invalide" "$preuves/configuration-fixture"
python3 -B novaOS/tests/services_configuration_courses.py "$NOVA_ARCH" 2 4096 "$preuves/course" "$NOVA_KERNEL_ELF" "$preuves/configuration-fixture"
python3 -B novaOS/tests/services_configuration_coupure.py "$NOVA_ARCH" 4096 2 "$preuves/course/volume.raw" "$preuves/coupure" "$preuves/configuration-fixture"
python3 -B novaOS/tests/abi_registre.py -v
```

Pour compléter le premier parcours, varier les vCPU (1, 2 et 4), les secteurs (512 et 4096) et l’architecture, avec des dossiers distincts. Le banc d’interruption exige une image source contenant les deux générations attendues et refuse de modifier l’original.

Les neuf contrôles du registre ABI ont réussi, dont les signatures antérieures, le dispatch et les consommateurs C/C++. L’archive distribuée du SDK 0.2.0 a été relue (SHA-256 `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`). Noyau affiché v0.20.0, NovaFS v0.9.13 et distribution UTM v0.17.0. Le diff de phase conserve le moteur de stockage, les formats, le SDK et les définitions ABI. Les campagnes longues de stockage, le matériel physique, l’interface UTM et l’installateur n’ont pas été requalifiés dans cette phase.

Deux défauts ont été relevés avant le gel des noyaux. L’ajout des nouveaux canaux dépassait le quota de quatre racines du gestionnaire. La répartition entre serveur, gestionnaire et client respecte désormais le quota existant. Le client de configuration associait également un délai non nul à un dépôt IPC asynchrone, ce qui provoquait `BAD_ARGUMENT` avant toute lecture. Le dépôt utilise maintenant un délai nul et l’attente de la réponse conserve son échéance. Le repli vers la copie antérieure exige une corruption établie, car une panne de lecture transitoire peut masquer une configuration confirmée plus récente.

Les 141 entrées de l’archive documentaire comprennent les séries, les résultats et un inventaire SHA-256 de la campagne complète. Son SHA-256 est `609c7d5d7907f9efefe4783da953a73adc53fbab32db37c8f5d5e6f1890d88dc`. La copie locale complète de 498 fichiers inclut en plus les ISO, les ELF figés, les disques, les traces et les données QMP. Elle est conservée dans `OS-worktrees/preuves-services-config-0220-phase1/services-configuration-v0.22.0-phase1-complete.tar.gz` (SHA-256 `e5824324e3ae59bfdc96c1caef2ffedae4202d524dfa3735c8caaf4a78bb0d89`). Chaque entrée des deux archives a été décompressée et comparée à l’empreinte de son fichier source.

**Suite soumise à la MOA/MOE.** Examiner la branche de phase 1 et son périmètre d’administration locale avant toute intégration. Les droits optionnels futurs, la délégation à des comptes attestés et la récupération administrative d’un magasin totalement endommagé appellent un contrat distinct. [Matrice serveur S07](reconciliation-v0300/acceptation-serveur.md) · [Journaux et résultats vérifiables](validation/services-configuration-v0.22.0-phase1.tar.gz)
