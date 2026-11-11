# Installation AArch64 dans UTM — audit de v0.15.0

**État de la qualification.** Sur disque VirtIO unique, aucun des deux firmwares AArch64 fournis par UTM n’attache le SID 48. L’IOMMU refuse son premier DMA avant toute lecture de secteur. Le timer physique et l’INTx du disque ont été livrés à une sonde EL1 sur GICv3 sans ITS. Le passage à l’installation reste suspendu à un arbitrage sur le pilote DXE et le transfert DMA. [Mesures et décision](#qualification-expérimentale-edk2-et-gicv3)

La phase 0 a été réalisée sur `main` au commit `a250cb25b530f001debf256f6bfdf8afbc0bef90`. Avec le moteur livré dans UTM 5.0.8, le firmware exécute NovaOS v0.14.0, puis l’admission du disque échoue avant le lancement du shell. Le passage de la phase 1 à la phase 2 reste bloqué. NovaFS conserve sa version v0.9.13 et son format P9.

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

**Arbitrage demandé.** Autoriser ou écarter le développement d’un pilote DXE virtio-IOMMU précoce et du contrat de remise à zéro vérifiable avant réemploi des pages, ainsi que l’intégration d’un profil GICv3 dans la HAL NovaOS. La recommandation est de poursuivre sur un firmware de laboratoire distinct avant de décider de sa distribution. La première lecture confinée de l’ESP, la reprise après échec de sortie EFI, la révocation des anciennes IOVA et le démarrage autonome de NovaShell sur P9 restent les critères d’acceptation. Ils sont inaccessibles avec les deux firmwares testés. L’adoption du firmware, les phases 2 à 4 et toute évolution d’ABI restent soumises à décision.
