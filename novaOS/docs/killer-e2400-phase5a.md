# Killer E2400 — reprise et qualification logicielle 5A

**27 novembre 2026 · NovaOS v0.25.0 · MSI-DESKTOP.** Le client et le service Ethernet v1 échangent des trames avec le nouvel ELF E2400 en Ring 3 sur un pair CPU. Le catalogue accepte le paquet signé. Le gestionnaire refuse sa réservation matérielle avant d’attribuer un bail. Le MSI, son PHY, son stockage et ses réglages de démarrage restent à qualifier sous A8.

## Origine et état de reprise

| Révision | Contenu vérifié | Décision 5A |
|---|---|---|
| `00335af793e456b6fa6546514f65305f9c114165`, branche `codex/pilote-killer-e2400` | Un commit de spécification issu de `5c7f1bfa313a3859ccc0830660438e76d598acb1`. Trois documents couvrent le contrôleur, son architecture et le protocole MSI. Aucun ELF, code de pilote ou relevé produit sur la carte | Conserver cette branche comme provenance. Reprendre la géométrie à titre d’hypothèse et vérifier son coût |
| `908976948ba58fc85e7fe38bfc315f689ed1c44e`, branche `codex/architecture-ethernet` | Gestionnaire 4F qualifié avec VirtIO-net mono-NIC sous QEMU, service Ethernet v1 et mécanismes de liaison et de révocation | Créer `codex/killer-e2400-phase5a` depuis ce commit. Réutiliser les composants communs et conserver leurs preuves |
| `2ed47c1b5e8373a11aa059a339c53ca8cb654f2b` | Révision de code exercée par les deux suites hôtes et les deux démarrages Q35 | Livrer un ELF x86_64 de développement, sa simulation, le paquet et ses essais dans des répertoires propres au E2400 |

[Étude du contrôleur](https://github.com/NaviStorm/NovaOS/blob/00335af793e456b6fa6546514f65305f9c114165/novaOS/docs/killer-e2400-phase0.md) · [Architecture et budget historiques](https://github.com/NaviStorm/NovaOS/blob/00335af793e456b6fa6546514f65305f9c114165/novaOS/docs/killer-e2400-architecture.md) · [Qualification 4F](gestionnaire-pilotes/phase4f-v0.25.0.md) · [Contrat Ethernet v1](architecture-ethernet/contrat-v1-phase2.md)

## Du catalogue au port Ethernet

Le catalogue de 4B reçoit ses clés de laboratoire par injection. Il vérifie le manifeste `novaos.driver/1`, la signature Ed25519, le membre ELF `pkg01`, les empreintes SHA-256 et le reçu signé du profil de médiation CPU. Le plan fige l’image, le profil, les deux capacités explicites TX/RX et le plafond de huit opérations. Il exige la fonction PCI Ethernet `1969:e0a1`, sous-système `1462:115a`, révision `0x10`, classe `02/00`, x86_64 et le profil expérimental. Cette identité provient du mandat historique. Le sous-système `7977` rencontré dans des relevés tiers est rejeté. Le prédicat du manifeste cible les sources internes NovaOS 0.20.x. Le jalon de livraison est v0.25.0. [Manifeste signé](../tests/killer_e2400/paquet/pilote.toml) · [Profil CPU](../user/killer-e2400/profil-cpu.toml)

Le banc extrait le membre unique du plan vérifié sur l’hôte. Dans une copie privée de la base 4F, une sonde de démarrage charge cet ELF, le service et le client existants dans trois processus distincts. Le service possède les quatre racines et délègue des feuilles. Le pilote accepte seulement le marqueur du banc et lie l’état à la session, au canal, au titulaire, à son incarnation et à l’époque de démarrage. Les droits d’administration PCI, MMIO, DMA et IRQ sont absents. Ce montage exerce l’exécution Ring 3 et les canaux réels de NovaOS. La réservation et le lancement d’une instance matérielle restent bloqués. [Extraction](../tests/killer_e2400/extraire.rs) · [Sonde du banc](../tests/killer_e2400/invite.rs) · [Point d’entrée du pilote](../user/killer-e2400/acteur.rs)

Le service, les tickets, les reçus, les notifications et les formats privés de transfert sont ceux d’Ethernet v1. Le mode principal groupe les trames dans les boîtes PUT/TAKE de 4 Kio. Le repli DATA se négocie à l’ouverture. L’adaptateur vérifie les MAC du laboratoire et l’EtherType `0x88b5`, borne les longueurs et limite la publication à un préfixe déjà préparé. Il distingue la progression du consommateur modulo 256 des huit crédits. Les retours RX sont décodés sur une copie CPU cohérente. Le nombre de RFD, l’index, le bit UPDATE et la longueur avec FCS sont contrôlés avant remise au service. Une incohérence de propriété fige l’instance. Les conventions de descripteur ont été recoupées avec le pilote Linux. La publication dans les anneaux TPD/RFD de la carte et l’acquisition cohérente des RRD par le courtier relèvent d’A7. [Adaptateur](../user/killer-e2400/adaptateur.rs) · [Protocole du pair CPU](../user/killer-e2400/service_simule.rs) · [Formats alx, Linux v6.12](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/hw.h)

## Budget de mémoire

Chaque cadre mesure 4 096 octets. Les droits suivants s’entendent du point de vue du NIC. Les anneaux ont des pages distinctes et leurs bases et fins doivent partager les 32 bits hauts de l’IOVA. Le calcul vérifie cette contrainte sur des valeurs fictives. [Géométrie testée](../user/killer-e2400/profil.rs)

| Objet physique proposé | Dimension | Cadres | Accès NIC |
|---|---:|---:|---|
| Anneau de transmission TPD | 256 × 16 octets | 1 | Lecture |
| Adresses de réception RFD | 256 × 8 octets | 1 | Lecture |
| Retours de réception RRD | 256 × 16 octets | 1 | Écriture |
| Tampons TX | 256 × 2 Kio | 128 | Lecture |
| Tampons RX | 256 × 2 Kio | 128 | Écriture |
| **Total proposé** | **1 036 Kio** | **259** | **130 en lecture, 129 en écriture** |

Le budget historique de 300 à 347 pages privées inclut ces 259 cadres, 3 à 4 pages de tables VT-d sous l’hypothèse d’une fenêtre IOVA de 2 Mio, 4 de suivi logiciel, 8 de copies, 2 à 8 d’IPC et 24 à 64 pour le processus et son support. L’infrastructure commune éventuellement nouvelle représente 4 à 16 pages supplémentaires selon cette même étude. La topologie du MSI, le provisionnement des tables et le coût d’un futur courtier détermineront le total réel. Les copies CPU du prototype figurent déjà dans son image résidente.

| Mesure effective sur Q35, identique aux deux démarrages | Résultat |
|---|---:|
| ELF / paquet signé sur disque | 25 080 / 25 241 octets |
| Segment exécutable / lecture seule / données en mémoire | 14 805 / 3 088 / 24 456 octets, pile de 65 536 octets, W^X |
| Espace utilisateur du seul pilote | 12 pages hors pile + 16 de pile = 28 pages, 112 Kio |
| Baisse des cadres libres à la création du pilote, avant tout canal | 52 cadres, 208 Kio, **incluant** les 28 pages utilisateur |
| Groupe complet client, service et pilote, différentiel au montage | 258 cadres avec boîtes, 256 avec DATA, **incluant** le pilote et l’IPC |
| Racines client / service / pilote | 0 / 4 / 0 |
| Boîtes et pages IPC du groupe | 2 boîtes et 8 pages IPC en PUT/TAKE, 0 boîte et 6 pages en DATA |
| Après chacune des six fermetures de chaque démarrage | 61 505 cadres libres, audit initial retrouvé, 0 slot processus occupé, 0 boîte résiduelle |
| Cadres exposés en DMA pendant ces essais | 0 |

Le test compte les 48 sélecteurs et vérifie le diff des composants communs depuis 4F. La limite globale de huit boîtes, les quatre racines par acteur, les huit crédits, la garde NVMe et le manifeste interne restent ceux de 4F. [Bilan de construction et mesures hôtes](validation/killer-e2400-phase5a/hotes.json) · [Comptages dans l’invité](../tests/killer_e2400/invite.rs)

## Matrice de qualification

| Périmètre | Oracle et résultat au commit de code |
|---|---|
| Darwin ARM64 natif et Darwin x86_64 sous Rosetta | 40 essais réussis sur chaque architecture. 21 du contrat v1, 12 de l’adaptateur et du pair CPU, 7 du catalogue et du gestionnaire. Les exécutables de test sont compilés pour chaque ISA sur le même Mac. L’ELF nu x86_64 est reconstruit sur ARM64 et comparé au membre signé. [Journaux et métadonnées](validation/killer-e2400-phase5a/hotes.json) |
| Confiance et admission | Signature, paquet ou profil altéré, révocation, plancher, ISA, sous-système, capacités, direction ou quota incompatibles sont rejetés. Le plan accepté conserve les empreintes. Les trois refus consécutifs de réservation de 259 cadres laissent l’usage initial et le groupe sans bail. Propriété inconnue, fournisseur non fiable, alias non autorisé et bail NVMe actif couverts. [Essais du catalogue](../tests/killer_e2400/catalogue.rs) |
| Propriété et anneaux sur copies CPU | Bourrage TX nul, longueurs, MAC et EtherType, saturation à huit, publication partielle, retour du crédit une seule fois, franchissement des indices, UPDATE absent, FCS exclu, RRD incohérent, ancienne liaison et indépendance entre deux sessions. Le retrait d’une génération exposée conserve par hypothèse les 259 cadres jusqu’à preuve externe. [Essais d’anneaux](../tests/killer_e2400/anneaux.rs) |
| Q35 x86_64, deux démarrages indépendants avec QEMU 11.1.3 TCG | Un vCPU `max`, 256 Mio, EDK2, démarrage sur image privée et `nic=none`. Par démarrage, le diagnostic de l’ELF traite 264 TX/RX en préfixes 3 + 5. L’appel au mode de lancement matériel VirtIO est refusé avec le statut 12. |
| Port v1, par démarrage Q35 | Six sessions. Deux nominales et deux avec perte de réponse traitent chacune 288 TX et 288 RX sur le pair CPU, trames de 60 et 1 514 octets, lots de 1 à 8. Le client contrôle les octets retournés, la contre-pression, les tickets, les compteurs, le réarmement et l’événement de fermeture. Ces quatre sessions totalisent 1 152 TX et 1 152 RX par démarrage. Les deux autres exercent le décès du pilote sur chaque transport. |
| Décès entre soumission et réponse | Huit tickets admis, un préfixe de quatre en issue inconnue, quatre annulés, ACK refusé. La révocation des deux racines du canal du pilote réveille le service. L’audit retrouve les cadres et ressources CPU de départ. |
| MSI-DESKTOP | Inventaire réel, PHY, IRQ, VT-d, trames au câble, débit, latence, retrait DMA, continuité du stockage et retour à Windows **non exécutés**. A7 et A8 restent ouverts. |

Les deux journaux série sont identiques octet pour octet. Les heures de verrou et les empreintes de noyau des deux constructions privées figurent dans les fiches distinctes. Les sections .text des deux noyaux sont identiques. Les sections .rodata, .symtab et .strtab diffèrent entre les constructions privées. L’ELF pilote sélectionné et le paquet signé ont été reproduits à l’octet. [Démarrage 1](validation/killer-e2400-phase5a/q35-1.json) · [Démarrage 2](validation/killer-e2400-phase5a/q35-2.json) · [Journaux bruts](validation/killer-e2400-phase5a/preuves.tar.gz)

Deux essais de mise au point ont motivé des corrections. Les tableaux temporaires de trames débordaient la pile initiale. Le stockage des seize copies a été déplacé dans la section de données à initialisation nulle avec une prise exclusive par processus. Lors du décès injecté, l’attente du superviseur seule laissait le service bloqué sur le canal. La sonde révoque désormais les deux racines du canal après constat du décès en utilisant le retrait de 4F. La matrice ci-dessus a été rejouée après ces corrections.

## Blocages matériels et décision A7

1. `Use::plan` refuse plus de 128 pages DMA par groupe et les preuves de traduction retirée et de propriété rendue reposent sur des masques `u128`. Le plafond global de laboratoire ne résout pas cette limite par groupe. À 128 entrées par anneau, le même calcul exige encore 131 cadres. Proposer à A7 une représentation bornée couvrant les 259 pages, avec droits et preuves par page attachés au bail, à son titulaire, à son incarnation et à sa génération. Tout refus doit précéder l’attribution. Une géométrie réduite exigerait une analyse et une qualification distinctes. [Quota](../libs/gestionnaire-pilotes/src/resources.rs) · [Preuves de retrait](../libs/gestionnaire-pilotes/src/lifecycle.rs)
2. Le lanceur 4F attend le VirtIO-net `1af4:1041`, le SID 56 et la géométrie de laboratoire `(22, 2, 16, 0)`. Le vérificateur DMAR Q35 admet une topologie fermée à une unité connue. Il ne couvre ni les portées réelles de RMRR ni les ponts ou alias potentiels du MSI. Le futur fournisseur doit prouver le groupe physique, l’isolation d’avec le stockage et le transfert de propriété du firmware. [Lanceur 4F](../user/gestionnaire-4d/acteur.rs) · [Périmètre DMAR Q35](../kernel/src/hardware/acpi/q35_dmar.rs)
3. A7 doit délimiter le courtier E2400 et ses opérations typées de lecture d’identité et de lien, d’initialisation des anneaux, de publication, de progression, d’IRQ, d’arrêt et de reset ciblé. Il faut vérifier les barrières et les effets de bord, ainsi que l’origine de la MAC. La particularité MSI/INTx traitée par `alx` interdit de présumer le mode d’interruption de cette révision. La route et les garanties de drain du contrôleur réel restent à établir. [Pilote alx, Linux v6.12](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/main.c)
4. A8 doit autoriser séparément l’inventaire puis les gestes physiques délimités. Une preuve absente ou obsolète maintient le groupe en quarantaine. Il faut un témoin indépendant des trames et de la continuité du stockage. [Protocole MSI et critères d’arrêt](killer-e2400-protocole-msi-5a.md)

## Reproduction

Depuis la racine du worktree, utiliser le cache Cargo de la qualification, Rust stable 1.99.1 avec les cibles Darwin ARM64, Darwin x86_64 et `x86_64-unknown-none`, Rosetta, Python 3.12 avec les dépendances du banc, QEMU/EDK2 et la chaîne de construction des images NovaOS. Renseigner `PYTHON` avec cet interpréteur et `NOVA_WORKTREES` avec le répertoire des worktrees qui porte le verrou commun. Les scripts créent des sorties neuves et la simulation utilise un firmware en lecture seule avec variables privées. [Banc hôtes](../tests/killer_e2400/hotes.py) · [Banc Q35](../tests/killer_e2400/invite.py)

```
sortie=$(mktemp -d "${NOVA_WORKTREES}/tmp/e2400-5a.XXXXXXXX")
"$PYTHON" -B novaOS/tests/killer_e2400/hotes.py "$sortie/hotes"
"$PYTHON" -B novaOS/tests/killer_e2400/invite.py "$sortie/q35-1" "$NOVA_WORKTREES"
"$PYTHON" -B novaOS/tests/killer_e2400/invite.py "$sortie/q35-2" "$NOVA_WORKTREES"
(cd novaOS/docs/validation/killer-e2400-phase5a && shasum -a 256 -c SHA256SUMS)
```
