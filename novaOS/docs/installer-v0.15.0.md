# Installation AArch64 dans UTM — audit de v0.15.0

**État au 12 novembre 2026.** Le pflash privé corrigé amorce le support protégé dans UTM graphique avec TPM CRB. NovaOS inspecte ensuite la cible dans son propre domaine DMA. Le banc mesure zéro écriture, y compris après confirmation. Le partitionnement et l’installation attendent un nouvel arbitrage. NovaOS reste en v0.14.0 et NovaFS en v0.9.13 P9. [Qualification TPM et DMA séquentielle](#qualification-tpm-et-dma-séquentielle)

La phase 0 a été réalisée sur `main` au commit `a250cb25b530f001debf256f6bfdf8afbc0bef90`. Dans ce premier audit du moteur livré dans UTM 5.0.8, le firmware exécutait NovaOS v0.14.0, puis l’admission du disque échouait avant le lancement du shell. NovaFS conserve sa version v0.9.13 et son format P9.

## Phase 0 — de l’UEFI au service de fichiers

| Composant examiné | Résultat |
|---|---|
| Support actuel | L’outil `iso.sh cli` produit une ISO hybride avec catalogue El Torito UEFI, table GPT, image système EFI Limine, configuration et noyau. L’ELF et la configuration sont dans l’arborescence de l’ISO. L’option `cli` transmet le mode de lancement à NovaOS. La construction ordinaire et les cibles `cli` préexistantes conservent leur fonctionnement. |
| Entrée du noyau | `BOOTAA64.EFI` est une application PE32+ AArch64. Limine charge les segments de l’ELF AArch64 et entre dans `nova_start`. Le noyau utilise la révision 3 du protocole pour la carte mémoire, la pagination à quatre niveaux, le HHDM, les bases de l’image et le framebuffer. Il copie les données qu’il conserve avant de restituer les pages du chargeur. |
| Découverte matérielle | La HAL vise un profil précis de la machine `virt`, avec GICv2, PL011, RTC PL031, ECAM haut et RAM vidéo. La configuration PCI du disque et ses capacités sont vérifiées dans ce profil. Le lecteur de configuration PCI ne constitue pas un catalogue de disques utilisable par un installateur. |
| VirtIO et DMA | Le contrôleur virtio-IOMMU est attendu en `00:05.0`, le disque en `00:06.0` (SID 48). Le noyau exige ACCESS_PLATFORM, FLUSH, des cadres DMA privés, une ligne INTx 37 exclusive, un domaine sans bypass et un PROBE sans région réservée. Le bail publié associe un unique transport à un PID et à une génération. INFO en donne la géométrie et les droits. |
| Stockage et P9 | L’IPC bloc permet INFO, READ, WRITE, FLUSH et CLOSE. `IpcDevice` traduit le bloc NovaFS zéro en secteur zéro du support présenté. Le montage décode l’ancrage et appelle `Engine::mount_current`. Une vue bornée de partition est nécessaire sur un disque GPT avant d’appeler le moteur. L’implémentation portable de `Engine::format_p9` existe et prend les identifiants, la politique d’accès et l’horloge. L’IPC de fichiers livré ne publie pas d’opération de formatage. |
| Services, console et commandes | Le lanceur de confiance ouvre le bail avant d’admettre le pilote bloc, le relais, le service NovaFS et le shell. Les binaires utilisateur sont embarqués dans le noyau et exécutés séparément en EL0. La console série dessert le shell. Le contrôle positif a lu un fichier par une commande externe et collecté les processus. |

[Construction UEFI](../scripts/iso.sh) · [Point d’entrée et mémoire](../kernel/src/main.rs) · [Contrat Limine](../kernel/src/boot.rs) · [Topologie qualifiée](../scripts/bloc_qemu.py) · [Domaine DMA ARM](../kernel/src/hardware/block/viommu.rs) · [Bail bloc](../kernel/src/hardware/block/lease.rs) · [Lancement](../kernel/src/sched/process/storage.rs) · [Adaptateur de volume](../user/novafs/device.rs) · [Formatage P9](../../novaFS/src/engine.rs)

## Phase 1 — reproduction sur Apple Silicon

Le banc utilise le moteur AArch64 QEMU 10.0.12 et le TPM logiciel de l’application UTM 5.0.8, en appelant leurs points d’entrée depuis deux petits exécutables hôtes. L’essai comparatif utilise QEMU 11.1.3 de Homebrew. Tous démarrent avec **le même code EDK2 fourni par UTM**, une copie privée de ses variables, `virt-10.0`, GICv2, un Cortex-A72 émulé en TCG, un vCPU, 256 Mio de RAM, la série et un TPM 2.0 émulé. Le test ne passe pas par l’interface graphique UTM. NovaOS n’utilise pas le TPM. [Lanceur de référence UTM](https://github.com/utmapp/UTM/blob/main/QEMULauncher/Bootstrap.c) · [Machines ARM versionnées de QEMU](https://www.qemu.org/docs/master/system/arm/virt)

Les entrées sont des fichiers temporaires créés par le banc. Le support optique est en lecture seule, le disque VirtIO et les variables du firmware ont des surcouches jetables. Les deux essais sur CD utilisent un volume P9 préparé par le mécanisme de démonstration v0.14. Les trois essais sans CD présentent exactement les octets de l’ISO hybride sur le disque VirtIO. Ce témoin isole la lecture UEFI du disque, sans prétendre avoir installé un volume P9 sur celui-ci.

| Configuration | Observation | Ce que l’essai établit |
|---|---|---|
| UTM, profil NovaOS inchangé | QEMU retourne `Property 'virt-10.0-machine.msi' not found` pour `msi=off`. | La ligne de commande de qualification v0.14 ne démarre pas telle quelle avec le moteur de cette version d’UTM. |
| UTM, CD + disque P9 protégé, `its=off` | Limine charge le noyau. PROBE annonce pour le SID 48 une région de type MSI entre `0x08020000` et `0x08020fff` inclus. NovaOS répond `BLOC ouverture refusee Route`. | Désactiver ITS ne retire pas le cadre MSI de GICv2m. Mettre `vectors=0` sur le disque ne le retire pas non plus. Le bail est refusé avant la publication des services. |
| QEMU 11.1.3, CD + disque P9 protégé, `msi=off` | NovaShell accepte `cat /documents/exemple.txt`, affiche `NovaOS`, puis `exit` conduit à la collecte et au retrait réussi du bail. | Le support, le firmware UTM, l’ELF et la chaîne de lecture P9 fonctionnent ensemble dans cette topologie. |
| UTM, disque seul, VirtIO ordinaire sans IOMMU | EDK2 charge le support en `Pci(0x6,0x0)`. NovaOS affiche sa version puis `BLOC ouverture refusee Feature`. | Le même support est amorçable comme disque. L’amorçage sans le contrôleur requis ne donne pas de bail à NovaFS. |
| UTM puis QEMU 11.1.3, disque seul, DMA protégé dès le reset | L’émulateur signale une traduction demandée pour le SID 48 encore inconnu. Aucune bannière noyau pendant la fenêtre de 30 secondes de chaque essai. | Sur ces configurations, le firmware n’a pas chargé le noyau depuis le disque protégé. L’attachement du SID par NovaOS intervient après le chargement de ce noyau. |

Ces observations distinguent deux obstacles. Mettre à disposition `msi=off` résout le premier dans le témoin sur CD. Le second concerne l’amorçage depuis le disque qui doit rester après éjection du CD. Le DTB produit avec le moteur UTM décrit bien un cadre GICv2m de 4 Kio à cette adresse. Dans le modèle amont de QEMU 10, la sélection de GICv2 crée ce cadre, même avec ITS désactivé. Le refus du pilote protège ici le contrat de confinement des DMA et des interruptions. [Construction du GIC de QEMU 10](https://github.com/qemu/qemu/blob/v10.0.0/hw/arm/virt.c#L861-L866) · [Traduction du SID avant attachement](https://github.com/qemu/qemu/blob/v10.0.0/hw/virtio/virtio-iommu.c#L1083-L1103) [Contrat bloc v0.10](stockage-bloc-v0.10.0.md) · [Vérification du domaine et du PROBE](../kernel/src/hardware/block/viommu.rs)

## Options présentées avant l’arbitrage A

Pour conserver UTM 5.0.8 et l’objectif d’un seul disque installé, nous recommandons **une qualification préalable de la chaîne de confiance dès le firmware**. Elle doit établir comment le firmware accède à l’ESP avec le DMA confiné, comment le noyau reprend cette protection, et comment l’acheminement des interruptions empêche le périphérique d’utiliser une destination non autorisée malgré la région GICv2m. Le bail actuel reste fermé tant que ces trois propriétés ne sont pas établies. Cela peut nécessiter un autre profil matériel, ou du travail sur le firmware et la HAL.

L’option B prévoyait un moteur exposant `msi=off` et un support permanent distinct du disque P9 protégé. L’arbitrage A l’a écartée pour la distribution. Le CD reste un témoin de laboratoire.

## Phases 2 à 4 — contrats en attente

* **Support d’installation.** La phase 1 n’a pas franchi son critère de passage sur le moteur UTM demandé. Les cibles de distribution `image`, `utm` et `installer` restent à réaliser. L’ISO de développement et son CD de qualification restent disponibles.
* **Sélection sûre.** L’installateur aura besoin d’un inventaire identifiant chaque support et celui qui a démarré la VM, d’une sélection explicite et d’un accès exclusif stable pendant la vérification et l’écriture. Il devra parcourir tout le disque cible, refuser le moindre octet non nul ou toute erreur de lecture, puis demander confirmation avant la première écriture. Une géométrie, un numéro de slot PCI ou une absence de GPT ne prouvent pas qu’un disque est vierge.
* **Organisation prévue.** GPT principale et de secours, ESP contenant l’application de démarrage, le noyau et sa configuration, puis partition de données P9 alignée à 4 Kio. Les bornes de la vue bloc devront être vérifiées pour chaque accès. Le formatage sur cette vue devra utiliser le moteur P9 et ses barrières de persistance. Le service général de fichiers ne doit recevoir aucun droit brut sur l’ESP ou la GPT. Le placement final et les GUID de type NovaOS restent à qualifier. [Spécification GPT UEFI](https://uefi.org/specs/UEFI/2.10/05_GUID_Partition_Table_Format.html)
* **État de la qualification.** Le partitionnement en invité, les refus de disques sales, la sélection parmi plusieurs cibles, les interruptions de copie, les échecs de FLUSH pendant l’installation, le redémarrage autonome sur P9 et sa persistance restent à tester après arbitrage. Aucune procédure d’installation ni mise à jour A/B n’est qualifiée. Le shell du témoin est une console de qualification sans comptes NovaAuth.

## Reproduction

Depuis la racine du dépôt sur macOS Apple Silicon, avec UTM installé dans `/Applications`, QEMU Homebrew, Xcode ou ses outils en ligne de commande, Rust stable, xorriso et les dépendances Cargo du projet disponibles. Le test a besoin d’un socket Unix privé pour le TPM. Il crée un sous-répertoire unique de `novaOS/build/` contenant les traces série et les mesures JSON.

```sh
make -C novaOS ARCH=aarch64 kernel
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
python3 novaOS/tests/installation_utm.py
```

Le banc rejoue les deux refus du noyau, vérifie les messages de l’IOMMU, lit les octets depuis NovaShell dans le témoin positif et compare les empreintes SHA-256 du support, du volume P9 et des variables privées avant et après la campagne. Il n’accepte aucun chemin de disque cible. Le code EDK2 UTM testé a l’empreinte `ee769c4bf42a5350d33345fc1b16d00156fe0c25fb3c68debd003bd844e4e3fa`. [Banc de reproduction](../tests/installation_utm.py)

## Temps et mémoire mesurés

Le tableau donne le premier passage de chaque configuration sur macOS ARM64, le 11 novembre 2026. Le temps part du lancement du processus QEMU. Le RSS est le maximum échantillonné à environ 250 ms pour ce processus hôte. Les cadres de 4 Kio encore libres sont lus sur la série lors de l’initialisation de la MMU, avant le shell.

| Configuration | Observation chronométrée | Pic RSS de l’émulateur (Kio) | Cadres invités libres à l’initialisation |
|---|---:|---:|---:|
| UTM, CD, disque protégé | Refus en 4,772 s | 509 552 | 57 153 |
| QEMU 11.1.3, CD, disque protégé | Invite en 16,274 s | 499 152 | 57 153 |
| UTM, disque témoin seul | Noyau puis refus en 4,681 s | 405 408 | 57 293 |
| UTM, disque protégé seul | Observation de 30,123 s | 384 848 | Noyau non atteint |
| QEMU 11.1.3, disque protégé seul | Observation de 30,175 s | 333 392 | Noyau non atteint |

La fin de lecture du fichier et la fermeture du shell du témoin positif sont observées à 16,391 s. Une seconde campagne sur de nouvelles images et de nouveaux états TPM a reproduit les cinq résultats. Sur le témoin positif, elle a atteint l’invite en 15,273 s. Les coûts de l’installateur et d’un système installé restent à mesurer. Les empreintes du volume P9, du support et des variables de firmware ont été identiques en début et fin de campagne.

## Contrôles du socle v0.14.0

Les contrôles historiques utilisent leur topologie QEMU livrée et leurs disques jetables. Ils vérifient séparément le socle conservé. Les commandes suivantes reproduisent les suites sélectionnées pour l’audit.

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test
    python3 novaFS/lab/p51-validation-suites.py "$arch"
done
python3 novaFS/lab/p9-interop.py
python3 novaFS/lab/p8-validation.py cli --sortie "${TMPDIR:-/tmp}/audit-utm-p8.json"
```

Les campagnes intégrales des 44 contrôles fichiers, des 30 contrôles d’espace de noms et des CLI v0.13 et v0.14 appartiennent à la qualification du commit de référence. Cet audit a exercé la lecture par NovaShell sur le témoin P9. Les suites complètes devront être rejouées sur toute future modification de la chaîne de démarrage ou du pilote. [Qualification v0.14.0](cli-v0.14.0.md)

| Exécution pendant l’audit | ARM64 | x86_64 |
|---|---:|---:|
| NovaOS, scénarios de démarrage historiques QEMU | 25/25 | 25/25 |
| NovaOS, groupes bloc QEMU, secteurs 512 et 4 096 | 24/24 | 24/24 |
| NovaOS, ELF multipage, cycle de vie et courses IPC | 3/3 | 3/3 |
| NovaFS autonome sur macOS, tests standards | 192 réussis | 192 réussis sous Rosetta |
| NovaFS autonome sur macOS, tests instrumentés | 193 réussis | 193 réussis sous Rosetta |
| Campagne différentielle NovaFS | 1 000 000 en 25,599 s | 1 000 000 en 40,919 s |

Chaque campagne différentielle a observé 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités. Les passages Cargo standards et instrumentés ont chacun signalé quatre tests ignorés par défaut. Les bancs d’interopérabilité P8 et P9 ont également abouti. Pour P9, chacun des quatre échanges architecture/secteur a comparé 59 017 octets et effectué deux restaurations. L’audit conserve les sources et licences de NovaOS et de NovaFS.

## Arbitrage A — prise en charge du disque dès le firmware

Base vérifiée sur `main` avec un arbre propre au commit `6db9d0e0e7ab7bc1d499ed7254dbd2c699d4f563`. L’arbitrage porte sur UTM 5.0.8, QEMU 10.0.12 et un disque VirtIO autonome. Le firmware essayé est celui de l’application UTM, d’empreinte SHA-256 donnée plus haut. Tous les disques de cette campagne ont `iommu_platform=on`, avec ATS désactivé et `boot-bypass=off` sur le contrôleur. Aucun support physique n’est utilisé.

### Ce que voit effectivement le firmware

Le banc installe une petite application AArch64 en tête d’une image UEFI hybride temporaire. EDK2 l’exécute par le chemin de démarrage normal. Elle interroge le protocole EDK II IOMMU, enregistre une notification de sortie des services et fait charger le binaire Limine vérifié depuis le même support. Elle utilise uniquement la série pour ses marqueurs. Elle laisse au firmware ses files et permissions DMA. La notification signale l’événement. La bannière ultérieure du noyau établit la poursuite du démarrage. Les traces QEMU des périphériques et la série empruntent le même journal. [Témoin UEFI](../tests/uefi_temoin/main.rs) · [Banc et assertions](../tests/uefi_phases.py) · [GUID EDK II interrogé](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Include/Protocol/IoMmu.h)

| Phase | Propriétaire et observation | Garantie établie |
|---|---|---|
| Reset puis initialisation DXE, disque seul | Le firmware met VirtIO à l’état 15 (`DRIVER_OK`). QEMU trace ensuite une lecture DMA du SID 48, puis « sid=48 is not known ». Aucun `ATTACH`, aucun `MAP`, aucune requête de secteur remise au gestionnaire de lecture bloc. | Le refus survient pendant l’amorçage du support, avant même le chargement du témoin EFI. Les traces identifient une traduction mémoire. Elles ne permettent pas d’attribuer un LBA ou une instruction précise du firmware à cette tentative. |
| Support optique de référence, avant la sortie UEFI | En chargeant le témoin depuis le contrôleur SCSI optique de laboratoire, `LocateProtocol(EDKII_IOMMU_PROTOCOL)` renvoie `EFI_NOT_FOUND`. Limine prend la main. | À cet instant le protocole n’est pas publié. Le transport optique historique ne démontre pas le confinement de ses propres DMA. Dans un deuxième essai, imposer également `ACCESS_PLATFORM` au contrôleur optique provoque le refus du SID 8 avant le témoin. |
| Notification de sortie, sur le support de référence | La ligne `OBS EFI notification ExitBootServices` précède le contrôle MMU de NovaOS et sa bannière. Aucun domaine de disque installé par le firmware n’a été observé. | La protection des sections par la MMU concerne le processeur. Ni la notification UEFI ni les permissions CPU ne constituent un transfert de domaine IOMMU. Sur disque seul, le firmware n’atteint pas cette transition dans la fenêtre observée. |
| EL1, UTM, GICv2m | La découverte du disque et du contrôleur réussit, puis le `PROBE` du SID 48 renvoie une réserve MSI. Refus `Route` avant l’attachement et avant le premier accès bloc. | NovaOS ne publie aucun bail pour ce disque. Le service P9 et NovaShell ne peuvent donc pas être validés dans cette configuration. |
| EL1, contrôle QEMU 11 avec MSI désactivé | Le noyau crée le domaine 1, attache le SID 48, installe ses sept traductions puis accède au disque P9. À la fermeture il retire les traductions et détache le SID. | Ce témoin qualifie le chemin après prise de contrôle par NovaOS sur la topologie historique. |

La séquence observée est cohérente avec le pilote bloc EDK II amont, qui configure l’anneau et les adresses de file avant les lectures. Sa notification de sortie réinitialise le périphérique. Une telle réinitialisation ne peut pas fournir les droits nécessaires à la **première** lecture de l’ESP. Même en cas de succès du démarrage optique, cette notification seule ne prouve pas que chaque DMA en vol est terminé et que les anciennes traductions ont été invalidées avant recyclage des pages. Les sources amont servent à interpréter les traces du binaire distribué. [Initialisation de VirtioBlkDxe](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/VirtioBlkDxe/VirtioBlk.c#L814-L889) · [Notification de sortie du pilote](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/VirtioBlkDxe/VirtioBlk.c#L1012-L1041)

### Routage PCI et limite du confinement MSI

Le DTB produit par le moteur **UTM installé** donne le masque PCI `<0x1800 0 0 7>`. Pour `00:06.0` et INTA, la ligne sélectionnée dans `interrupt-map` aboutit à SPI 5 à niveau haut, soit **INTID 37**. Le pilote vérifie le type PCI, le pin et l’exclusivité de cette ligne sur le bus racine. La HAL utilise le distributeur et l’interface CPU GICv2 à adresses fixes. Le code attend le disque en `00:06.0`, le contrôleur en `00:05.0` et une disposition précise des capacités et des BAR VirtIO. Cette découverte vérifie une topologie prédéfinie. Elle ne sélectionne pas un disque d’installation arbitraire. [Vérifications PCI](../kernel/src/hardware/block/pci.rs) · [HAL GICv2](../kernel/src/arch/aarch64/timer.rs)

Avec GICv2 et `its=off`, le DTB décrit aussi un cadre GICv2m de 4 Kio à `0x08020000`. La propriété PCI `msi-map` vise ce cadre et couvre le SID 48. Le `PROBE` invité confirme les bornes inclusives `0x08020000–0x08020fff`. Le modèle QEMU amont crée cette réserve au branchement de l’IOMMU. Pour un endpoint connu, sa traduction traite la réserve MSI avant de consulter les mappages ordinaires du domaine. Les sept pages privées, l’exclusivité d’INTx et `vectors=0` n’établissent donc aucun filtrage de cette destination MSI par source. Le banc vérifie les tables et la décision du pilote. Il n’émet aucune interruption MSI de test. [Installation de la réserve](https://github.com/qemu/qemu/blob/v10.0.0/hw/arm/virt.c#L2689-L2718) · [Ordre de traduction](https://github.com/qemu/qemu/blob/v10.0.0/hw/virtio/virtio-iommu.c#L1093-L1140) · [PROBE avant ATTACH](../kernel/src/hardware/block/viommu.rs)

Le **même moteur UTM** accepte `gic-version=3,its=off`. Le DTB produit conserve le routage PCI INTx du disque et ne présente plus de `msi-map`. Un démarrage séparé sur ce disque seul refuse encore le premier DMA du SID 48. Ce profil mérite une qualification des interruptions GICv3 si retenu. NovaOS v0.14.0 possède une HAL GICv2, et cet essai s’arrête avant l’exécution du noyau.

### Décision et conditions de reprise

L’installation autonome reste bloquée sur le firmware fourni. Un pilote chargé depuis l’ESP du même disque, un changement dans Limine ou un correctif EL1 arriveraient après le premier accès refusé. La suite proposée conserve UTM, son moteur et le disque unique. Elle demande un nouvel arbitrage sur **un pilote IOMMU de confiance disponible avant le pilote de stockage dans le firmware** et sur la qualification du profil **GICv3 sans ITS**.

Le pilote précoce devrait attacher chaque contrôleur d’amorçage avant son premier DMA, limiter les traductions aux pages nécessaires et aux directions permises, puis arrêter les files, attendre la fin des transferts et invalider les anciennes traductions avant toute restitution des pages EFI. NovaOS devrait vérifier cet état, conserver le refus par défaut et installer ses propres traductions avant de publier un bail. Une qualification devra couvrir l’échec et la répétition de la sortie EFI, les refus des autres SID et le démarrage autonome après retrait du support. La fourniture d’un tel firmware et la HAL GICv3 restent à arbitrer.

### Reproduction et régressions de l’arbitrage A

Sur le Mac hôte, le nouveau banc utilise la cible Rust UEFI et `fdtget` (paquet `dtc`), en plus des prérequis de phase 1. `r-efi` 6.0.0 est verrouillé dans le banc uniquement. Le premier téléchargement de la cible et de la dépendance nécessite l’accès au registre correspondant.

```sh
rustup target add --toolchain stable aarch64-unknown-uefi
rustup run stable cargo fetch --locked --manifest-path novaOS/tests/uefi_temoin/Cargo.toml
make -C novaOS ARCH=aarch64 kernel
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
python3 novaOS/tests/uefi_phases.py
```

Le banc conserve les deux DTB, les journaux et son bilan dans `novaOS/build/arbitrage-a-*/`. Il compare les empreintes de l’image amorçable, du volume P9, des variables de départ et du firmware après les essais. Les écritures du témoin interactif vont dans une surcouche QEMU. Les durées de 30 secondes sur les cas bloqués correspondent à la fenêtre d’observation.

| Essai avec le firmware UTM | Résultat de la campagne instrumentée |
|---|---|
| UTM, GICv2, CD de référence et disque protégé | Notification de sortie, bannière puis refus `Route` en 4,787 s. Zéro attachement et zéro lecture bloc. |
| UTM, GICv2, CD et disque tous deux protégés | SID 8 refusé à la première lecture DMA. Aucun témoin EFI, observation de 30,133 s. |
| UTM, GICv2, disque seul protégé | SID 48 refusé. Zéro attachement, zéro lecture bloc en 30,042 s. |
| UTM, GICv3 sans ITS, disque seul protégé | Même refus du SID 48, zéro lecture bloc en 30,040 s. |
| QEMU 11.1.3, GICv2 sans MSI, CD de référence et disque protégé | Invite en 15,535 s. `cat /documents/exemple.txt` affiche `NovaOS`. Fermeture vérifiée à 15,638 s. Un attachement, sept mappages, sept retraits et un détachement. Le modèle a traité 10 640 requêtes de lecture et 32 d’écriture sur la surcouche. |
| QEMU 11.1.3, GICv2 sans MSI, disque seul protégé | SID 48 refusé. Zéro attachement et zéro lecture bloc en 30,203 s. |

Lors des premiers passages instrumentés, le lecteur de journaux attendait CRLF avant la réponse de `cat`. Une ligne de trace QEMU, terminée par LF, pouvait la précéder et empêcher l’envoi de `exit`. Le lecteur reconnaît maintenant les deux terminaisons à cet endroit et exige toujours la réponse exacte du fichier, la nouvelle invite et la confirmation de fermeture du bail. Le tableau rapporte la campagne complète après correction.

Les commandes de la section « Contrôles du socle v0.14.0 » ont été réexécutées sur cette base. Résultats de cette campagne :

| Contrôle | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages QEMU historiques | 25/25 | 25/25 |
| Groupes bloc, incluant protections et révocation DMA, secteurs 512 et 4 096 | 24/24 | 24/24 |
| ELF multipage, cycle de vie, courses IPC | 3/3 | 3/3 |
| NovaFS macOS, standards / instrumentés | 192 / 193 réussis | 192 / 193 réussis sous Rosetta |
| Opérations différentielles NovaFS | 1 000 000 en 25,249 s | 1 000 000 en 40,676 s |

Chaque passe Cargo conserve quatre tests ignorés par défaut. Chaque campagne différentielle a compté 345 183 publications, 54 643 pénuries et 1 000 remontages audités. Les deux bancs d’interopérabilité NovaFS P8 et P9 se sont terminés avec le code zéro. Les essais de firmware ici décrits utilisent un disque avec secteurs de 512 octets. Les deux géométries figurent dans les régressions bloc. L’interface graphique UTM, le partitionnement depuis l’invité et la persistance après installation restent hors des résultats obtenus.

## Qualification expérimentale EDK2 et GICv3

Base vérifiée sur `main` au commit `e52a077d6fb4b527d8ddc75d8b16869f3af39d3b`, avec arbre propre. Le banc utilise les bibliothèques de l’application UTM 5.0.8 installée, QEMU 10.0.12, `virt-10.0`, TCG, un Cortex-A72 et 256 Mio. Chaque démarrage UEFI utilise un disque VirtIO unique en `00:06.0`, une image amorçable jetable en surcouche et des variables privées correspondant à la ROM. Le contrôleur en `00:05.0` impose `boot-bypass=off`. Le disque exige `iommu_platform=on`, avec ATS désactivé et zéro vecteur MSI-X.

### Firmwares disponibles et initialisation précoce

| Code AArch64 fourni par UTM | SHA-256 du code pflash | Modules identifiables dans le volume DXE |
|---|---|---|
| `edk2-aarch64-code.fd` | `ee769c4bf42a5350d33345fc1b16d00156fe0c25fb3c68debd003bd844e4e3fa` | 82 noms de modules distincts dans les chemins CodeView |
| `edk2-aarch64-secure-code.fd` | `c85a57de1ac39e550a6529bd66a4214eb1d8c14dcda7e22dedf72566a769fbc7` | 84 noms |

Les deux volumes LZMA contiennent des références à `ArmGicDxe`, `ArmTimerDxe`, `PciHostBridgeDxe`, `PciBusDxe`, `Virtio10` et `VirtioBlkDxe`. Aucun nom extrait ne désigne un pilote IOMMU ou SMMU. La preuve comportementale est la tentative du SID 48 avant le premier `ATTACH`. La révision source exacte des ROM reste inconnue. L’essai de la variante `-secure` utilise son modèle de variables fourni. Le refus précède la lecture de l’application EFI et toute vérification de sa signature sur ce disque.

UTM 5.0.8 construit ses arguments avec GICv3 par défaut pour AArch64 et `virtualization=on` pour cette machine en TCG. Son générateur sélectionne le code pflash standard ou Secure Boot et prévoit le cas d’un pflash personnalisé. Cette possibilité permettrait d’essayer une construction séparée. Les essais utilisent le moteur de l’application directement, sans import de VM dans son interface graphique. [Générateur d’arguments UTM 5.0.8](https://github.com/utmapp/UTM/blob/v5.0.8/Configuration/UTMQemuConfiguration%2BArguments.swift#L548-L605)

L’arbre de comparaison EDK II `edk2-stable202502`, commit `fbe0805b2091393406952e84724188f8c1941837`, fournit les interfaces nécessaires. `VirtioBlkDxe` négocie le bit 33, mappe l’anneau avant son activation, puis mappe l’en-tête, les données et le statut de chaque requête. Le transport moderne délègue à `EFI_PCI_IO_PROTOCOL.Map`. Le pont racine peut appeler `EDKII_IOMMU_PROTOCOL.Map` et le bus PCI appelle ensuite `SetAttribute` avec le handle de la fonction et les droits demandés. Le volume ArmVirtQemu examiné ne contient aucun fournisseur virtio-IOMMU pour ce contrôleur. Le module OVMF pour SEV/TDX vise la mémoire confidentielle x86. Le module ARM `NonCoherentIoMmuDxe` traite la cohérence mémoire et sa méthode `SetAttribute` retourne succès sans programmer de domaine. [Composition du volume ArmVirt](https://github.com/tianocore/edk2/blob/edk2-stable202502/ArmVirtPkg/ArmVirtQemuFvMain.fdf.inc) · [Pilote bloc](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/VirtioBlkDxe/VirtioBlk.c) · [Transport PCI VirtIO 1.0](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/Virtio10Dxe/Virtio10.c) · [Droits par fonction PCI](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Bus/Pci/PciBusDxe/PciIo.c) · [Module de cohérence ARM](https://github.com/tianocore/edk2/blob/edk2-stable202502/EmbeddedPkg/Drivers/NonCoherentIoMmuDxe/NonCoherentIoMmuDxe.c)

Le point d’intégration se situe dès la connexion du transport PCI. Dans la source examinée, `Virtio10BindingStart` demande la maîtrise de bus. La simple présence d’un DXE dans le volume, ou sa position dans la liste FDF, ne garantit donc pas l’ordre voulu. Il faudrait interdire la connexion du pilote de disque tant que le contrôleur de confiance n’a pas initialisé ses propres files et que le chemin PCI du disque n’est pas associé au SID attendu. Toute erreur d’attachement ou d’octroi doit empêcher la publication des adresses de file. [Connexion du transport](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/Virtio10Dxe/Virtio10.c#L1058-L1140) · [Contrat IOMMU d’EDK II](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Include/Protocol/IoMmu.h)

### Matrice des démarrages et des interruptions

| Moteur et entrée | Résultat observé | Portée |
|---|---|---|
| UTM, firmware standard, disque seul, GICv3 sans ITS | Première traduction en lecture après `DRIVER_OK`, SID 48 inconnu, IOVA `0x4e8ac000`. Zéro `ATTACH`, `MAP`, lecture ou écriture bloc en 30,007 s. | Refus avant le témoin de l’ESP, Limine et NovaOS. |
| UTM, firmware Secure Boot, même disque et même GIC | Même SID et même direction, IOVA `0x4a9a7000`, mêmes compteurs nuls en 30,007 s. | L’ESP protégée reste inaccessible. |
| UTM, mêmes deux ROM, avec `virtualization=on` (EL2 exposé) | Refus au même SID et à l’IOVA propre à chaque ROM. Zéro attachement, mappage ou opération bloc en 30,013 s (standard) et 30,008 s (Secure Boot). | La propriété supplémentaire du profil TCG d’UTM reproduit le blocage. |
| UTM, sonde EL1 chargée en RAM par le banc, même GIC et disque en `00:06.0` | Timer physique 30 puis INTx 37, acquittés et terminés. | Qualification de la route de configuration du disque. Aucune file de données activée. |
| UTM, GICv2 sur CD, témoin antérieur `e52a077` | Région MSI GICv2m `0x08020000–0x08020fff`, ouverture refusée avec `Route`. | Refus du bail avant attachement. |
| QEMU 11.1.3, CD, GICv2 avec MSI désactivé, même témoin antérieur | NovaShell lit P9. Le domaine 1 du SID 48 reçoit sept mappages, puis sept retraits et un détachement. | Chemin NovaOS après le chargement par CD. |

Les quatre essais UEFI ont utilisé le même disque amorçable (SHA-256 `bad1a5064c8502d2c6d07db494f38df05c86a70589c2883cedfc5393e1b7d93d`). Le seul INTID acquitté et terminé par les deux ROM, avec et sans exposition d’EL2, est 27. Les résultats portent sur TCG, des secteurs de 512 octets et un vCPU. Le démarrage par HVF et la validation de l’interface graphique UTM ne figurent pas dans cette campagne.

Le DTB UTM annonce GICD à `0x08000000` (64 Kio), la région GICR à `0x080a0000` et INTA du slot 6 sur SPI 5, donc INTID 37, à niveau haut. Il n’annonce ni cadre GICv2m, ni ITS, ni `msi-map`. Le `iommu-map` conserve l’identité du RID 48 et exclut le RID 40 du contrôleur lui-même. Une réponse invitée `PROBE` sans réserve reste à mesurer pour le SID 48 sur cette topologie.

Le firmware utilise l’interface système du GICv3. Dans le profil de référence, ses lectures de `ICC_IAR1` et écritures de `ICC_EOIR1` portent sur le timer virtuel 27. La sonde EL1 indépendante utilise le redistributeur et le timer physique non sécurisé 30. Après cette première IRQ, le banc porte de 1 à 2 Mio la capacité d’un fichier raw créé pour cet essai. Le disque lève INTA. Le gestionnaire reçoit 37, lit le bit de changement de configuration dans l’ISR VirtIO et la nouvelle capacité de 4 096 secteurs. La ligne retombe, le gestionnaire la masque, termine l’IRQ et vérifie les bits enable, pending et active à zéro. La maîtrise de bus PCI du disque reste coupée et les traces ne contiennent aucun DMA, attachement ou accès bloc. [Sonde EL1](../tests/gic3_el1/sonde.c) · [Notification de changement de capacité dans QEMU](https://github.com/qemu/qemu/blob/v10.0.0/hw/block/virtio-blk.c) · [Assertions sur les traces](../tests/qualification_gic3.py)

La HAL livrée de NovaOS mappe une page du distributeur et l’interface CPU GICv2 à `0x08010000`. Pour utiliser le GICv3, elle doit sélectionner le bon profil avant son premier accès, mapper GICR et le registre de routage GICD du SPI, puis acquitter et terminer les IRQ via les registres `ICC_*`. La sonde démontre la livraison physique des deux sources sous TCG. L’ordonnanceur NovaOS, une interruption de **complétion** du disque, la purge pendant un bail et la révocation sous GICv3 restent à qualifier ensemble. [Mappages EL1](../kernel/src/arch/aarch64/paging.rs) · [HAL de production](../kernel/src/arch/aarch64/timer.rs)

### Domaine DMA et transfert de responsabilité

| Étape | Garantie mesurée ou condition à satisfaire |
|---|---|
| Reset et première traduction | Sur les ROM livrées, le SID inconnu est refusé avant la remise d’un secteur au modèle bloc. Le journal fournit une IOVA et une direction, sans identifier de LBA. |
| Première lecture autorisée | Un DXE de confiance résident dans le pflash devrait initialiser ses files de contrôle privées, vérifier la topologie et les réserves par `PROBE`, puis attacher le SID 48 à un domaine vide, sans bypass. Chaque `MAP` doit être acquitté avant l’utilisation de la file. Le tampon de lecture ne doit accorder au disque que l’écriture. Les objets de l’anneau et les tampons communs doivent occuper des pages dédiées, sans données EFI voisines. Les autres SID restent refusés. Aucun démarrage mesuré n’a atteint cette étape sous UEFI. |
| Sortie des services | Le callback amont de `VirtioBlkDxe` écrit zéro dans le statut. La révocation doit également arrêter les soumissions, drainer les transferts, confirmer le reset et l’arrêt de la maîtrise de bus, retirer les mappages et détacher le SID avant recyclage. Le `PciIoUnmap` amont ignore le retour de `SetAttribute(..., 0)` avant d’appeler `Unmap`. Le fournisseur doit retenir les pages tant que la révocation effective peut échouer. Un rejet de `MapKey`, une nouvelle tentative et une faute pendant le retrait doivent maintenir le refus ou la quarantaine. [Callback VirtIO](https://github.com/tianocore/edk2/blob/edk2-stable202502/OvmfPkg/VirtioBlkDxe/VirtioBlk.c#L1040-L1060) · [Ordre des événements DXE](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Core/Dxe/DxeMain/DxeMain.c#L777-L818) |
| Reprise par NovaOS | Le noyau utilise les cadres déclarés disponibles, puis restitue les cadres du chargeur avant l’ouverture du bail bloc. La garantie de quiescence doit donc être établie **avant la première réutilisation possible**. Le contrat Limine v0.14.0 ne transmet pas de preuve de retrait DMA. Sur la topologie historique, le pilote installe un domaine neuf et sept pages privées. Les tests vérifient le refus d’écriture sur un tampon en lecture seule ou démappé, le traitement d’une IRQ tardive, le retrait et les cadres maintenus en quarantaine en cas de transfert bloqué. [Entrée du noyau](../kernel/src/main.rs) · [Contrat de démarrage](../kernel/src/boot.rs) · [Domaine ARM](../kernel/src/hardware/block/viommu.rs) · [Retrait du bail](../kernel/src/hardware/block/lease.rs) |

### Reproduction et régressions

Sur macOS ARM64, avec les prérequis de l’arbitrage A, le Clang de Xcode et le `rust-lld` du toolchain stable, lancer depuis la racine du dépôt les commandes ci-dessous. Les deux supports pflash, leurs modèles de variables et le disque amorçable sont contrôlés par empreinte. Le banc conserve le DTB, les journaux et le bilan JSON dans un répertoire unique `novaOS/build/qualification-gic3-*/`. Les essais pflash demandent un socket Unix privé pour le TPM.

```sh
make -C novaOS ARCH=aarch64 kernel
python3 novaOS/tests/qualification_gic3.py
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test
    python3 novaFS/lab/p51-validation-suites.py "$arch"
done
```

| Contrôle réexécuté | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages historiques NovaOS sous QEMU | 25/25 | 25/25 |
| Groupes bloc, secteurs 512 et 4 096, isolation et révocation incluses | 24/24 | 24/24 |
| ELF multipage, cycle de vie des processus, courses IPC | 3/3 | 3/3 |
| NovaFS standard, exécutables Mach-O vérifiés | 192 réussis, natif | 192 réussis, Rosetta |
| NovaFS instrumenté | 193 réussis, natif | 193 réussis, Rosetta |
| NovaFS différentiel | 1 000 000 en 25,566 s | 1 000 000 en 50,223 s |

Chaque passe Cargo compte quatre cas ignorés par défaut. La campagne différentielle séparée a vérifié, par architecture, 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités.

**Arbitrage au commit `572bbd7`.** Autoriser ou écarter le développement d’un pilote DXE virtio-IOMMU précoce et du contrat de remise à zéro vérifiable avant réemploi des pages, ainsi que l’intégration d’un profil GICv3 dans la HAL NovaOS. La recommandation est de poursuivre sur un firmware de laboratoire distinct avant de décider de sa distribution. La première lecture confinée de l’ESP, la reprise après échec de sortie EFI, la révocation des anciennes IOVA et le démarrage autonome de NovaShell sur P9 restent les critères d’acceptation. Ils sont inaccessibles avec les deux firmwares testés. L’adoption du firmware, les phases 2 à 4 et toute évolution d’ABI restent soumises à décision.

## Prototype DXE et HAL GICv3

Le travail part du HEAD propre `572bbd79057826dc5accbb2e139c51f572822064` sur `main`. Le pflash expérimental charge, depuis le seul disque, le témoin EFI, le Limine du dépôt et le noyau. Après la sortie d’UEFI, NovaOS vérifie le contenu de 512 octets au LBA 16, reçoit l’INTx de complétion et ferme son domaine. Le secteur porte un motif connu du banc, dans l’espace libre entre le MBR et la partition FAT. Le démarrage de NovaShell sur une installation P9 autonome reste à qualifier. [Banc UTM](../tests/prototype_uefi.py) · [Mesures par essai](validation/prototype-uefi/resultats.json) · [Journaux bruts compressés](validation/prototype-uefi/journaux.tar.gz)

### Protection avant la première lecture

Dans l’exemple Intel, le PEIM récupère les informations VT-d, prépare les unités de remappage, construit les tables et active la traduction. Le DXE expose les méthodes d’association et de retrait de l’interface IOMMU d’EDK II. La description DMAR, les tables de contexte et les commandes d’invalidation de ce matériel sont spécifiques à Intel. L’apport retenu pour NovaOS est l’ordre des opérations entre fournisseur de protection, bus PCI et client disque. [PEI Intel VT-d](https://github.com/tianocore/edk2-platforms/blob/master/Silicon/Intel/IntelSiliconPkg/Feature/VTd/IntelVTdDmarPei/IntelVTdDmarPei.c#L669-L703) · [DXE Intel VT-d](https://github.com/tianocore/edk2-platforms/blob/master/Silicon/Intel/IntelSiliconPkg/Feature/VTd/IntelVTdDxe/IntelVTdDxe.c)

Le pilote virtio-IOMMU de Linux associe les identifiants décrits par le firmware, transmet les droits de lecture et d’écriture et synchronise les demandes soumises à la virtqueue. Dans QEMU 10, le traitement d’un retrait notifie les plages, retire l’entrée de la table du domaine puis produit `virtio_iommu_unmap_done`. Un reset du périphérique conserve la valeur de bypass, tandis qu’un reset système la rétablit depuis `boot-bypass`. Le prototype vérifie chaque réponse de la file avant de changer le propriétaire d’une page. Les sources Linux et QEMU ont servi à l’étude du protocole. Les sources originales du DXE portent la licence MIT du projet. Les fichiers EDK II modifiés dans la copie de travail conservent leurs notices amont. [VirtIO 1.3, §5.13](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html) · [Linux v6.14](https://github.com/torvalds/linux/blob/v6.14/drivers/iommu/virtio-iommu.c) · [Retrait dans QEMU 10](https://github.com/qemu/qemu/blob/v10.0.0/hw/virtio/virtio-iommu.c#L790-L830) · [Reset du modèle QEMU](https://github.com/qemu/qemu/blob/v10.0.0/hw/virtio/virtio-iommu.c#L1282-L1295)

Le firmware de banc provient d’`ArmVirtQemu.dsc`, révision EDK II `fbe0805b2091393406952e84724188f8c1941837` (`edk2-stable202502`), en AARCH64 RELEASE avec TPM 2.0. La construction inclut `NovaIommuDxe` dans le volume flash. La dépendance du pont racine PCI impose la présence d’`EDKII_IOMMU_PROTOCOL`. Le transport VirtIO moderne active le décodage de ses BAR et diffère la maîtrise de bus. Le bus PCI annule un jeton dont l’autorisation a échoué et propage une erreur de retrait. Le pilote bloc vérifie aussi la longueur rendue par l’anneau utilisé. [Configuration de la copie EDK II](../firmware/preparer.py) · [Contrat IOMMU EDK II](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Include/Protocol/IoMmu.h)

1. Le modèle démarre avec `boot-bypass=off`. Le DXE vérifie la topologie FDT et son `iommu-map`. Pour ce profil, le segment PCI est en correspondance RID/SID sur toute sa plage, à l’exception du contrôleur. Il recoupe le RID de ce contrôleur avec son identité PCI, puis identifie l’unique disque moderne `1af4:1042` par son handle et son emplacement PCI. Le slot 6 donne le SID 48 et le slot 7 donne le SID 56. Une demande provenant du contrôleur SCSI ne reçoit aucun droit.
2. Le contrôleur utilise deux files privées en RAM, une pour les commandes et une pour les fautes. Le DXE vérifie le granule de 4 Kio, les plages d’IOVA et de domaines, `ACCESS_PLATFORM` pour le disque, le bypass à zéro et une réponse `PROBE` sans réserve. Il attend le succès d’`ATTACH` au domaine vide 2. Les premiers `MAP` couvrent l’anneau du disque. Le DXE active ensuite sa maîtrise de bus et relit le bit BME.
3. Chaque transfert possède un jeton et une IOVA qui progresse pendant cette exécution du firmware. L’en-tête et les données destinées au disque passent par des pages privées initialement à zéro, lisibles par le périphérique. Les données de lecture et l’octet de statut ne lui accordent que l’écriture. Les descripteurs et la partie disponible de l’anneau sont en lecture seule. La partie utilisée occupe une page privée bidirectionnelle sur le banc. La file de contrôle, les métadonnées DXE et la pile du demandeur restent hors des mappages du disque.
4. Avant une révocation, le fournisseur vérifie les indices disponible/utilisé et l’identité du descripteur terminé. Il émet `UNMAP` sur chaque plage entière. La progression de la file, la barrière mémoire, l’identité du descripteur, la longueur et le statut de la réponse sont contrôlés. La recopie vers le demandeur puis la restitution des pages de transit viennent après cet acquittement. Une réponse incertaine coupe la maîtrise de bus du disque et immobilise les pages. [Domaine et commandes](../firmware/NovaIommuDxe/Domain.c) · [Mappages et propriétaires](../firmware/NovaIommuDxe/Mapping.c)

### Sortie d’UEFI et reprise par NovaOS

Le callback `BeforeExitBootServices` ferme l’admission. Seul l’anneau du disque au repos peut encore être publié. Le DXE vérifie le reset du disque et la lecture de BME à zéro, retire les deux plages de l’anneau et attend `DETACH`. Il vérifie ensuite le reset du contrôleur, sa maîtrise de bus à zéro, le bypass toujours nul et l’égalité du nombre de jetons accordés et révoqués. Il réalise ce retrait sans allocation ni libération EFI. Au premier appel, EDK II délivre cet événement avant la validation du `MapKey`. Le témoin à clé périmée observe le refus, relit la carte mémoire et réussit sa deuxième sortie avec le domaine déjà fermé. [Ordre des événements dans le cœur DXE](https://github.com/tianocore/edk2/blob/edk2-stable202502/MdeModulePkg/Core/Dxe/DxeMain/DxeMain.c#L777-L818) · [Témoin EFI](../tests/uefi_temoin/main.rs)

Le noyau peut alors initialiser son allocateur et reprendre les pages du chargeur. À la découverte PCI, le profil expérimental vérifie BME et le statut VirtIO à zéro avant sa propre remise en état. Il attache le disque au domaine 1 et y accorde sept pages privées. Le banc recoupe les plages physiques du firmware avec les cadres réellement obtenus de l’allocateur NovaOS et écrits puis relus par leur nouveau propriétaire. Lors de l’essai dédié, les 97 pages physiques distinctes historiquement mappées par le DXE ont été réallouées de cette manière. Ce recoupement est fait sur l’hôte et n’ajoute aucun champ à l’ABI Limine ou NovaOS. [Découverte PCI](../kernel/src/hardware/block/pci.rs) · [Domaine du noyau](../kernel/src/hardware/block/viommu.rs) · [Sonde de réemploi](../kernel/src/memory/probes.rs)

### HAL et matrice de démarrage

La fonctionnalité Cargo `gicv3-lab` sélectionne le contrôleur du banc pour AArch64. La HAL vérifie la révision GICD et l’affinité GICR du PE, réveille son redistributeur, configure le groupe 1 et route le SPI à niveau haut vers ce PE. Elle relit les priorités du timer physique PPI 30 (`0x80`) et du disque SPI 37 (`0x40`), le masque CPU et le mode EOI. Le gestionnaire acquitte avec `ICC_IAR1_EL1`, traite l’ISR VirtIO et termine par `ICC_EOIR1_EL1`. La lecture de test exige un nouvel index utilisé, la longueur de 513 octets incluant le statut, un statut de succès, un nouvel IRQ et le motif attendu. Le DTB et le `PROBE` du disque concordent sur l’absence de fenêtre MSI. Le pilote bloc exige une ligne INTx exclusive. [HAL du banc](../kernel/src/arch/aarch64/gicv3.rs) · [Complétion du transport](../kernel/src/hardware/block/mod.rs) · [Contrôle des traces](../tests/prototype_traces.py)

| Configuration ou épreuve | Résultat observé |
|---|---|
| Pflash standard et Secure Boot **livrés par UTM**, disque seul protégé, GICv3 sans ITS, avec ou sans EL2 | Les quatre témoins de l’audit `572bbd7` refusent le premier DMA du SID inconnu avant toute lecture bloc. Ces mesures ont été conservées. |
| **DXE expérimental**, disque seul au slot 6, démarrage à froid | 146 lectures bloc par le firmware. 463 jetons accordés et retirés, soit 464 plages après séparation de l’anneau. Le noyau lit le LBA 16, vérifie ses 512 octets, reçoit et acquitte l’INTx 37, puis retire ses sept mappages. |
| Même disque, `virtualization=on` | Même passage du reset à la lecture vérifiée. Le modèle TCG et le profil GICv3 sont ceux du moteur fourni dans UTM. |
| Disque seul déplacé au slot 7 | Le DXE découvre le SID 56, charge Limine et le noyau, puis ferme son domaine. Le profil PCI NovaOS, fixé au slot 6 et à l’INTx 37, retourne `Absent`. |
| CD prioritaire sur VirtIO-SCSI protégé, disque au slot 6 | Le DXE refuse le RID 8 du contrôleur optique. BDS sélectionne le disque. NovaOS complète la lecture. Un amorçage **depuis le CD** demanderait un domaine et un pilote optique qualifiés. |
| Redémarrage QMP dans la même VM | Deux séquences complètes d’attachement, de lecture, de retrait et de détachement par propriétaire. Les mêmes variables EFI privées servent aux deux démarrages. |
| Arrêt forcé pendant la première lecture EFI | `blkdebug` confirme la suspension de la requête. Le compteur du backend indique zéro lecture et zéro écriture terminées. Cinq plages restent épinglées à la destruction de la VM. Une nouvelle VM reprend avec les mêmes variables et le même disque et termine la lecture noyau. |
| Première clé de carte mémoire volontairement périmée | Un seul retrait du domaine firmware, refus de la première sortie, puis réussite de la seconde tentative. Le témoin termine après EFI. |
| Variante négative qui retient le premier `UNMAP` avant publication | Délai du canal de contrôle, BME relu à zéro, cinq plages conservées en quarantaine. La série ne franchit pas la sortie EFI. |
| Témoin historique QEMU 11.1.3 avec CD et `msi=off` | La lecture P9 depuis NovaShell reste un résultat historique distinct. |

Le test négatif côté noyau exerce son propre tampon privé, d’abord en lecture seule pour le disque, puis démappé. Dans les deux cas, le témoin mémoire reste intact et le contrôleur signale une faute. Le test masque une IRQ de lecture en vol, retire le bail, purge la route et vérifie le nouveau propriétaire. Une IRQ tardive est acquittée sans livraison à l’ancien client. Pour chaque démarrage positif, le vérificateur indépendant associe chaque traduction observée au SID, à la plage et aux droits actifs. Il rapproche les retraits demandés des événements `unmap_done` du modèle et exige un ensemble vide au détachement. Les deux arrêts avec pages épinglées ont leurs propres critères. [Journal et compteurs par profil](validation/prototype-uefi/resultats.json)

### Reproduction

Sur macOS ARM64, le banc utilise UTM 5.0.8 dans `/Applications`, ses bibliothèques QEMU 10.0.12 et swtpm, le socket Unix privé du TPM, Xcode Command Line Tools, LLVM/LLD, ACPICA (`iasl`), Rust stable, `qemu-img`, `xorriso` et Python 3.11 ou ultérieur. Les cibles Rust sont `aarch64-unknown-uefi` et `aarch64-unknown-none-softfloat`. Depuis la racine du dépôt, utiliser une copie de travail jetable. Les commandes de construction EDK II ci-dessous s’exécutent dans Bash.

```sh
set -e
NOVA_SRC="$PWD"
NOVA_LAB="$(mktemp -d /tmp/nova-uefi.XXXXXXXX)"
git clone --depth 1 --branch edk2-stable202502 https://github.com/tianocore/edk2.git "$NOVA_LAB/edk2"
cd "$NOVA_LAB/edk2"
git submodule update --init --depth 1 --jobs 5 \
  BaseTools/Source/C/BrotliCompress/brotli MdeModulePkg/Library/BrotliCustomDecompressLib/brotli \
  MdePkg/Library/BaseFdtLib/libfdt MdePkg/Library/MipiSysTLib/mipisyst \
  CryptoPkg/Library/OpensslLib/openssl CryptoPkg/Library/MbedTlsLib/mbedtls \
  MdeModulePkg/Universal/RegularExpressionDxe/oniguruma SecurityPkg/DeviceSecurity/SpdmLib/libspdm
make -C BaseTools/Source/C -j6 CC=/usr/bin/clang CXX=/usr/bin/clang++ EXTRA_OPTFLAGS=-Wno-error
export CLANGDWARF_BIN="$(brew --prefix llvm)/bin/"
export PATH="$(brew --prefix lld)/bin:$(brew --prefix acpica)/bin:${CLANGDWARF_BIN}:$PATH"
export PYTHON_COMMAND="$(command -v python3)"
. ./edksetup.sh
for variante in normal sans-ack; do
    options=()
    if [ "$variante" = sans-ack ]; then options+=(--retrait-suspendu); fi
    python3 "$NOVA_SRC/novaOS/firmware/preparer.py" "$PWD" "${options[@]}"
    build -a AARCH64 -t CLANGDWARF -b RELEASE -p ArmVirtPkg/ArmVirtQemu.dsc -n 6 -D TPM2_ENABLE=TRUE
    mkdir -p "$NOVA_LAB/$variante"
    cp Build/ArmVirtQemu-AARCH64/RELEASE_CLANGDWARF/FV/QEMU_{EFI,VARS}.fd "$NOVA_LAB/$variante/"
done
cd "$NOVA_SRC/novaOS"
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked \
  --target aarch64-unknown-none-softfloat --features gicv3-lab --target-dir "$NOVA_LAB/kernel"
python3 tests/prototype_uefi.py "$NOVA_LAB/normal" "$NOVA_LAB/kernel/aarch64-unknown-none-softfloat/release/novaos" \
  --fv-sans-ack "$NOVA_LAB/sans-ack"
```

Le banc prépare des pflash de 64 Mio, un disque FAT jetable et des variables privées pour chaque scénario. Il conserve les ROM, l’ELF, les traces invitées et hôtes et leurs empreintes dans le répertoire `build/prototype-dxe-*` affiché au lancement. Pour vérifier les preuves archivées du 12 novembre, depuis la racine du dépôt, exécuter les commandes suivantes.

```sh
NOVA_PREUVES="$(mktemp -d /tmp/nova-preuves.XXXXXXXX)"
tar -xzf novaOS/docs/validation/prototype-uefi/journaux.tar.gz -C "$NOVA_PREUVES"
python3 novaOS/tests/prototype_traces.py "$NOVA_PREUVES"
```

### Régressions, limites et décision

| Contrôle rejoué | AArch64 | x86_64 |
|---|---:|---:|
| NovaOS, scénarios historiques de démarrage | 25/25 | 25/25 |
| NovaOS, groupes bloc sur secteurs de 512 et 4 096 octets, isolation et révocation comprises | 24/24 | 24/24 |
| NovaOS, ELF, cycle des processus et courses IPC | 3/3 | 3/3 |
| NovaFS v0.9.13, tests standards puis instrumentés | 192 puis 193 | 192 puis 193 |
| NovaFS, opérations différentielles | 1 000 000 | 1 000 000 |

L’hôte ARM64 a exécuté la cible native et la cible x86_64 sous Rosetta. Quatre cas sont ignorés par défaut dans chaque passe Cargo. Par architecture, le différentiel a observé 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités. Les exécutables de contrôle ARM64 et x86_64 ont été identifiés par leur en-tête Mach-O. L’indicateur macOS `sysctl.proc_translated` valait 1 sous x86_64. Les deux contrôles d’interopérabilité NovaFS P8/P9 se sont également terminés avec le code zéro. Les journaux des régressions accompagnent les traces du prototype.

**Périmètre démontré.** Les mesures couvrent un vCPU, TCG Cortex-A72, 256 Mio, `virt-10.0`, des secteurs de 512 octets, un disque moderne avec `iommu_platform=on,ats=off,vectors=0`, le contrôleur virtio-IOMMU de QEMU et une chaîne EFI de confiance. Le domaine firmware et la HAL sont expérimentaux. Le profil HHDM, l’ABI, le format P9 et les variantes historiques du noyau conservent leurs contrats. Le DXE refuse la demande suivante à l’épuisement de sa plage d’IOVA ou des 32 jetons simultanés.

**Limites de validation.** Le générateur d’arguments d’UTM prévoit un pflash personnalisé. Les essais utilisent directement ses bibliothèques installées. L’import d’une VM par l’interface graphique, HVF, le matériel physique, le multiprocesseur et le démarrage signé avec des clés de distribution restent hors de la preuve. Le logiciel du contrôleur et les accès de plateforme de confiance, notamment le transport du firmware QEMU, sont dans le périmètre de confiance. Le profil DXE couvre exclusivement le disque identifié. Le retrait observé avec ATS désactivé porte sur les traductions du modèle QEMU. Un périphérique à cache de traduction propre ou un branchement à chaud demanderait un autre banc. Une panne en cours de retrait retient ses pages jusqu’à l’arrêt de la VM. [Générateur UTM 5.0.8](https://github.com/utmapp/UTM/blob/v5.0.8/Configuration/UTMQemuConfiguration%2BArguments.swift#L548-L605)

**Prochaine décision.** Le prototype permet de proposer une qualification de distribution avec profil UTM importable, politique de signature et cycle de mise à jour du pflash. Son adoption et la reprise des phases de l’installateur nécessitent un arbitrage distinct. NovaOS reste en v0.14.0 et NovaFS en v0.9.13 P9.

## Import graphique et support d’installation

La mission d’installation a repris sur `main` à `bb0678d03a72682d3070611ca75c16de9ba5c2d2`, avec un arbre propre. Les essais utilisent le firmware normal et le noyau du prototype ci-dessus. Chaque paquet UTM possède son propre code pflash en lecture seule, ses variables et son état TPM. Le support FAT de qualification contient le témoin EFI, Limine et le noyau en mode `dma-qual`. Il est présenté en lecture seule. La seconde VM ajoute une cible jetable de 64 Mio, entièrement nulle, accessible en écriture. Les deux profils ont été ouverts et lancés dans la bibliothèque graphique d’UTM sur Apple Silicon. Le journal de lancement confirme TCG, un Cortex-A72, 256 Mio, GICv3 sans ITS, un TPM 2.0 CRB, le disque moderne avec `iommu_platform=on,ats=off,vectors=0` et l’IOMMU avec `boot-bypass=off`.

| Point d’entrée et TPM | Disques présentés | Dernier jalon | E/S disque |
|---|---|---|---|
| Interface UTM, CRB | Support seul | Console UEFI, exception synchrone à `0x252C8` | Aucun événement VirtIO bloc observé |
| Interface UTM, CRB | Support et cible vierge | Même exception, avant DXE | Aucun événement VirtIO bloc observé. Empreinte du conteneur cible identique avant/après |
| Bibliothèques UTM, CRB | Support seul | Même exception | Source 0 lecture, 0 écriture |
| Bibliothèques UTM, CRB | Support et cible vierge | Même exception | Source et cible 0 lecture, 0 écriture chacune |
| Bibliothèques UTM, TIS | Support seul | NovaOS v0.14.0, lecture du motif de 512 octets au LBA 16, une INTx reçue et acquittée | Source 147 lectures terminées, 0 écriture |
| Bibliothèques UTM, TIS | Support et cible vierge | Shell interne EDK II, aucune unité montée | Source et cible 0 lecture, 0 écriture chacune. Aucun ATTACH ni MAP dans la trace |

Les compteurs du banc direct proviennent de `query-blockstats` pour chaque backend. Le changement TIS/CRB est la seule différence de périphérique dans chaque paire à nombre de disques constant. Chaque essai démarre à froid. Les deux campagnes de comparaison, puis la campagne chronométrée, ont donné les mêmes issues. Le SHA-256 du contenu brut de la cible avant et après les essais directs vaut `3b6a07d0d404fab4e23b6d34bc6696a6a312dd92821332385e5af7c01c421351`. Dans l’interface, l’empreinte du conteneur qcow2 cible vaut `1b7af784841125887e688254229e282829e17245d0e47fe7280da9b68a30d9e1` aux deux contrôles. Après l’arrêt de la VM, `qemu-img compare` confirme également l’égalité des 64 Mio de contenu logique avec une image entièrement nulle. [Résultats QMP](validation/import-utm/resultats.json) · [Consoles, exceptions, traces et commandes graphiques](validation/import-utm/journaux.tar.gz)

**Arrêt dans le TPM.** Sur la machine `virt`, UTM construit `-device tpm-crb-device`. Le banc du prototype avait construit `tpm-tis-device`. Le journal d’exception ARM du contrôle CRB donne un Data Abort EL1, `ESR=0x96000050`, `ELR=0x252c8` et `FAR=0xc000068`. Dans le pflash de SHA-256 `0e4fc20b8585e8c6e446d9a411e174e36be0b5bb99e5c0a4146953d3edb46a77`, cette instruction est `str x1,[x0]` dans `Tcg2Pei/MmioWrite64Internal`. Elle correspond à l’écriture de l’adresse du tampon de réponse du CRB au déplacement `0x68`. Le DXE NovaIommu n’a pas encore pris la main. Une correction doit qualifier la largeur et la sémantique d’accès de ce registre sur le périphérique réellement fourni par UTM. [Générateur UTM 5.0.8](https://github.com/utmapp/UTM/blob/v5.0.8/Configuration/UTMQemuConfiguration%2BArguments.swift#L1271-L1290) · [Commande CRB d’EDK II](https://github.com/tianocore/edk2/blob/edk2-stable202502/SecurityPkg/Library/Tpm2DeviceLibDTpm/Tpm2Ptp.c#L268-L277)

**Autorité DMA du support et de la cible.** Sous TIS, le témoin seul reçoit le domaine firmware 2 pour le SID 48, avec BME coupé jusqu’au premier MAP acquitté. Sa sortie UEFI annonce 463 droits, 463 retraits, zéro droit actif et BME coupé. Le domaine noyau 1 termine ensuite sa lecture. Avec les deux disques présents, `Identify` compte deux fonctions PCI modernes `1af4:1042` et renvoie `EFI_ACCESS_DENIED`. Le support amorçable lui-même reste inaccessible. Le firmware lie les jetons DMA à un seul handle et à une seule file de disque. Le noyau possède un bail exclusif global, une seule source d’interruption et recherche le SID `6 << 3`. Enfin, le client P9 traduit son bloc zéro au début du disque entier. Le sélectionner tel quel pour un volume partitionné exposerait la GPT. [Politique DXE](../firmware/NovaIommuDxe/Domain.c) · [Transport bloc](../kernel/src/hardware/block/mod.rs) · [Bail](../kernel/src/hardware/block/lease.rs) · [Vue bloc P9](../user/novafs/device.rs)

**Arbitrage proposé.** Conserver le TPM CRB fourni par UTM et qualifier son accès MMIO dans un nouveau pflash privé. Pour le DMA, accorder au firmware uniquement le support désigné par le profil de démarrage de confiance. Recouper ce chemin avec la topologie PCI et l’IOMMU avant le premier DMA. Un chemin absent ou ambigu impose le refus. Charger les ressources de l’installateur en RAM, révoquer le domaine firmware, puis énumérer les cibles sous NovaOS. Attacher exclusivement la cible identifiée, avec inspection en lecture seule, contrôle de tous ses octets et confirmation de l’identité et de la capacité avant la première autorisation WRITE sur le disque. Même une lecture bloc exige des pages DMA inscriptibles pour les données et le statut. Le SID et le drapeau lecture seule ne suffisent pas à authentifier le rôle d’un disque. Cette politique séquentielle demande une décision sur la source de confiance et la durée des baux. Autoriser simultanément deux domaines imposerait de qualifier en plus la séparation de leurs files, interruptions, pages et chemins de révocation.

**Installation et intégrité.** Aucun partitionnement n’a eu lieu. Les types, tailles et identifiants GPT, la vue bornée pour P9, le contrôle de disque vierge et le flux de confirmation restent à implémenter après arbitrage. Les cas disque absent, non vierge, trop petit, sélection multiple, interruption de partitionnement ou de copie, échec de synchronisation et vérification finale exigent un installateur exécutable. Le démarrage depuis la cible après retrait du support, les commandes NovaShell et la persistance de l’identité, du contenu et des dates après arrêt normal ou forcé restent à démontrer sur une installation. Les cibles Make `image`, `utm` et `installer` sont donc en attente. La construction EDK II utilise `TPM2_ENABLE=TRUE` avec la valeur par défaut `SECURE_BOOT_ENABLE=FALSE`. Les empreintes identifient les binaires du banc. Une chaîne signée avec des clés de distribution et une session NovaAuth restent à qualifier séparément. [Configuration ArmVirtQemu](https://github.com/tianocore/edk2/blob/edk2-stable202502/ArmVirtPkg/ArmVirtQemu.dsc#L24-L36)

### Mesures et reproduction de cet audit

Sur le témoin TIS monodisque, les premiers messages série arrivent à 0,031 s pour UEFI, 0,275 s pour le DXE, 5,393 s pour le lancement de Limine, 5,524 s pour la notification de sortie des services, 5,558 s pour la bannière NovaOS et 5,565 s pour la lecture. Ces instants sont relatifs au lancement du processus QEMU et relevés par sondage hôte toutes les 5 ms. Le noyau annonce 56 735 cadres libres. La durée individuelle des révocations et le pic de mémoire résident ne sont pas instrumentés dans ce profil. Le montage P9, le démarrage des services installés, la consommation par service, le temps d’installation, le débit d’écriture et le délai jusqu’à NovaShell n’ont pas de mesure sur un système installé.

Depuis la racine du dépôt, utiliser le répertoire produit par `prototype_uefi.py` dans la procédure de construction précédente. Les deux scripts créent exclusivement de nouveaux répertoires et disques. Les UUID et les horodatages FAT propres à chaque exécution peuvent différer.

```sh
NOVA_ARTEFACTS="novaOS/build/prototype-dxe-7q2kfpno"  # remplacer par la sortie de sa construction
NOVA_IMPORT="$(mktemp -d /tmp/nova-import.XXXXXXXX)"
python3 novaOS/tests/profils_utm.py "$NOVA_ARTEFACTS" "$NOVA_IMPORT/profils"
python3 novaOS/tests/controle_tpm_utm.py "$NOVA_ARTEFACTS" "$NOVA_IMPORT/mesures"
```

Dans UTM 5.0.8, ouvrir successivement `NovaOS-audit-un-disque.utm` et `NovaOS-audit-deux-disques.utm` via Fichier → Ouvrir. Démarrer chaque VM et observer sa console série. La commande effective figure dans `Data/debug.log` du paquet. Le script de comparaison utilise le QEMU et le swtpm installés avec UTM, ainsi qu’un socket Unix privé.

### Validation du 12 novembre après l’import

| Suite rejouée | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages historiques | 25/25 | 25/25 |
| Groupes bloc, isolation, révocation et persistance | 24/24 | 24/24 |
| ELF, cycle processus et courses IPC | 3/3 | 3/3 |
| CLI puis CLI P9, secteurs 512 et 4 096 octets | 2/2 + 2/2 | 2/2 + 2/2 |
| NovaFS standard puis instrumenté | 192 + 193 | 192 + 193 |
| Opérations différentielles NovaFS | 1 000 000 | 1 000 000 |

Sur macOS ARM64, les binaires hôtes NovaFS sont respectivement Mach-O arm64 et x86_64. Rosetta rapporte `sysctl.proc_translated=1`. Quatre tests sont ignorés par défaut dans chacune des deux passes Cargo. Le banc relance séparément le différentiel et deux contrôles facultatifs. Les campagnes mémoire supplémentaires à 128 et 256 Mio, stockage et IPC passent sur les deux architectures. À 64 Mio, la suite historique classe explicitement l’ABI « non exécutée » avant l’entrée du noyau, avec une exigence firmware de 128 Mio sur ARM et une pénurie de mémoire du chargeur sur x86. Cette limite est distincte des 25 scénarios historiques.

Le banc AArch64 du prototype a également rejoué les onze cas sur le moteur UTM avec TIS, dont le timer, les INTx répétées, le changement de slot, le second appel à `ExitBootServices`, la coupure en vol et la reprise. Au démarrage à froid, QEMU compte 464 MAP et 464 UNMAP firmware, pour 463 jetons, la file VirtIO occupant deux plages avec des droits distincts. Le scénario de réemploi retrouve 97 pages firmware réallouées. Les traces du défaut de retrait conservent cinq plages et la console confirme la quarantaine. Le démarrage à froid et la reprise normale finissent à zéro plage active. [Journaux et bilans des suites](validation/import-utm/journaux.tar.gz)

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test
    make -C novaOS ARCH="$arch" ram-test storage-test ipc-test
    python3 novaFS/lab/p51-validation-suites.py "$arch"
done
```

## Qualification TPM et DMA séquentielle

Le point de départ est le HEAD propre `a1a528124120ba31ea8fa4fc300953b0ff0bdfcc` de `main`. Les essais utilisent le QEMU 10.0.12 d’UTM 5.0.8 sur Apple Silicon, un vCPU Cortex-A72 sous TCG, 256 Mio et `virt-10.0` avec GICv3 sans ITS. Le disque d’amorçage contient l’ESP FAT de qualification, le témoin EFI, Limine et le noyau. Il reste en lecture seule. La cible est un second fichier jetable. Aucun CD n’est nécessaire pour ces démarrages.

### Défaut CRB et correction du pflash privé

L’exception enregistrée avec le pflash précédent provient de `Tcg2Pei`. À `ELR=0x252c8`, `str x1,[x0]` tente d’écrire huit octets à `FAR=0x0c000068`, l’adresse de réponse `TPM_CRB_CTRL_RSP_ADDR` de la localité 0. L’adresse est alignée sur huit octets. Le syndrome `ESR=0x96000050` indique une écriture avec abort externe synchrone à EL1. Cette instruction précède le démarrage du DXE et l’accès au support. Le périphérique sysbus d’UTM utilise le modèle commun CRB dont les écritures MMIO valides sont limitées à quatre octets. Le modèle initialise lui-même les adresses et tailles des tampons au reset. Ces deux propriétés expliquent à la fois l’arrêt et le choix du correctif. [Exception et témoin antérieurs](validation/import-utm/journaux.tar.gz) · [Transport EDK II d’origine](https://github.com/tianocore/edk2/blob/edk2-stable202502/SecurityPkg/Library/Tpm2DeviceLibDTpm/Tpm2Ptp.c#L265-L300) · [Largeur MMIO dans le moteur livré](https://github.com/utmapp/qemu/blob/v10.0.12-utm/hw/tpm/tpm_crb_common.c#L117-L124) · [Descripteurs au reset](https://github.com/utmapp/qemu/blob/v10.0.12-utm/hw/tpm/tpm_crb_common.c#L185-L200)

La spécification TCG distingue la FIFO TIS et l’interface CRB. En TIS, le pilote échange les octets par la FIFO et utilise l’état, le compteur de rafale et la commande de départ. En CRB, il remplit un tampon de commande, déclenche START, attend sa complétion et lit la réponse. Le profil PC Client à tampon intégré décrit des registres d’adresse et de taille en lecture seule. La disposition du tampon dépend de l’interface et de la plateforme. [TCG PTP 1.07, §6.5.2 et §6.5.3](https://trustedcomputinggroup.org/wp-content/uploads/PC-Client-Specific-Platform-TPM-Profile-for-TPM-2p0-v1p07_Pub.pdf#page=110)

Avant la première instruction invitée, QMP lit six mots de 32 bits à partir de `0x0c000058`. Les descripteurs de commande et de réponse indiquent tous deux une capacité de `0xf80` octets, une adresse basse `0x0c000080` et une adresse haute nulle. Dans la copie privée d’EDK II, le transport CRB lit ces valeurs, exige ce tampon intégré et vérifie les bornes de commande, d’en-tête et de réponse. Il conserve la procédure de localité, les délais, START, l’annulation et le retour à l’état idle. Une disposition inattendue produit une erreur firmware. Les registres descriptifs ne reçoivent aucune écriture dans les traces corrigées. [Préparation de la copie privée](../firmware/preparer.py) · [Relevé QMP et tests du transport](../tests/dma_sequentiel.py)

Avant le premier domaine disque, le DXE exige le protocole EFI TCG2, un TPM présent et une réponse réelle à `TPM2_GetCapability(TPM_PT_MANUFACTURER)`. Il vérifie le tag, la taille, le code retour, le sélecteur, le nombre, la propriété et la valeur. CRB et TIS ont tous deux retourné `0x49424d00` via swtpm. Le témoin sans périphérique TPM s’arrête en quarantaine avant tout ATTACH ou accès disque. Ce contrôle établit le fonctionnement du transport TPM2 dans cette VM. L’attestation d’une plateforme et la validation de signatures de distribution demandent des essais distincts. [Contrôle EFI TCG2](../firmware/NovaIommuDxe/Profil.c)

### Identité et passage de responsabilité

1. **Profil.** Deux entrées SMBIOS de type 41 lient les désignations `NovaOS-Install-v1` et `NovaOS-Target-v1` aux objets QEMU `support` et `destination` par `pcidev`. QEMU inscrit le BDF réel de chaque objet. Le DXE exige l’unicité des rôles, vérifie le segment, le bus, la fonction, les identifiants du contrôleur et des disques, la traduction RID/SID du DTB et le bit VirtIO RO du support. Un support absent, inscriptible, ambigu ou mal désigné ferme l’accès. Une cible déclarée doit exister. Le profil et l’association de ses objets à leurs fichiers sont de confiance. Une substitution du fichier source par l’administrateur de l’hôte échappe à cette vérification. [Générateur du profil](../tests/profils_utm.py) · [SMBIOS QEMU](https://github.com/utmapp/qemu/blob/v10.0.12-utm/hw/smbios/smbios.c) · [Admission DXE](../firmware/NovaIommuDxe/Domain.c)
2. **UEFI.** Le fournisseur EDK II IOMMU est une dépendance du pont PCI. Les deux disques démarrent avec BME à zéro. Seul le support obtient le domaine 2 après PROBE sans réserve et ATTACH acquitté. Ses premiers MAP de file précèdent son BME et sa première lecture. La cible conserve BME à zéro et aucune traduction pendant la phase firmware. Le contrôleur impose `boot-bypass=off`, les disques exigent ACCESS_PLATFORM et le profil désactive ATS. [Mappages et notification de sortie](../firmware/NovaIommuDxe/Mapping.c)
3. **Avant ExitBootServices.** Le DXE ferme les admissions, contrôle la dernière complétion et l’absence de transaction pendante, réinitialise le disque, relit BME, puis retire les plages de file. Chaque UNMAP et le DETACH exigent la progression de l’anneau utilisé, le descripteur et la longueur attendus, un statut nul et une file de fautes vide. Après le reset du contrôleur, le DXE vérifie encore le refus par défaut. Sur le moteur qualifié, le traitement UNMAP invalide les traductions et efface la plage avant la réponse. Un acquittement incertain immobilise les pages et empêche la remise de contrôle. [Retrait dans QEMU](https://github.com/utmapp/qemu/blob/v10.0.12-utm/hw/virtio/virtio-iommu.c) · [Séquence firmware](../firmware/NovaIommuDxe/Domain.c)
4. **NovaOS.** Le noyau relit la table du profil par le port MMIO de fw_cfg, sans DMA, et contrôle que le support a rendu sa maîtrise de bus. Il vérifie le BDF et le type de la cible. UEFI peut avoir remis `PCI_COMMAND` à zéro sur la fonction refusée. Une lecture de BAR donnait alors `0xff`. Le noyau active uniquement le décodage mémoire CPU, relit MSE=1 et BME=0 et vérifie le statut VirtIO de reset. Il crée son domaine 1 et six traductions privées pour le minimum requis par IN et GET_ID. GET_ID confirme les vingt octets de la série `NOVA-CIBLE-000000001` avant la première lecture de secteur. La ligne PCI INTA exclusive est dirigée vers l’INTID 38 au slot 7 et 37 au slot 6. [Découverte PCI](../kernel/src/hardware/block/pci.rs) · [Domaine noyau](../kernel/src/hardware/block/viommu.rs) · [HAL GICv3](../kernel/src/arch/aarch64/gicv3.rs)
5. **Contrôle et consentement.** Le noyau exige une géométrie cohérente, une cible accessible en écriture et un minimum expérimental de 64 Mio. Il préremplit le tampon de réception avec un motif non nul, contrôle l’interruption, la longueur utilisée et le statut de chaque lecture puis examine tous les octets. Le premier octet non nul provoque le refus, quelle que soit sa signification. Le terminal affiche série, SID, capacité et taille de secteur. Après purge des frappes anticipées, il exige la saisie exacte de `AUTORISER <série> <capacité en octets>`. Sur confirmation, le noyau revérifie le profil, la géométrie, la série et toute la surface. Il crée alors la septième traduction qui permet au disque de lire le tampon destiné aux commandes OUT. Le banc referme immédiatement le domaine. Aucun bail bloc n’est publié à EL0 et aucune commande OUT ou FLUSH n’est émise par l’inspection. [Inspection et confirmation](../kernel/src/hardware/block/qualification.rs) · [Droits du transport](../kernel/src/hardware/block/mod.rs)

### Matrice de démarrage et preuves de refus

| Point d’entrée ou cas | Observation |
|---|---|
| UTM graphique, CRB intégré, support seul | Le témoin EFI lance Limine, NovaOS v0.14.0 affiche `CIBLE absente` puis `Err(Absent)`. |
| UTM graphique, CRB intégré, support et cible | La console affiche le retrait de 319 droits sur 319, zéro actif et BME=0. Elle présente la série, le SID 56, les 67 108 864 octets vérifiés et des secteurs de 512 octets. La saisie `NON` produit `confirmation refusee` et `Ok(())`. |
| UTM graphique, TIS avec TPM logiciel privé | Le générateur intégré sélectionne CRB pour `virt-10.0`. Deux essais avec `tpm-tis-device` et swtpm externe ont échoué avant l’entrée du firmware. Le moteur rapporte `Operation not permitted` sur le socket privé, y compris à l’intérieur du paquet ouvert par UTM. |
| QEMU direct, CRB et TIS, cible vierge | Pour chaque interface, 16 384 lectures de 4 096 octets au premier prompt. Après confirmation correcte, 16 384 nouvelles lectures, un seul MAP du tampon OUT puis son retrait. Zéro écriture. |
| Refus explicite ou confirmation avec capacité erronée | Une seule passe, aucun MAP du tampon OUT. |
| Objets PCI déplacés, secteurs de 4 096 octets | Le support passe au SID 56 et la cible au SID 48. Le noyau reçoit l’INTID 37 et termine le contrôle complet. |
| Rôle absent, dupliqué, inversé ou support inscriptible | Aucun domaine, aucune opération bloc sur les deux disques. |
| TPM absent | Arrêt explicite sur GetCapability, deux BME à zéro, aucun domaine. |
| Série cible incorrecte ou capacité de 4 Mio | GET_ID reçoit une complétion. Aucune lecture de secteur, aucun prompt et aucune autorisation d’écriture. |
| Signatures MBR, GPT, FAT et NovaFS | Refus respectivement aux octets 510, 512, 82 et 4 096 des images préparées. |
| Donnée au dernier octet | Le noyau lit les 64 Mio et refuse l’octet 67 108 863. |
| Erreur de lecture injectée au secteur 16 | Deux pages lues, puis erreur d’E/S et fermeture du domaine sans confirmation. |
| Réemploi après retrait | Les 107 cadres physiques distincts mappés par le DXE dans cet essai sont tous observés parmi les cadres réalloués par NovaOS. |
| UNMAP suspendu à la transition | Deux plages de l’anneau restent actives dans le domaine firmware, les deux BME disque sont à zéro et le noyau ne démarre pas. |
| Coupure au même point, puis reprise | La VM est arrêtée avant libération. Le même disque, les mêmes variables et l’état persistant du swtpm servent à la reprise avec le pflash normal. Le nouveau démarrage inspecte la cible et attend un nouveau consentement. |
| Redémarrage après confirmation | Un reset système dans le même QEMU et le même swtpm réinitialise le droit. Le second démarrage relit 64 Mio, redemande confirmation et prend en compte le refus. |

Les deux profils CRB graphiques ont été amorcés avec l’ELF final. Les journaux de lancement confirment le périphérique TPM, les rôles SMBIOS, les pflash privés et les options d’isolation. Leurs traces de débogage s’arrêtent pendant les premières lectures UEFI. Les résultats de fin de démarrage proviennent de la console série visible dans UTM. Une comparaison hors ligne de l’intégralité des 64 Mio de la cible graphique donne une image identique au témoin nul. Les compteurs QMP par phase et la chronologie complète des deux domaines proviennent du banc direct. [Sélection native de l’interface TPM](https://github.com/utmapp/UTM/blob/v5.0.8/Configuration/UTMQemuConfiguration%2BArguments.swift#L1271-L1290)

Sur le cas CRB à deux disques, la série compte 319 jetons firmware. Les deux parties de l’anneau expliquent les 320 MAP et 320 UNMAP observés. Le DETACH du support précède l’ATTACH de la cible. Les lectures de chaque phase sont attribuées à un seul objet bloc. Avant la première saisie, QMP compte 67 108 864 octets lus et zéro écrit sur la cible. La trace contient les six pages du domaine 1 et ne comporte aucun MAP de l’IOVA OUT `0x08006000`. Après la saisie correcte, QMP compte 134 217 728 octets lus, zéro opération d’écriture et zéro octet écrit. Les 32 770 fins d’interruption INTID 38 correspondent aux deux GET_ID et aux 32 768 lectures. Le SHA-256 du contenu brut de la cible reste `3b6a07d0d404fab4e23b6d34bc6696a6a312dd92821332385e5af7c01c421351`. Les compteurs de synchronisation du backend peuvent avancer lors d’un reset. Les compteurs de données, le chemin des commandes et les empreintes sont vérifiés séparément. [Compteurs par phase et empreintes](validation/dma-sequentielle/resultats.json) · [Journaux de qualification](validation/dma-sequentielle/journaux.tar.gz)

### Régressions et limites

| Suite rejouée | AArch64 | x86_64 |
|---|---:|---:|
| Démarrages historiques | 25/25 | 25/25 |
| Groupes bloc, secteurs de 512 et 4 096 octets | 24/24 | 24/24 |
| ELF, processus et IPC | 3/3 | 3/3 |
| CLI et CLI P9 | 4/4 | 4/4 |
| NovaFS standard et instrumenté | 192 et 193 | 192 et 193 |
| Opérations différentielles NovaFS | 1 000 000 | 1 000 000 |
| Prototype historique DMA/GICv3 avec TPM TIS | 11/11 | GICv3 spécifique à ARM |

Les exécutables de test NovaFS ont été contrôlés par leur en-tête Mach-O. ARM64 s’exécute en natif, x86_64 sous Rosetta (`sysctl.proc_translated=1`). Les quatre cas ignorés par défaut dans chacune des passes Cargo sont comptabilisés dans les bilans. Le pilote de validation exécute aussi le différentiel et les contrôles facultatifs prévus. Par architecture, le différentiel a observé 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités. [Banc NovaFS](../../novaFS/lab/p51-validation-suites.py) · [Résultats et journaux](validation/dma-sequentielle/journaux.tar.gz)

Cette qualification couvre un profil statique, un vCPU, les deux tailles de secteur testées et une chaîne firmware/chargeur de confiance. Le contrôle d’identité repose sur le profil et le périphérique émulé. La persistance de l’exclusivité du fichier cible côté hôte fait partie des hypothèses du test. Le seuil de 64 Mio sert au banc et ne dimensionne aucune future partition. HVF, le matériel physique, le hotplug, les caches de traduction ATS et une distribution signée avec clés de production restent à qualifier. L’ABI et NovaFS P9 conservent leurs versions. Le prochain arbitrage peut autoriser un prototype de partitionnement sur une cible explicitement confirmée, avec vue de partition bornée et essais de coupure propres à l’installateur. Le démarrage autonome depuis une installation P9 reste à démontrer.

### Reproduire le jalon

Réutiliser l’environnement de construction EDK II décrit plus haut, sur une copie privée de la révision indiquée. Les quatre constructions emploient `TPM2_ENABLE=TRUE`. Pour les onze scénarios historiques, utiliser la politique monodisque avec `--crb`, puis `--crb --retrait-suspendu` pour son injection. Pour le banc séquentiel et les deux paquets UTM, exécuter dans Bash les commandes suivantes, avec `NOVA_SRC` et `NOVA_LAB` définis comme dans la procédure précédente.

```sh
set -e
cd "$NOVA_LAB/edk2"
for variante in sequentiel retrait-transfert; do
    options=(--sequentiel)
    if [ "$variante" = retrait-transfert ]; then options+=(--retrait-transfert); fi
    python3 "$NOVA_SRC/novaOS/firmware/preparer.py" "$PWD" "${options[@]}"
    build -a AARCH64 -t CLANGDWARF -b RELEASE -p ArmVirtPkg/ArmVirtQemu.dsc -n 6 -D TPM2_ENABLE=TRUE
    mkdir "$NOVA_LAB/$variante"
    cp Build/ArmVirtQemu-AARCH64/RELEASE_CLANGDWARF/FV/QEMU_{EFI,VARS}.fd "$NOVA_LAB/$variante/"
done

cd "$NOVA_SRC/novaOS"
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked \
  --target aarch64-unknown-none-softfloat --features dma-sequentiel --target-dir "$NOVA_LAB/kernel-sequentiel"
NOVA_ELF="$NOVA_LAB/kernel-sequentiel/aarch64-unknown-none-softfloat/release/novaos"
python3 tests/dma_sequentiel.py "$NOVA_LAB/sequentiel" "$NOVA_ELF" "$NOVA_LAB/mesures-sequentielles" \
  --fv-suspendu "$NOVA_LAB/retrait-transfert"

mkdir "$NOVA_LAB/paquet"
cp "$NOVA_ELF" "$NOVA_LAB/paquet/novaos.elf"
cp "$NOVA_LAB/sequentiel/QEMU_EFI.fd" "$NOVA_LAB/paquet/prototype.fd"
cp "$NOVA_LAB/sequentiel/QEMU_VARS.fd" "$NOVA_LAB/paquet/modele-vars.fd"
python3 tests/profils_utm.py "$NOVA_LAB/paquet" "$NOVA_LAB/UTM" --sequentiel
```

Dans UTM, ouvrir chaque paquet CRB et démarrer sa VM. Chaque VM a ses propres variables EFI et son propre état TPM. La banque `Data/code.fd` vaut 64 Mio après remplissage. Les empreintes identifient la construction testée. Le compilateur EDK II inscrit sa date de construction et les systèmes FAT de test portent des métadonnées d’exécution.

| Artefact testé | SHA-256 |
|---|---|
| FV séquentiel privé, 3 Mio | `7d620d2ce2fb3d82eae47ef8aa953bc64096cdf2e4a70e785582088109a4a604` |
| Banque pflash séquentielle, 64 Mio | `3660b1cad23b75102a923475c1ee457ffc0fd1ce085ae25813257f09d918263d` |
| ELF de qualification AArch64 | `976ab6a85c492cc9089396577607d47e41f56773399dfc2c6e03df672699e6a6` |
| FV privé avec injection à la sortie | `52e3213b3847273189a344464126f24dbe3403c6eec84032daeb246f4729e61d` |
| Code EDK II fourni par UTM 5.0.8, mesuré après la campagne | `ee769c4bf42a5350d33345fc1b16d00156fe0c25fb3c68debd003bd844e4e3fa` |

Les adaptations s’appliquent à la copie locale d’EDK II et conservent les notices amont. Les sources QEMU, Linux et Intel VT-d servent de références documentaires. La livraison Git comprend les modifications, les mesures et les journaux. [Licence EDK II](../licenses/edk2.txt) · [Source UTM de QEMU 10.0.12](https://github.com/utmapp/UTM/blob/v5.0.8/patches/sources#L24)

Pour vérifier les preuves archivées depuis la racine du dépôt, extraire l’archive dans un nouveau répertoire. Les traces séquentielles conservent les événements IOMMU, bloc, EOI et registres de contrôle CRB. Le contenu des tampons TPM est exclu et les pointeurs d’objets bloc hôtes sont remplacés par des identifiants distincts par VM. Les journaux graphiques contiennent la commande effective et les événements sélectionnés. Les bilans NovaFS, les jalons NovaOS et les traces du prototype historique figurent dans `regressions/`.

```sh
NOVA_PREUVES="$(mktemp -d /tmp/nova-relecture.XXXXXXXX)"
tar -xzf novaOS/docs/validation/dma-sequentielle/journaux.tar.gz -C "$NOVA_PREUVES"
PYTHONPATH=novaOS/tests python3 - "$NOVA_PREUVES/sequentiel" <<'PY'
import json
import sys
from pathlib import Path
from dma_sequentiel import CAS, Cas, PROMPT, traces
dossier = Path(sys.argv[1])
essais = json.loads(Path("novaOS/docs/validation/dma-sequentielle/resultats.json").read_text())["essais"]
for nom, cas in {**CAS, "reprise": Cas(PROMPT)}.items():
    attendu = essais["coupure"]["traces_reprise"] if nom == "reprise" else essais[nom]
    obtenu = json.loads(json.dumps(traces(dossier, nom, cas)))
    assert all(attendu[k] == valeur for k, valeur in obtenu.items()), nom
print("23 scénarios et reprise après coupure vérifiés")
PY
python3 novaOS/tests/prototype_traces.py "$NOVA_PREUVES/regressions/prototype"
```
