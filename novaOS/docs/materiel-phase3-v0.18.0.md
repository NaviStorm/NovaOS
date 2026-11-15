# NovaOS v0.18.0 — Phase 3 — Qualification Intel VT-d sur Q35

Le 14 novembre 2026, le pilote EDU en Ring 3 a demandé et obtenu un transfert de 4 096 octets entre deux pages privées sur QEMU x86_64, avec le remappage Intel VT-d actif. Les 12 démarrages de phase 3 utilisent le code `00f8a73b4c2b9060f2f83f0a6913e4b911cc9029` et le même ELF de SHA-256 `d2dcdbc989968e3d959ff5fddd9a3dfc8f5d87465aa8d9d577fe66503ae36dce`. Le périmètre qualifié couvre un seul émetteur PCI directement connecté au bus racine, après sa reprise par le noyau. Une limite du modèle empêche de garantir une notification pour chaque accès interdit. [Séries, QMP et traces des huit VM VT-d](validation/materiel-phase3-vtd.json) · [Quatre VM de refus à la prise de contrôle](validation/materiel-phase3-dmar.json)

## Point de départ et configuration

La phase 2 avait qualifié la page MMIO, l'INTx et le retrait SMP du pilote EDU. Le contrat DMA générique refusait l'attribution physique. Le stockage VirtIO disposait déjà de son propre chemin VT-d sur Q35. La phase 3 ajoute un courtier privé EDU, sélectionné par la feature x86_64 `vtd-lab` et un scénario de laboratoire. Le noyau ordinaire conserve son refus d'attribution physique générique. Les services de stockage, l'ABI native, NovaFS v0.9.13 P9 et les versions publiées gardent leurs contrats. [Phase 2](materiel-phase2-v0.18.0.md) · [Contrat DMA générique](../kernel/src/hardware/physical_dma.rs) · [Remappeur du stockage classique](../kernel/src/hardware/block/vtd.rs)

Le banc utilise macOS 27.2 (26B123), QEMU 11.1.3 en TCG, Q35, CPU `max`, 256 Mio, EDK2 du paquet QEMU et un ISO de test en lecture seule. Les variables du firmware utilisent un instantané. Le profil positif ajoute `edu` au slot 6 et `intel-iommu,aw-bits=48,intremap=off,eim=off,caching-mode=on,device-iotlb=off,scalable-mode=off,dma-translation=on`. L'oracle hôte relit PCI_COMMAND, les registres de l'unité, les 4 Kio de la racine, des contextes et de la file d'invalidation et le mot de réception WAIT. Il compare les événements QEMU aux observations invitées. [Lanceur](../scripts/qemu.sh) · [Oracle hôte](../tests/vtd_edu.py)

## Du firmware à la première transaction

Le BSP vérifie les cinq fonctions PCI attendues, la BAR de l'EDU et l'état hérité de ses registres DMA. L'EDU passe de PCI_COMMAND `0x0007` à `0x0002`. Sur les six ports AHCI, il vérifie les commandes et tags actifs, arrête les moteurs de commandes et de FIS puis relit leurs états. Il ferme BME sur les quatre pairs. Le profil conserve les pages du chargeur pendant toute la VM. En présence d'une fonction étrangère, le noyau s'arrête avant toute attribution et QMP mesure encore le BME hérité. La protection avant ce point dépend du firmware. [Admission DMAR et transition PCI](materiel-phase3-dmar.md)

La DMAR observée annonce HAW 48 et un seul DRHD du segment zéro à `0xfed90000`, avec portée explicite pour l'EDU `0000:00:06.0` (SID `0x0030`). Le noyau relit VER `0x10`, CAP `0x80d2008c222f0686` et ECAP `0xf42`. Il programme une racine legacy, une table de contextes, quatre niveaux de traduction de second niveau avec feuilles de 4 Kio, une file d'invalidation à descripteurs de 128 bits et une page WAIT. Initialement, tous les contextes sont absents. Après SRTP, QIE et TE, GSTS vaut `0xc4000000`. Les tables sont publiées avec maintenance du cache lorsque ECAP.C vaut zéro. Une panne après publication de la racine garde ses cadres au noyau. [Backend VT-d](../kernel/src/hardware/vtd_lab.rs) · [Formats et conditions d'admission](../kernel/src/hardware/vtd_lab/format.rs) · [Spécification Intel VT-d](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf)

## Domaine, droits et retrait

- Un seul bail actif associe le SID au DID 1, au PID titulaire et à une génération. Les fenêtres IOVA sont disjointes, par pas de 64 Kio à partir de `0x02000000`. Le quota est de 3 584 générations par démarrage, sans recyclage de leur fenêtre. Chaque bail alloue deux cadres neufs ou remis à zéro. La première page autorise la lecture par l'EDU et la seconde l'écriture. Le deuxième demandeur, un ancien bail, les mauvais sens et les doubles mappings sont refusés. Le pilote ne fournit ni cadre physique ni IOVA. [Attribution et mappings](../kernel/src/hardware/vtd_lab.rs)
- Le pilote possède deux endpoints IPC privés, SEND et RECEIVE. PREPARE attribue les deux pages prédéfinies. READ et WRITE demandent chacune un transfert d'une page avec contrôle des bornes sur toute sa longueur. Le noyau conserve les registres et le contexte. Le motif d'une incarnation doit avoir été lu par l'EDU avant toute exportation. La comparaison de la destination couvre chaque octet. [Contrat Ring 3 et sentinelles](materiel-phase3-edu-dma.md)
- Entre l'extraction d'une trame et l'écriture du registre de démarrage, son titulaire peut mourir ou perdre son endpoint. Le courtier revérifie le PID vivant et le droit SEND sous la garde qui sérialise aussi la mort et la révocation. Il conserve cette garde jusqu'à la soumission. Le cas `Late` laisse sortir le pilote après extraction et vérifie le refus avant le registre de démarrage, même avec le domaine encore installé. [Superviseur et test](../kernel/src/sched/process/vtd.rs)
- Le retrait ferme l'admission, attend la fin réelle du moteur EDU, vérifie le repos et relit BME à zéro. Le courtier efface le contexte et les tables, invalide globalement les contextes puis l'IOTLB avec drainage des lectures et écritures. Le mot WAIT doit porter la nouvelle séquence **et** IQH doit avoir rejoint IQT. Le courtier revérifie l'unité, le contexte et la preuve de drainage empruntant l'EDU avant de restituer les pages. Une ancienne IOVA après détachement produit la raison 2. Sur un bail ultérieur, l'allocateur remet effectivement au moins un même cadre à zéro. Le test ne conserve aucune sentinelle dans un cadre déjà rendu à l'allocateur. [Retrait côté VT-d](../kernel/src/hardware/vtd_lab.rs) · [Drainage côté périphérique](../kernel/src/hardware/edu_dma.rs)

## Résultats des douze démarrages

| Profil Q35 | vCPU | Observations déterminantes |
|---|---|---|
| Standard, trois VM | 1, 2, 4 | 10 scénarios réussis par VM. Copie de 4 Kio, concurrence, permissions à chaud et à froid, page étrangère vérifiée par son propriétaire, révocation, mort en vol, commande tardive et redémarrage du pilote. Cinq FRCD, cinq IRQ et 22 séquences contexte/IOTLB/WAIT par VM. |
| Reçu retardé | 2 | Le superviseur masque volontairement l'observation d'un vrai WAIT. Deux cadres restent épinglés. La relecture du reçu autorise leur restitution unique, puis un nouveau pilote transfère ses données. Cinq séquences acquittées. |
| Erreur de file IQE | 2 | Après drainage, QEMU refuse un descripteur de contexte réservé et émet une IRQ. QMP mesure FSTS=`0x10`, IQH=`0x60`, IQT=`0x90`, reçu nul et contexte vide. Les deux cadres restent en quarantaine après une seconde demande de retrait. Toute nouvelle attribution échoue. |
| IOMMU absente | 1 | DMAR absente. Le pilote sans capacité reçoit un refus. PCI_COMMAND EDU final nul. |
| Largeur 39 bits | 1 | Unité visible par QMP, DMAR refusée avant programmation de la racine. |
| Caching Mode désactivé | 1 | DMAR 48 bits admise puis CAP.CM=0 refusé par le backend. RTADDR et les registres actifs de file restent à zéro. |
| DRHD tronqué, DMAR double | 1 par VM | Erreurs ACPI de longueur et de format. Aucune capacité DMA attribuée. Deux VM distinctes. |
| Deuxième EDU | 1 | Topologie refusée avant reprise. EDU d'origine encore à PCI_COMMAND `0x0007`, racine nulle. |
| EDU déjà programmée | 1 | QTest précharge la longueur sur la carte arrêtée, sans lancer de commande. L'invité refuse l'adoption. QMP retrouve MSE seul, racine nulle. |

Le profil standard fait migrer le superviseur du BSP vers les CPU 0, 1 ou 3 suivant la taille de la VM. Il observe RUN actif à la révocation et, dans un essai distinct, à la mort du pilote. Le signalement des fautes utilise un MSI de l'unité adressé au BSP, vecteur `0x53`. Le collecteur rapproche le SID et l'adresse du FRCD avec le contexte installé pour établir le DID. Le FRCD legacy ne transporte pas directement le DID. L'absence de contexte après retrait donne DID inconnu. Les motifs observés sont 2 (contexte absent), 5 (écriture refusée) et 6 (lecture refusée). Le journal privé est borné à 32 enregistrements avant refus conservateur. [Preuves VT-d et vecteurs](validation/materiel-phase3-vtd.json)

À cache chaud, après une lecture autorisée qui a rempli l'IOTLB, une écriture interdite laisse les deux pages de 4 Kio intactes. Aucun FRCD ni MSI de faute n'arrive. À froid, les mêmes permissions et les mêmes motifs donnent le refus **et** l'interruption. Dans QEMU, le chemin de cache peut renvoyer des permissions que la couche mémoire fait ensuite respecter en dehors du chemin FRCD. RUN à zéro et un FSTS nul ne suffisent donc pas à prouver qu'une opération a transféré des données. [Analyse EDU et QEMU](materiel-phase3-edu-dma.md)

## Ressources, compatibilité et reproduction

Le service réserve huit pages de 4 Kio (32 Kio) tant que l'unité a connaissance de la racine. Un bail utilise au plus deux pages de données supplémentaires (8 Kio). La sonde interprocessus retient en plus la page RO du témoin jusqu'à la fin de l'essai. Chaque scénario de la campagne normale ramène le nombre de pages IPC et de cadres DMA clients à zéro et les compteurs du tas au point initial après collecte. Pour l'IQE persistante, huit cadres de service et deux cadres clients restent volontairement détenus jusqu'au redémarrage. Ces chiffres mesurent les cadres, pas la consommation globale de la VM. [Preuves des compteurs](validation/materiel-phase3-vtd.json)

| Régression exécutée | Résultat |
|---|---|
| Modèles Rust Darwin ARM64 et x86_64 Rosetta | 64/64 chacun, dont 16 ACPI, 3 format VT-d et 4 contrat DMA. Les modèles couvrent notamment les échecs injectés à la préparation, au drainage, à l'invalidation et à la confirmation. |
| Ancien pilote MMIO/INTx, QEMU à 2 vCPU | 11/11 scénarios sur AArch64 et 11/11 sur x86_64. Ce contrôle couvre le code de prise de BAR partagé entre les phases 2 et 3. |
| Noyaux ordinaires | Compilation AArch64 et x86_64 réussie. SMP et FPU/SIMD réussis sur QEMU x86_64 à 4 vCPU. |
| Bloc classique x86_64 | 4 contrôles DMA et cycle de vie par géométrie (512 et 4 096 octets), puis 2 essais WRITE/FLUSH avec relecture de l'image dans une nouvelle VM. |
| Fichiers et P9 x86_64 | Création et transactions du banc fichiers dans les deux géométries, relecture indépendante et nouvelle VM en lecture seule. En complément, deux volumes P9 jetables ont été montés avec le profil ordinaire `files-info`, identifiant du LV vérifié et SHA-256 avant/après identique. |

Les moteurs et formats de stockage ne sont pas modifiés. Le polling VirtIO et le chemin classique restent les valeurs par défaut. Conformément au périmètre, les campagnes longues NovaFS, les coupures complètes, l'endurance IPC, les benchmarks et l'interface graphique n'ont pas été rejoués. [Résultats des régressions](validation/materiel-phase3-regressions.json)

Depuis la racine du dépôt, les cibles suivantes reproduisent la qualification. Chaque exécution crée un nouveau répertoire de preuves sous `novaOS/build/vtd-phase3`. Les traces QEMU versionnées sont en gzip/base64, avec SHA-256 des octets décompressés.

```sh
make -C novaOS materiel-modeles-test
make -C novaOS ARCH=x86_64 vtd-edu-test
make -C novaOS ARCH=x86_64 kernel
(cd novaOS && export NOVA_ARCH=x86_64 && python3 -B tests/bloc.py --only qualify && python3 -B tests/bloc.py --only durable)
```

## Limites et décision proposée

Les modèles Rust vérifient les règles de refus et de possession. Le banc Q35 exécute des transactions PCI, des invalidations et des interruptions dans l'émulateur. La qualification sur un système Intel physique reste ouverte. Les garanties reposent sur la fin de commande telle que définie par l'EDU et sur la topologie fermée de cette VM. Le DMA antérieur à la reprise du firmware, les ponts et alias PCIe, plusieurs domaines simultanés, le hotplug, ATS/PASID, le mode scalable et le remappage générique des interruptions sortent du périmètre. Les acquisitions après sortie de veille ou reprise d'une unité déjà programmée restent également hors qualification. [Périmètre PCI](materiel-phase3-dmar.md) · [Modèle EDU de QEMU](https://www.qemu.org/docs/master/specs/edu.html)

**Recommandation MOA/MOE.** Conserver ce backend expérimental sur la branche matérielle. Avant d'autoriser un autre périphérique, exiger une preuve de drainage propre à son contrôleur et une qualification de tout son groupe PCI. Pour une politique exigeant une alerte à chaque tentative interdite, résoudre d'abord la limite de signalement à chaud sur le modèle retenu. La fusion et le développement d'un autre backend relèvent du prochain arbitrage.
