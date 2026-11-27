# NovaOS v0.25.0 — Gestionnaire dynamique x86_64 sous QEMU Q35

**Périmètre de la phase 4F.** A4 autorise un groupe NIC dynamique et A5 la relève de ses acteurs de contrôle pendant le même démarrage. Sur le profil double, le NVMe garde son pilote et son bail indépendants. Les paquets, la politique de confiance et le matériel sont ceux du laboratoire. Les résultats AArch64 de 4D et 4E.1 conservent leur provenance. [Architecture 4A](README.md) · [Lancement ARM 4D](phase4d-v0.25.0.md) · [Reprise ARM 4E.1](phase4e1-v0.25.0.md)

## Portage et chaîne de décision

Le même gestionnaire Rust `no_std` sélectionne le plan sur les deux ISA. Sur Q35, la découverte médiée vérifie le port PCI 00:07.0, le SID 56, le couple 1af4:1041 et la désactivation initiale de la maîtrise du bus. Le paquet `pkg01` contient un ELF64 `ET_EXEC` pour `EM_X86_64`. La politique de développement injectée authentifie le manifeste, le paquet et le profil signé. Le chargeur scelle le membre sélectionné, compare ses segments chargés, puis attribue les capacités au processus Ring 3. Le service Ethernet v1 garde les capacités TX/RX, les huit crédits et les transports groupés PUT/TAKE ou DATA négociés à l’ouverture. Le pilote utilise la médiation existante pour ses deux files de seize entrées. [Préparation des paquets](../../user/gestionnaire-4d/preparer.py) · [Gestionnaire commun](../../user/gestionnaire-4d/acteur.rs) · [Admission et mesure](../../kernel/src/sched/process/net_preparation/dynamique.rs)

L’adaptateur Q35 corrobore les 22 adresses du NIC, les permissions de chaque feuille et l’état des registres VT-d. La clé du journal comprend l’époque de démarrage, le contrôleur, le groupe et la génération. Le retrait rapproche l’intention du titulaire, le reçu et la carte encore détenue par le superviseur. Le gestionnaire et le lanceur reçoivent une vue bornée sur leurs liaisons IPC. Le dispositif de restitution reste chez le propriétaire matériel. [Adaptateur VT-d](../../kernel/src/hardware/net_q35/gestionnaire.rs) · [Journal et reprise communs](../../kernel/src/sched/process/net_preparation/dynamique/reprise.rs)

| Profil | Preuve requise avant restitution | Cadres à la clôture |
|---|---|---|
| Q35 simple | Arrêt du NIC et du bus master, retrait de la seule entrée de racine, invalidations de contexte et d’IOTLB avec drainage, WAIT attendu, tête et queue concordantes, absence de faute | 27 cadres privés rendus et 3 permanents. Une incertitude garde les 30 cadres |
| Q35 double | Drain individuel lié au PID et à la génération, désactivation du NIC, invalidation ciblée, WAIT consommé une seule fois, entrée du NIC retirée et contexte NVMe toujours concordant | 26 privés rendus, dont 22 buffers et 4 tables. Pendant le bail disque, 6 cadres NVMe et 8 cadres du superviseur restent présents. Après clôture, 8 permanents |
| Q35 double, reçu NIC non livré | La disparition de l’entrée du NIC ou un reçu du disque ne suffit pas à clore le bail | Les 26 cadres restent en quarantaine. Après clôture du disque, 34 cadres restent épinglés |

En profil double, la réservation consomme un numéro de processus neuf sous le verrou de l’ordonnanceur. Un jeton privé à usage unique fournit ce numéro lors de la création du pilote. Cela lie le futur processus au bail déjà réservé. Un candidat rejeté consomme uniquement un numéro. La garde NVMe est exercée sur une commande en vol, avant de rapprocher sa complétion du CID et du titulaire d’origine. La racine VT-d et la traduction NVMe restent actives pendant le retrait du NIC. [Raccord du profil double](../../tests/gestionnaire_4f/raccord_duo.patch) · [Contrôle des deux domaines](../../tests/ethernet_v1/retrait_duo.py)

Sur le profil simple, le chemin historique de la file d’invalidation arrivait au bord de la page après des cycles répétés. Le raccord calcule l’offset modulo 4 096 pour chaque soumission. L’oracle rapproche les cookies successifs, la tête, la queue et les derniers descripteurs encore présents.

Le profil double est reconstruit dans un répertoire privé à partir des modules audités au commit `56db7d5c0d04490cd284b91567cbbd4736534266`. Le banc vérifie les empreintes des sources avant et après application des raccords 3D et 4F et archive la liste des modules et des ELF. La fonctionnalité Q35 simple est activée par `gestionnaire-4f`. L’admission double passe exclusivement par le superviseur des deux domaines. [Construction du profil](../../tests/ethernet_v1/port_construire_q35.py)

## Matrice de qualification

| Essai | Résultat | Limite et provenance |
|---|---|---|
| Hôtes 4B–4E.1 | **57 succès sur Darwin ARM64 natif et 57 sous Rosetta x86_64**. Par ISA, 26 tests du catalogue, 28 du gestionnaire, 3 du journal. Compilation des bibliothèques pour les deux cibles nues et démarrage des noyaux correspondants | Le microbenchmark monothread du catalogue reste marqué ignoré dans sa suite fonctionnelle. Les six objets de préparation ARM sont identiques octet pour octet à 4E.1 sur le même ELF. Les deux croisements d’ISA sont refusés |
| Q35 simple, découverte à fermeture | Trois démarrages par scénario. Sélection PCI, paquet signé, comparaison des segments avant attribution, lancement Ring 3, publication v1, trames réelles au pair et reçu unique | Le superviseur matériel reste celui du profil fermé. La production et les familles physiques nécessitent d’autres preuves |
| Reprises du gestionnaire | Avant et après sélection, à la réservation, après attribution, décès détecté lors du contrôle, publication incertaine, trafic actif et réconciliation | Le PID, l’incarnation et la liaison changent. L’ancienne capacité est refusée. Le rejeu renvoie au seul pilote réellement créé |
| Reprises du lanceur et concurrence | Avant engagement, intention en vol, après création, décès détecté et trafic actif. Deux candidats à une relève donnent un gagnant et un refus `REVOKED (10)` sans remise de vue au perdant | Le reçu de publication incertaine est remis une fois. Les captures vérifient les octets transmis et l’absence de trame supplémentaire |
| Journal perdu ou incohérent, reçu retardé ou absent | Le gel conserve les 30 cadres du profil simple lorsqu’il manque une preuve. Le reçu différé autorise ensuite le retrait et une génération neuve. Le doublon est refusé | Le modèle absent diffère la **livraison logicielle** du reçu. Le QTest peut déjà voir le WAIT. Le superviseur continue de refuser la restitution |
| Q35 double, NIC et NVMe | Fermeture nominale, décès du NIC, retrait ciblé, différé et absent. Relèves du gestionnaire, du lanceur et réconciliation sur le contrôleur partagé. CQE du même CID après la sonde de réadmission refusée | Bail NVMe, contexte, droits directionnels et propriétaire contrôlés. La garde s’applique aussi après restitution physique du NIC pendant ce bail |
| Q35 simple et double en endurance | Trois démarrages de 256 cycles sur **chacun** des profils. Simple, 768 relèves du gestionnaire, 31 732 cadres libres, zéro processus et zéro page IPC à chaque fin de cycle, 3 cadres VT-d permanents et 6 tours de file par démarrage. Double, 23 704 cadres libres à chaque fin de cycle, zéro processus, zéro page IPC, 8 permanents et 12 tours de file. Le pair vérifie 256 trames par sens et par démarrage simple, et 36 864 par sens et par démarrage double | Sur le double, chaque nouvelle admission du NIC suit la clôture de l’ancien bail disque et l’ouverture d’un **nouveau** bail NVMe |
| Non-régression ARM | Trois démarrages avec décès observé, deux cycles chacun. Trois démarrages de l’écho dynamique à 1 514 octets, volume P9 et ELF de l’ESP relus | Le dossier 4E.1 conserve les résultats de sa campagne plus large à sa propre révision |

Le profil Q35 simple comprend 22 scénarios dynamiques et un témoin statique. Les scénarios courts de défaillance injectent une faute par démarrage. Les longues séries exercent les générations et le bouclage réel des files. L’oracle rapproche le PID, l’incarnation et la génération du journal des créations ELF observées. Les comptages couvrent les 48 sélecteurs ABI, les quatre racines IPC par acteur et les huit boîtes globales. En pression IPC, huit boîtes occupent 32 Kio avant la négociation. Le port choisit DATA et le pair vérifie 144 trames par sens au cours de la relève. Les permissions matérielles et le contrat Ethernet v1 restent ceux du profil signé. [Oracle du lancement et de la relève](../../tests/gestionnaire_4f/qualifier.py) · [Double domaine et contrôles de continuité](../../tests/gestionnaire_4f/double.py)

La première exécution à huit boîtes a mis en évidence une erreur de l’oracle hôte. Le tableau des trames ne comportait pas le mode DATA sous pression. Une exception survenue après un contrôle positif pouvait alors laisser un succès intermédiaire dans le JSON. L’oracle exige désormais 144 trames par sens pour ce mode, une exception annule le succès et la relecture exige les preuves de cycle et de profil. La capture de diagnostic est conservée à part. Les trois résultats publiés utilisent le lanceur corrigé et le même ELF.

## Ressources et temps de contrôle

Les valeurs ci-dessous proviennent des ELF effectivement exécutés et des compteurs physiques invités. Un cadre vaut 4 096 octets. Les fichiers ELF contiennent aussi les métadonnées hors des segments chargés.

| Image ou acteur | Q35 dynamique | ARM dynamique |
|---|---:|---:|
| Noyau du profil écho, octets du fichier | 10 120 800 | 8 924 696 |
| Image résidente, cadres | 2 190 | 1 905 |
| Gestionnaire ELF, octets | 243 088 | 230 800 |
| Lanceur/pair ELF, octets | 16 912 | 16 824 |
| Pilote ELF, octets | 16 944 | 12 784 |
| Service Ethernet ELF, octets | 801 408 | 807 008 |
| Client Ethernet ELF, octets | 768 552 | 763 144 |

À la même révision et sur le même écho, le noyau Q35 statique occupe 10 302 600 octets et 2 240 cadres. En dynamique, le code exécutable gagne 7 cadres, le segment inscriptible en gagne 1 et le segment en lecture seule en perd 58. Le total de l’image baisse ainsi de 50 cadres. À la publication, les processus de contrôle contribuent au contraire à une **hausse de 123 cadres résidents** pour le profil complet. L’ELF Q35 double du retrait ciblé mesure 11 204 344 octets pour 2 447 cadres d’image. Les tables VT-d, les pages de l’amorceur et la topologie mémoire diffèrent entre profils.

| À la première publication, en cadres sauf processus | Q35 statique | Q35 dynamique | ARM dynamique | Q35 double, retrait ciblé |
|---|---:|---:|---:|---:|
| Processus | 3 | 5 | 5 | 9 |
| Tables CPU | 132 | 151 | 195 | 179 |
| Piles noyau / utilisateur | 80 / 192 | 112 / 224 | 112 / 224 | 176 / 416 |
| Pages utilisateur hors piles | 25 | 106 | 103 | 204 |
| Pages IPC | 10 | 18 | 18 | 31 |
| Tas noyau résident | 3 | 3 | 16 | 3 |
| Cadres DMA | 30 | 30 | 38 | 40 |
| Total résident déclaré | 2 713 | 2 836 | 2 613 | 3 497 |

Sur Q35 simple, le pic de l’admission initiale est de 504 cadres par rapport à l’entrée du cycle. Il atteint 585 avec deux candidats au rôle. L’admission suivante consomme au plus 501 cadres supplémentaires après création du contrôleur permanent. Sur Q35 double, l’endurance atteint 515 cadres au premier cycle puis 512, mesurés avec les acteurs de stockage déjà ouverts. Le minimum de cadres libres pendant le retrait nominal est égal au nombre libre à l’entrée du retrait, soit **zéro cadre supplémentaire** pendant cet intervalle. Les cadres déjà épinglés restent inclus au point de départ. ARM atteint 493 cadres sur l’écho avec le stockage ouvert.

Le gestionnaire Q35 réserve 16 pages de pile et en touche un bloc maximal observé de 16 512 octets au démarrage nominal. Le lanceur réserve également 16 pages et en touche 1 664 octets. Le pilote, le service et le client réservent chacun 64 pages et touchent respectivement 15 232, 155 008 et 1 664 octets. Le tas utilisateur réservé au gestionnaire fait 65 536 octets sur chaque ISA, avec un pic vivant de 21 944 octets sur Q35 et 21 946 sur ARM. Le journal occupe 392 octets inclus dans les 752 octets de l’objet d’autorité. Les objets auxiliaires représentent 123 octets avant alignement. Les dix pages IPC du contrôle initial couvrent aussi les structures des canaux. En transfert Q35 simple, les cinq acteurs utilisent deux boîtes globales, dix-huit pages IPC et au plus quatre racines par acteur.

| Temps invité, écho de 1 514 octets, médiane de trois démarrages | Q35 simple | ARM |
|---|---:|---:|
| Contrôle initial du gestionnaire | 296,56 ms | 30,56 ms |
| Chargement scellé | 2,74 ms | 1,44 ms |
| Activation jusqu’à l’ouverture mesurée | 291,21 ms | 22,85 ms |
| Fermeture jusqu’à la restitution mesurée | 276,22 ms | 15,60 ms |

Sur les 768 relèves du gestionnaire après attribution en endurance simple, les p50, p95 et p99 atteignent respectivement 17,55, 20,15 et 22,28 ms. Les intervalles de contrôle ci-dessus se recouvrent. Sur Q35 simple, les trois décès du gestionnaire détectés donnent **17,91 ms** de reprise médiane (17,41–19,08). Pour le lanceur, la médiane est **5,82 ms** (5,35–6,91). Une découverte entièrement reprise avant sélection prend 318,27 ms en médiane, et une relève en réconciliation 19,19 ms. L’horloge couvre le traitement de la vacance et le rattachement. Le délai entre le décès et le premier échange qui le détecte dépend du service.

## Ethernet et stockage

Écho Ethernet v1 de 1 514 octets, 1 024 lots par cellule et par démarrage. Chaque série complète contient 13 312 trames par sens. Le débit utile TX égale le débit RX sur ce vecteur. La latence mesure l’aller-retour du lot. Les colonnes donnent la médiane des valeurs par démarrage. Q35 4F est mesuré en six paires alternées (statique puis dynamique, ordre inversé à la paire suivante). Le témoin 3E et le profil dynamique ARM ont chacun trois démarrages. « IPC/TX » compte les appels client/service, service/pilote et courtier pour tout l’écho, divisés par le nombre de trames émises. La relecture PCAP reconstruit le motif attendu octet par octet et compare les deux directions.

| Profil | Lot | Mbit/s par sens | p50 ms | p95 ms | p99 ms | IPC/TX |
|---|---:|---:|---:|---:|---:|---:|
| Q35 statique historique 3E | 1 | 11,90 | 0,982 | 1,181 | 1,268 | 51,869 |
| | 4 | 28,01 | 1,728 | 1,920 | 2,035 | 21,017 |
| | 8 | 36,32 | 2,689 | 2,920 | 3,038 | 14,951 |
| Q35 statique, révision 4F | 1 | 12,18 | 0,959 | 1,158 | 1,229 | 51,961 |
| | 4 | 28,32 | 1,703 | 1,854 | 1,933 | 21,038 |
| | 8 | 33,92 | 2,879 | 3,096 | 3,291 | 15,278 |
| Q35 dynamique, révision 4F | 1 | 10,88 | 1,082 | 1,296 | 1,347 | 51,825 |
| | 4 | 25,81 | 1,882 | 2,027 | 2,103 | 21,008 |
| | 8 | 33,77 | 2,929 | 3,102 | 3,240 | 14,954 |
| ARM dynamique, révision 4F | 1 | 7,99 | 1,494 | 1,660 | 1,811 | 50,999 |
| | 4 | 18,75 | 2,563 | 2,773 | 2,986 | 20,996 |
| | 8 | 25,21 | 3,827 | 4,034 | 4,275 | 14,938 |

Face au statique de même révision, le **rapport des médianes** du lot de huit donne −0,44 % de débit, −1,55 % de p99 et −2,12 % d’IPC/TX. Les différences de débit de chacune des six paires vont de −4,26 % à +2,17 %. Leur médiane est +0,15 %. Le p99 varie de 3,15 à 3,41 ms en dynamique et de 3,05 à 3,46 ms en statique. Le lot unitaire conserve un écart de débit de −10,7 % et un p99 supérieur de 9,6 %. Le lot de quatre donne −8,9 % de débit et +8,8 % de p99. Le nombre d’IPC par trame explique peu ces deux derniers écarts. La comparaison couvre les images complètes et l’ordonnancement sous TCG. La cause des écarts sur petits lots reste ouverte.

La première série séquentielle donnait 31,37 contre 34,90 Mbit/s en lot de huit et un p99 de 4,03 contre 3,29 ms. Les horodatages de deux démarrages dynamiques recoupent la compilation des témoins de vacance. La série alternée utilise exactement les mêmes deux ELF et a démarré après la fin des compilations. Le bilan conserve les deux jeux de mesures et les douze acquisitions du rejeu. Le matériel émulé, l’horloge et les firmwares diffèrent entre Q35 et ARM. Les autres tailles et régimes de trafic conservent leurs preuves historiques. [Comparaison en paires](../../tests/gestionnaire_4f/comparaison.py) · [Mesures Ethernet 3E](../architecture-ethernet/phase3e.md)

Sur le profil double, l’oracle lit la trace des commandes NVMe et leurs CQE, les deux barrières de publication, les ancres et les octets de la copie P9 jetable. Le fichier `/sortie` fait 4 231 octets. Son SHA-256 final est `9d691dbec9d08e43fb398131d7e4a7f53c0086e2b9380a652f3ab68c558ad849`. L’endurance alterne les contenus pour imposer une transaction à chaque cycle. Chaque démarrage part de la génération P9 3 et se termine à 259. Les 256 chronologies sont vérifiées. Le lecteur indépendant inspecte les ancres et le contenu du **volume final** de chaque démarrage. Les images des 255 générations intermédiaires ne sont pas archivées individuellement.

| Durée d’une commande NVMe pendant l’endurance double | Échantillons sur trois démarrages | p50 ms | p95 ms | p99 ms |
|---|---:|---:|---:|---:|
| READ | 23 781 | 3,92 | 5,08 | 5,75 |
| WRITE | 5 376 | 3,98 | 5,28 | 6,42 |
| FLUSH | 2 304 | 3,86 | 5,11 | 6,22 |

Ces intervalles vont de la soumission à la récolte de la CQE sur l’horloge invitée. Les scénarios courts avec reçu immédiat, différé ou absent comportent chacun 31 commandes après le WAIT NIC. Le cas différé vérifie les trois types de CQE pendant la réconciliation, puis le CID 37 en vol lors du refus de réadmission. Le cas immédiat retrouve le CID 10. Le cas absent conserve les 26 cadres tout en terminant la transaction disque. L’instantané QTest des cas différé et absent vérifie le contexte NIC retiré et le domaine NVMe valide avant la clôture du disque.

Dans six démarrages doubles supplémentaires, le banc ouvre l’observation **après** la perte du gestionnaire ou du lanceur et maintient le rôle vacant jusqu’à trois CQE réussies de types distincts. Il recoupe chaque CID avec l’opcode de la trace QEMU. Avant le remplacement, il compte 15 READ, 1 WRITE et 1 FLUSH sur chacun des trois démarrages côté gestionnaire. Côté lanceur, il compte respectivement 14, 15 et 14 READ, puis 1 WRITE et 1 FLUSH par démarrage. Le pair vérifie 144 trames par sens et l’oracle contrôle la récupération de la publication, la réception avant relève et le volume final. Le délai de sécurité peut faire échouer l’essai. Seules les CQE alimentent le témoin. Les mesures de temps de reprise proviennent des séries sans ce rendez-vous imposé. [Observation des CQE](../../tests/gestionnaire_4f/continuite_nvme.patch)

## Reproduction

Les cellules Q35 simples utilisent QEMU 11.1.3 TCG, `q35,smm=on`, CPU `max`, 1 vCPU et 256 Mio. Le profil double désactive SATA et utilise l’OVMF de laboratoire et le support d’amorçage PIO de 34 Mio. Le témoin ARM utilise QEMU 10.0.12 TCG, `virt-10.0,gic-version=3,its=off`, Cortex-A72, 1 vCPU et 256 Mio. Les SHA-256 complets des exécutables QEMU, firmwares, noyaux, supports et lecteurs figurent dans les attestations de chaque démarrage. Les noyaux et modèles restent dans les archives locales du banc. Un ELF reconstruit sur un autre chemin peut avoir une autre empreinte.

La base est la livraison ARM 4E.1 `b0ae035cbd9fd6a5ad5cf3fa9be2f986e401e18a`, dont la révision qualifiée est `17bdcd7b0ff6ee112d1c34291ce9d2a7eee37d27`. L’architecture 4A est `05b5788476735a97450ca490c0b86ad07c3ff8d7` et la livraison 4D est `de70363f4eab48ccfea11963951451593f37cc59`. Le noyau conserve le manifeste interne NovaOS v0.20.0.

| Preuve 4F | Révision réellement utilisée |
|---|---|
| Noyaux Q35 simple, témoin statique, doubles courts et écho ARM | `bef587b2f28b7120f3f3e494aadd8ce517660e41` |
| Endurance double et renouvellement du bail disque | `e740380d6915f76c91740cbb3275cdf389b32c79` |
| Décès du gestionnaire sur ARM | `2eeff6e6aaef16c979cdd3601aa96033be312b28` |
| Témoin READ/WRITE/FLUSH durant la vacance | `1eb439ed52304a6c99a44c7666081933d0e04ae7` |
| Saturation des boîtes et repli négocié Q35 | `794a39ad7636bae65b9bd1e655075a5702398916` |
| 57 tests hôtes par ISA et compilations des composants nus | `87b308f2224577cb23faa15a60538bdf189fd269`, rustc 1.99.1 |
| Écho statique historique 3E, relu hors VM | `fd3ff113fcaf39bee804eebdcedc44557f41817d` |
| Cinq démarrages du témoin 3C archivé en 3D, relus hors VM | `0169c75148c8359c9235f3f19359133a0e002115` |

La capsule présente **111 démarrages Q35** (102 dynamiques, dont les six répétitions appariées, et neuf statiques) et **six démarrages ARM** de ce mandat. Les huit témoins historiques relus ont leurs dates et leurs révisions d’origine. Les 1 309 pièces comprennent les journaux des invités, traces QEMU, captures du pair, résultats QTest, volumes témoins et relevés de construction. Les ELF sont construits avant leur série et identifiés par leur SHA à chaque démarrage. Des séries se sont poursuivies pendant les modifications du banc et la rédaction du rapport. Le champ global `sources_suivies_modifiees` le signale dans les pièces concernées. Les rapports conservent séparément la révision de l’ELF et celle du lanceur. Pour chaque démarrage simple, la synthèse vérifie aussi l’empreinte du script de lancement contre le blob Git annoncé. La synthèse relit les journaux, les cycles VT-d, les captures d’écho et chacun des volumes NovaFS avec le lecteur hôte identifié.

Pour rejouer, utiliser un worktree de cette livraison, les archives signées du laboratoire et un nouveau répertoire de destination. Le banc utilise le verrou QEMU partagé à chaque démarrage. Les deux constructions ARM et x86_64 nécessitent des répertoires Cargo privés distincts. Le venv du laboratoire fournit les outils de préparation OVMF et de signature. Les destinations du script de synthèse sont données dans l’ordre de son aide.

Depuis la racine du dépôt, avec `bash` et le laboratoire installé :

```bash
set -eu
projet=$(git rev-parse --show-toplevel)
labo=$(dirname "$projet")
export NOVAOS_TMPDIR="$labo/tmp" TMPDIR="$labo/tmp/"
export PYTHONDONTWRITEBYTECODE=1 CARGO_PROFILE_RELEASE_STRIP=none
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
dest="$labo/archives-gestionnaire-4f-rejeu"
mkdir "$dest"
banc=novaOS/tests/gestionnaire_4f

export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4f-rejeu-simple"
for cas in nominal avant-selection apres-selection reservation attribution deces-observe \
           lanceur-avant lanceur-engage lanceur-apres lanceur-observe gestionnaire-actif \
           lanceur-actif publication reconciliation concurrence journal-perdu \
           non-corroborable differe absent pression statique ethernet endurance
do
    python3 -B "$banc/qualifier.py" "$dest/q35" "$cas"
done
python3 -B "$banc/double.py" "$dest/q35" nominal port duo-retrait duo-differe duo-absent
python3 -B "$banc/double.py" "$dest/q35" deces-observe duo-differe
python3 -B "$banc/double.py" "$dest/q35" lanceur-observe duo-retrait
python3 -B "$banc/double.py" "$dest/q35" reconciliation duo-differe
python3 -B "$banc/double.py" "$dest/endurance" nominal duo-cycles
python3 -B "$banc/double.py" "$dest/vacance" gestionnaire-actif port
python3 -B "$banc/double.py" "$dest/vacance" lanceur-actif port

export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4f-rejeu-arm"
python3 -B novaOS/tests/gestionnaire_4e1/qualifier.py "$dest/arm-reprise" deces-observe
python3 -B "$banc/arm.py" "$dest/arm-echo"
arm=( "$CARGO_TARGET_DIR"/aarch64-unknown-none-softfloat/release/build/novaos-*/out/gestionnaire.elf )
x86=( "$NOVAOS_TMPDIR"/4f-rejeu-simple/x86_64-unknown-none/release/build/novaos-*/out/gestionnaire.elf )
test "${#arm[@]}" -eq 1
test "${#x86[@]}" -eq 1
objets_arm=$(dirname "${arm[0]}")
objets_x86=$(dirname "${x86[0]}")
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/4f-rejeu-hotes"
python3 -B "$banc/hotes.py" "$objets_arm/net_preparation.bin" "$objets_x86/net_preparation.bin" > "$dest/hotes.log"
python3 -B "$banc/comparaison.py" "$dest/q35" "$dest/comparaison"
python3 -B "$banc/preuves.py" "$dest/q35" "$dest/q35" "$dest/endurance" \
    "$dest/arm-echo" "$dest/arm-reprise" "$dest/hotes.log" "$dest/capsule" \
    --objets-arm "$objets_arm" --double-actif "$dest/vacance" --comparaison "$dest/comparaison"
python3 -B "$banc/preuves.py" --verifier "$dest/capsule"
```

La vérification autonome de la capsule publiée lit chaque pièce en flux, recalcule sa taille et son SHA-256 et exige l’inventaire complet. Pour réexaminer le système de fichiers hors invité, la capsule contient le disque fictif du premier démarrage de chaque cas double. Le lecteur P9 du laboratoire et les empreintes des volumes de tous les démarrages figurent dans les rapports de campagne. [Synthèse par démarrage](../validation/gestionnaire-pilotes-phase4f/bilan.json) · [Inventaire](../validation/gestionnaire-pilotes-phase4f/inventaire.json) · [Capsule brute](../validation/gestionnaire-pilotes-phase4f/preuves.tar.gz) · [SHA-256](../validation/gestionnaire-pilotes-phase4f/SHA256SUMS)

## Réserves et décision proposée à la MOA

**Proposition de clôture.** Clore le prototype commun pour le lancement d’un seul groupe réseau et la reprise des deux rôles sous QEMU AArch64 et x86_64 dans la même époque. A5 reste partiel. La perte du noyau ou du superviseur exige une autorité survivante, une preuve de fraîcheur et un nouveau protocole de transfert. Les cadres à l’issue incertaine restent imputés au groupe jusqu’à une preuve propre à son matériel.

**A6** reste ouvert pour une réadmission positive du NIC pendant le même bail NVMe actif. **A7** couvre tout nouveau rôle de mémoire ou périmètre de reset. **A8** exige une autorisation distincte pour chaque banc physique. Pour le Killer E2400 du MSI et l’Intel I219-V du Lenovo, il reste à établir la topologie IOMMU réelle, les alias et les fonctions sœurs, la propriété firmware, les permissions d’anneaux propres à la famille, le routage et le masquage IRQ, puis la séquence de drainage et de restitution sur silicium. L’anneau partagé en lecture-écriture de l’Intel requiert son arbitrage de droits. La qualification devra conserver un stockage indépendant et un moyen de constater les écritures tardives et les fautes. Le dossier Lenovo reste local. [Études physiques et séparation des familles](../architecture-ethernet/README.md) · [Arbitrages A5 à A8](qualification.md)

Les réserves historiques couvrent `BUSY (24)`, la latence NovaFS sous trafic, la rétention PIO, les dépendances QEMU/OVMF et la reproductibilité bit à bit des ELF. La persistance du catalogue, le TPM de reprise, le démarrage de production et la qualification physique relèvent de mandats distincts. [Réserves de retrait 3E](../architecture-ethernet/phase3e.md)
