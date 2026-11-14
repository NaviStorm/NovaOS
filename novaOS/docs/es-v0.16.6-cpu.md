# NovaOS v0.16.6 — Coût CPU de la sauvegarde groupée

## Périmètre

Qualification du prototype sur `codex/es-groupees-0166`, à partir de `4acf3a76d8bd4bab5f293cba6105c92cfcbae205`. La sauvegarde testée part de `vi`, passe par la confirmation de `nsh`, crée un fichier sous un nom libre, puis attend l’acquittement de F2. Le montage est un volume autonome NovaFS v0.9.13 P9. La distribution v0.16.5 et le contrat de remplacement ont leur propre cycle de validation. [Qualification précédente du pilote](es-v0.16.6-dma.md) · [Relevés individuels](validation/es-v0.16.6-cpu.json)

## Traitement des données

Dans la référence, chaque ajout d’extent appelait l’insertion générique dans le B+Tree. Plusieurs entrées consécutives faisaient décoder, copier, sérialiser et hacher plusieurs fois la même branche privée. La recherche de la page source reparcourait aussi le chemin pour chaque run. Enfin, un caractère Unicode dans une fenêtre pouvait faire reprendre le décodage de tout son suffixe. [Préparation de référence](es-v0.16.6-plages.md)

Le nouveau chemin réserve un identifiant d’objet neuf et ajoute les clés d’extents en ordre strictement croissant. Il conserve en mémoire la branche droite de l’arbre, au plus huit nœuds. Lors d’une scission, il scelle le nœud gauche et garde le droit ouvert. En fin de préparation, il scelle les derniers nœuds depuis la feuille vers la racine. Chaque référence provisoire reste dans cette branche privée jusqu’à la fixation du fils. Le manifeste reçoit uniquement la racine définitive. Les nœuds utilisent les fonctions P9 de scission, d’encodage et de vérification. Le lecteur historique peut vérifier les deux générations. [Construction de l’index](../../novaFS/lab/es-groupees/arbre.rs) · [Contrôle des deux racines et des scissions à plusieurs niveaux](../../novaFS/src/tree/tests.rs)

La source est parcourue par fenêtres d’au plus 256 pages. Un recul d’au plus 15 pages logiques retrouve un extent qui chevauche le début de la fenêtre. Le parcours contrôle les références, la famille, les niveaux, le minimum des fils, leurs bornes supérieures héritées et la continuité des extents utiles. Le moteur lit et vérifie chaque page de la source, y compris les octets qui seront supprimés. Dans une fenêtre ASCII intégralement contrôlée, les portions recopiées peuvent reprendre ce résultat lorsque le décodeur de destination est dans un état neutre. Les littéraux, les raccords UTF-8, CRLF et les contrôles restent vérifiés. Le décodeur repart par morceaux de 256 octets pour le texte mixte. [Préparation et validation des octets](../../novaFS/lab/es-groupees/preparation.rs)

Une première candidate avec le nouvel index a encore pris **1 029,642 ms** hors sonde pour l’insertion de 4 Mio au milieu en secteurs de 512 octets. Le délai entre la commande et l’invite de confirmation atteignait 326,497 ms, suivi de 703,145 ms jusqu’au succès. Sur un autre démarrage instrumenté du même cas, F2 était acquis à 615,011 ms sur l’horloge invitée. En entrant les caractères de `w copie`, l’éditeur parcourait une longue ligne pour les besoins de pagination et de projection à chaque caractère, alors que seule la ligne de commande avait changé.

Dans le profil du prototype, les frappes ordinaires et l’effacement en mode commande ou recherche mettent à jour la ligne d’invite et son curseur. L’entrée dans le mode, l’exécution de la commande, la recherche effective, Esc, les séquences terminal et la géométrie restent traités par le chemin complet. Les requêtes de fichier, le courtier de consentement et l’ordre de publication sont identiques. Les tests de flux suivent le contenu affiché et la position du curseur, puis exécutent la recherche et relisent la source. [Fixture de l’éditeur](../tests/fixtures/io-0166-invite.patch) · [Contrôle de l’invite dans l’invité](../tests/es_groupees_invite.py)

Chaque page candidate reçoit une empreinte distincte. SHA-256 couvre le domaine P9, les deux identifiants de volume, l’adresse physique, la génération de naissance et les 4096 octets de la page, bourrage compris. Une copie vers une autre adresse exige donc un nouveau calcul. Le moteur calcule les empreintes d’un lot dans une table bornée à 8 Kio et forme les runs avec un vecteur de 16 empreintes réutilisé. Les deux fenêtres sont allouées à la taille nécessaire, au plus 1 Mio chacune. Leur mémoire initialisée est effacée par mots alignés avant restitution, y compris en cas d’erreur. Les deux architectures invitées utilisent le SHA-256 logiciel existant avec la même politique d’isolation des registres. [Domaine des empreintes](../../novaFS/src/codec.rs) · [Contrat cryptographique et de récupération](../../novaFS/AGENTS.md)

Un témoin isolé remplace uniquement le SHA-256 des données par l’autre bibliothèque RustCrypto déjà verrouillée dans le dépôt. Les deux versions utilisent leur backend logiciel et le niveau d’optimisation 3. Sur le moteur UTM lancé directement, en alternant deux fois les binaires par secteur sur la même grande insertion au milieu, SHA source + candidat prend **59,27–61,21 ms** avec `sha2` 0.10.9 et **77,83–81,43 ms** avec `sha2` 0.11.1. Les huit destinations sont relues intégralement avec le lecteur P9. Le prototype retient 0.10.9. [Mesures et empreintes des binaires du témoin SHA](validation/es-v0.16.6-cpu.json)

Un autre témoin compare les backends RustCrypto 0.10.9 `force-soft` et `force-soft-compact` sans changer le format d’entrée ni l’oracle. En deux nouveaux passages par secteur, la somme des deux SHA prend 55,46–57,02 ms sur le backend courant et 110,58–111,58 ms sur le compact. Les huit sorties de 8 Mio sont conformes. Un simple changement de backend logiciel ne fournit donc pas le gain recherché dans ces deux comparaisons. [Témoins SHA-256 et empreintes des images](validation/es-v0.16.6-cpu.json)

Le prêt multipage demeure scellé pendant le transfert. Le moteur contrôle encore l’identité et la révision de la source, le nom libre, le droit de destination et l’admission finale du prêt avant F1. F1 ordonne les pages CoW et le manifeste. L’ancre alternée est ensuite écrite et F2 doit réussir avant le succès. Un retour du périphérique sur OUT autorise la réutilisation des pages DMA de la requête. La barrière de publication décrit une garantie supplémentaire du stockage. [Ordre des écritures](../../novaFS/docs/novafs-recovery.md) · [Contrat DMA et révocation](es-v0.16.6-dma.md)

Une sonde dédiée encadre la vérification des pages du prêt, le passage en lecture seule et la restitution des droits. Sur le moteur UTM direct, le total des trois fenêtres vaut **4,8–5,1 ms** en politique ordinaire et **14,3–33,1 ms** en arrière-plan. Les huit essais portent sur une insertion de 4 Mio dans une source de 4 Mio, avec les deux géométries. La mesure inclut les invalidations actuelles du TLB. Ce poste seul ne peut expliquer l’écart de plusieurs centaines de millisecondes entre les deux politiques. [Sonde de durée du prêt](../tests/fixtures/io-0166-temps-pret.patch) · [Huit mesures et contrôles du contenu](validation/es-v0.16.6-cpu.json)

## Protocole de mesure

- Le chronomètre monotone du Mac part à l’envoi de `w copie`, après l’entrée en mode commande, et s’arrête à la réception de « fichier enregistré ». La réponse `OUI` est automatique. Le temps de préparation des frappes précède cette fenêtre. La création et la grande insertion injectent un motif de 4 194 304 octets par la fixture de charge du clavier, dans le vrai tampon de l’éditeur. Le fichier source dense a 4 194 304 octets. La grande édition mixte ajoute aussi une modification Unicode à une autre position. Le lecteur hôte construit le résultat attendu indépendamment. [Oracles de contenu et scénarios](../tests/es_groupees_elargies.py)
- L’ELF sans sonde fournit les temps d’acceptation. Un second ELF conserve le même chemin. Il marque le départ dans `vi` avant l’encodage du plan, puis mesure le temps jusqu’au retour de F2 dans le moteur. Les compteurs détaillent le SHA source, le SHA candidat, les extents, le texte, les copies, les réservations de pages, les fenêtres et leur effacement. Les fenêtres de copie incluent la vérification des droits lors de la lecture du prêt. Les chronos de publication, des lots, de l’anneau et du backend se recouvrent. Les phases CPU mesurent du temps écoulé dans le code invité, interruptions et ordonnancement compris. [Définition des compteurs](../tests/vi_ipc_sauvegarde.py)
- Une sonde compte les allers-retours sur les quatre liaisons durant la seule sauvegarde, les dons acquittés et les contrôles locaux de capacités et de prêt. Les copies synchrones dans le noyau, les notifications MMIO et les interruptions ont leurs compteurs distincts. Le relevé fournit également le total majoré des copies locales du prêt. [Convention IPC du prototype](es-v0.16.6-dma.md#publication-et-méthode-de-mesure)
- Hôte Apple M4 Max, 12 cœurs de performance et 4 d’efficacité, 128 Gio, APFS, macOS 27.2 (26B123), Rust 1.99.1. QEMU direct 11.1.3 utilise RAW writeback, un vCPU TCG et 256 Mio. Le témoin QEMU 10.0.12 utilise les bibliothèques de l’application, le même ELF AArch64, le même firmware privé et des disques GPT en RAW ou QCOW2. L’application UTM 5.0.8 démarre des copies jetables avec un Cortex-A72 TCG, 256 Mio, GICv3 sans ITS, TPM CRB, VirtIO-IOMMU et le disque au SID 56. Les traces bloc et IOMMU du profil sont actives. Les sauvegardes de l’application sont sérialisées. D’autres processus du Mac, dont certaines régressions, peuvent tourner pendant les mesures. Le banc ne purge pas le cache APFS. [Topologie et empreintes des invités](validation/es-v0.16.6-cpu.json)
- Dans l’application, la sonde hôte lit `info blockstats` sur un second port série configuré comme moniteur QEMU de la VM jetable. Les lectures de compteurs de `drivecible` encadrent le chronomètre sans entrer dans sa durée. Les deltas IN, OUT et FLUSH doivent correspondre exactement aux compteurs invités. Le témoin du moteur lancé directement utilise `query-blockstats`. Les millisecondes de ces compteurs incluent le traitement de ce nœud QEMU et son attente. L’attribution exclusive au SSD, à APFS ou à la planification de macOS demanderait une autre instrumentation. [Banc de l’application](../tests/es_groupees_utm.py) · [Ports série et moniteur UTM](https://github.com/utmapp/UTM/blob/v5.0.8/Configuration/UTMQemuConfiguration%2BArguments.swift) · [Compteurs bloc QEMU](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html#object-blockdevicestats)
- Deux relevés Darwin du processus QEMU encadrent l’envoi de la commande et la réception du succès, à l’intérieur des deux lectures HMP. L’identification combine le lanceur, le moteur AArch64 et l’UUID du point SPICE privé. Le second relevé vérifie l’identité de l’exécutable et le début de vie du processus. Le temps CPU additionne tous ses threads, y compris le backend bloc. Les compteurs d’instructions et de cycles, lorsqu’ils sont disponibles, portent sur le travail exécuté par le Mac. La disposition de `rusage_info_v6` a été comparée au SDK macOS (464 octets). Un étalonnage de 45,142 ms selon `process_time_ns` a donné 45,160 ms après conversion Mach. L’échantillon d’un seul processus n’attribue pas le coût des sous-traitants APFS, UTM ou des périphériques physiques. [Sonde Darwin](../tests/cpu_hote_utm.py) · [Structure publiée par Apple](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/resource.h)
- Chaque succès UTM de référence est suivi d’un nouvel amorçage autonome, d’une exportation, d’un contrôle GPT, d’une vérification P9 des deux slots et d’une comparaison de la source et de la destination octet par octet. L’ELF effectivement embarqué est identifié depuis l’ESP. Les arrêts injectés utilisent `utmctl stop --force` pendant que l’invité est suspendu au point choisi. La perte de courant du Mac reste hors qualification faute de banc dédié. [Conditions de reprise et d’ambiguïté P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

Pour établir le relevé final, le lecteur hôte a ouvert en lecture seule chaque partition P9 exportée de la matrice UTM, y compris les essais instrumentés. Il a extrait `/source` et `/copie` depuis le slot vérifié de plus grande génération. Le relevé associe à chaque essai cette génération, la longueur de la destination et les deux SHA-256. Le vérificateur compare la destination à celle du même scénario QEMU et la source au motif initial. [Oracles de contenu](../tests/es_groupees_elargies.py) · [Vérificateur des mesures](../tests/es_groupees_verifier_mesures.py)

Pendant la série du 13 novembre, macOS a signalé la session verrouillée à 17 h 07 (UTC+1). L’application UTM continuait d’amorcer les VM, de traiter les commandes du terminal série et d’exporter les disques après redémarrage. À la demande de la MOA, la vérification visuelle de la fenêtre Terminal a été omise. Le rendu de l’invite a été exercé dans les quatre combinaisons architecture/secteur par lecture du flux réel, des cellules et de la position du curseur. Les temps hors sonde représentent le profil PTY réellement exécuté. Les compteurs par type de cœur et le témoin de politique macOS permettent de comparer les conditions de planification sur des essais instrumentés distincts. Le type de cœur de chaque sauvegarde hors sonde demeure inconnu. [Banc de mesure du moteur](../tests/es_groupees_moteur_utm.py)

Le profilage a également relevé un ralentissement du stockage sur la candidate intermédiaire. Pour une création de 4 Mio en QCOW2 clairsemé/4096, un passage instrumenté a duré **1 164,963 ms**. Le nœud QEMU a compté **517,153 ms** pour les 22 écritures et **97,744 ms** pour les deux FLUSH. Le SHA candidat a pris 92,254 ms et la construction d’extents 5,836 ms dans l’invité. Cette observation et les chronos avant la correction de l’invite restent consultables avec leur ELF. [Passages intermédiaires et compteurs du disque](validation/es-v0.16.6-cpu.json)

Sur la candidate finale, l’insertion localisée au milieu en 512 octets a donné un passage instrumenté de **3 117,063 ms**, avec F2 à **3 040,164 ms**. La fenêtre de SHA-256 candidat s’est étendue sur 2 631,437 ms d’horloge invitée. Sur la fenêtre complète, Darwin a compté 507,122 ms de CPU pour le processus QEMU et le nœud bloc a cumulé 20,033 ms de lecture, 16,779 ms d’écriture et 3,562 ms pour les deux FLUSH. Attribuer la totalité des 2 631 ms au calcul SHA serait incompatible avec le temps CPU mesuré. Le reste comprend du temps pendant lequel l’hôte n’a pas exécuté ce processus. L’attribution à une cause précise de planification ou de suspension est indéterminée. La sauvegarde a utilisé 43 échanges avec contrôles et la destination a été intégralement relue au redémarrage. [Passage individuel `pair-cpu-512-inserer-milieu-courant`](validation/es-v0.16.6-cpu.json)

## Résultats hors sonde

Temps du terminal entre commande et succès durable, en millisecondes. Les agrégats par environnement réunissent plusieurs scénarios. La table suivante donne chaque scénario UTM et le JSON fournit aussi chaque cellule QEMU.

| Environnement | Architecture | Essais | p50 | p95 | p99 | Maximum | ≥ 1 s |
|---|---|---|---|---|---|---|---|
| QEMU 11.1.3 RAW | aarch64 | 106 | 109,138 | 169,429 | 210,212 | 223,052 | 0 |
| QEMU 11.1.3 RAW | x86_64 | 106 | 157,600 | 290,387 | 304,595 | 315,227 | 0 |
| Moteur UTM 10.0.12 direct RAW | aarch64 | 6 | 136,381 | 187,065 | 187,065 | 187,065 | 0 |
| Moteur UTM 10.0.12 direct QCOW2 | aarch64 | 6 | 143,699 | 337,931 | 337,931 | 337,931 | 0 |
| Application UTM 5.0.8 QCOW2 clairsemé | aarch64 | 102 | 343,595 | 897,782 | 1083,198 | 2696,987 | 3 |
| Application UTM 5.0.8 QCOW2 préalloué | aarch64 | 24 | 337,907 | 947,653 | 1077,014 | 1077,014 | 1 |

### Grandes insertions dans une source de 4 Mio

Pour ces quatre scénarios, la destination atteint 8 Mio, à un octet près pour la modification mixte. Les autres scénarios de l’application portent sur une création ou une édition localisée. La référence provient de la première matrice de 34 sauvegardes. Les campagnes étant de tailles différentes, les fractions décrivent uniquement les essais exécutés.

| Application UTM | Famille | Sous 1 s / total | Minimum (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Maximum (ms) |
|---|---|---|---|---|---|---|---|
| Référence 4acf3a7, clairsemé | Création ou édition localisée | 26 / 26 | 449,117 | 589,616 | 828,889 | 902,208 | 902,208 |
| Référence 4acf3a7, clairsemé | Insertion de 4 Mio | 3 / 8 | 744,472 | 1019,201 | 1205,886 | 1205,886 | 1205,886 |
| Candidate, clairsemé | Création ou édition localisée | 77 / 78 | 89,731 | 100,625 | 739,136 | 2696,987 | 2696,987 |
| Candidate, clairsemé | Insertion de 4 Mio | 22 / 24 | 133,804 | 762,014 | 1002,748 | 1083,198 | 1083,198 |
| Candidate, préalloué | Création ou édition localisée | 12 / 12 | 90,506 | 106,054 | 673,235 | 673,235 | 673,235 |
| Candidate, préalloué | Insertion de 4 Mio | 11 / 12 | 135,420 | 799,296 | 1077,014 | 1077,014 | 1077,014 |

### Scénarios UTM sur QCOW2 clairsemé

Chaque cellule indique p50 / p95 / p99 en millisecondes. Les répétitions utilisent des démarrages indépendants. La dernière colonne reprend le maximum hors sonde observé à la référence sur les deux géométries, dans la matrice comparable.

| Scénario | 512 octets | 4096 octets | Maximum de référence (ms) |
|---|---|---|---|
| Ajout de 4 Mio en tête | 164,3 / 865,1 / 865,1 | 815,2 / 851,1 / 851,1 | 1107,2 |
| Ajout de 4 Mio en fin | 568,6 / 795,9 / 795,9 | 762,0 / 804,9 / 804,9 | 790,2 |
| Ajout de 4 Mio au milieu | 854,4 / 1083,2 / 1083,2 | 823,7 / 897,8 / 897,8 | 1205,9 |
| Ajout de 4 Mio et édition en deux autres positions | 959,7 / 998,4 / 998,4 | 137,3 / 1002,7 / 1002,7 | 1163,6 |
| Création de 4 Mio | 378,6 / 395,5 / 395,5 | 398,6 / 649,1 / 649,1 | 589,6 |
| Insertion en tête | 689,2 / 2697,0 / 2697,0 | 466,0 / 622,8 / 622,8 | 774,6 |
| Insertion au milieu | 93,2 / 308,4 / 308,4 | 200,8 / 504,7 / 504,7 | 902,2 |
| Insertion en fin | 588,5 / 654,2 / 654,2 | 98,7 / 641,5 / 641,5 | 828,9 |
| Insertion à trois positions | 93,2 / 519,0 / 519,0 | 97,1 / 762,7 / 762,7 | 525,9 |
| Suppression en tête | 98,3 / 454,1 / 454,1 | 95,0 / 503,9 / 503,9 | 536,6 |
| Suppression au milieu | 96,4 / 319,0 / 319,0 | 97,4 / 343,6 / 343,6 | 593,1 |
| Suppression en fin | 94,9 / 583,8 / 583,8 | 93,6 / 692,1 / 692,1 | 563,4 |
| Suppression à trois positions | 100,6 / 622,8 / 622,8 | 107,8 / 739,1 / 739,1 | 645,3 |
| Remplacement en tête | 95,6 / 648,0 / 648,0 | 96,6 / 506,8 / 506,8 | 730,5 |
| Remplacement au milieu | 96,5 / 494,3 / 494,3 | 601,5 / 640,4 / 640,4 | 633,8 |
| Remplacement en fin | 366,1 / 518,3 / 518,3 | 427,6 / 470,3 / 470,3 | 689,7 |
| Remplacement à trois positions | 366,5 / 542,7 / 542,7 | 433,2 / 839,1 / 839,1 | 727,4 |

### Effet de la préallocation QCOW2

Provisionnement complet par `qemu-img convert -o preallocation=full` avant import. Les deux profils conservent les clusters de 64 Kio, la même image logique, les traces et les barrières. Les valeurs suivantes sont les temps individuels dans l’ordre des répétitions.

| Scénario | Secteur | Clairsemé (ms) | Préalloué (ms) | Écart des p50, préalloué − clairsemé (ms) |
|---|---|---|---|---|
| Création de 4 Mio | 512 | 395,510 / 97,773 / 378,595 | 617,209 / 106,054 / 90,506 | -272,541 |
| Création de 4 Mio | 4096 | 649,108 / 94,353 / 398,581 | 337,907 / 115,949 / 101,340 | -282,632 |
| Insertion au milieu | 512 | 308,422 / 93,157 / 92,921 | 673,235 / 453,416 / 101,523 | 360,259 |
| Insertion au milieu | 4096 | 200,824 / 95,509 / 504,651 | 655,170 / 98,121 / 95,030 | -102,703 |
| Ajout de 4 Mio au milieu | 512 | 1083,198 / 740,615 / 854,398 | 877,613 / 836,599 / 138,409 | -17,799 |
| Ajout de 4 Mio au milieu | 4096 | 733,588 / 897,782 / 823,669 | 724,964 / 801,875 / 151,843 | -98,705 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | 959,745 / 998,371 / 651,279 | 799,296 / 803,117 / 136,319 | -160,449 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | 1002,748 / 137,331 / 133,804 | 1077,014 / 947,653 / 135,420 | 810,322 |

### Profils invités et backend hôte

Comparaison instrumentée sur démarrages distincts, même Mac, même profil et même jeu de données. Les lignes de la référence regroupent dans un seul compteur le SHA candidat et les extents. Pour la candidate, la colonne correspondante additionne les deux mesures disjointes. La durée jusqu’à F2 a pour origine le marqueur invité du départ de la sauvegarde. Les deux FLUSH apparaissent aussi dans la dernière colonne.

| Opération | Secteur | Version | SHA source | SHA candidat + extents | Texte | Jusqu’à F2 | Commande → succès | Backend IN / OUT / FLUSH (ms) |
|---|---|---|---|---|---|---|---|---|
| Ajout de 4 Mio au milieu | 512 | 4acf3a7 | 146,3 | 444,7 | 80,4 | 1103,5 | 1464,4 | 35,4 / 42,7 / 18,9 |
| Ajout de 4 Mio au milieu | 512 | Candidate | 117,5 | 265,4 | 44,7 | 701,7 | 828,9 | 13,1 / 14,5 / 2,7 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | 4acf3a7 | 89,1 | 264,7 | 57,9 | 836,8 | 891,2 | 17,8 / 19,1 / 2,1 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | Candidate | 101,2 | 167,5 | 35,5 | 785,2 | 851,1 | 32,9 / 13,6 / 3,7 |
| Création de 4 Mio | 512 | 4acf3a7 | 0,0 | 199,8 | 22,1 | 451,5 | 603,7 | 1,6 / 10,3 / 4,2 |
| Création de 4 Mio | 512 | Candidate | 0,0 | 137,4 | 27,0 | 395,6 | 484,4 | 1,7 / 10,7 / 3,7 |
| Insertion au milieu | 512 | 4acf3a7 | 18,6 | 28,9 | 6,7 | 98,8 | 116,4 | 2,5 / 1,5 / 1,7 |
| Insertion au milieu | 512 | Candidate | 132,6 | 2641,4 | 23,3 | 3040,2 | 3117,1 | 20,0 / 16,8 / 3,6 |
| Ajout de 4 Mio au milieu | 4096 | 4acf3a7 | 79,7 | 279,0 | 39,7 | 761,9 | 1217,5 | 37,8 / 13,5 / 3,3 |
| Ajout de 4 Mio au milieu | 4096 | Candidate | 126,4 | 255,0 | 61,2 | 760,5 | 870,4 | 15,5 / 32,9 / 6,6 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | 4acf3a7 | 110,1 | 211,9 | 51,5 | 700,4 | 883,2 | 31,4 / 12,9 / 2,2 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Candidate | 56,3 | 116,2 | 22,0 | 596,4 | 846,9 | 46,2 / 23,9 / 5,2 |
| Création de 4 Mio | 4096 | 4acf3a7 | 0,0 | 180,5 | 20,5 | 405,7 | 552,6 | 1,2 / 8,8 / 4,6 |
| Création de 4 Mio | 4096 | Candidate | 0,0 | 117,4 | 26,4 | 332,0 | 465,4 | 0,7 / 10,1 / 3,3 |
| Insertion au milieu | 4096 | 4acf3a7 | 54,9 | 83,0 | 21,8 | 460,3 | 500,4 | 5,5 / 4,7 / 2,3 |
| Insertion au milieu | 4096 | Candidate | 18,1 | 19,8 | 3,5 | 77,9 | 96,6 | 2,6 / 1,6 / 1,3 |

Pour la grande insertion mixte, les mesures détaillées ci-dessous utilisent le même ELF instrumenté et des disques QCOW2 clairsemés. Le moniteur HMP est dans l’application et QMP dans le moteur lancé directement. Toutes les valeurs sont en millisecondes, sauf les décomptes IN/OUT/FLUSH.

| Secteur | Lancement | SHA source / candidat | Construction / recherche extents | Copie source et prêt | Alloc. fenêtres / pages | Effacement | DMA IN / OUT | Backend IN / OUT / FLUSH | Requêtes IN/OUT/FLUSH |
|---|---|---|---|---|---|---|---|---|---|
| 512 | Application | 98,2 / 233,4 | 10,6 / 3,9 | 41,8 | 3,0 / 0,9 | 1,5 | 10,4 / 31,4 | 48,8 / 150,1 / 23,6 | 143/45/2 |
| 512 | Moteur direct | 18,5 / 36,9 | 2,2 / 0,8 | 5,3 | 1,1 / 0,1 | 0,4 | 2,0 / 2,9 | 2,3 / 2,3 / 1,2 | 143/45/2 |
| 4096 | Application | 103,0 / 215,8 | 11,0 / 3,6 | 34,9 | 5,9 / 0,7 | 3,1 | 9,5 / 20,0 | 14,4 / 14,5 / 3,1 | 143/45/2 |
| 4096 | Moteur direct | 18,7 / 37,1 | 2,2 / 0,8 | 5,4 | 1,1 / 0,1 | 0,4 | 2,1 / 2,9 | 2,1 / 2,3 / 1,2 | 143/45/2 |

### CPU du processus QEMU et latence du nœud QCOW2

Chaque ligne correspond à une seule sauvegarde avec le même noyau instrumenté. Le CPU utilisateur et système du processus cumule ses threads. Le nombre d’instructions hôtes comprend traduction TCG, services de périphériques et traitement des traces. Les durées du backend se recouvrent avec le CPU du processus. Une case vide indique que ce compteur était absent de la version de la sonde utilisée pour cette exécution.

| Scénario | Secteur | Lancement | Commande (ms) | CPU utilisateur / système (ms) | Instructions hôtes (milliards) | Cycles hôtes (milliards) | Temps CPU sur cœurs P (%) | Backend IN / OUT / F1+F2 (ms) |
|---|---|---|---|---|---|---|---|---|
| Ajout de 4 Mio au milieu | 512 | Application, clairsemé | 860,8 | 812,9 / 87,3 | 3,283 | 0,965 | 0,0 | 11,8 / 17,5 / 3,3 |
| Ajout de 4 Mio au milieu | 512 | Application, préalloué | 824,2 | 766,1 / 83,6 | 3,351 | 1,002 | 0,0 | 16,4 / 13,1 / 4,9 |
| Ajout de 4 Mio au milieu | 512 | Moteur direct | 133,5 | 127,1 / 9,5 | 3,193 | 0,547 | 100,0 | 2,3 / 2,4 / 1,3 |
| Ajout de 4 Mio au milieu | 4096 | Application, clairsemé | 879,0 | 776,5 / 89,6 | — | — | — | 22,7 / 16,4 / 3,8 |
| Ajout de 4 Mio au milieu | 4096 | Application, préalloué | 696,4 | 647,2 / 63,9 | 3,367 | 1,013 | 0,0 | 12,8 / 11,2 / 5,4 |
| Ajout de 4 Mio au milieu | 4096 | Moteur direct | 134,2 | 128,2 / 9,0 | 3,246 | 0,553 | 100,0 | 2,3 / 2,3 / 1,2 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | Application, clairsemé | 851,1 | 592,9 / 84,0 | 3,286 | 1,007 | 0,0 | 32,9 / 13,6 / 3,7 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | Moteur direct | 122,6 | 115,4 / 8,6 | 3,024 | 0,508 | 100,0 | 2,3 / 2,3 / 1,2 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Application, clairsemé | 846,9 | 391,7 / 118,6 | 3,601 | 1,315 | 0,0 | 46,2 / 23,9 / 5,2 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Application, préalloué | 821,4 | 620,9 / 73,6 | 3,326 | 1,019 | 0,0 | 35,1 / 12,2 / 2,5 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Moteur direct | 121,6 | 115,6 / 8,8 | 3,026 | 0,508 | 100,0 | 2,1 / 2,3 / 1,2 |

### Effet de la planification macOS à moteur et stockage identiques

Sur le moteur direct, une paire alterne la politique ordinaire et `taskpolicy -b` du seul processus QEMU jetable. Cette option peut également modifier la politique d’E/S de macOS. Le noyau invité, le volume, le firmware, le scénario et les traces de périphérique sont communs. La comparaison utilise deux démarrages par secteur et politique. Les temps de commande sont donnés individuellement dans l’ordre des répétitions.

| Opération | Secteur | Politique hôte | Commande → succès (ms) | CPU total (ms) | Part CPU sur cœurs P (%) | Backend OUT / FLUSH (ms) |
|---|---|---|---|---|---|---|
| Ajout de 4 Mio au milieu | 512 | Ordinaire | 125,986 / 129,011 | 127,2 / 130,5 | 100,0 / 100,0 | 2,2/1,1, 2,5/1,2 |
| Ajout de 4 Mio au milieu | 512 | Arrière-plan | 774,520 / 770,839 | 787,5 / 779,0 | 0,0 / 0,0 | 13,1/4,8, 14,6/4,1 |
| Ajout de 4 Mio au milieu | 4096 | Ordinaire | 141,485 / 132,632 | 135,7 / 134,1 | 100,0 / 100,0 | 2,2/0,8, 2,3/1,2 |
| Ajout de 4 Mio au milieu | 4096 | Arrière-plan | 873,168 / 805,438 | 884,1 / 824,6 | 0,0 / 0,0 | 27,6/3,1, 15,0/2,7 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | Ordinaire | 119,210 / 122,343 | 120,5 / 123,7 | 100,0 / 100,0 | 2,3/1,2, 2,2/1,1 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | Arrière-plan | 733,411 / 716,908 | 748,0 / 720,8 | 0,0 / 0,0 | 12,2/3,6, 10,5/2,4 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Ordinaire | 124,288 / 122,807 | 125,7 / 124,3 | 100,0 / 100,0 | 2,1/1,1, 2,4/1,2 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | Arrière-plan | 676,636 / 1063,168 | 594,2 / 805,5 | 0,0 / 0,0 | 25,4/3,7, 36,1/5,2 |

Les compteurs `ri_user_ptime` et `ri_system_ptime` de Darwin comptabilisent l’exécution sur les cœurs de performance. La sélection effective des cœurs dépend de macOS. La classe d’ordonnancement d’une VM lancée par l’application ne se déduit pas du seul temps CPU. [Comptabilisation XNU](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/task.c) · [Ordonnancement sur Apple Silicon](https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon/)

### Demande macOS de sortie d’arrière-plan dans UTM

Le témoin exécute `taskpolicy -B -p` après le démarrage du seul QEMU identifié par l’UUID de la VM jetable. L’option requiert un processus hôte capable d’effectuer cette demande. Une mesure instrumentée relève les cœurs réellement utilisés. Le profil courant conserve ses propres résultats hors sonde.

| Opération | Secteur | Hors sonde (ms), chaque exécution | p50 / p95 / p99 (ms) | Instrumenté (ms) | CPU sur cœurs P (%) |
|---|---|---|---|---|---|
| Ajout de 4 Mio au milieu | 512 | 150,393 / 135,501 / 138,249 | 138,2 / 150,4 / 150,4 | 147,356 | 100,00 |
| Ajout de 4 Mio au milieu | 4096 | 150,171 / 148,761 / 151,201 | 150,2 / 151,2 / 151,2 | 151,399 | 99,92 |
| Ajout de 4 Mio et édition en deux autres positions | 512 | 782,905 / 764,076 / 929,432 | 782,9 / 929,4 / 929,4 | 880,376 | 0,00 |
| Ajout de 4 Mio et édition en deux autres positions | 4096 | 755,059 / 844,438 / 644,953 | 755,1 / 844,4 / 844,4 | 607,595 | 0,00 |

### Dépassements et observations instrumentées

4 sauvegardes hors sonde ont atteint ou dépassé 1 000 ms.

| Essai hors sonde | Commande → succès (ms) | OUI → succès (ms) |
|---|---|---|
| `vm-cpu-utm-4096-ajout-4m-multiple-r1-p0` | 1002,748 | 918,268 |
| `vm-cpu-utm-4096-ajout-4m-multiple-r1-p1` | 1077,014 | 993,434 |
| `vm-cpu-utm-512-ajout-4m-milieu-r1-p0` | 1083,198 | 997,719 |
| `vm-cpu-utm-512-inserer-debut-r3-p0` | 2696,987 | 2675,626 |

Les 170 observations instrumentées de la candidate comptent de 36 à 57 échanges avec contrôles et exactement deux FLUSH par sauvegarde. Parmi elles, 3 atteignent ou dépassent 1000 ms avec les sondes actives.

| Essai instrumenté | Commande → succès (ms) | Départ invité → F2 (ms) |
|---|---|---|
| `vm-cpu-utm-sonde-4096-ajout-4m-multiple-r1-p0` | 1150,938 | 952,472 |
| `vm-cpu-utm-sonde-512-ajout-4m-multiple-rhmp-p0` | 1015,444 | 887,744 |
| `pair-cpu-512-inserer-milieu-courant` | 3117,063 | 3040,164 |

## Mémoire, sécurité et reprise

Sur le banc Darwin, le compteur de l’allocateur du moteur passe de **14 085 à 1 354 allocations** pour une sauvegarde groupée de référence. Ce décompte est identique sur les deux architectures, les deux géométries et les trois répétitions. Le pic de mémoire vivante supplémentaire, calculé par rapport au début du même appel, passe de 2 276 944 à 2 266 848 octets sur ARM64 et de 2 276 208 à 2 266 704 sur x86_64. Le contrôle utilise un exécutable de lecture v0.16.5 construit séparément. Dans l’invité, le pic de cadres compte la session entière. Le banc publie aussi les pages privées et la pile observées par PID dans QEMU. [Allocations, pics par essai et empreintes des ELF](validation/es-v0.16.6-cpu.json)

Sur les 106 mesures QEMU hors sonde par architecture, le pic de cadres de la session atteint 9 754 contre 9 753 à la référence sur AArch64, et 9 765 contre 9 763 sur x86_64. En comparant à géométrie et scénario identiques, la hausse la plus forte compte un cadre de 4 Kio sur AArch64 et deux cadres sur x86_64. Le pic global inclut l’éditeur, son historique et les services. [Pics de session par exécution et valeurs de référence](validation/es-v0.16.6-cpu.json)

L’arène DMA du pilote est plafonnée à 64 pages de 4 Kio par sens, soit 512 Kio au total. Dans les quatre essais QEMU limités à huit entrées de vring, la chaîne utilise au plus six pages de données, plus l’en-tête et le statut. Les reprises relisent les octets attendus avec le lecteur hôte. Le contrôle de terminaison, la fermeture de l’admission lors d’une révocation et le drainage des opérations en vol gardent leur ordre. [Banc de coupures et de révocation](../tests/coupure_cow_groupee.py)

| Qualification exécutée | Couverture |
|---|---|
| Concurrence QEMU | Dans les quatre couples architecture/secteur, écriture concurrente détectée, suppression d’une source ouverte refusée, remplacement du nom détecté, modifications locales conservées et octets UTF-8 vérifiés. |
| Coupures QEMU | Neuf points par couple, soit 36 essais. Les points couvrent les pages et une écriture en vol, avant, pendant et après F1/F2, ainsi que le retrait. Remontage normal et relecture exacte à chaque reprise. |
| Coupures forcées UTM/QCOW2 | Six essais sur les secteurs 512 et 4096. Aux points pages et F1, l’export immédiat et le redémarrage retrouvent les générations 66/67, la source intacte et la destination absente. Après F2, le premier export retrouve 67/68 et la destination complète de 8 Mio. La relecture de cette destination au démarrage suivant retrouve 68/69 après la mise à jour d’accès. Aucun message de succès n’a précédé l’arrêt injecté. |
| Défauts et révocation QEMU | Cinq essais par couple, soit 20. Retrait demandé pendant une commande en vol, paramètres refusés avant le premier kick, longueur de complétion incohérente et erreur d’écriture d’un segment, avec témoin réussi. Le JSON distingue chaque résultat et chaque acquittement. |
| Espace insuffisant QEMU | Quatre sauvegardes refusées. Leurs générations correspondent au témoin qui ouvre seulement la source. Le nom de destination est absent et le tiers est relu intact. |
| Régressions NovaOS | Les 20 cibles Make par architecture ont terminé avec succès, dont 25 démarrages, 24 groupes bloc, les contrôles ELF/cycle de vie/IPC et quatre essais CLI. Les suites de fichiers, namespace, shell, scripts, terminal, éditeur, écran, volumes, mémoire, progression de montage et performances figurent par nom dans les relevés. La suite RAM a démarré à 128 et 256 Mio sur les deux architectures. À 64 Mio, le firmware arrête l’essai avant le noyau sur les deux architectures. |
| NovaFS Darwin | Par architecture, 196 tests standards, 199 instrumentés, 1 000 000 d’opérations différentielles, 2 304 injections sur les ancres et 486 supplémentaires sur le CoW groupé. Six tests sont déclarés ignorés dans les listes Cargo standard et instrumentée. Les campagnes dédiées couvrent aussi les snapshots et clones, la migration et les permutations d’écriture, la compression, le chiffrement et les échanges ARM64/x86_64. |
| Cœur NovaFS sans système d’exploitation | Quatre compilations en mode release, avec le profil normal et avec toutes les fonctionnalités de laboratoire, sur `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Les commandes sont exécutées depuis `novaFS/` avec la configuration cryptographique de la cible. |
| Microprogramme | Les 11 scénarios GICv3 et les 23 scénarios de transfert DMA séquentiel tournent sur le moteur UTM AArch64 avec les ROM expérimentales identifiées dans le relevé. |

Le contrôle du nouvel index force plusieurs niveaux de scission et vérifie l’ancienne racine, la nouvelle et les tranches lues. Il produit aussi des nœuds aux empreintes valides dont une feuille déborde sur l’intervalle de son grand-parent. Le parcours par fenêtre refuse cette incohérence. Le banc de coupures Darwin distingue l’ancien état, le nouveau et le refus si la paire d’ancres est invérifiable. La déchirure d’une ancre et le recul physique de tout le support conservent les limites établies pour P9. [Test de l’index](../../novaFS/src/tree/tests.rs) · [Récupération P9](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

Le contrôle des capacités s’applique aussi aux grandes insertions. Le total conservateur additionne les échanges par liaison, les dons acquittés et les contrôles de capacités et du prêt. L’ajout comptable des copies mémoire synchrones internes donne jusqu’à **65** sur certains passages, dont **huit** copies locales. Le plus grand nombre d’allers-retours IPC avec contrôles est **57**. Le vérificateur reconstruit les deux totaux depuis leurs composantes. [Convention de comptage et journaux](validation/es-v0.16.6-cpu.json)

Pendant la campagne intermédiaire, lors d’un premier démarrage de création sur secteur 4096, la capture série commençait après l’attachement firmware. Le banc a arrêté cet essai avant la sauvegarde. Le journal périphérique de cette VM montre l’attachement du SID 56 au domaine 2 avant la première lecture, avec zéro faute IOMMU. La répétition sur une image neuve a conservé toute la trace série et donné 346,514 ms, avec relecture intégrale après redémarrage. Les répétitions livrées utilisent les mêmes contrôles de démarrage. [Incidents de capture et observations retenues](validation/es-v0.16.6-cpu.json)

La reprise du banc après interruption du chat a trouvé une VM instrumentée déjà arrêtée. La capture de 333 octets se terminait dans le firmware, avant `nsh`. L’image et le journal ont été archivés, puis le scénario d’insertion au milieu en 4096 a été rejoué depuis un disque neuf. Les interruptions de collecte survenues avant l’ouverture de `vi` sont inventoriées séparément des durées de sauvegarde. [Incidents de collecte](validation/es-v0.16.6-cpu.json)

## Décision d’intégration

Le prototype reste sur `codex/es-groupees-0166`. Le plafond de 64 échanges et la récupération après les coupures exercées sont satisfaits. Dans l’application UTM, 122 des 126 sauvegardes hors sonde ont pris moins d’une seconde. Trois des 36 grandes insertions et une des 90 créations ou éditions localisées ont dépassé le seuil. Les 212 sauvegardes QEMU direct et les 12 lancées directement avec le moteur livré par UTM ont respecté le délai. La performance systématique exigée reste donc à qualifier dans l’application. Le message de succès est postérieur à F2 dans les essais instrumentés. [Dépassements individuels](#dépassements-et-observations-instrumentées)

Sur trois démarrages indépendants en QCOW2 clairsemé/512, la même insertion du caractère Ω en tête du fichier de 4 Mio a pris 689,232 ms, 93,704 ms et **2 696,987 ms** hors sonde. Pour ce dernier passage, le délai après `OUI` atteint **2 675,626 ms**. L’invité a ensuite redémarré et la relecture a vérifié les 4 194 306 octets de la destination. Les sondes CPU et bloc étant absentes de cet ELF, l’origine précise du délai de cette exécution reste indéterminée. [Relevés par exécution](validation/es-v0.16.6-cpu.json)

La préparation incrémentale élimine une partie mesurable du travail invité. Pour une grande insertion au milieu en secteurs de 512 octets, le temps SHA candidat et construction des extents est passé de 444,7 à 265,4 ms, et le temps jusqu’à F2 de 1 103,5 à 701,7 ms sur la paire instrumentée. Pour les grandes insertions mixtes en 4096, les valeurs correspondantes sont 211,9 à 116,2 ms et 700,4 à 596,4 ms. Ces démarrages ont aussi des temps de traitement bloc différents. [Comparaisons avant/après](#profils-invités-et-backend-hôte)

Avec le même moteur et le même disque logique, les huit grandes insertions du témoin en politique ordinaire ont pris 119,2 à 141,5 ms. Les huit démarrages en arrière-plan ont donné 676,6 à 1 063,2 ms. Dans l’application, la demande de sortie d’arrière-plan a donné 12 sauvegardes sur 12 sous la seconde pour les deux grandes insertions testées. Deux des quatre observations instrumentées de ce témoin ont néanmoins mesuré un temps négligeable sur les cœurs de performance. Le choix du cœur varie et cette demande ne réserve ni du CPU ni une latence d’E/S. [Témoin dans l’application](#demande-macos-de-sortie-darrière-plan-dans-utm)

Pour un nouvel arbitrage, qualifier un profil UTM qui précise le mode d’exécution de QEMU, l’état de la session hôte et la charge du stockage. Alterner les profils sur les mêmes scénarios et enregistrer l’ordonnancement du thread vCPU si un long intervalle sépare encore le temps CPU du temps écoulé. La préallocation seule a laissé un dépassement, les deux variantes de SHA-256 logiciel essayées augmentaient le coût, et le format P9 rattache chaque empreinte à l’adresse et à la génération. Une accélération qui emploierait les registres SIMD demanderait une décision distincte sur leur isolation et des mesures de son bénéfice en TCG. [Politique des registres AArch64](../kernel/src/arch/aarch64/context.rs) · [Contrôle des propriétaires des blocs P9](../../novaFS/src/fs/check.rs)

## Reproduire une cellule

Construire les archives hors sonde, instrumentée et de coupures selon l’ordre de fixtures du rapport DMA. Appliquer aussi `io-0166-invite.patch` avec `patch -p1 -F0` après la fixture `io-0166-virtio-multipage`. La préparation de l’index suit le même mécanisme que le moteur groupé. La fonctionnalité Cargo l’active dans le laboratoire hôte. La fixture `io-0166-plages` l’inclut dans l’invité expérimental. Les 59 fichiers du chemin UTM reconstruits en appliquant les fixtures ont été comparés octet par octet aux sources de l’archive chronométrée. Pour chronométrer les transitions du prêt, appliquer en dernier `io-0166-temps-pret.patch` sur une archive instrumentée distincte. Le build normal du dépôt laisse le prototype désactivé. Utiliser des sorties neuves et les mêmes secteurs pour la génération, l’invité et le lecteur. [Combinaisons des fixtures et construction des ELF](es-v0.16.6-dma.md#reproduction)

```sh
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF" "$GENERATEUR" "$SORTIE/froid-1" 4096 --cas ajout-4m-multiple --sans-sonde
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF" "$GENERATEUR" "$SORTIE/prealloue-1" 4096 --cas ajout-4m-multiple --sans-sonde --preallocation
python3 -B novaOS/tests/es_groupees_utm.py "$PROFIL_UTM" "$ELF_SONDE" "$GENERATEUR" "$SORTIE/hmp" 4096 --cas ajout-4m-multiple --moniteur-hote
python3 -B novaOS/tests/es_groupees_moteur_utm.py "$PROFIL_UTM" "$ELF_SONDE" "$GENERATEUR" "$SORTIE/qmp" 4096 --cas ajout-4m-multiple --format qcow2 --sonde
python3 -B novaOS/tests/es_groupees_elargies.py aarch64 512 inserer-milieu "$GENERATEUR" "$SORTIE/raw.json" --sans-sonde
python3 -B novaOS/tests/es_groupees_invite.py aarch64 512 "$GENERATEUR"
```

Pour obtenir les percentiles de chaque cellule, trier ses temps hors sonde et prendre l’observation de rang `ceil(p × n)` avec des rangs commençant à un. Ce calcul conserve les dépassements individuels. Avec trois observations, p95 et p99 coïncident avec le maximum. Il s’agit de percentiles empiriques de la campagne. Les dates, empreintes des binaires, essais instrumentés, témoins historiques et cas de refus sont disponibles séparément dans le relevé. [Mesures et percentiles par scénario, secteur et architecture](validation/es-v0.16.6-cpu.json)

Pour reproduire le témoin de planification, ajouter `--arriere-plan` au banc du moteur direct instrumenté. Pour tester la demande de sortie d’arrière-plan du seul lanceur de la VM jetable, ajouter `--sortie-arriere-plan` au banc de l’application. Ces essais ont leurs propres entrées dans le relevé.

```sh
python3 -B novaOS/tests/es_groupees_verifier_mesures.py novaOS/docs/validation/es-v0.16.6-cpu.json --exiger-seuil
```

Les mesures du thread TCG, de chaque requête FLUSH et de la charge hôte ont fait l’objet d’une campagne ultérieure sur les mêmes scénarios. [Qualification des latences dans UTM](es-v0.16.6-vcpu.md)
