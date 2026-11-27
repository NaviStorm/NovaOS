# NovaOS v0.25.0 — Killer E2400 — Qualification 5B

Le 27 novembre 2026, le gestionnaire simulé a réservé et suivi les **259 cadres** du profil Killer. Dans l’invité x86_64, l’acteur Ring 3 a exercé 264 TX et 264 RX. Au retrait, un reset a laissé les 259 cadres retenus. Les reçus de la génération ont ensuite permis d’en rendre 129, puis les 130 restants. Cette qualification autorise la poursuite du travail logiciel dans le périmètre A7 simulé. L’état du contrôleur physique du MSI demeure inconnu. [Preuve Ring 3](validation/killer-e2400-phase5b/q35-5b.json) · [Inventaire MSI en lecture seule](killer-e2400-inventaire-msi-5b.md)

## Révisions et périmètre

| Révision | Ce qui a été exécuté |
|---|---|
| `7c3bb99a2f63326818024ac8e498ec329c6ad345` | Point de comparaison 5A issu de 4F. Bancs hôtes et Q35 historiques. Préparation 5B dans `b65e7c4062255b0982d7423cc85f7544ff1c942e`. [Témoin](killer-e2400-temoin-5b.md) |
| `7cbba950fb0b7571984ccfed16384ee9ac2edffb` | Registre, courtier et tests 5B. Deux retraits Q35 VirtIO-net avec NVMe actif |
| `45c7d67b7ccdbe537d2c7e7994c686596fd5f445` | Correction du chronométrage hôte. Matrice Darwin, trois démarrages VirtIO-net ARM et témoin des six sessions E2400 historiques |
| `82c52560c3a6a45174516dad2319e75bbccfd6d8` | Sonde de tas du processus de test, exécution Ring 3 instrumentée. Vérification Cargo du gestionnaire avec `e2400-sim` pour les deux cibles nues |

Les deux derniers commits de code modifient uniquement les bancs. Le paquet signé de 5A, le noyau, ses 48 sélecteurs, son manifeste interne 0.20.0, les services Ethernet, VirtIO-net et NovaFS gardent les sources du témoin. Le nouveau courtier utilise les types de l’adaptateur 5A en mémoire privée. Seul le banc active le profil Cargo `e2400-sim`. [Profil signé de la simulation](../user/killer-e2400/profil-cpu.toml) · [Qualification historique](killer-e2400-phase5a.md)

## Propriété des cadres et attribution

Le registre résident conserve trois octets par page réservée. Deux portent un numéro logique dans le quota du simulateur, un porte sa direction et les états d’exposition, d’arrêt de sa source, de retrait de traduction et de retour de propriété. Le groupe contient une seule copie du bail. Toute consultation exige l’identité complète du bail, dont le démarrage, le rattachement à l’inventaire, le titulaire, l’incarnation, la génération, le domaine et le profil. Les numéros logiques sont indépendants des adresses PCI et IOVA. [Registre](../libs/gestionnaire-pilotes/src/dma.rs) · [Gestionnaire](../libs/gestionnaire-pilotes/src/manager.rs)

La réservation vérifie les plafonds et les droits du plan, effectue l’allocation faillible, attribue des numéros libres, puis publie le bail. Le quota global de laboratoire reste de 4 096 cadres. Un groupe en découverte n’alloue aucune entrée de page. Les rôles multiples exigent une carte explicite des directions. Les deux cartes VirtIO de 22 pages sont liées à l’identité du pilote, à sa dépendance de profil signée, à son empreinte et à sa géométrie. Les masques de membres PCI restent dimensionnés pour les 128 emplacements d’inventaire. [Ressources](../libs/gestionnaire-pilotes/src/resources.rs) · [Correspondance VirtIO](../libs/gestionnaire-pilotes/src/virtio_lab_layout.rs)

Chaque reçu décrit les pages concernées par trois ensembles transitoires de cinq mots de 64 bits. Une preuve hors géométrie refuse le reçu entier. Un numéro de série périmé ou dupliqué, ou un bail divergent, conserve l’état précédent. Un cadre retourne à l’allocateur logique lorsque sa source est arrêtée, sa traduction retirée et sa propriété rendue. Les cadres déjà rendus peuvent être réalloués pendant la quarantaine des autres. La clôture complète du groupe exige en plus l’arrêt de la source du groupe. Une annulation avant exposition libère la réservation. Les reçus de ce composant proviennent exclusivement de l’oracle CPU. [Cycle de vie](../libs/gestionnaire-pilotes/src/lifecycle.rs) · [Essais de réconciliation](../libs/gestionnaire-pilotes/tests/groupes.rs)

## Contrats du courtier

| Opération | Contrôle exercé |
|---|---|
| Identifier et réserver | PCIe réseau `1969:e0a1`, sous-système `1462:115a`, révision `10`, classe `02/00`. Le fournisseur confirme l’époque, la propriété firmware, les alias, les unités IOMMU et l’isolement reset/IRQ. Les nœuds du stockage sont contrôlés indépendamment. Le vecteur utilise le segment 2, bus 19. La garde Q35 refuse une attribution sur le contrôleur protégé avec un bail NVMe actif |
| Ouvrir | Le permis vaut pour un plan, un titulaire, une incarnation et un domaine. Les deux feuilles TX/RX doivent être actives. Les deux mots PHY fournis par le banc doivent correspondre et être valides. Les lectures admises portent sur PHY ID1/ID2, producteur TX, consommateur RX et notification en attente |
| Préparer et publier TX | Copie privée filtrée MAC et EtherType `0x88b5`, TPD de 16 octets, tampon logique de 2 Kio. L’ordre de publication doit être établi avant le préfixe. Une vue immuable du tampon existe entre publication et complétion. Le consommateur libère les crédits une seule fois |
| Recevoir et notifier | Publication du RFD sous crédit, lecture de la copie RRD avec UPDATE, origine, index, taille et FCS vérifiés par l’adaptateur. Les indices bouclent modulo 256. L’IRQ simulée regroupe les événements et son réarmement relit le niveau. Les barrières de compilateur bornent l’ordre du modèle CPU |
| Révoquer | Les feuilles expirent, le courtier sépare tickets incertains et annulés. Ni l’arrêt logique ni le reset simulé n’effacent le compte des cadres exposés. Les preuves sont consommées par le gestionnaire après réconciliation |

La carte E2400 affecte les pages 0 et 1 aux TPD/RFD en lecture pour le NIC, la page 2 aux RRD en écriture, les pages 3 à 130 aux tampons TX en lecture et 131 à 258 aux tampons RX en écriture. Cela représente 130 pages lisibles et 129 inscriptibles. La géométrie proposée vaut 1 036 Kio de mémoire DMA pour un éventuel pilote physique. Le banc utilise des numéros et des copies CPU. Il attribue **zéro cadre DMA physique**. [Profil de médiation](../user/killer-e2400/profil_gestionnaire.rs) · [Courtier](../user/killer-e2400/courtier_simule.rs)

## Mémoire et IPC

| Objet | Témoin 5A | Mesure 5B |
|---|---:|---:|
| Un groupe découvert | 400 o | 368 o |
| 32 groupes découverts | 12 800 o | 11 776 o |
| Un groupe de 22 pages, en-tête + entrées | 400 o | 434 o, dont 66 o d’entrées |
| Entrées pour 128 / 129 / 258 / 259 pages | Registre fixe limité à 128 | 384 / 387 / 774 / **777 o** |
| Un groupe E2400 de 259 pages | Réservation refusée | **1 145 o**, dont 368 o d’en-tête |
| Transaction de réserve | Suivi par masques | 512 o de table temporaire de numéros utilisés sur la pile, indépendamment de la géométrie non vide |
| Reçu de preuves | Deux masques de pages et un indicateur global | 240 o pour l’observation complète, dont trois ensembles de 40 o |

Les entrées restent allouées jusqu’à la clôture ou à l’annulation du groupe, même après un retour partiel des cadres. La borne structurelle pour les 32 en-têtes et 32 vecteurs de 259 entrées est **36 640 octets**, hors inventaire, plans, liens et reçus. Le gestionnaire commun mesure 2 416 octets d’en-tête. La capacité mesurée pour un emplacement de plan vaut 576 octets sur le profil commun et 720 avec le profil 5B. Le permis remis au banc occupe 152 octets. Le courtier a un en-tête de 384 octets et 12 248 octets de tas pour huit copies TX, leurs TPD et leur état. [Mesures détaillées](validation/killer-e2400-phase5b/hotes-5b.json)

Dans le processus Q35 instrumenté, le catalogue, le gestionnaire et le courtier partagent un tas qui culmine à **27 464 octets vivants**. L’allocateur obtient au maximum **16 pages de 4 Kio**. Après destruction des objets du scénario, ses allocations vivantes reviennent à zéro. Il conserve ses 16 pages de réserve jusqu’à la sortie du processus, où le noyau confirme la restitution. L’ELF de qualification mesure 275 856 octets. Ses segments chargés couvrent 53 pages de code, 13 en lecture seule et une de données. Ces grandeurs se recouvrent avec les objets déjà détaillés et excluent la pile et les tables du noyau. [Fiche et empreinte ELF](validation/killer-e2400-phase5b/q35-5b.json)

La requête DATA de réservation et sa réponse occupent 152 octets ensemble, dont 64 de requête et 88 de réponse. L’acquittement ajoute 152 octets, soit **304 octets nominaux** après HELLO. Le rejeu exerce 152 octets de plus avec le même reçu. Les trois ensembles de preuves ne traversent pas le canal DATA. Les appels du courtier sur les 264 TX/RX sont des appels internes au processus de test. Le témoin à trois processus exerce séparément les IPC noyau des transports Ethernet. [Scénario 5B](../tests/killer_e2400/scenario_5b.rs) · [Témoin des transports](../tests/killer_e2400/invite.rs)

## Temps CPU sur Darwin

Temps CPU du thread en microsecondes, p50 / p99 de 256 réservations et 256 reçus complets pour chaque géométrie. La réservation comprend les vérifications du gestionnaire. La sélection du catalogue précède l’intervalle. Le coût de lecture de l’horloge reste inclus.

| Cadres | ARM64 réserve | ARM64 reçu | x86_64 Rosetta réserve | x86_64 Rosetta reçu |
|---:|---:|---:|---:|---:|
| 22 | 1,083 / 2,167 | 0,084 / 0,125 | 2,125 / 2,666 | 0,250 / 0,292 |
| 128 | 1,167 / 6,083 | 0,167 / 0,250 | 2,250 / 2,833 | 0,417 / 0,542 |
| 129 | 1,167 / 2,208 | 0,208 / 0,250 | 2,250 / 7,625 | 0,417 / 0,542 |
| 258 | 1,292 / 6,250 | 0,250 / 0,292 | 2,375 / 7,708 | 0,667 / 0,833 |
| 259 | **1,292 / 1,626** | **0,291 / 0,333** | **2,334 / 3,084** | **0,667 / 0,834** |

Sur la ligne 259, la sonde vide p50 / p99 vaut 0,083 / 0,125 µs sur ARM et 0,167 / 0,250 µs sous Rosetta. Sur 128 séries de 264 TX/RX CPU de 60 octets, contrôles de contenu, saturation, notifications et assertions inclus, le lot complet mesure **108,750 / 122,167 µs** sur ARM et **193,754 / 207,921 µs** sous Rosetta. Ces mesures concernent le processus Darwin et le pair fictif. Elles ne mesurent pas une latence au câble.

Sous Rosetta, la première mesure brute du compteur de thread sous-estimait le coût CPU d’un facteur d’environ 41,67 sur ce Mac. Le banc étalonne chaque série sur trois intervalles CPU de 30 ms avec `getrusage`, puis valide la conversion sur un quatrième intervalle indépendant, avec une tolérance de 2 %. Les valeurs brutes et les contrôles figurent dans le JSON. Le temps écoulé du cycle historique de 22 pages, incluant la sélection, passe en p50 / p99 de 630,333 / 830,458 à 647,125 / 689,334 µs sur ARM, et de 1 512,417 / 1 549,208 à 1 650,417 / 1 717,916 µs sous Rosetta. Une série par version permet de constater la hausse sous Rosetta. Elle ne permet pas d’attribuer cette variation au seul registre. [Banc de mesure](../libs/gestionnaire-pilotes/tests/memoire.rs) · [Étalonnage](../libs/gestionnaire-pilotes/tests/support/cpu.rs)

## Matrice de qualification

| Essai | Résultat et limite de la preuve |
|---|---|
| Hôtes Darwin, Rust 1.99.1 | **103/103** en ARM64 natif et **103/103** en x86_64 sous Rosetta. Par ISA, 40 E2400 historiques + 4 essais 5B, 33 gestionnaire, 8 VirtIO-net, 16 NVMe et 2 diagnostics NVMe |
| Bornes, propriété et allouabilité | 0, 1, 22, 128, 129, 258 et 259 acceptés pour une carte valide. 260 et les extrêmes hors intervalle refusés. Vérification de chaque page, de reçus non contigus, des directions et de la réutilisation d’un numéro rendu pendant une autre quarantaine |
| Réservation partielle et admission | Carte de rôles incomplète ou modifiée, quota 258, topologie incomplète, voisin non fiable, portée reset/IRQ commune, révision périmée et garde NVMe exercés. Panne réelle de l’allocateur de test sur les 777 octets. Ressources et révision conservées, nouvel essai de réservation réussi à la génération 1 |
| Formation des preuves et retrait | Dans le même scénario sur les hôtes et sous Q35, le reset laisse 259 cadres. L’arrêt limité à 129 pages et la propriété donnée pour toutes les pages laissent encore 259 cadres. La preuve ultérieure de retrait des traductions pour les 129 premières les libère. Un duplicata, un ancien reçu et une mauvaise génération conservent les 130 restants. La dernière preuve autorise le retour du groupe |
| Endurance du gestionnaire | 256 cycles du témoin de 22 pages par ISA avec variation finale du tas de zéro octet. Le banc de coût exerce séparément 256 cycles pour chacune des cinq géométries mesurées |
| Ring 3 Q35 E2400 5B | QEMU 11.1.3 TCG, 1 vCPU, 256 Mio, sans NIC. Profil 5A signé vérifié, transfert dans le courtier de test de 33 lots de huit, publication 3 + 5, indices 255 → 0, huit crédits, saturation, retour des crédits une seule fois. Une progression impossible entraîne quatre tickets incertains, quatre annulés et la révocation |
| Contrat Ethernet E2400 historique | Six sessions distinctes sur Q35, paquet 5A inchangé et noyau de banc dérivé de 4F. Quatre sessions à 288 TX et 288 RX chacune, lots de 1 à 8 sur PUT/TAKE et DATA. Deux décès sur préfixe publié. Quatre racines maximum, deux boîtes et huit pages IPC en PUT/TAKE, zéro boîte et six pages IPC en DATA, 61 505 cadres libres retrouvés après chaque session |
| VirtIO-net ARM | QEMU 10.0.12. Trois démarrages indépendants du témoin 4F, 13 312 trames de 1 514 octets par sens et par démarrage. Empreinte ELF relue sur l’ESP, volume P9 vérifié, fin avec zéro cadre épinglé et zéro page mappée |
| VirtIO-net + NVMe Q35 | Deux démarrages du modèle de laboratoire 11.1.3. Chaque fois, quatre trames par sens et **22 READ, 7 WRITE, 3 FLUSH** confirmés. 31 opérations de stockage après l’attente de retrait du NIC. Le lecteur hôte contrôle séparément les deux volumes NovaFS P9 et l’ordre des barrières. Avec le reçu, zéro cadre NIC retenu et réadmission refusée pendant le READ CID 10 suivi de sa CQE. En l’absence de reçu, **26 cadres NIC en quarantaine**, NVMe actif dans son domaine indépendant |
| Cibles nues et invariants | `cargo check` du gestionnaire avec `e2400-sim` réussi pour ARM64 et x86_64. L’ELF Ring 3 du courtier est lié et démarré sur x86_64. Le plafond global reste à huit boîtes et la session à huit crédits. Le premier appel de compilation nue a choisi un autre compilateur du PATH et échoué à trouver `core`. La commande reproduite ci-dessous sélectionne explicitement le compilateur stable installé. Sur ARM, Cargo signale un avertissement de compatibilité future dans les dépendances existantes `memchr 2.8.3` et `sha2 0.11.1` |
| MSI-DESKTOP | Inventaire Windows, MMIO, MDIO, IRQ, DMA et trafic au câble non exécutés. L’identité, les alias, le PHY et les garanties de drainage physiques restent à établir |

Les 22 pages VirtIO ont conservé leurs directions et leurs tests de retrait. Leur suivi actif coûte 34 octets supplémentaires par groupe. Les 26 cadres retenus dans le témoin Q35 incluent les ressources de son domaine. Le résultat de ce témoin et les 259 pages logiques de l’E2400 concernent deux périphériques et deux modes de preuve distincts. La réserve historique sur `BUSY (24)` subsiste. [Résultats des profils historiques](validation/killer-e2400-phase5b/compatibilite.json) · [Journaux, captures et volumes fictifs](validation/killer-e2400-phase5b/preuves-5b.tar.gz)

## Inventaire physique et décisions restantes

A8 autorise l’opérateur présent devant le MSI à consulter l’inventaire Windows, les identifiants PCI, la topologie visible, la version du pilote, Secure Boot, BitLocker et les tables ACPI/DMAR accessibles. Les relevés privés restent hors dépôt. Le prochain examen doit résoudre les fonctions sœurs, les alias DMA, les dépendances de stockage, les portées de reset et d’IRQ, ainsi que la propriété firmware. Une donnée inconnue impose le refus avant attribution. [Procédure MSI](killer-e2400-inventaire-msi-5b.md)

Un nouvel arbitrage A7 doit couvrir chaque opération privilégiée réellement nécessaire et sa preuve de retrait, notamment l’accès BAR/MDIO, la configuration IRQ, le bus mastering, les mappages IOMMU directionnels, les barrières DMA et le reset physique. Un A8 distinct doit identifier le binaire et le support avant tout démarrage NovaOS sur MSI, essai au câble ou E/S sur support d’essai. Écriture Windows ou disque interne, changement UEFI, configuration de démarrage, clés et désactivation de Secure Boot restent hors périmètre. [Étapes physiques réservées](killer-e2400-protocole-msi-5a.md)

## Reproduction

Utiliser des destinations neuves sous le répertoire temporaire des worktrees. Le Python du laboratoire fournit les dépendances historiques de préparation OVMF. Les scénarios invités prennent le verrou QEMU partagé. Le banc de temps CPU s’exécute séparément des constructions. Les deux volumes distribués sont des images fictives Q35 de 4 Mio. Après extraction de l’archive de livraison, le lecteur P9 de l’audit a revérifié les deux ancres de chaque volume et retrouvé le contenu consigné par le banc. Les chemins de l’hôte sont remplacés par des variables dans les journaux diffusés. Le fichier `empreintes.json` de l’archive relie chaque pièce à ses empreintes originale et distribuée.

```sh
W=$(dirname "$PWD")
export NOVAOS_TMPDIR="$W/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
D="$NOVAOS_TMPDIR/reproduction-killer-5b"
mkdir "$D"
export PATH="$W/tmp/ovmf-net-outils-20261125/venv12/bin:$PATH"
export PYTHONDONTWRITEBYTECODE=1
export RUSTC="$(rustup which --toolchain stable rustc)"
T=novaOS/tests/killer_e2400
python -B "$T/hotes_5b.py" "$D/hotes"
python -B "$T/invite_5b.py" "$D/courtier" "$W"
python -B "$T/invite.py" "$D/contrat" "$W"
export CARGO_PROFILE_RELEASE_STRIP=none
CARGO_TARGET_DIR="$D/cargo-q35" python -B novaOS/tests/gestionnaire_4f/double.py "$D/double" nominal duo-retrait duo-absent --boots 1
CARGO_TARGET_DIR="$D/cargo-arm" python -B novaOS/tests/gestionnaire_4f/arm.py "$D/arm"
for cible in aarch64-unknown-none-softfloat x86_64-unknown-none
do
  CARGO_TARGET_DIR="$D/cible-$cible" rustup run stable cargo check --release --locked --offline \
    --manifest-path novaOS/libs/gestionnaire-pilotes/Cargo.toml --features e2400-sim --target "$cible" > "$D/$cible.log" 2>&1 || exit 1
done
python -B "$T/preuves_5b.py" "$D/hotes" "$D/courtier" "$D/contrat" "$D/double" "$D/arm" "$D/preuves" \
  --cibles "$D/aarch64-unknown-none-softfloat.log" "$D/x86_64-unknown-none.log"
(cd novaOS/docs/validation/killer-e2400-phase5b && shasum -a 256 -c SHA256SUMS)
```
