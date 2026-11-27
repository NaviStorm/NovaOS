# Premier lancement dynamique VirtIO-net sous NovaOS v0.25.0 — Phase 4D

Le profil de laboratoire AArch64 lance un pilote sélectionné en EL0 pour le NIC PCI `00:07.0`. Le même pilote binaire de 12 784 octets est présent dans le noyau témoin 3E. Le gestionnaire et le lanceur libèrent leurs processus et leurs liaisons après le reçu de création. Le service Ethernet v1, son client et le pilote effectuent ensuite les transferts. Le disque VirtIO-blk indépendant reste sous son superviseur.

La révision invitée qualifiée est `572bf3cf9852f7d1b80a12bc3bb45043f81a732f`. La base 4C est `34bb4e6a49eb28999665821496e7923e8b1fe557`, le témoin 3E effectivement redémarré est `fd3ff113fcaf39bee804eebdcedc44557f41817d`, issu de la livraison `ff9b0322d9f449058a2b575b3d7b7fe868b0d7ad`. Le jalon documentaire est v0.25.0. Le noyau du banc conserve sa version interne v0.20.0 et l’ABI native v1. [Résultats par démarrage](../validation/gestionnaire-pilotes-phase4d/bilan.json) · [Empreintes des captures](../validation/gestionnaire-pilotes-phase4d/captures.json)

## Contrat du groupe

| Étape | Autorité, opération et condition de passage |
|---|---|
| Découverte | Le superviseur relit les 32 emplacements PCI du modèle. Il contrôle les fonctions présentes, le SID 56, la classe Ethernet, les routes INTx et le bit BME à zéro. Il donne au gestionnaire une observation bornée et une nouvelle époque d’inventaire. |
| Admission | Le gestionnaire en EL0 exécute le catalogue 4B.1 et le registre 4C. Il vérifie le manifeste TOML v1, l’origine laboratoire, l’époque de sécurité, la dépendance au profil système signé, la signature du `pkg01` et les SHA-256 du paquet et de l’ELF. Les trois ancres sont injectées à la construction du profil. Le quota signé exige huit opérations en vol au plus. |
| Plan | Sur la première paire de canaux DATA, le pair ouvre la liaison, demande la sélection, rejoue la résolution, lit les octets du plan par curseur et acquitte. Il calcule lui-même leur empreinte. Le gestionnaire vérifie les capacités TX et RX, cinq rôles DMA directionnels, deux files de 16 entrées et 22 cadres, avec listes MMIO et IRQ vides pour le pilote. |
| Réservation et préparation | Le superviseur conserve le groupe, ses cadres et ses preuves. La seconde paire DATA porte `Prepare`, puis `Launch`, avec une époque distincte, un identifiant de requête et une génération. Le superviseur attend le dépôt du titulaire sur sa racine SEND, relit l’inventaire courant et scelle les octets du membre avec l’objet, le démarrage, la révision et la longueur attendus. |
| Création et attribution | Au dernier contrôle, le superviseur vérifie les révisions du catalogue et de la politique immuables du démarrage, l’époque active, la génération et l’identité PCI. Le chargeur relit dans l’espace du processus chaque segment `PT_LOAD` du membre scellé et vérifie aussi les plages à zéro lorsqu’il y en a. Sur ce pilote, 7 660 octets sont comparés, dont zéro octet de BSS. La capacité du courtier est créée pour l’incarnation obtenue. L’identité applicative provient de l’empreinte du pilote. |
| Reçu et publication | Le lanceur rend le PID et l’incarnation effectivement créés. Une consultation et une réémission de `Launch` retournent le même reçu avant l’ACK. Le banc exerce cette perte *logique* en ignorant la première réponse déjà livrée. Après la fin des contrôleurs, le courtier initialise le NIC et le service répond à OPEN. Le premier OPEN réussi borne la mesure de publication. |
| Retrait | Les soumissions, crédits, tickets et prêts RX du service sont fermés suivant le contrat 3E. Le superviseur croise titulaire, incarnation, génération, lecture des registres, complétions IOMMU et détachement. La disponibilité du gestionnaire n’intervient pas dans la libération. Toute preuve manquante maintient les pages concernées épinglées. La réadmission utilise une nouvelle incarnation. |

Le noyau met les acteurs de contrôle à disposition pour une admission à la fois. Chaque acteur crée trois racines. Les capacités de requête, de réponse et de signalement sont directionnelles. L’espace de configuration et les portes de synchronisation sont privés au profil. Le compilateur rejette l’option `gestionnaire-4d` pour une autre cible que l’invité AArch64 prévu. Le profil utilise une confiance de laboratoire et un catalogue immuable embarqué. La persistance, la fraîcheur après remise à zéro et la découverte d’une topologie arbitraire restent ouvertes. [Acteur catalogue et gestionnaire](../../user/gestionnaire-4d/acteur.rs) · [Contrat DATA](../../user/gestionnaire-4d/lancement.rs) · [Supervision et scellement](../../kernel/src/sched/process/net_preparation/dynamique.rs)

## Qualification

Le modèle de référence est QEMU 10.0.12, `virt-10.0,gic-version=3,its=off`, un Cortex-A72 sous TCG, 256 Mio. L’IOMMU VirtIO a des granules de 4 Kio et le contournement au démarrage est désactivé. Le pair Ethernet utilise un socket de laboratoire. Les comparaisons de débit ont le même modèle `539a6c4c…c66d5`, le même firmware `921f2295…91805`, le même relais Limine et la même provenance de disque. Le modèle à lecture MMIO perturbée est épinglé séparément. Tous les disques de travail sont des copies privées.

| Scénario 4A | Partie exercée | Limite de la preuve |
|---|---|---|
| T01, paquet et versions | Sous NovaOS, admission du paquet signé, refus de signature, de droits insuffisants, d’une révision périmée avant réservation et avant attribution, refus de scellement sur empreinte divergente. Relecture de l’ELF sur la FAT de démarrage et de ses segments chargés en EL0. | Les autres combinaisons de versions et d’architectures appartiennent aux suites hôtes. |
| T02, confiance | Sous NovaOS, refus d’une clé révoquée et d’un paquet de développement en politique de distribution. Aucun démarrage du pilote dans ces deux cas. | Politique injectée, sans preuve de fraîcheur persistante après redémarrage. |
| T07, capacités | Liaisons natives directionnelles, refus du plan amputé de TX, ancienne capacité du courtier refusée à chaque réadmission. Traces indépendantes des traductions et des permissions du NIC et du stockage. | Un groupe ARM et ses acteurs de laboratoire. Le second groupe dynamique et les autres classes demandent leur propre mandat. |
| T08, transitions | Réservation interrompue après trois mappings, empreinte erronée au lancement, révision invalidée au dernier contrôle, perte du gestionnaire après attribution et avant init. Un reçu de création consulté puis rejoué. Décès du pilote, révocation et publication Ethernet incertaine pendant le trafic. | La séquence de contrôle est bornée au prototype. Suspension/reprise générale et reprise après perte du superviseur restent ouvertes. |
| T09, clôture ARM | RX partiel, reçus du reset et de l’IOMMU constatés, différés ou absents, réponse de reset incohérente. Le banc recoupe les registres et les traductions QEMU avec les cadres retenus et vérifie le stockage. | Les observations sont propres aux modèles QEMU identifiés. Le détachement n’atteste aucun silicium. |
| T15, ressources | Zéro, six et huit boîtes déjà prises à l’ouverture, refus avec quatre racines préexistantes, transport DATA négocié ou PUT/TAKE, trois endurances de 256 cycles avec état de fin par cycle. | Les cadres proviennent des compteurs de l’invité AArch64. Le lancement matériel x86_64 reste hors A4. |
| T17, contrat et coût | Comparaison au pair du témoin historique, du chemin statique à la même révision et du lancement dynamique. Lots de 1, 4 et 8 trames de 1 514 octets, temps de contrôle mesurés séparément. | Écho bidirectionnel mesuré sous TCG. Les mesures TX seul, RX seul et sur machine physique appartiennent à d’autres périmètres. |

Sur Darwin ARM64 natif et x86_64 sous Rosetta, chacun des deux environnements a exécuté les **26 tests du catalogue et les 28 tests du gestionnaire**, soit 108 exécutions réussies. Un banc de mesure isolé du catalogue reste ignoré par construction sur chaque hôte. Les deux bibliothèques et les deux ELF de contrôle compilent sur les deux cibles nues. Le noyau Q35 historique compile avec sa garde. Son lancement dynamique n’entre pas dans A4. Les tests hôtes portent sur la sélection, les automates et l’encodage. Seuls les invités et les observations du modèle alimentent le bilan DMA. [Recette hôte](../../tests/gestionnaire_4d/hotes.py) · [Inventaire des pièces et SHA-256](../validation/gestionnaire-pilotes-phase4d/inventaire.json)

## Endurance, refus et stockage

**87 démarrages indépendants ont réussi**, trois pour chacun des 29 scénarios. Ils comprennent 81 démarrages dynamiques, trois du témoin statique à la même révision et trois de l’ELF 3E archivé. Les trois endurances nominales de 256 cycles ont transmis **110 592 trames dans chaque sens**. Sur chacun des 768 cycles, le groupe disposait de 61 429 cadres libres avant admission, a atteint 60 939 au point bas et a retrouvé 61 429 à la clôture. À cet instant, le noyau comptait zéro processus utilisateur, zéro page IPC et 16 cadres DMA pour le stockage actif et son contrôleur. Après leur fermeture, le compteur DMA vaut zéro. Les identités de processus, les incarnations, les époques et les générations changent entre admissions dans chaque démarrage.

| Injection ou preuve | Observation sur chacun des trois démarrages correspondants |
|---|---|
| Signature, clé révoquée, distribution, capacités insuffisantes, révision périmée avant réservation, quatre racines déjà créées | Aucun pilote lancé, aucun mapping NIC, aucune trame. Les acteurs d’admission sont collectés. |
| Arrêt après trois réservations de page | 3 MAP, 3 UNMAP acquittés et 1 détachement, aucune traduction du NIC. |
| Empreinte divergente à la création ou plan périmé au dernier contrôle | 22 MAP annulés par 22 UNMAP et un détachement. Aucune session réseau. |
| Décès du gestionnaire après attribution et avant initialisation | Une incarnation pilote avait été créée. Le superviseur a fermé les acteurs, produit 22 UNMAP et détaché le NIC. Aucune traduction NIC, aucune trame, zéro cadre DMA et zéro page IPC à la fin. |
| Fermeture normale, révocation, décès du pilote, publication incertaine, reset différé avec reçu valable | Retour des 22 cadres NIC après les preuves. Le profil court et le reçu IOMMU différé ont aussi exercé une seconde admission. |
| Reset TX absent ou incohérent après RX partiel | 8 cadres NIC rendus, **14 NIC + 9 contrôleur** retenus, réattribution refusée. Le modèle MMIO 3E a aussi exercé l’absence, le retard et l’incohérence de lecture sur le même ELF dynamique. |
| Réponse IOMMU absente | **22 NIC + 9 contrôleur** retenus. La lecture et le FLUSH du stockage aboutissent pendant l’attente. Un mauvais numéro et une génération ancienne sont refusés. |
| Réponse IOMMU différée puis disponible | Les 22 cadres restent épinglés pendant l’attente. Leur restitution puis la nouvelle admission sont observées. |

Le pair hôte vérifie le contenu des trames et enregistre les deux directions en PCAP. Les neuf captures de performance des trois chemins ont la même empreinte des longueurs et octets, hors horodatages, `8658591aaea5181e38ea30d58185d21170f2843ae174ad4ccc3b9a1d9595d0be`. Le contrôleur du banc relit séparément les octets du volume P9 sur le disque de travail. Les **87 volumes** concordent avec le témoin. Pendant l’endurance, les 49 152 lectures de 4 Kio donnent un p50 de 0,071 ms et un p99 de 0,271 ms. Les 12 288 FLUSH donnent respectivement 0,058 et 0,241 ms.

## Mémoire mesurée dans l’invité

Un cadre vaut 4 Kio. Le pic de l’admission complète est calculé sur chaque allocation et libération à partir du minimum de la liste libre. La référence de cycle comprend déjà le disque ouvert. Les colonnes « résident » et « retenu total » comptent le noyau entier. Dans ce profil, le second total inclut également 469 cadres de mémoire vidéo. Les cadres DMA sont déjà inclus dans ces totaux.

| Instantané nominal | Processus | Tables CPU | Piles noyau / utilisateur | IPC | DMA | Résident | Retenu total |
|---|---:|---:|---:|---:|---:|---:|---:|
| Avant ouverture du stockage | 0 | 150 | 32 / 0 | 0 | 0 | 2 026 | 2 495 |
| Gestionnaire et lanceur en contrôle | 2 | 171 | 64 / 32 | 8 | 16 | 2 224 | 2 693 |
| Capacités du pilote attribuées, cinq acteurs présents | 5 | 195 | 112 / 224 | 16 | 38 | **2 544** | **3 013** |
| Contrôleurs terminés, port ouvert | 3 | 176 | 80 / 192 | 10 | 38 | 2 378 | 2 847 |
| NIC restitué, stockage ouvert | 0 | 150 | 32 / 0 | 0 | 16 | 2 054 | 2 523 |
| Stockage fermé | 0 | 150 | 32 / 0 | 0 | 0 | 2 038 | 2 507 |

La différence finale de 12 cadres par rapport au premier relevé correspond à la croissance initiale du tas noyau de 4 à 16 cadres. Elle reste stable durant les 256 cycles de chaque série. Les 38 cadres DMA en activité comprennent 22 pages NIC, 7 pages du disque et 9 pages communes du contrôleur VirtIO-IOMMU. Les files de requêtes et d’événements et les tampons de commandes du contrôleur résident dans ces 9 pages. Les mappings du dispositif VirtIO-IOMMU sont gérés dans le modèle hôte. Les 195 tables au pic sont les pages de traduction CPU de l’invité.

| Périmètre | Cadres supplémentaires | Détail |
|---|---:|---|
| Sélection et plan, deux acteurs EL0 | **171** | Inclut code, données, tas, piles, tables et IPC des deux acteurs pendant cette phase. |
| Admission et cycle nominal | **490** | 1 960 Kio au-dessus du disque ouvert. Mesuré sur les 768 cycles. Le pilote, le service et le client coexistent brièvement avec les deux contrôleurs. |
| Six boîtes préexistantes | **788** | 8 boîtes au point de coexistence, 17 endpoints, 29 pages IPC. Deux boîtes disponibles pour PUT/TAKE lors du provisionnement. |
| Huit boîtes préexistantes | **790** | 8 boîtes au point de coexistence, 19 endpoints, 31 pages IPC. Repli DATA à l’ouverture. Les acteurs et piles utilisés pour occuper le pool sont inclus dans le pic du banc. |

En exploitation du port nominal, les cinq endpoints utilisent dix pages IPC réparties en cinq pages d’endpoints, une de nœuds, une de handles locaux, une d’attentes et deux boîtes. Les acteurs client, service et pilote créent respectivement 0, 4 et 1 racines. Le gestionnaire et le lanceur créent trois racines chacun. Huit boîtes de 4 Kio constituent le maximum global. Le contrat du groupe conserve huit crédits.

**Comparaison avec les 602 cadres prévisionnels.** L’estimation 4C couvre les données communes et leur espace de travail à 64 paquets, 128 nœuds et 32 groupes. Le prototype utilise un paquet, un candidat NIC, un groupe et un plan. Son contrôle mesuré monte à 171 cadres supplémentaires. Le budget opérationnel à retenir pour ce seul cycle est **490 cadres supplémentaires avec stockage déjà ouvert**, auxquels s’ajoute l’image noyau propre au profil. Reproduire la pression maximale du banc demande 790 cadres supplémentaires. Les valeurs de 171 et 490 se recouvrent. Les trois acteurs du port, les pages de traduction et les acteurs de pression ne peuvent pas être extrapolés aux maxima du catalogue. [Périmètre de l’enveloppe 4C](../../libs/gestionnaire-pilotes/BUDGET.md)

| Acteur | Taille ELF | Cadres des segments RX / R / RW | Pile utilisateur réservée | Empreinte de pile observée |
|---|---:|---:|---:|---:|
| Gestionnaire et catalogue | 226 704 o | 45 / 9 / 1 | 64 Kio | 16 896 o |
| Pair et lanceur | 16 816 o | 3 / 1 / 0 | 64 Kio | 1 280 o |
| Pilote VirtIO-net | 12 784 o | 2 / 1 / 0 | 256 Kio | 15 232 o |
| Service Ethernet | 807 008 o | 9 / 4 / 0 | 256 Kio | 155 136 o |
| Client du banc | 763 144 o | 5 / 1 / 0 | 256 Kio | 1 792 o |

Les tailles ELF incluent toutes les sections du fichier lié. Seuls les segments chargeables alimentent l’espace du processus. Les sections hors chargement restent comptées si le fichier entier est embarqué dans l’image noyau. Les empreintes de pile proviennent du premier bloc non nul de 128 octets lu sur des piles initialement nulles. Il s’agit du maximum **observé** aux points de mesure. Un octet réécrit à zéro entre deux sondes peut leur échapper.

Le tas du gestionnaire réserve 65 536 octets. À la fin de la sélection, 14 465 octets de charges sont vivants, avec un pic de 21 946. Le plus grand bloc encore allouable à alignement huit vaut 48 744 octets. Sur les 51 071 octets de différence entre réservé et vivant, 2 327 couvrent ensemble les métadonnées, l’alignement et les trous. Une mesure de fragmentation externe isolée demanderait la carte des blocs de l’allocateur. L’index occupe 88 octets, les documents du catalogue 1 131, la politique 423, l’inventaire 2 232, le registre de groupe 400, les emplacements de plans 576, le plan sélectionné 735 et le journal de reçus 11 264. Ces vues décrivent des objets déjà comptés dans les segments ou le tas.

Le paquet signé occupe 12 943 octets. Sa séquence complète apparaît deux fois dans les segments chargeables du noyau dynamique, dont une dans l’ELF du gestionnaire. Son chargement en EL0 matérialise sa section en mémoire utilisateur. La fenêtre virtuelle de staging a une capacité de 8 Mio. Le membre de 12 784 octets utilise quatre cadres de contenu pendant le scellement, puis les segments installés et une page d’en-têtes prennent le relais. L’image noyau occupe **1 839 cadres en 4D, 1 779 à la même révision en statique et 1 410 dans l’archive 3E**. L’écart imputable à l’activation du profil dans cette construction vaut donc 60 cadres, soit 240 Kio. L’écart historique supplémentaire de 369 cadres appartient aux deux constructions statiques comparées.

## Temps de contrôle et chemin Ethernet

Les temps de contrôle proviennent des 768 cycles nominaux et du compteur invité à 62,5 MHz. Les durées de création et de réconciliation recouvrent une partie des durées d’activation et de retrait. La vérification inclut le profil de confiance, l’admission des signatures et les empreintes. La sélection inclut les échanges DATA et la relecture du plan.

| Opération, en ms | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|
| Découverte PCI | 0,076 | 0,094 | 0,134 | 0,198 |
| Vérification et admission du paquet | 11,801 | 14,049 | 14,914 | 15,830 |
| Inventaire EL0 | 1,052 | 1,272 | 1,939 | 2,181 |
| Sélection et IPC du plan | 5,535 | 6,801 | 7,176 | 7,757 |
| Création du processus scellé | 2,565 | 2,904 | 3,109 | 3,637 |
| Activation du groupe vérifié jusqu’à OPEN | 42,757 | 47,963 | 49,897 | 55,293 |
| Clôture jusqu’à restitution | 8,986 | 11,826 | 13,510 | 15,192 |
| Retrait par le superviseur | 0,529 | 0,685 | 0,914 | 1,426 |
| Réconciliation nominale | 0,405 | 0,530 | 0,709 | 1,064 |

Les bornes 4A visent respectivement 100 ms pour la découverte, 250 ms pour l’activation, 500 ms pour la clôture et 100 ms pour la réconciliation avec preuve disponible. Les maxima observés du profil nominal tiennent dans ces bornes. L’objectif de 1 000 ms pour la récupération du gestionnaire et la reprise après perte du superviseur restent à exercer sous A5. Une preuve définitivement absente conserve une durée d’attente ouverte. [Définition des objectifs](budgets.md)

Chaque cellule Ethernet regroupe 1 024 lots par démarrage, trois démarrages. Les valeurs sont les médianes des débits et des quantiles calculés séparément à chaque démarrage. Le débit utile par sens utilise la somme des temps des lots. Les latences sont des allers-retours par lot.

| Chemin | Lot | Mbit/s | p50 ms | p95 ms | p99 ms |
|---|---:|---:|---:|---:|---:|
| 3E historique | 1 | 8,50 | 1,380 | 1,723 | 1,822 |
| Statique, même révision | 1 | 8,84 | 1,336 | 1,493 | 2,138 |
| **4D** | **1** | **8,73** | **1,359** | **1,530** | **1,658** |
| 3E historique | 4 | 19,29 | 2,494 | 2,664 | 2,974 |
| Statique, même révision | 4 | 20,36 | 2,376 | 2,493 | 2,719 |
| **4D** | **4** | **19,94** | **2,389** | **2,707** | **2,924** |
| 3E historique | 8 | 25,35 | 3,777 | 4,131 | 4,344 |
| Statique, même révision | 8 | 27,02 | 3,558 | 3,717 | 4,068 |
| **4D** | **8** | **26,60** | **3,583** | **4,012** | **4,374** |

Sur l’intervalle complet du pair, le débit médian par sens vaut 20,14 Mbit/s en 3E, 21,30 en statique à la même révision et 21,13 en 4D. En lot de huit, 4D gagne 4,92 % de débit face à l’archive 3E et son p99 augmente de 0,69 %. Face au statique de la même révision, le débit baisse de 1,58 % et le p99 monte de 7,52 %. Les trois séries mesurent la variabilité sous TCG. Elles ne permettent pas d’attribuer ces écarts à un coût déterministe du lancement.

| Médianes par démarrage, 13 312 trames émises et autant reçues | 3E | Statique même révision | 4D |
|---|---:|---:|---:|
| IPC SEND/RECEIVE/PUT/TAKE des trois acteurs par trame émise | 19,588 | 19,590 | 19,587 |
| Ensemble des entrées ABI comptées par trame émise | 19,819 | 19,821 | 19,818 |
| Changements de contexte | 289 716 | 289 580 | 289 721 |
| Réveils | 110 278 | 110 313 | 110 376 |
| Appels observés au courtier du pilote | 45 581 | 45 583 | 45 582 |
| Ticks CPU invité / ticks inactifs au bilan | 1 747 / 1 | 1 655 / 1 | 1 668 / 1 |
| CPU du processus QEMU, utilisateur + système, secondes | 8,266 + 0,655 | 7,908 + 0,627 | 7,966 + 0,649 |

Le dénominateur « trame émise » inclut le cycle d’écho et son retour. Le coût par trame comptée dans les deux directions est la moitié de ce chiffre. Les compteurs CPU invités sont des échantillons d’ordonnancement. Le temps CPU hôte couvre tout le processus QEMU, amorçage compris. L’arrêt effectif des deux contrôleurs avant la première trame et les trois seuls acteurs observés pendant le trafic établissent que le gestionnaire reste hors du chemin de données.

## Reproduction et conservation des preuves

La capsule contient les sorties série, les rapports du banc et les relevés QMP des 87 démarrages, les constructions et les objets signés du laboratoire. Le premier démarrage de chaque scénario contient aussi sa trace QEMU et sa capture PCAP complètes. Les empreintes des traces et captures des deux autres démarrages figurent dans l’inventaire. Le laboratoire conserve les trois exemplaires et les ELF de démarrage. La relecture de livraison a vérifié les 708 membres de la capsule, reproduit le bilan octet par octet, recalculé les 29 empreintes Ethernet du premier démarrage et rejoué les 16 oracles de cycle matériel ainsi que les 10 oracles de refus applicables à ces pièces. Les arguments QEMU des pièces d’origine identifient les fichiers de travail éphémères utilisés lors de l’essai. [Capsule de preuves](../validation/gestionnaire-pilotes-phase4d/preuves.tar.gz) · [Sommes SHA-256](../validation/gestionnaire-pilotes-phase4d/SHA256SUMS)

Pour contrôler la livraison sans QEMU, exécuter depuis la racine du worktree Ethernet. Le bilan peut être recalculé intégralement à partir de la capsule diffusée.

~~~sh
preuves="$PWD/novaOS/docs/validation/gestionnaire-pilotes-phase4d"
(cd "$preuves" && shasum -a 256 -c SHA256SUMS)
relecture=$(mktemp -d)
tar -xzf "$preuves/preuves.tar.gz" -C "$relecture"
python3 -B novaOS/tests/gestionnaire_4d/bilan.py "$relecture" "$relecture/bilan.json"
cmp "$preuves/bilan.json" "$relecture/bilan.json"
~~~

Le rejeu en invité suppose les archives 3E, le worktree du témoin en lecture seule, l’image du disque P9 et les firmwares identifiés du laboratoire. Le lanceur vérifie les empreintes disponibles, acquiert le verrou QEMU partagé pour chaque démarrage et utilise un disque privé. Le sous-répertoire de compilation doit être propre à l’essai. Depuis un checkout propre de la livraison, la recette recompile les mêmes sources invitées et identifie la nouvelle construction. Le commit de la livraison diffère du commit des mesures par le rapport et les outils de contrôle. [Lanceur du banc](../../tests/gestionnaire_4d/qualifier.py) · [Relecture des captures et constitution de la capsule](../../tests/gestionnaire_4d/preuves.py)

~~~sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
laboratoire=$(mktemp -d "$NOVAOS_TMPDIR/phase4d.XXXXXX")
export CARGO_TARGET_DIR="$laboratoire/cargo"
export TMPDIR="$NOVAOS_TMPDIR/"
export CARGO_PROFILE_RELEASE_STRIP=none
python3 -B novaOS/tests/gestionnaire_4d/hotes.py
python3 -B novaOS/tests/gestionnaire_4d/test_esp.py
python3 -B novaOS/tests/gestionnaire_4d/qualifier.py "$laboratoire/campagne" tous --demarrages 3
python3 -B novaOS/tests/gestionnaire_4d/bilan.py "$laboratoire/campagne" "$laboratoire/bilan.json"
~~~

Le test hôte demande macOS ARM64 avec Rosetta, Python avec le module cryptography, les cibles Rust Darwin et nues, et LLVM. Le banc de cette livraison a utilisé macOS 27.2.1 et rustc 1.99.1. Les fichiers de construction donnent les SHA-256 de chaque ELF et paquet. Le modèle perturbé du registre de reset est celui de 3E, de SHA-256 `8bb72b975946a577f0900107188bfc2d47cc8aeae20b1b03d1e5dcb04b40383d`. Il est utilisé uniquement pour les trois cas MMIO, jamais pour comparer les performances. L’archive de qualification des phases précédentes conserve sa propre provenance.

## Défauts corrigés pendant l’intégration

- La construction creuse de l’ESP omettait des secteurs nuls du nouvel ELF. En appliquant ce relevé sur un disque déjà rempli, certains secteurs gardaient des octets de l’ancien fichier. Dans le gestionnaire, cela altérait l’état initial de l’allocateur. Le profil écrit tous les secteurs alloués du nouvel ELF et un lecteur distinct contrôle les deux FAT, la chaîne, la longueur et l’empreinte du fichier avant chaque démarrage. Un test ciblé reproduit l’ancien mélange avec des données fictives. [Test de l’ESP](../../tests/gestionnaire_4d/test_esp.py)
- Le compteur résident omettait la page d’en-têtes d’un ELF installé. Elle apparaît désormais sous `elf_entetes` et revient à zéro à la destruction du processus. Les champs de pic physique proviennent du minimum de cadres libres de l’allocateur pendant toute la phase.
- Le lancement pouvait entamer la négociation du NIC avant l’issue du reçu natif. L’initialisation attend maintenant la fin de l’attribution et du dialogue de lancement. Les acteurs de contrôle sont collectés indépendamment du sort des cadres DMA, y compris lors d’une réservation interrompue.

## Réserves et prérequis de 4E

L’arbitrage A4 porte sur un seul NIC et sur le profil ARM décrit ici. La garde Q35 reste applicable. Les clés, le catalogue, la révision de politique et le raccordement au superviseur correspondent au laboratoire. Le PID et les capacités du pilote sont attribués à chaque lancement. La reconnexion d’un lanceur survivant, le remplacement en ligne du catalogue et le journal durable des reçus ne sont pas qualifiés.

- **A5.** Désigner l’autorité de récupération indépendante, sa preuve de fraîcheur et les règles de rejeu avant toute expérience de perte du superviseur. Définir la charge mémoire admissible d’une quarantaine persistante et le protocole de transfert du journal.
- **A6.** Maintenir le refus de réadmission positive du NIC Q35 sous bail NVMe actif jusqu’à un arbitrage distinct. Définir un oracle de deux groupes et de panne commune avant tout élargissement.
- **A7 et A8.** Soumettre chaque rôle matériel ou périmètre de reset supplémentaire à décision, puis établir un mandat séparé par banc physique.
- Les réserves 3E sur les performances de NovaFS sous trafic et `BUSY (24)` demeurent. La comparaison des copies P9 couvre leur contenu à la fin des essais. Ces scénarios n’injectent ni nouvelle transaction applicative d’écriture NovaFS ni perte d’alimentation.
- Le modèle expérimental, son firmware et les ELF effectivement démarrés sont identifiés par empreinte. La reproductibilité bit à bit du noyau après reconstruction dans d’autres environnements reste à démontrer. [Référence 3E](../architecture-ethernet/phase3e.md) · [Arbitrages 4A](qualification.md)
