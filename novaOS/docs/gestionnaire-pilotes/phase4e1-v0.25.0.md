# NovaOS v0.25.0 — Reprise du gestionnaire et du lanceur, phase 4E.1

Le 27 novembre 2026, **A5 est qualifié partiellement pour un groupe VirtIO-net AArch64 sous QEMU, pendant un même démarrage**. Le gestionnaire et le lanceur ont été remplacés avec une nouvelle identité et de nouvelles liaisons. Le pilote en cours a conservé son bail. Sur **768 reprises nominales**, le p95 est de **19,94 ms** pour une cible de 1 000 ms. Après chaque cycle, le nombre de cadres libres est revenu exactement à sa valeur initiale. [Bilan par scénario et démarrage](../validation/gestionnaire-pilotes-phase4e1/bilan.json)

| Périmètre | Révision et résultats |
|---|---|
| Profil de reprise | `17bdcd7b0ff6ee112d1c34291ce9d2a7eee37d27`. **69 démarrages**, trois pour chacun des 23 scénarios, 876 cycles, 861 remplacements de contrôle, 870 créations de pilote, 111 132 trames dans chaque sens. Les six cycles sans création correspondent aux refus de scellement et de numéro. |
| Témoin 4D | Trois démarrages supplémentaires du profil 4D, reconstruit à la même révision. Deux cycles par démarrage, pic de 490 cadres. La qualification historique conserve sa révision `572bf3cf9852f7d1b80a12bc3bb45043f81a732f` et sa livraison `de70363f4eab48ccfea11963951451593f37cc59`. |
| Erreur commune de collecte IOMMU | Trois démarrages supplémentaires d’un ELF de diagnostic. Le banc `b81ef4a20a6bf7651ff403009a5fe109fd3181ad` construit une copie du noyau qualifié et archive son diff. Le collecteur reçoit une erreur injectée. Les refus, les traductions maintenues et les 38 cadres retenus sont observés. |
| Hôtes et cibles nues | Sur Darwin ARM64 natif puis x86_64 sous Rosetta, 26 tests du catalogue, 28 du gestionnaire et 3 tests des vues de reprise ont réussi sur chaque ISA. Un test de mesure du catalogue est marqué ignoré par la suite d’origine sur chaque ISA. Les bibliothèques catalogue et gestionnaire compilent pour `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. L’invité de qualification utilise uniquement AArch64. |

## Autorité survivante et responsabilités

| Acteur | Ce qu’il possède et ce qu’il peut faire |
|---|---|
| Gestionnaire EL0 | Sélectionne depuis le catalogue authentifié en 4D. Après remplacement, reconstruit une vue bornée du plan, du groupe, du titulaire, des cadres épinglés, des mappings et des issues. Il contrôle la cohérence des transitions avant d’accuser réception. |
| Lanceur EL0 | Demande la préparation et le lancement avec une intention stable. Consulte l’issue du lancement et son titulaire. Le scellement et la publication native restent effectués par les mécanismes 4D. |
| Superviseur | Conserve un journal volatil, sérialise la prise d’un rôle vacant et vérifie le résultat de processus du noyau. `Preparation` reste propriétaire du bail NIC et de ses 22 cadres physiques. Le journal en conserve les références pour comparaison. |
| Contrôleur VirtIO-IOMMU | Détient les attributions et les reçus matériels. Sa table associe SID, domaine, génération, page et permission directionnelle. La réconciliation confronte cette attribution aux cadres du bail. |
| Pilote et service Ethernet | Le pilote conserve seul sa liaison au courtier. Le service conserve la propriété des tickets, des crédits et des acquittements Ethernet v1. Ils peuvent achever une réception pendant la vacance du contrôle. |
| Stockage | Le bloc conserve son bail et son domaine distincts. La qualification distingue une panne limitée au contrôle du NIC d’une incertitude du contrôleur commun. |

Le journal contient le BootID complet, le contrôleur PCI 00:05.0, le groupe NIC 00:07.0, l’époque d’inventaire, le SHA-256 du plan, la génération matérielle et l’identité complète du processus pilote. Le PID accompagne le triplet BootID, incarnation et génération d’instance. La vue remise aux acteurs contient les masques de propriété et de traduction. Les adresses physiques et le handle du courtier restent privés. [Journal et supervision](../../kernel/src/sched/process/net_preparation/dynamique/reprise.rs) · [Validation des vues EL0](../../user/gestionnaire-4d/reprise_etat.rs)

**Transfert du contrôle.** Un échec d’échange déclenche une consultation du résultat natif du processus. Seul un décès confirmé ouvre une vacance. Le superviseur récolte l’ancienne instance, vérifie sa révocation, crée une image EL0 et lui provisionne deux racines privées, SEND et RECEIVE. La requête envoyée avec l’ancienne capacité échoue. La requête envoyée sur la nouvelle racine avec l’ancienne époque est refusée par le contrôle ordinaire du message. Le nouveau titulaire confirme les 23 mots de la vue par quatre fragments de SHA-256 sur sa liaison. La capacité IPC authentifie l’émetteur. Deux candidats peuvent déposer leur demande avant l’attribution du rôle. Un seul reçoit la vue, l’autre termine avec `REVOKED (10)`. [Protocole des acteurs](../../user/gestionnaire-4d/reprise.rs)

Avant la première sélection, aucun plan ni bail NIC n’existe encore. Le banc ferme les deux extrémités de la découverte interrompue, recrée la paire et reprend la sélection vérifiée avec une nouvelle époque. Après sélection, le remplacement concerne uniquement le rôle perdu. Le profil détecte aussi un décès survenu en dehors de l’injection de vacance, au prochain échange de contrôle. Le temps écoulé avant cet échange dépend de l’activité du service.

**Intentions et reçus.** Une place conserve l’intention de lancement et son issue. Une place distincte est réservée à la clôture. Un second identifiant de création est refusé lorsque la place est occupée. Le rejeu de l’intention en cours renvoie à l’opération déjà engagée. Après création, le noyau corrobore le titulaire et sa liaison au courtier avant que le remplacement puisse consulter l’issue. Un refus de scellement produit une issue certaine avec zéro création. Pour la publication Ethernet incertaine exercée, le superviseur conserve la séquence avant l’effet, enregistre le résultat effectif puis remet ce résultat une fois au pilote. Le pair contrôle l’unicité de la trame. Les autres tickets restent dans leur service propriétaire.

Le superviseur compacte le journal après la restitution corroborée, la résolution de l’intention, la remise du résultat de publication éventuel et la clôture des dialogues. Une preuve manquante conserve le bail. La disparition ou la contradiction du journal gèle toute nouvelle admission du groupe pendant l’époque. Le journal ne dispose d’aucune opération de libération de cadre. Les vérifications de reset, retrait des traductions, acquittement, purge IRQ et restitution demeurent dans le chemin matériel. [Retrait du NIC](../../kernel/src/hardware/block/net_arm.rs) · [Attributions du contrôleur](../../kernel/src/hardware/block/viommu_partage.rs)

## Matrice de qualification

| Scénario 4A | Preuve obtenue en 4E.1 | Limite de la preuve |
|---|---|---|
| T08 — cycle de vie | Reprise avant et après sélection, pendant la réservation après trois MAP, après attribution, pendant la publication, la réconciliation et la quarantaine. Lancement refusé, engagé ou confirmé. Titulaire et reçu corroborés. | Un groupe, un plan et une topologie de laboratoire. |
| T11 — décès et concurrence | Décès du pilote avec opérations en vol, puis du gestionnaire. Deux EL0 demandent le rôle, un seul est admis. Après les deux décès, le rejeu conserve une seule création. Décès du lanceur avant création, après engagement et après création effective. | La relève est amorcée par le superviseur du profil. La cadence d’un futur service permanent de surveillance reste à définir. |
| T12 — perte d’autorité | Effacement de la copie du journal et, séparément, altération d’une seule référence dans cette copie. Dans les deux cas, 22 cadres NIC restent épinglés, la réadmission échoue et les ressources CPU sont collectées. | **T12 partiel.** Le superviseur réside dans le noyau. La perte de ce noyau met fin à l’époque autorisée. Sa récupération et la fraîcheur après redémarrage restent ouvertes. |
| T13 — contrôleur partagé | Reçus différés, numéro et génération obsolètes, reçu IOMMU absent. Dans le diagnostic commun, READ, FLUSH, réconciliation NIC et retrait disque échouent. Les deux attributions restent observables dans la trace. | Le résultat négatif du collecteur est injecté. Les défaillances physiques de l’IOMMU et la remise en service de l’unité commune restent à qualifier. |
| T14 — épuisement | Le dernier numéro de liaison disponible est consommé, la suivante échoue avant MAP ou lancement. Une intention occupée refuse un autre identifiant. La clôture reste possible sous pression avec huit boîtes globales. Une quarantaine empêche une nouvelle réservation. | Les plafonds de la fixture et le journal mono-groupe bornent cette qualification. |
| T16 — stockage indépendant | Pendant l’absence du gestionnaire ou du lanceur, READ, WRITE, FLUSH et relecture réussissent. Après chaque démarrage, le lecteur NovaFS hôte valide les deux ancres et extrait les fichiers témoins. | Le WRITE réécrit à l’identique un secteur existant de la copie jetable. La création d’une nouvelle transaction P9, une coupure F1/F2 et l’amorçage sur catalogue indisponible restent hors de ces essais. |

Le jalon v0.25.0 conserve le manifeste interne NovaOS v0.20.0. Les preuves 4C de simulation et les qualifications historiques Ethernet et NovaFS conservent leur provenance. Les 48 sélecteurs, les huit crédits Ethernet, le noyau Q35, la garde NVMe, NovaAuth et NovaFS ont été contrôlés par comparaison des sources protégées. À l’exécution, les deux premiers acteurs de contrôle ont chacun quatre racines, leur remplaçant en a deux. Le port conserve les comptes client/service/pilote de 0/4/1. Sous pression, huit boîtes et 33 pages IPC au total ont été observées lors de la coexistence. [Oracles du banc](../../tests/gestionnaire_4e1/qualifier.py) · [Référence 4D](phase4d-v0.25.0.md)

## Continuité et quarantaine

Le scénario de contrôle actif couvre l’intervalle entre le décès et l’authentification du remplaçant. Le stockage y effectue 64 lectures de 4 Kio, une réécriture de 512 octets, un FLUSH suivi d’une relecture de ce secteur, ainsi que 16 FLUSH du banc historique. Une réception Ethernet aboutit dans cet intervalle avec le PID et la génération du pilote initial. Le scénario distinct de publication perdue conserve la réponse du courtier pendant la vacance puis la remet sans republier la trame.

Le lecteur natif a retrouvé, sur chacun des 69 démarrages principaux et des trois diagnostics communs, les ancres des générations 4 et 5 et le contenu attendu de `/source` et `/sortie`. Les 2 097 152 octets du volume relu ont pour SHA-256 `70326c6a7cf17c60dbc56083d9b64e5e421162c372e3856a8c622508cf282d52`. Le lecteur de PCAP contrôle séparément les deux directions, les longueurs, les adresses MAC et les octets Ethernet. Les captures normalisées des trois démarrages de chaque scénario principal concordent. [Lecteur du volume](../../tests/gestionnaire_4e1/stockage.py) · [Empreintes des captures](../validation/gestionnaire-pilotes-phase4e1/captures.json)

| Fin du scénario | Cadres DMA retenus | Observation indépendante |
|---|---:|---|
| Clôture nominale | 0 après fermeture du disque | Après chaque cycle NIC, 7 cadres de stockage et 9 du contrôleur restent nécessaires au disque ouvert. |
| Preuve de reset TX insuffisante | 14 NIC + 9 contrôleur | Huit cadres NIC restitués page par page. Le domaine conserve les traductions des 14 autres. |
| Reçu IOMMU absent, journal perdu ou non corroborable | 22 NIC + 9 contrôleur | Vingt traductions NIC restent présentes. Deux UNMAP ont atteint le modèle. Les 22 cadres restent chez le propriétaire en l’absence d’une clôture complète. Le disque a pu fermer son propre bail. |
| Erreur du collecteur commun | 22 NIC + 7 disque + 9 contrôleur | Après un UNMAP réellement exécuté, le diagnostic livre une erreur au collecteur. Le chemin ordinaire conserve les 38 cadres. Les domaines 3 et 1 gardent respectivement 20 et 7 traductions. Le bus mastering du disque a été désactivé. |

L’ELF du diagnostic a pour SHA-256 `b64e2c9aa2ffb424e7e6735984975072267a9d9c8633d85d27e1c25675006c97`. La copie altère seulement le résultat présenté au collecteur et ajoute les observations du banc. Aucun acquittement positif n’est fabriqué. Les sources livrées du noyau demeurent celles de la révision qualifiée. [Recette du diagnostic et contrôles des deux domaines](../../tests/gestionnaire_4e1/controleur.py)

## Endurance, temps et mémoire

Les trois démarrages d’endurance totalisent 768 cycles et 110 592 trames dans chaque sens. Chacun des 768 cycles se termine avec **61 363 cadres libres**, zéro processus utilisateur, zéro page IPC, zéro page de pile utilisateur et 16 cadres DMA du stockage et du contrôleur. Le pic de consommation supplémentaire reste à 493 cadres. Le contrôle porte sur chaque cycle individuel.

| Démarrage | Reprises | p50 | p95 | Maximum |
|---|---:|---:|---:|---:|
| 1 | 256 | 19,313 ms | 20,876 ms | 32,045 ms |
| 2 | 256 | 18,193 ms | 19,116 ms | 20,410 ms |
| 3 | 256 | 18,160 ms | 19,418 ms | 21,819 ms |
| Ensemble | 768 | 18,380 ms | **19,941 ms** | 32,045 ms |

Les quantiles utilisent le rang supérieur sur le compteur invité de 62,5 MHz. La mesure nominale va du déclenchement du décès à l’authentification et à l’acquittement de la vue du remplaçant. Pour les décès détectés au dialogue, le chronométrage part du constat. Pour la reprise avant sélection, il comprend la nouvelle sélection. L’environnement utilise TCG, un Cortex-A72, un vCPU, 256 Mio, GICv3 et la machine `virt-10.0` avec le firmware de laboratoire identifié dans le bilan.

| Consommation | Référence 4D | 4E.1 |
|---|---:|---:|
| Sélection et contrôle | 171 cadres | 174 cadres |
| Pic du cycle nominal avec stockage ouvert | 490 cadres | 493 cadres |
| Pic avec pression IPC | 790 cadres | 793 cadres |
| Pic avec deux candidats concurrents | — | 571 cadres |
| Image noyau du profil | 1 839 cadres historiques, 1 838 pour le témoin reconstruit | 1 905 cadres, soit +67 face au témoin de même révision |

Le surcoût historique de 60 cadres comparait le profil dynamique 4D à son témoin statique. Le différentiel de 67 cadres mesure l’image entière du profil 4E.1, avec ses ELF embarqués et son instrumentation. Le journal occupe **392 octets**, inclus dans les **752 octets** de l’objet de session et de son verrou. Les objets statiques auxiliaires représentent 123 octets supplémentaires, hors alignement inter-objets du lieur. Le journal est à capacité fixe, sans allocation sur le tas. Pour le scénario après attribution, l’échantillonneur compte 176 allocations physiques pendant la sélection et 94 pendant le lancement, pour des pics respectifs de 174 et 88 cadres. Le pic de cycle inclut les allocations et libérations intermédiaires. Ces enveloppes se recouvrent.

| Point observé | Processus | Pages IPC | Piles utilisateur | Piles noyau | Tables | Pages utilisateur hors piles | Cadres DMA |
|---|---:|---:|---:|---:|---:|---:|---:|
| Contrôle initial | 2 | 10 | 32 | 64 | 171 | 78 | 16 |
| Port et contrôleurs initiaux | 5 | 18 | 224 | 112 | 195 | 103 | 38 |
| Gestionnaire disparu après attribution | 4 | 14 | 208 | 96 | 184 | 30 | 38 |
| Gestionnaire remplacé | 5 | 16 | 224 | 112 | 192 | 87 | 38 |
| Deux candidats en concurrence | 6 | 18 | 240 | 128 | 200 | 144 | 38 |
| Fin de cycle nominal | 0 | 0 | 0 | 32 | 150 | 0 | 16 |

## Reproduction et arbitrages

Les empreintes d’image, du modèle, du firmware, du relais EFI, des paquets et du lecteur hôte accompagnent les résultats. La capsule contient 783 fichiers vérifiés après archivage, dont les séries, traces IOMMU, captures Ethernet, relevés QMP et manifestes de construction. Les ELF exécutés restent dans l’archive locale du laboratoire. Le lanceur relit dans l’ESP l’ELF de chaque démarrage. [Capsule complète](../validation/gestionnaire-pilotes-phase4e1/preuves.tar.gz) · [Inventaire](../validation/gestionnaire-pilotes-phase4e1/inventaire.json) · [Sommes de contrôle](../validation/gestionnaire-pilotes-phase4e1/SHA256SUMS)

Depuis un worktree propre de la livraison, avec les dépendances et les archives du banc 4D disponibles dans les worktrees voisins, choisir une nouvelle destination. Le verrou QEMU est acquis par le lanceur historique pour chaque démarrage. Le diagnostic extrait sa base Git dans une copie temporaire privée.

```sh
projet=$(git rev-parse --show-toplevel)
labo=$(dirname "$projet")
export NOVAOS_TMPDIR="$labo/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
export PYTHONDONTWRITEBYTECODE=1
export CARGO_PROFILE_RELEASE_STRIP=none
destination="$labo/archives-gestionnaire-4e1-rejeu"
mkdir -p "$destination"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4e1-rejeu-hotes"
python3 -B novaOS/tests/gestionnaire_4e1/hotes.py > "$destination/hotes.log"
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4e1-rejeu-invite"
python3 -B novaOS/tests/gestionnaire_4e1/qualifier.py "$destination/campagne" tous --demarrages 3
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4e1-rejeu-temoin"
python3 -B novaOS/tests/gestionnaire_4d/qualifier.py "$destination/temoin" reset-court --demarrages 3
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4e1-rejeu-controleur"
python3 -B novaOS/tests/gestionnaire_4e1/controleur.py "$destination/defaut"
python3 -B novaOS/tests/gestionnaire_4e1/preuves.py "$destination/campagne" "$destination/temoin" "$destination/hotes.log" "$destination/defaut" "$destination/capsule"
```

**Arbitrage A5 restant.** Une reprise après perte du superviseur exige une autorité dont le domaine de panne soit distinct, une preuve de fraîcheur et une règle de transfert pour les cadres exposés. La MOA/MOE doit choisir cette autorité et la preuve requise avant d’autoriser une réattribution dans ce cas. Le BootID et le journal volatil qualifient uniquement la même époque. Le TPM logiciel du banc de démarrage ne participe pas à la reprise.

**A6 à A8 restent ouverts.** La garde Q35 sous le même bail NVMe actif est conservée. Les réserves historiques relatives à `BUSY (24)`, aux performances NovaFS sous trafic, au PIO, aux dépendances QEMU/firmware et à la reproductibilité bit à bit des ELF restent applicables. La production, les pilotes physiques et la qualification sur silicium relèvent de mandats distincts. [Arbitrages architecturaux](qualification.md) · [Réserves Ethernet 3E](../architecture-ethernet/phase3e.md)
