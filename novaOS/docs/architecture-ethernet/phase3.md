# Premier port Ethernet v1 sur VirtIO-net

**Qualification QEMU du 26 novembre 2026.** Le port transmet et reçoit des trames Ethernet entre un client isolé et un pair de laboratoire sur AArch64 EL0 et x86_64 Ring 3. Le service utilise la boîte CPU privée de 4 Kio et négocie DATA à l’ouverture en cas d’indisponibilité. Le courtier publie un préfixe de 1 à 8 trames validées sur un anneau matériel de 16 entrées. Une fermeture peut immobiliser des pages DMA même après la fin de tous les tickets TX. Les essais utilisent uniquement des périphériques émulés.

## Sources et périmètre

| Référence | Rôle |
|---|---|
| `c4b1c2100b5db4fa0591a37dd094266b0b0557d9` | Architecture et décision du transport, preuves synthétiques des phases 2 et 2B |
| `56db7d5c0d04490cd284b91567cbbd4736534266` | Audit VirtIO-net et provenance des machines, oracles et firmwares |
| `776a752d5ba4b2ea1c3d36c6e0a0e0aa70200072`, `e81d8e9f59b436ea3e47b694f0f014814d214136` | Socle du relais Q35 et version historique du recyclage des anneaux. Le fichier de qualification ARM repris est identique au second commit |
| `3eebef82f1f2e0b482f2bdb57e2d4d98045c8929` | Reprise ciblée du pilote et du banc des deux machines. [Origine par chemin et blob Git](../validation/ethernet-v1-phase3/origines.json) |
| `5ae3649578716da62d8a82c4e102a8b26b655d60`, `8bd3f9c718310939a23907384c4cca19cf0500b2` | Raccord puis rattachement du catalogue, du titulaire et de l’époque aux instances du noyau |
| `5b1da04b82e08ffebc32edb3475bc6592aa09f13` | Correction du filtrage RX et du témoin de neuvième crédit, qualifiée avant intégration du Q35 double |
| `0d2a629a0fb98f7bd9292646f1a88f52813817d1` | Source finale des 28 démarrages simples et des deux démarrages Q35 avec NVMe |

Le catalogue, le service et l’adaptateur sont installés par le lanceur de cette image de laboratoire. Le client reçoit deux feuilles non délégables. Le service possède deux paires d’endpoints et le pilote une racine SEND non délégable vers le courtier. Le service parle en trames, crédits, tickets et reçus. Les pointeurs de descripteurs, les index VirtIO, les BAR, les IRQ, les traductions DMA et l’en-tête VirtIO de 12 octets restent dans le pilote et le superviseur. [Service](../../user/ethernet/service.rs) · [Protocole du backend](../../user/ethernet/backend.rs) · [Adaptateur](../../user/ethernet/adaptateur.rs) · [Courtier](../../kernel/src/hardware/block/net_trames/v1.rs)

Les commandes privées 32 à 39 sélectionnent la longueur du préfixe déjà validé. Elles passent par la racine matérielle existante du pilote. Le profil historique garde son chemin de publication de 16 trames.

La MAC invitée est `52:54:00:12:34:56`, le pair `02:4e:56:00:00:01` et l’EtherType `0x88b5`. Un socket privé de QEMU relie chaque invité au pair. Le contrôle d’intégrité compare tous les octets de chaque trame à un oracle hôte. Le PCAP conserve les deux sens, sans FCS. Les 16 premières trames de chaque sens ont aussi été comparées entre le pilote historique compilé sans le nouveau profil et le port v1, sur les deux ISA. Les SHA-256 individuels des 16 trames par sens concordent également avec les journaux de pair antérieurs au raccord, `arm-final-512` et `q35-final`. [Comparaison aux originaux](../validation/ethernet-v1-phase3/equivalence-historique.json)

Les empreintes des trames préfixées de leur longueur sont `a4d459cab40119ab52491cc4d1a8be57b8724b5bd639292059b410c5cb972958` en TX et `0246ff94a46978fd3594e693f958f0f67ab375e1dcd045abe5bbd4cc1929873d` en RX.

## Matrice de conformité

| Exigence | Exercice et résultat |
|---|---|
| Découverte, ouverture et autorité | Catalogue privé de 32 octets, NEGOTIATE, OPEN et LINK. Le service vérifie la liaison installée avec le boot ID, le titulaire réel, son incarnation, le canal et l’époque. Demande de droit ADMIN, ancienne époque, accès du client à la porte matérielle et duplication des délégations refusés. Session RX/observation sans TX exercée. |
| Transports | `port` utilise PUT/TAKE sur les deux liaisons. `data` utilise uniquement DATA. `plein` provisionne les huit boîtes avant la création des liaisons Ethernet et le client ouvre ensuite en DATA. Chaque fragment DATA porte son numéro et sa longueur. Le choix reste fixe pendant la session. |
| Trames, admission et filtres | `port` échange 144 trames par sens. MAC destination, source ou EtherType hors profil en TX font refuser le lot avant émission. `filtre` fait recevoir 16 trames d’un autre EtherType ou d’une autre source puis 16 trames admises. Le client reçoit les 16 trames attendues et ferme normalement après les 32 complétions RX. |
| Huit crédits et publication | Les tailles 1 à 8 sont présentes dans l’histogramme matériel. Avec huit crédits engagés, la requête TX supplémentaire est refusée. `boite` vérifie également le refus réel du neuvième PREPARER par le courtier. Un préfixe publié et un suffixe retenu sont visibles par QUERY. |
| Reçus et complétions | L’admission, la publication et la complétion sont distinctes. Test d’une réponse TX perdue suivie d’une répétition identique et d’une consultation, sans doublon au pair. Après publication incertaine, quatre tickets inconnus gardent leur crédit et quatre tickets du suffixe sont annulés. ACK refuse de libérer le préfixe inconnu. |
| Notifications et statistiques | ARM sur la révision, attente IPC effectivement bloquée avant les envois du pair, notification RX, relecture des huit slots et réarmement. RELEASE est idempotent avant réemploi et refuse l’ancienne génération. STATS et les compteurs du pair sont contrôlés. |
| Bouclage et endurance | `endurance` transmet 65 552 trames par sens, soit 8 208 lots logiques. Les compteurs TX et RX franchissent 65 536. Les deux index matériels reviennent à 16. Les octets des charges longues permettent de distinguer les tours. |
| Fermeture et révocation | Drainage prouvé dans le parcours nominal. `court` exerce une fermeture après une seule trame. `revocation` retire la racine matérielle après une publication et vérifie le refus de l’ancien handle. `deces` et `incertain` conservent les pages incertaines. |
| Domaines et stockage | Sur ARM, retrait et rattachement du disque avec le NIC conservé, lectures et FLUSH après la première publication et avant la collecte du lot, relecture après la panne du pilote et vérification du volume P9 d’origine. Sur Q35 simple, contrôle indépendant des entrées VT-d utilisées, des permissions directionnelles et de BME à la fin de la session. Sur Q35 PIO à deux domaines, transaction NovaFS confirmée après la perte du NIC, selon les mesures ci-dessous. |
| ABI, ressources et comportement historique | Registre de 48 appels vérifié, 9 tests du registre réussis. Aucun changement dans `novaOS/abi/` depuis la phase 2B. Le témoin sans `ethernet-v1-port` échange ses 16 trames et conserve son comportement de publication. Les 17 tests synthétiques historiques et les campagnes générales ne sont pas rejoués. |

Le profil active uniquement les capacités du port de laboratoire. Le client attend les notifications en IPC bloquant. Le service sonde le backend sur expiration courte lorsqu’une attente RX est armée. Le lien reflète l’état du backend de laboratoire. Le multicast, les transitions de porteuse d’un réseau physique et les sessions applicatives multiples n’ont pas de qualification sur ce raccord. Les preuves du contrat complet conservent leur provenance. [Contrat v1](contrat-v1-phase2.md) · [Phase 2](phase2.md) · [Phase 2B](phase2b.md)

## Ressources et propriété

- À l’ouverture, le client, le service et le pilote ont respectivement 0, 4 et 1 racine. Le montage nominal réserve cinq endpoints et deux boîtes de 4 Kio, soit 8 Kio de boîte. Le repli DATA explicite réserve cinq endpoints et zéro boîte. Le scénario de pénurie comptabilise treize endpoints, huit boîtes préexistantes et 23 pages IPC lors de la création des liaisons. Le banc libère ensuite ses réservations et le client conserve DATA jusqu’à la fermeture. Les deux propriétaires du banc ont chacun quatre racines. Les huit pages comptent pour 32 Kio.
- Les trois espaces privés incluent 192 pages de pile utilisateur, 48 pages supplémentaires de pile noyau, leurs images et leurs tables. Le compteur des cadres libres couvre également les allocations IPC. Les lignes `RAM phase=eth3_*` permettent de distinguer ces postes. La table de mesure de 16 384 mots occupe en plus 128 Kio statiques dans l’image du noyau instrumenté.
- Les 22 pages DMA de l’interface sont distinctes des pages CPU de l’IPC. Les descripteurs, anneaux disponibles et données TX sont lisibles par le NIC. Les anneaux utilisés et données RX sont inscriptibles par le NIC. Une complétion TX authentifiée par son identifiant rend le crédit matériel réutilisable. La restitution des pages exige également le drainage, l’arrêt du bus et le retrait des traductions.
- Après drainage complet, le profil ARM rend ses 38 cadres épinglés et le profil Q35 simple rend 27 de ses 30 cadres, avec trois cadres permanents. Lorsque les 16 descripteurs RX ne sont pas tous revenus, la remise à zéro et la désactivation de BME ne suffisent pas à prouver leur restitution. Le profil simple conserve alors 31 cadres ARM après retrait du disque, ou 30 cadres Q35. Le redémarrage de l’invité clôt ces essais. La récupération en ligne de ces pages reste ouverte.

| ISA | Cas | Boîtes globales au montage | Endpoints au montage | Pages IPC au montage | Cadres supplémentaires à l’ouverture | Cadres supplémentaires à la publication | Pages IPC après collecte |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A72 | port | 2 | 5 | 9 | 298 | 299 | 0 |
| A72 | data | 0 | 5 | 7 | 296 | 297 | 0 |
| A72 | plein | 8 | 13 | 23 | 297 | 297 | 0 |
| Q35 | port | 2 | 5 | 9 | 298 | 298 | 0 |
| Q35 | data | 0 | 5 | 7 | 296 | 296 | 0 |
| Q35 | plein | 8 | 13 | 23 | 296 | 296 | 0 |

## Débit et latence

Chaque série dédiée transporte 4 608 trames dans chaque sens, pour 128 lots de chaque taille de 1 à 8. La latence du lot part du SEND de la requête TX et s’arrête au retour IPC final du client après réception et restitution. Le débit est exprimé en Mbit/s décimaux d’octets Ethernet émis dans un sens. L’écho RX de même taille est transporté en parallèle du parcours, sans doubler le numérateur. Le débit par cellule utilise la somme des durées de ses lots. Le débit de série utilise la durée continue entre le premier départ et la dernière arrivée.

| ISA | Octets | Lot | Mbit/s TX | p50 (ms) | p95 (ms) | p99 (ms) | p99,9 (ms) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A72 | 60 | 1 | 0,215 | 2,13 | 2,30 | 3,09 | 13,95 |
| A72 | 60 | 4 | 0,384 | 4,90 | 5,51 | 6,22 | 6,39 |
| A72 | 60 | 8 | 0,447 | 8,50 | 9,35 | 11,02 | 12,04 |
| A72 | 1514 | 1 | 5,300 | 2,18 | 2,32 | 2,98 | 13,86 |
| A72 | 1514 | 4 | 9,210 | 5,09 | 5,71 | 6,62 | 12,66 |
| A72 | 1514 | 8 | 10,799 | 8,87 | 9,52 | 10,08 | 12,18 |
| Q35 | 60 | 1 | 0,312 | 1,48 | 1,64 | 4,56 | 5,19 |
| Q35 | 60 | 4 | 0,562 | 3,34 | 3,80 | 3,98 | 3,99 |
| Q35 | 60 | 8 | 0,651 | 5,82 | 6,35 | 6,50 | 6,56 |
| Q35 | 1514 | 1 | 7,690 | 1,52 | 1,72 | 2,11 | 5,42 |
| Q35 | 1514 | 4 | 13,060 | 3,62 | 4,15 | 4,39 | 4,40 |
| Q35 | 1514 | 8 | 15,062 | 6,34 | 6,93 | 7,25 | 8,84 |

Le p99,9 de chaque cellule de 128 observations est le maximum observé par la règle du rang supérieur. Un démarrage par cellule mesure ce raccord. Les seuils comparatifs et les séries inter-démarrages de la phase 2B appartiennent à son expérience synthétique. Les 32 cellules et les temps CPU hôte sont disponibles dans le [bilan machine](../validation/ethernet-v1-phase3/bilan.json).

| ISA | Octets | Mbit/s TX de série | Appels IPC des trois acteurs | Changements de contexte BSP | Médiation moyenne par appel matériel (µs) | CPU du processus QEMU sur le démarrage (s) |
| --- | --- | --- | --- | --- | --- | --- |
| A72 | 60 | 0,397 | 226220 | 268014 | 5,46 | 6,41 |
| A72 | 1514 | 9,576 | 226212 | 268257 | 8,21 | 6,75 |
| Q35 | 60 | 0,581 | 226414 | 268965 | 4,88 | 6,48 |
| Q35 | 1514 | 13,448 | 226393 | 268981 | 9,11 | 6,86 |

Le coût de médiation mesure le temps passé dans la validation, la copie privée éventuelle, l’opération matérielle et l’écriture du résultat au pilote, sur toutes les tentatives y compris WOULD_BLOCK. Le temps d’ordonnancement et les trajets IPC jusqu’au client sont inclus dans la latence de bout en bout. Les vecteurs IPC distinguent SEND, RECEIVE, PUT, TAKE, FEATURES et les autres appels IPC pris en compte. PUT et TAKE comportent chacun une copie CPU sur chaque liaison pour chaque trame en boîte. S’y ajoutent les copies du journal, de l’adaptateur et du courtier vers ou depuis les pages DMA.

## Machines, dépendances et réserves

- **AArch64.** QEMU 10.0.12 de laboratoire, machine `virt-10.0`, Cortex-A72, TCG, un vCPU, 256 Mio, GICv3 sans ITS, VirtIO-IOMMU, firmware ArmVirt/DXE et TPM du banc. Le chargeur Limine direct du témoin échoue avec la chaîne boot-ID du nouveau profil sur A72. Le nouveau profil utilise ensemble le relais EFI et le second étage Limine corrigé. Le second étage A72 corrigé a pour SHA-256 `7b9cae50fc4649db9fb1ed48102c2b90d5df0ae2ad1113149d1b7f8707349b7e`. Les SHA-256 effectivement employés figurent dans chaque `campagne.json`. Le diagnostic avec CPU `max` reste distinct des preuves A72.
- **x86_64.** QEMU 11.1.3 corrigé pour RX, Q35, SMM, TCG, un vCPU, 256 Mio, OVMF de laboratoire, VT-d et INTx GSI 23. L’OVMF expérimental et le modèle QEMU font partie de la configuration vérifiée. Les entrées EFI autorisées, les lignes de lancement et les SHA-256 sont conservés. Le Q35 simple utilise son support de démarrage historique.
- **NovaFS.** Le parcours fichiers historique en mode 11 retourne `BUSY (24)` sur les deux géométries dans l’audit source. Cet essai distinct reste ouvert. Dans les comparaisons historiques, le p95 FLUSH P9-4096 est passé de 146 à 174 µs. Le pic P9-512 a atteint 407,008 µs et le maximum ultérieur sous trafic 3 288,992 µs. La matrice du port emploie P9-512. Les mesures spécifiques de ce lot figurent ci-dessous. [Audit historique, réserves](https://github.com/NaviStorm/NovaOS/blob/56db7d5c0d04490cd284b91567cbbd4736534266/novaOS/docs/virtio-net-audit-cloture-20261126.md)
- **ELF.** Deux constructions hors ligne dans des répertoires Cargo différents, même machine et même compilateur. Pour chaque ISA, l’entrée et les quatre segments PT_LOAD sont identiques, avec leurs adresses, permissions et longueurs. Les empreintes des ELF complets divergent. Le contrôle du démarrage utilise le SHA du fichier réellement exécuté. [Comparaison des ELF](../validation/ethernet-v1-phase3/reproductibilite.json)

### Stockage sous trafic et continuité

Sur A72, le banc a exécuté les lectures de 4 Kio et les FLUSH suivants après la première publication Ethernet et avant la collecte du lot. Les observations de chaque opération sont dans les séries invitées. Le débit du service de fichiers complet sous charge reste à qualifier séparément.

| Cas | Opération | Nombre | p50 (µs) | p95 (µs) | Maximum (µs) |
| --- | --- | --- | --- | --- | --- |
| port | READ 4 Kio | 64 | 76,00 | 100,00 | 326,99 |
| port | FLUSH | 16 | 70,00 | 161,01 | 161,01 |
| deces | READ 4 Kio | 64 | 75,01 | 102,00 | 1592,00 |
| deces | FLUSH | 16 | 69,01 | 172,00 | 172,00 |

Sur Q35 PIO, le modèle historique `q35,smm=on,sata=off` conserve deux fonctions PCI distinctes. Le NVMe utilise SID 48 et DID 1, le NIC SID 56 et DID 3. Un namespace de 512 octets par secteur est copié depuis la graine NovaFS. Les sept processus possèdent au plus quatre racines. Les contrôles d’alias évaluent les trois espaces Ethernet ainsi que les quatre espaces du stockage. Le superviseur, ses vérifications d’IRQ, de BME et des invalidations QI proviennent de quinze modules repris à l’octet près au commit audité. La recette les sélectionne uniquement dans la copie de compilation de ce profil. À la création du raccord, le montage conjoint compte 18 endpoints, trois boîtes et 24 pages IPC. Le dernier bilan NVMe retrouve zéro page IPC.

| Parcours Q35 double | Trames Ethernet TX/RX | E/S NVMe réussies | E/S après désactivation du BME du NIC | Pages épinglées à la fin |
|---|---:|---:|---:|---:|
| Drainage normal | 144/144 | 32 | 20 | 8 permanentes du superviseur |
| Décès du pilote en vol | 4/4 | 32 | 31 | 8 permanentes et 26 du NIC en quarantaine |

Le parcours nominal a enregistré 288 complétions VirtIO entre la première et la dernière commande NVMe, et 9 commandes NVMe entre la première et la dernière complétion VirtIO. Dans les deux cas, le lecteur NovaFS indépendant a confirmé les deux ancres et le passage de l’état `ancien` à `nouveau`. La chronologie des CQE et le journal applicatif placent l’acquittement après F2. Le contrôleur du disque a terminé 7 WRITE, 3 FLUSH et 32 commandes au total. Après le décès du NIC, QTest a relu ses 22 feuilles DMA directionnelles, ses quatre pages de tables et le reçu WAIT du retrait du disque. Le support de boot PIO de 34 Mio reste une réserve historique distincte des pages du port. [Preuves Q35 double](../validation/ethernet-v1-phase3/stockage-q35.json) · [Journaux, traces et namespaces](../validation/ethernet-v1-phase3/stockage-q35.tar.gz)

## Reproduction et preuves

Depuis la racine du worktree, préparer un nouveau répertoire de sortie. Les sources du témoin et les archives d’entrée sont lues en place. Le banc acquiert et libère le verrou partagé pour chaque VM. Les données d’essai et les écritures du firmware ont une destination privée.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export RUSTC="$(rustup which --toolchain stable rustc)"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/ethernet-v1-rejeu"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
export NOVAOS_ETH3_CASE=boite
(cd novaOS && rustup run stable cargo build --locked --offline --release \
  --target aarch64-unknown-none-softfloat --features net-arm-lab,ethernet-v1-port)
(cd novaOS && rustup run stable cargo build --locked --offline --release \
  --target x86_64-unknown-none --features net-q35-lab,ethernet-v1-port)
out=../archives-ethernet-phase3/rejeu-port
outils=../archives-ethernet-phase3/reproduction-8bd3f9c
python -B novaOS/tests/ethernet_v1/port_qemu.py arm boite \
  "$CARGO_TARGET_DIR/aarch64-unknown-none-softfloat/release/novaos" "$out/boite/arm" \
  ../pilote-virtio-net ../archives-virtio-net "$outils/limine-enroll" "$outils/BOOTAA64.EFI" \
  --limine-arm "$outils/limine-a72.EFI"
python -B novaOS/tests/ethernet_v1/port_qemu.py q35 boite \
  "$CARGO_TARGET_DIR/x86_64-unknown-none/release/novaos" "$out/boite/q35" \
  ../pilote-virtio-net ../archives-virtio-net "$outils/limine-enroll" "$outils/BOOTAA64.EFI"
```

Pour les deux essais Q35 avec NVMe, conserver le même environnement et utiliser le constructeur de profil versionné.

```sh
duo=../archives-ethernet-phase3/rejeu-construction-duo
python -B novaOS/tests/ethernet_v1/port_construire_q35.py "$duo" --revision 0d2a629a0fb98f7bd9292646f1a88f52813817d1
for cas in port deces
do
  python -B novaOS/tests/ethernet_v1/port_q35_stockage.py "$cas" "$duo/$cas.elf" \
    "../archives-ethernet-phase3/rejeu-duo/$cas/q35" ../pilote-virtio-net ../archives-virtio-net \
    ../archives-ethernet-phase3/reproduction-8bd3f9c/limine-enroll \
    ../pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs
done
```

Les cas simples sont `port`, `boite`, `data`, `plein`, `veille`, `filtre`, `court`, `deces`, `incertain`, `revocation`, `mesure60`, `mesure1514`, `endurance` et `temoin16`. Le cas `filtre` utilise l’ELF de `veille`. Pour `temoin16`, sélectionner `net-ethernet-lab` à la place de `ethernet-v1-port`, et le chargeur ARM historique direct. L’endurance utilise `--delai 220`. Les arguments complets et les journaux sont dans les preuves.

La série brute `OS-worktrees/archives-ethernet-phase3/lot-0d2a629/` contient les ELF démarrés, captures PCAP gzip, traces QEMU, états PCI et journaux. Le consolidateur relit et hache les octets, contrôle les décomptes, les MAC/EtherType, les préfixes du témoin et les effectifs des cellules. Le paquet Git contient les arguments, les résultats, les séries invitées et des captures représentatives dans les deux sens. Les grosses traces d’endurance restent dans l’archive durable, indexées par SHA-256. [Preuves compactes](../validation/ethernet-v1-phase3/preuves.tar.gz) · [Consolidateur](../../tests/ethernet_v1/port_preuves.py)

Sur Darwin ARM64 natif et sur x86_64 sous Rosetta, les tests ciblés couvrent la rétention du crédit incertain, le bornage et le remplissage TX, puis le refus d’un en-tête RX invalide. Chaque architecture a passé ces trois tests. Les journaux hôtes et le contrôle du registre ABI figurent dans le paquet des preuves Q35.

## Corrections et arbitrages

Le contrôle du neuvième crédit du premier banc attendait que les 16 emplacements VirtIO soient occupés, situation inaccessible avec huit crédits. Le témoin compte désormais les crédits occupés et exige l’observation du refus par le superviseur. La réception d’une trame étrangère terminait aussi la boucle de l’adaptateur. L’adaptateur retire maintenant cette trame du flux livré, compte sa complétion pour réarmer l’anneau et continue le service. Pendant la mise au point du Q35 double, une relecture tardive du scénario Limine après restitution de ses pages a arrêté le noyau. Le lanceur conserve désormais uniquement le discriminant validé avant cette restitution.

Une politique de récupération en ligne devra définir la preuve de retrait suffisante pour un anneau RX partiellement consommé. La restitution des 34 Mio du support PIO et la reproductibilité du fichier ELF complet restent également à instruire. Le port expérimental utilise les droits existants et les 48 sélecteurs ABI v1.

## Installation dynamique ultérieure

Le lanceur expérimental connaît les trois images à la compilation. Une installation dynamique demandera un gestionnaire qui vérifie un manifeste et son ELF signé, sélectionne l’instance matérielle et son groupe d’isolation, négocie l’interface du backend, contrôle les quotas puis publie atomiquement le port et ses délégations. Un remplacement devra clôturer les sessions, classer les tickets en vol, prouver le retrait matériel ou conserver la quarantaine puis émettre de nouvelles incarnations et époques. Cette qualification reste limitée à l’émulation de laboratoire.
