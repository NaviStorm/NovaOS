# NovaOS v0.25.0 — Phase 3C, cycle de vie du port Ethernet v1

## Décision

Le port permet la restitution en ligne et le rattachement sur les deux profils simples QEMU lorsque le superviseur détient une preuve de retrait pour la génération courante. Sur AArch64, un anneau RX partiel autorise une restitution **page par page**. Sur Q35 simple, un drainage VT-d confirmé autorise la restitution des **27 cadres privés**. Sur Q35 avec NVMe, le superviseur à deux domaines conserve son contrat de drainage par titulaire.

Sources qualifiées `0ba527c879f4d1c9349885bf3038425a2233d233`. Référence de phase 3 `dc498394108210f0479189a30c3e5d91de601c78`, clôture de phase 3B `32f372820af79f7f0669a6dae731c0e56beffa39`. Le banc double superpose dans une copie temporaire quinze modules vérifiés par blob au commit d’audit `56db7d5c0d04490cd284b91567cbbd4736534266`. v0.25.0 désigne le jalon du mandat. Le manifeste interne NovaOS reste en v0.20.0. Les manifestes de construction identifient chaque ELF. [Phase 3](phase3.md) · [Phase 3B](phase3b.md) · [Recette du profil double](../../tests/ethernet_v1/port_construire_q35.py)

## Conditions de restitution

1. Le courtier ferme la racine SEND du pilote. Le superviseur masque INTx, coupe BME, relit l’identité PCI et le registre de commande, demande le reset et attend le statut zéro. Il sélectionne et relit les deux files de seize entrées et vérifie leur désactivation. La route d’interruption doit aussi être purgée. En cas de défaut, les cadres exposés gardent leur propriétaire de quarantaine.
2. Le superviseur compare les index disponibles et utilisés avec les publications de cette incarnation. Il récupère les entrées `used` arrivées depuis la dernière collecte du pilote. Il contrôle la distance modulo 65 536, l’unicité de chaque ID, l’ensemble des descripteurs publiés, les longueurs RX (72 à 1 526 octets, en-tête privé compris) et la longueur TX nulle. Un index ou un retour incohérent invalide la preuve.
3. Le reçu d’un ticket Ethernet appartient au service. Le retrait DMA appartient au superviseur. Après une réponse perdue, la consultation retrouve le statut incertain. La clôture du matériel et le retour de pages à l’allocateur ne transforment aucun ticket en succès. Le test `recu-collecte` vérifie deux consultations des huit tickets et le refus de leur acquittement. Le pair compte huit TX uniques. Les reçus sont consultables dans le périmètre de la session v1. La réouverture crée une autre session, avec son propre journal.
4. Une restitution efface les 4 Kio de chaque cadre par le chemin existant de l’allocateur. Le prochain titulaire reçoit une nouvelle incarnation, une nouvelle époque et une génération matérielle strictement croissante. Une ancienne clé est présentée à la porte SEND du nouveau pilote. Les séries répétées réessaient également les numéros précédent et courant avant toute allocation pour une seconde admission. Le maximum de génération reste monotone même après une demande refusée.

**VirtIO et VirtIO-IOMMU.** Le reset acquitté termine les interactions avec les virtqueues jusqu’à leur réinitialisation. La spécification impose une file inactive avant de retirer ses buffers. Elle prévoit aussi un protocole distinct de reset individuel lorsque la fonctionnalité correspondante a été négociée. Les commandes UNMAP et DETACH ont des réponses dans la file du contrôleur. Après détachement, le comportement de l’endpoint dépend de la configuration de bypass. Le banc vérifie que le bypass au démarrage est désactivé. Le protocole VirtIO-IOMMU employé ne donne au superviseur aucun reçu séparé attestant le drainage de lectures et écritures PCI déjà traduites. Pour ces profils, NovaOS demande donc aussi une preuve positive de propriété des buffers, même après le reset. [OASIS, VirtIO 1.3, §§ 2.4, 2.6.1, 3.3.1 et 5.13.6](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)

Chaque page de données contient deux slots de 2 Kio. Une page RX est restituable si les deux descripteurs sont revenus. Une page TX l’est si aucun des deux descripteurs n’a de publication en vol. La politique conserve ensemble les six pages d’anneaux tant qu’un descripteur reste incertain. Le propriétaire du domaine vérifie chaque UNMAP avant de rendre un cadre. En cas de retrait partiel, le contexte SID 56 / DID 3 reste fermé aux nouvelles attributions. Le disque SID 48 / DID 1 continue à fonctionner. En cas de retrait complet, les 22 traductions sont retirées et le DETACH acquitté précède la libération du lot.

**VT-d.** Une invalidation globale de l’IOTLB avec DR et DW draine les requêtes mémoire non engagées déjà présentes dans le complexe racine, dans le périmètre défini par Intel. Les transactions vers un pair et les caches d’un endpoint ATS demandent des garanties différentes. Le profil simple exige l’inventaire PCI conventionnel connu, refuse un NIC PCIe, vérifie les capacités de drainage, le fonctionnement de la traduction et de la file d’invalidation, et constate que tous les BME sont coupés. Il efface l’unique entrée de racine puis publie les écritures des tables et de la file. Le DRHD traite CC global, IOTLB global DR+DW et WAIT avec écriture d’un cookie unique. Le collecteur relit les adresses de racine et de file, la tête et la queue, la page de statut, FSTS et la racine vide. QTest relit les descripteurs et le reçu en RAM pendant l’arrêt du vCPU. Ces garanties portent sur les modèles de laboratoire et leur topologie. [Intel, VT-d, § 6.5.4 et descripteurs d’invalidation](https://cdrdv2-public.intel.com/671081/vt-directed-io-spec.pdf)

**Reçus différés.** Le propriétaire de quarantaine garde les cadres, leur génération et la séquence de la commande. Le superviseur peut reprendre la collecte sans réémettre la commande. Une génération ou une séquence différente est refusée. Une seconde tentative après restitution est refusée également. Sur ARM, le superviseur de stockage effectue une lecture et un FLUSH entre le retour en quarantaine et la reprise. Dans le test négatif ARM, le contrôleur a déjà enregistré deux UNMAP, mais la politique de restitution ne reçoit pas l’autorisation de terminer le retrait. Vingt traductions NIC restent actives et les 22 cadres restent épinglés. Sur Q35, QTest distingue un WAIT exécuté mais non livré au propriétaire et l’absence de preuve de reset, où la racine demeure publiée. Une expiration maintient le bail et ses pages en quarantaine.

## Matrice de qualification

| Scénario | AArch64, cycles · TX/RX · NIC rendus/retenus | Q35 simple, cycles · TX/RX · privés rendus/retenus |
|---|---|---|
| Fermeture drainée | 1 · 144/144 · 22/0 | 1 · 144/144 · 27/0 |
| Quatre cycles drainés | 4 · 576/576 · 22/0 | 4 · 576/576 · 27/0 |
| Clôture après une trame | 1 · 1/1 · 8/14 | 1 · 1/1 · 27/0 |
| Réouvertures après RX partiel | 1 · 1/1 · 8/14 | 4 · 4/4 · 27/0 |
| Décès du pilote après publication | 1 · 4/4 · 10/12 | 1 · 4/4 · 27/0 |
| Décès, masque de 4 TX retenu | 1 · 4/4 · 8/14 | 1 · 4/4 · 27/0 |
| Réponse de publication perdue | 1 · 4/4 · 10/12 | 1 · 4/4 · 27/0 |
| Révocation de la racine matérielle | 1 · 4/4 · 10/12 | 1 · 4/4 · 27/0 |
| Transfert groupé interrompu | 1 · 0/0 · 8/14 | 1 · 0/0 · 27/0 |
| Réponse de collecte perdue | 1 · 8/8 · 12/10 | 1 · 8/8 · 27/0 |
| Dernier lot RX après disparition du client | 2 · 0/32 · 22/0 | 2 · 0/32 · 27/0 |
| Preuve du reset non livrée | 1 · 1/1 · 0/22 | 1 · 1/1 · 0/27 |
| UNMAP livré en différé | 2 · 288/288 · 22/0 | Hors profil |
| UNMAP enregistré, livraison retenue | 1 · 144/144 · 0/22 | Hors profil |
| WAIT livré en différé | Hors profil | 2 · 2/2 · 27/0 |
| WAIT exécuté, livraison retenue | Hors profil | 1 · 1/1 · 0/27 |

Les trames TX/RX sont cumulées sur les cycles du démarrage. Les cadres rendus/retenus décrivent la dernière génération. La colonne Q35 inclut les cinq tables privées. Les trois ancres Q35 et les pages du contrôleur ARM sont comptées séparément ci-dessous.

Les masques dans le bilan JSON distinguent les complétions déjà prises par le pilote de celles relues par le superviseur. Dans les cas de décès ou de révocation, quatre TX avaient été publiés et le pilote en avait collecté zéro à la clôture. Le modèle peut avoir terminé leur DMA au moment du reset. L’essai `deces-recu-tx-absent` retient alors la livraison du masque des quatre TX à la politique de restitution. La mesure porte sur la décision du superviseur en présence de quatre descripteurs incertains. La durée d’une transaction physique en aval reste inconnue. Dans `rattrapage`, le client meurt après le premier TAKE et le pair injecte ensuite seize véritables RX par cycle. Le superviseur observe leurs seize ID alors que le pilote en a lu zéro. Les deux cycles aboutissent à une nouvelle admission dans le même invité. Les fichiers PCAP donnent les deux sens séparément.

## Cadres CPU, IPC et DMA

Un cadre représente 4 Kio. Les valeurs suivantes proviennent des quatre cycles nominaux du même démarrage.

| Mesure | AArch64 | Q35 simple |
|---|---:|---:|
| Générations admises | 4 / 5 / 6 / 7 | 1 / 2 / 3 / 4 |
| Cadres libres avant création et après collecte des acteurs à chaque cycle | 61 839 / 61 839 | 32 677 / 32 677 |
| Cadres libres à l’ouverture des acteurs | 61 538 | 32 376 |
| Cadres supplémentaires à l’ouverture | 301 | 301 |
| Tables CPU / piles noyau / piles utilisateur / images utilisateur / IPC supplémentaires | 26 / 48 / 192 / 25 / 10 | 26 / 48 / 192 / 25 / 10 |
| Cadres DMA à l’ouverture | 38 | 30 |
| Cadres DMA après retrait du NIC, disque encore ouvert sur ARM | 16 | 3 |
| Cadres DMA de fin de profil | 0 | 3 |

Sur AArch64, le pic de 38 cadres DMA comprend 22 NIC (6 anneaux, 8 pages RX, 8 pages TX), 9 pour le contrôleur et 7 pour le disque. Sur Q35 simple, le pic est de 30. Après une seule trame, ARM récupère 8 pages TX et garde 14 pages NIC, auxquelles s’ajoutent 9 pages de contrôle après la fermeture du disque. Q35 simple rend ses 22 pages NIC et ses 5 tables privées. En l’absence de preuve de reset, ARM garde 31 cadres après fermeture du disque et Q35 simple en garde 30, dont 3 permanents.

Le relevé au montage nominal donne cinq endpoints, deux boîtes et neuf pages IPC. Le point de mesure suivant, à l’ouverture des trois acteurs, compte dix pages IPC. Après collecte, chacune des quatre itérations retrouve zéro page IPC et zéro processus. Les deux mots de suivi de la génération et de la clé occupent 16 octets statiques du noyau. Sur les ELF écho, le propriétaire de quarantaine occupe 728 octets sur ARM (712 en 3B) et 824 octets sur Q35, verrou inclus. Ces octets sont déjà comptés dans l’image. Le contrôleur partagé ARM occupe par ailleurs 1 848 octets statiques, verrou compris.

Le client, le service et le pilote disposent respectivement de 0, 4 et 1 racine IPC. Le lot utilise toujours huit crédits maximum. Les deux boîtes privées de 4 Kio sont comprises dans les comptes IPC. Le choix explicite de DATA laisse le transport fixé à l’ouverture. Le scénario de pénurie retrouve les huit boîtes préexistantes (32 Kio), treize endpoints et 23 pages IPC lors du montage. Le registre ABI v1 conserve ses 48 sélecteurs. Son sous-arbre Git est identique à celui de la phase 3B (objet `2a547403a796377c339dcc2d18b746ad5e2f2750`). [Contrat du port](phase3.md) · [Tests du contrat et des retours DMA](../../tests/ethernet_v1/retrait_dma.rs)

Sur ARM, les neuf pages de contrôle VirtIO-IOMMU comprennent les deux files et leurs buffers de commande/événement. Les traductions du modèle résident côté QEMU. Le relevé distingue leurs IOVA vivantes des cadres physiques épinglés par NovaOS. Sur Q35 simple, les cinq tables privées sont la table de contexte et les quatre niveaux de traduction. Les trois ancres permanentes sont la racine, la file d’invalidation et la page WAIT. Sur Q35 double, huit pages de service restent attachées au DRHD, dont la racine et les contextes, quatre tables disque, la file et le WAIT. Au pic de 40 cadres DMA s’y ajoutent six cadres NVMe et 26 cadres propres au NIC. Les données DMA figurent déjà dans la mémoire résidente.

## Continuité NovaFS

Sur AArch64, le témoin P9-512 de 2 Mio est relu à l’offset 61 Mio et comparé à son SHA-256 `70326c6a7cf17c60dbc56083d9b64e5e421162c372e3856a8c622508cf282d52`. Les cycles nominaux vérifient des lectures et FLUSH pendant le trafic, puis une lecture après chaque retrait. Les deux reprises différées font également progresser le disque pendant que les 22 pages du NIC restent épinglées. Entre l’UNMAP de la page 0 et le retrait suivant, la trace montre 21 traductions en lecture et 11 en écriture du disque à chacun des deux cycles. Avec livraison retenue, ces compteurs valent 32 et 17.

| Q35 avec NVMe | TX/RX au pair | Commandes NVMe réussies | Après coupure BME du NIC | DMA final |
|---|---:|---:|---:|---|
| Retrait normal | 144/144 | 32 | 26 | 8 permanents |
| Décès du pilote | 4/4 | 32 | 31 | 22 NIC + 4 tables en quarantaine, 8 permanents |

Les deux fonctions PCI ont des périodes de BME actif qui se recouvrent. Le lecteur NovaFS indépendant retrouve les deux ancres et la génération 4. Le contenu de `/sortie` (4 231 octets) a pour SHA-256 `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`. Le journal et les CQE placent la confirmation applicative après le second FLUSH.

Le superviseur Q35 double exige un jeton `Drain` lié au titulaire et à la génération avant de supprimer l’entrée de contexte du seul NIC et de recueillir son WAIT. Le travailleur audité n’émet pas ce jeton sur le chemin de mort locale. Enlever la racine globale retirerait aussi les traductions NVMe. La politique conserve donc les 26 pages du NIC dans ce cas. Une évolution de la preuve de retrait **par endpoint** sur ce superviseur partagé demanderait une qualification distincte.

## Non-régression ciblée

| ISA et série | TX utile, Mbit/s | p99 du lot, ms | IPC par trame TX |
|---|---:|---:|---:|
| AArch64 · 3B publiée | 26,10 | 4,30 | 14,94 |
| AArch64 · témoin 3B rejoué | 27,40 | 3,78 | 14,94 |
| AArch64 · clôture 3B rejouée | 27,17 | 3,91 | 14,93 |
| AArch64 · candidat 3C | 27,50 | 3,72 | 14,94 |
| x86_64 · 3B publiée | 36,21 | 3,14 | 14,98 |
| x86_64 · témoin 3B rejoué | 37,51 | 2,88 | 14,96 |
| x86_64 · clôture 3B rejouée | 37,33 | 2,91 | 14,96 |
| x86_64 · candidat 3C | 36,11 | 3,04 | 14,96 |

| ISA et série | TX des trois démarrages, Mbit/s | p99 des trois démarrages, ms | IPC des trois démarrages |
|---|---|---|---|
| AArch64 · 3B | 27,56 / 27,40 / 26,06 | 3,78 / 3,71 / 3,92 | 14,94 / 14,94 / 14,94 |
| AArch64 · 3C | 27,50 / 26,41 / 27,79 | 3,66 / 4,07 / 3,72 | 14,94 / 14,94 / 14,94 |
| x86_64 · 3B | 38,00 / 36,31 / 37,51 | 2,80 / 3,27 / 2,88 | 14,95 / 15,00 / 14,96 |
| x86_64 · 3C | 37,05 / 34,72 / 36,11 | 2,82 / 3,17 / 3,04 | 14,96 / 14,97 / 14,95 |

Le témoin du tableau publié est l’ELF de `ff864f5bec14315e7374375a92ca47912a0abdc5`. Le second témoin est l’ELF de clôture 3B de `1a7de0e80b91e8b94525ef1e337d8daed97e0406`. Les sources du noyau, des acteurs, de l’ABI, de NovaFS et des composants de ce second témoin sont identiques à celles du commit de clôture `32f3728`. Il a été rejoué trois fois par ISA, sous les mêmes profils, après la série principale.

- **AArch64.** Par rapport au témoin apparié, le débit gagne 0,34 %, le p99 baisse de 1,38 % et le nombre d’IPC est identique. Par rapport au code de clôture 3B, le débit gagne 1,20 %, le p99 baisse de 4,80 % et l’écart d’IPC est de +0,0043 appel par trame.
- **x86_64.** Par rapport au témoin apparié, le débit baisse de **3,71 %** et le p99 du lot augmente de **5,52 %**. Par rapport au code de clôture 3B, les écarts sont respectivement de −3,25 % et +4,28 %. Le débit de 36,11 Mbit/s reste aussi légèrement inférieur aux 36,21 Mbit/s publiés. Le p99 de 3,04 ms et les 14,96 IPC par trame restent en dessous des références publiées de 3,14 ms et 14,98.
- **Contre-épreuve Q35.** Deux autres paires, dans les ordres 3C/3B puis 3B/3C, donnent 36,38 contre 37,92 et 36,33 contre 37,78 Mbit/s (3C contre 3B). Leurs p99 sont de 3,00 contre 2,82 et 2,95 contre 2,84 ms. La baisse de débit se retrouve dans les cinq comparaisons appariées. Ces quatre démarrages sont consignés séparément des médianes principales.
- **Interprétation.** Sur Q35, les médianes restent à une publication matérielle et environ 44 attentes bloquées par lot. Les changements de contexte passent de 132,02 à 132,13 par lot. Le retrait, l’effacement et la réconciliation des pages interviennent après le dernier lot chronométré. Le code machine du courtier a changé de disposition et de taille (3 348 à 3 304 octets pour `Port::demande` dans les ELF Q35). Ces mesures ne permettent pas d’imputer le ralentissement à cette différence. La cause de l’écart Q35 reste ouverte.
- **Critères conservés.** En prenant pour dénominateur les débits antérieurs au regroupement publiés en 3B, le candidat atteint 2,58× sur ARM et 2,37× sur Q35. Les p99 et les IPC restent également inférieurs à cette référence initiale. Cette comparaison concerne la seule cellule décisive de huit trames de 1 514 octets.

Un vCPU TCG et 256 Mio dans les deux profils simples. AArch64 utilise Cortex-A72, `virt-10.0,gic-version=3,its=off`, QEMU 10.0.12 et le relais EFI de laboratoire. Q35 utilise `max`, QEMU 11.1.3 et OVMF de laboratoire avec enrôlement propre à l’ELF. Le consolidateur compare modèles, topologies, firmwares et options puis vérifie la configuration de démarrage scellée de chaque Q35. L’ordre témoin/candidat est inversé au deuxième passage. Chaque démarrage traite 1 024 lots pour chacune des tailles 1, 4 et 8. Les chiffres retenus concernent 3 072 lots de huit par variante et par ISA. Le débit compte les octets Ethernet TX. Le p99 du lot inclut réception, restitution et acquittement. Les durées d’arrêt et de réouverture sont hors de cette fenêtre. [Critères originaux](criteres-phase3b.md) · [Mesures par démarrage](../validation/ethernet-v1-phase3c/bilan.json)

## Réserves et décisions ultérieures

- La qualification couvre les modèles QEMU et firmwares identifiés. Le mode fichiers historique `BUSY (24)`, le support de démarrage PIO de 34 Mio du Q35 double, la variabilité des FLUSH NovaFS sous trafic et la reproductibilité intégrale des ELF restent ouverts. Chaque ELF démarré est identifié par son SHA-256.
- Sur ARM, un RX encore prêté après reset demeure en quarantaine. Un mécanisme attestant le drainage en aval ou une démonstration propre à une autre plateforme devra précéder sa restitution. Sur le Q35 simple, les trois ancres appartiennent au contrôleur encore actif. La récupération de ces ancres demanderait une preuve de désinstallation du DRHD.
- Les réouvertures Q35 ont lieu dans une même exécution du superviseur qui conserve les trois ancres. À la fin du scénario, le point d’entrée du banc arrête le vCPU. Le transfert de ces ancres à un autre superviseur ou leur reprise après son redémarrage restent à qualifier.
- Le futur gestionnaire devra conserver l’identité du groupe isolable, du titulaire, de l’incarnation et de la génération, ainsi que le reçu exact des invalidations. Il devra pouvoir reprendre la réconciliation après la disparition d’un acteur, borner les files de retraits en attente et refuser une nouvelle admission quand le propriétaire précédent subsiste. La qualification de la reprise d’un contrôleur physique, du bouclage de sa file QI sur un grand nombre de générations et de la concurrence entre plusieurs retraits reste à faire.
- Les arbitrages portent sur ces extensions de preuve et de cycle de vie. Le raccord livré utilise les primitives et permissions existantes. Les tickets, filtres MAC/EtherType et permissions directionnelles sont ceux du contrat v1. L’installation dynamique reste un chantier ultérieur.

## Preuves et reproduction

Les 46 démarrages de la série principale se répartissent entre 32 profils simples de cycle de vie et de transport, deux essais de stockage Q35 et douze mesures en écho. Quatre démarrages distincts contre-vérifient l’écart Q35 et six autres rejouent le témoin de clôture 3B, soit 56 démarrages contrôlés au total. Pour les tests hôtes, 12 tests sélectionnés réussissent sur Darwin ARM64 natif et sur x86_64 sous Rosetta, dont trois tests du code de décision effectivement utilisé par le noyau. Les neuf contrôles du registre ABI réussissent et les fichiers générés sont à jour. Les sous-arbres ABI, NovaFS, composants et distribution ont les mêmes objets Git qu’à la clôture de phase 3B.

L’archive brute se trouve dans `OS-worktrees/archives-ethernet-phase3c`. Les répertoires marqués `0ba527c` contiennent les essais de livraison. Les séries intermédiaires restent identifiées par leur propre commit. Le paquet Git contient 464 pièces vérifiées, dont les arguments de lancement, journaux, relevés QMP, captures et manifestes de construction. L’inventaire couvre 738 fichiers bruts. Quatorze rapports signalent une modification locale au lancement pendant l’édition du consolidateur hors VM. Les noyaux proviennent d’extractions Git immuables, et le SHA-256 de chaque ELF exécuté a été vérifié. L’inventaire lie par SHA-256 chaque ELF, trace, PCAP, namespace et support produit des séries retenues. Les contrôles se relisent hors VM. [Bilan par démarrage](../validation/ethernet-v1-phase3c/bilan.json) · [Inventaire SHA-256](../validation/ethernet-v1-phase3c/inventaire.json) · [Pièces compactes](../validation/ethernet-v1-phase3c/preuves.tar.gz) · [Empreintes du dossier](../validation/ethernet-v1-phase3c/SHA256SUMS)

Depuis le worktree Ethernet, avec le répertoire temporaire et le Python 3.12 du laboratoire. Pour un nouveau rejeu, employer un répertoire frère libre. Le lanceur acquiert le verrou QEMU partagé et vérifie le SHA de l’image construite avant chaque démarrage.

```bash
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
rev=0ba527c879f4d1c9349885bf3038425a2233d233
s=0ba527c
brut=../archives-ethernet-phase3c
banc=novaOS/tests/ethernet_v1

# Relire les archives existantes dans un dossier de sortie distinct.
python -B "$banc/bilan_cycle_vie.py" "$brut" "$rev" "$brut/relecture-$s" --contre-q35 --cloture-3b

# Construire les ELF d'un nouveau rejeu.
rejeu=../archives-ethernet-phase3c-rejeu
images="$rejeu/images-$s"
validation="$rejeu/validation-$s"
communs=(cycles cycles-court rattrapage court port deces deces-recu-tx-absent
         incertain revocation segment-interrompu recu-collecte preuve-absente)
python -B "$banc/performance.py" compiler "$images" "${communs[@]}" \
  recu-arm-differe recu-arm-absent recu-vtd-differe recu-vtd-absent \
  echo1514 choix-data plein --revision "$rev"
python -B "$banc/performance.py" rejouer "$images" "${communs[@]}" \
  --cycle-vie --repetitions 1 --sortie "$validation"
python -B "$banc/performance.py" rejouer "$images" recu-arm-differe recu-arm-absent \
  --cycle-vie --arch arm --repetitions 1 --sortie "$validation"
python -B "$banc/performance.py" rejouer "$images" recu-vtd-differe recu-vtd-absent \
  --cycle-vie --arch q35 --repetitions 1 --sortie "$validation"
python -B "$banc/performance.py" rejouer "$images" choix-data plein \
  --repetitions 1 --sortie "$validation"
```

Les essais écho utilisent la même commande `rejouer` avec `echo1514`, la référence `../archives-ethernet-phase3b/images-ff864f5` et le candidat `$images`. Les répertoires cibles sont `echo-3b-$s` et `echo-3c-$s`. Appliquer l’option `--cycle-vie` au candidat. L’ordre des passages est 3B/3C, 3C/3B, 3B/3C, avec `--repetitions` successivement à 1, 2 et 3. Les runs existants sont repris uniquement si leur empreinte, leur cas et leur statut correspondent.

La contre-épreuve Q35 utilise les mêmes commandes avec `--arch q35` et les destinations `contre-3b-$s` et `contre-3c-$s`, en ordre 3C/3B puis 3B/3C. Le témoin de clôture utilise `../archives-ethernet-phase3b/images-1a7de0e`, `--repetitions 3` et `--sortie "$rejeu/cloture-3b-$s"` sur les deux ISA.

Le profil double utilise `port_construire_q35.py "$rejeu/images-duo-$s" --revision "$rev"` sous le même verrou. Rejouer `port` et `deces` avec les chemins de la recette de phase 3 et leur propre `duo-$s/<cas>/q35`. Les fichiers `contrat-$s/{arm64,x86_64}.json` contiennent les commandes Rust exactes et le mode de traduction des tests hôtes. [Recette NVMe](phase3.md#reproduction-et-preuves) · [Lanceur](../../tests/ethernet_v1/port_qemu.py) · [Consolidateur](../../tests/ethernet_v1/bilan_cycle_vie.py)
