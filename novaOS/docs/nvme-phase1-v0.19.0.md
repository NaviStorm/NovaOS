# NovaOS v0.19.0 — Phase 1 du pilote NVMe en lecture seule

Le profil expérimental `nvme-lab` a identifié un contrôleur NVMe sous QEMU Q35 x86_64, lu une image RAW de 1 Mio en secteurs de 512 et 4 096 octets et redémarré son pilote après retrait confirmé du domaine DMA. Le client a comparé chaque octet demandé. L’hôte a relu indépendamment le fichier RAW, les commandes du contrôleur et les tables VT-d. Cette phase concerne la branche `codex/nvme-0190`, issue de la conception `d6a84a35c15d474a2670a2f2b10eedb247cfecd8` sur NovaOS v0.18.0. Le code exécuté correspond au commit `bbaff0f95320ff50063a12f98e4e5c3cc8dd6345`. [Matrice et preuves QEMU](validation/nvme-phase1-qemu.json) · [Conception](nvme-v0.19.0.md)

## Audit préalable et adaptations

| Composant repris de v0.18.0 | Adaptation de phase 1 |
|---|---|
| Inventaire PCI/PCIe, MCFG/DMAR, contrôle des BAR et reprise des pairs Q35 | Ajout d’un profil fermé pour une fonction NVMe au bus racine. Il vérifie classe, identité, emplacement, décodage, étendues, topologie, groupe et absence de files héritées avant de déléguer le contrôleur. Le gestionnaire Ring 3 recoupe le matériel observé et l’autorisation de l’image de pilote. [Admission](../kernel/src/hardware/nvme_domain.rs) |
| Moteur VT-d EDU, épinglage, permissions, file d’invalidation et reçu WAIT | Le profil NVMe affecte cinq cadres privés à un unique bail. Il ajoute son propre critère de fin du contrôleur et le recoupe avec le retrait du remappeur. Le profil EDU conserve son admission et ses deux pages clientes. [Moteur VT-d](../kernel/src/hardware/vtd_lab.rs) |
| Chargeur de processus isolés, droits IPC directionnels et chemin existant du service bloc | Le profil construit un gestionnaire et trois images de session en Rust, sans nouveau sélecteur d’ABI publique. Le service bloc expérimental autorise les lectures par ticket. Les constructions ordinaires continuent d’utiliser le stockage classique et le polling VirtIO. [Contrat IPC et budgets des acteurs](nvme-phase1-ipc.md) |
| Outils QEMU, ISO Limine, journal série, contrôle QMP et lecture hôte des tables | Le nouveau banc crée sa propre image RAW et vérifie les plages lues, l’intégrité du fichier entier, les commandes NVMe, les permissions et les reçus du remappeur. [Banc QEMU](../tests/nvme_qemu.py) · [Contrelecture du domaine](../tests/nvme_domaine.py) |

## Frontière entre processus et superviseur

Le gestionnaire sélectionne le pilote. Une instance comprend ensuite le client de test, le service bloc et le pilote NVMe en Ring 3, chacun dans son espace d’adresses. Le service bloc enregistre la lecture avec son NSID, ses LBA et sa longueur. Le pilote orchestre l’initialisation, les commandes et la fermeture. Il présente son ticket pour déclencher la lecture. Le courtier privilégié vérifie capacités, incarnation, ticket et bornes, compose la véritable SQE, publie la doorbell et valide le CQE. Les octets achevés passent directement du courtier au service bloc. Les trois processus restent sans alias vers les treize cadres privés de l’IOMMU et du contrôleur. [Courtier](../kernel/src/sched/process/nvme/broker.rs) · [Câblage des capacités](../kernel/src/sched/process/ipc/nvme_port.rs)

La branche implémente le profil PCI `0000:00:06.0`, VID:DID `1b36:0010`, classe `01/08/02`, SID 48. La BAR0 64 bits de 16 Kio se situe à `0xc000000000` dans le banc. Le superviseur mappe seulement les deux premières pages pour les registres et doorbells. MSI est absent, les deux vecteurs MSI-X restent désactivés et INTx est masquée. Le CD-ROM d’amorçage est sélectionné explicitement. Le profil positif exige que CC, CSTS, AQA, ASQ et ACQ soient tous nuls au passage de responsabilité. Le témoin d’amorçage moins restrictif laisse un contrôleur actif après EDK2 et NovaOS refuse son attribution. [Contrôle du groupe PCI](../kernel/src/hardware/pci_inventory/lab_resources.rs) · [Preuves d’amorçage](validation/nvme-phase1-qemu.json)

## Contrôleur et résultats Identify

| Registres | Usage dans le profil |
|---|---|
| CAP `00h`, VS `08h` | Vérification du jeu NVM, de la version 1.4, de la taille de page, de la profondeur disponible, du pas des doorbells et du délai RDY annoncé. |
| AQA `24h`, ASQ `28h`, ACQ `30h` | Programmation et relecture de la paire administrative de profondeur 16, avant ouverture de BME. |
| CC `14h`, CSTS `1ch` | Activation, observation de RDY/CFS, arrêt normal par SHN/SHST, désactivation et contrôle de RDY=0. |
| Doorbells à partir de `1000h` | Publication ordonnée de la queue de soumission, lecture de la file de complétion avec phase attendue, vérification SQID/CID/SQHD/statut, puis avancée de la tête. |

Le pilote demande successivement Identify Controller, la liste des namespaces actifs et Identify Namespace. Il négocie une paire d’E/S, crée la CQ avant la SQ et vérifie chaque complétion. Les registres et les entrées des deux paires de files restent privés. Le courtier publie au plus une commande à la fois, écrit une SQE de 64 octets et lit une CQE de 16 octets en retour. Pour READ, les PRP décrivent une seule page privée alignée en PRP1. Le débit et le coût du polling n’ont pas été mesurés dans cette phase. [Machine d’états](../kernel/src/hardware/nvme/mod.rs) · [Files et doorbells](../kernel/src/hardware/nvme/queue.rs) · [Spécification de conception et références NVMe](nvme-v0.19.0-controleur.md)

| Observation | Valeur QEMU |
|---|---|
| Identify Controller | VID `0x1b36`, VS `0x00010400`, série `NOVA0190READ`, modèle `QEMU NVMe Ctrl`, MDTS 7. |
| Identify Namespace | Un seul NSID actif, 1. EUI-64 `4e4f564130313930`. Le format sélectionné a des LBA sans métadonnées ni protection PI. |
| Géométries | 2 048 LBA × 512 octets ou 256 LBA × 4 096 octets. Dans les deux profils, NSZE et NCAP sont cohérents et l’image fait exactement 1 048 576 octets. |
| Limites appliquées | 4 096 octets par READ, soit huit LBA de 512 ou un LBA de 4 096. NSID étranger, longueur nulle, dépassement de plage, écriture et FLUSH sont refusés. |

## Vérification des données et de l’arrêt

Les 20 essais QEMU de la matrice ont abouti avec l’ELF de noyau dont le SHA-256 est `86cfce68cf0670a7ee9fbcc4e557bffbc7bb661ae6cd38d5c96fd2292473ca39`. Le profil de lecture croise les deux géométries avec 1, 2 et 4 vCPU. Chaque VM de lecture effectue deux sessions. La seconde utilise trois nouveaux PID et une nouvelle époque. Pour chaque session, l’hôte exige les trois statuts de sortie Ring 3 et la séquence matérielle des lectures. Le client vérifie le contenu octet par octet et l’hôte rapproche son FNV-1a 64 bits d’une relecture des plages du RAW. Le SHA-256 avant/après du fichier entier est `0bdeae2c9e25d26126f1d7ba12e4e9040fdde6879e979632fb09ff68a02e6458`. [Matrice, traces et empreintes](validation/nvme-phase1-qemu.json)

| Secteur | Premier bloc | Dernier bloc | Lecture de 4 Kio au milieu |
|---|---|---|---|
| 512 octets | LBA 0, `4ff921accdcd0025` | LBA 2 047, `e5263f1001620225` | Huit blocs dès le LBA 1 024, `c2f2805fc79c5925` |
| 4 096 octets | LBA 0, `988973e58c8ae325` | LBA 255, `76f7bf15c8b58f25` | Un bloc au LBA 128, `c2f2805fc79c5925` |

L’arrêt normal supprime la SQ puis la CQ d’E/S, vérifie les deux CQE administratifs, attend l’arrêt du contrôleur, désactive CC.EN et revérifie les registres. Le domaine ferme ensuite BME, retire les traductions, ordonne les invalidations de contexte et d’IOTLB avec drainage puis attend son reçu WAIT. La réouverture exige ce retrait complet. Le banc démarre la session suivante dans la même VM avec de nouveaux PID et de nouveaux tickets. [Séquence d’arrêt et règles de retrait](../kernel/src/sched/process/nvme.rs)

| Incident exercé | Observation et décision |
|---|---|
| Contrôleur absent, autre identité PCI, BAR supplémentaire, topologie imprévue, absence de remappeur ou contrôleur hérité actif | Le noyau refuse la délégation et conserve les pages du firmware dans ce chemin de refus. Le profil hérité conserve séparément les commandes administratives émises par le firmware. |
| Mauvaise série ou NSID inattendu | Identify invalide ferme l’admission. Aucune lecture de données du namespace. |
| Annulation avant soumission, décès du client | Le ticket annulé est refusé lors de la tentative de publication. La fermeture du client interdit une lecture supplémentaire. Le superviseur peut retirer proprement une file sans commande en vol. |
| Mort du pilote ou retrait de sa capacité après la doorbell | L’incarnation ne peut plus fournir de résultat. Les cinq pages restent en quarantaine quand l’achèvement matériel manque. Le chemin de révocation vérifie le refus de la tentative suivante depuis le pilote. |
| Expiration pendant READ | `blkdebug` suspend la lecture réelle au point `read_aio`. L’invité observe l’échéance de cinq secondes, le pilote et le service bloc transmettent l’erreur et le client reçoit un échec sans données. Le banc ne reprend le backend qu’après observation du bilan et maintient les cadres épinglés. |
| DMA non autorisé | Deux VM présentent une destination DMA non mappée, pendant Identify dans l’une et READ du LBA 0 dans l’autre. QEMU annonce dans les deux cas une CQE de statut zéro. Le courtier détecte la faute VT-d mémorisée pour le SID 48 avant de valider les données. La première écriture interdite vise l’IOVA `0x02004000` (raison 5). L’hôte observe la PTE absente et les 4 096 octets intacts du cadre témoin. Le client reçoit zéro octet et les cadres restent en quarantaine. |
| CQE tardive, phase d’anneau, CID/SQID incohérent, statut d’erreur | Le modèle Rust injecte directement les entrées de file et les transitions de registres, y compris après le délai. Le courtier refuse une complétion incohérente ou périmée. Ces injections synthétiques ont une provenance distincte des essais QEMU. [Tests du contrôleur](../tests/nvme_controleur.rs) |

## Mémoire, compatibilité et limites

Le groupe actif occupe six pages utilisateur de code et DATA plus trois pages de pile, soit 36 Kio privés. Le budget DMA transitoire est de cinq pages de 4 Kio, soit 20 Kio. Par ordre de mapping, les droits du contrôleur sont lecture de l’ASQ, écriture de l’ACQ, lecture de l’IOSQ, écriture de l’IOCQ et écriture de la destination READ/Identify. Le profil en lecture seule se dispense de la page source de WRITE envisagée dans la conception. Les huit pages du remappeur, soit 32 Kio, restent réservées pour la durée de vie de l’unité. Un arrêt normal laisse ces huit pages et zéro page IPC de la session. Une quarantaine après soumission peut conserver treize pages, soit 52 Kio, sans aucun réemploi. La matrice fournit les relevés de pages résidentes, IPC, piles et tables par VM et par incarnation. [Relevés par cellule](validation/nvme-phase1-qemu.json)

Le compteur résident couvre l’invité entier. La variation d’une page entre sessions correspond à une page IPC. Les mesures après retrait portent sur la même VM et incluent les réservations permanentes.

| vCPU | Pages résidentes de 4 Kio en session | Pages résidentes après retrait |
|---|---:|---:|
| 1 | 963–964 | 879 |
| 2 | 1 010–1 011 | 926 |
| 4 | 1 076–1 077 | 992 |

Les neuf tests du modèle de contrôleur ont réussi sur chacune des deux architectures hôtes. Les régressions ciblées comprennent les modèles de découverte, de ressources et de DMA sur Darwin ARM64 natif et x86_64 sous Rosetta, ainsi que les chemins Q35 EDU de retrait normal et incertain. Les 66 tests matériels ont réussi sur chaque architecture hôte. Le témoin EDU normal a exercé 10 scénarios, 6 interruptions de faute et 22 invalidations acquittées. Dans chacun des invités AArch64 et x86_64, la suite IPC classique a vérifié 17 cas. Le test de stockage classique a exercé 8 processus. Les dix séries de correctifs des profils d’E/S groupées se sont appliquées sans tolérance de contexte dans des copies isolées. Ce dernier contrôle porte sur l’application des correctifs. L’ABI native et NovaFS v0.9.13 P9 conservent leurs formats et leurs droits. Le million d’opérations différentielles NovaFS, les campagnes de coupures, l’endurance IPC, la GUI et les benchmarks restent hors de cette qualification ciblée. [Régressions](validation/nvme-phase1-regressions.json)

Le domaine testé appartient à un contrôleur QEMU sur une topologie PCIe fixe. L’arrêt complet et le retrait sont démontrés sur ce modèle. Les contrôleurs physiques, leur FLR, le pair-à-pair, les backends AMD-Vi/ARM SMMU, UTM et l’amorçage autonome depuis NVMe demandent d’autres qualifications. Le profil ne s’applique pas au SSD du Dynabook. Une quarantaine peut rendre le contrôleur indisponible pour le reste du démarrage. MSI-X, plusieurs commandes d’E/S simultanées, PRP multipages, SGL et montage NovaFS sur NVMe restent différés.

## Reproduire depuis la racine du dépôt

Le banc utilise Rust stable avec la cible `x86_64-unknown-none`, Python 3, `xorriso`, les binaires Limine du dépôt et `qemu-system-x86_64` 11.1.3 avec ses images EDK2. Il construit des ISO en lecture seule, attache uniquement un RAW jetable et crée un répertoire de preuves neuf pour chaque exécution. Les variables `OVMF_CODE` et `OVMF_VARS` permettent de sélectionner les images EDK2. La seconde est ouverte en snapshot. Le banc inclut un journal série, un inventaire PCI QMP, les pages VT-d utiles et la trace des commandes. Le fichier `bilan.json` associe leurs empreintes à l’ELF et aux firmwares effectivement utilisés.

```sh
cd novaOS
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
  --release --locked --target x86_64-unknown-none --features nvme-lab \
  --target-dir build/nvme-cargo

python3 -B tests/nvme_qemu.py \
  --elf build/nvme-cargo/x86_64-unknown-none/release/novaos \
  --profil lecture --vcpus 2 --secteur 512

# Matrice ciblée complète dans un nouveau répertoire de preuves.
python3 -B tests/nvme_qemu.py \
  --elf build/nvme-cargo/x86_64-unknown-none/release/novaos
```

**Arbitrage proposé pour la phase 2.** Examiner les preuves de retrait et la perte de disponibilité en quarantaine avant d’autoriser une voie d’écriture. Un futur ticket WRITE sur image jetable devra figer les octets de la source et borner sa plage. La qualification de FLUSH devra distinguer la complétion du transfert, la persistance promise par le backend et l’acquittement applicatif. Le premier montage P9 sur NVMe nécessitera une validation distincte.
