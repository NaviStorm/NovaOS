# Contrat et résultats du prototype OVMF Q35

Le prototype de laboratoire maintient le NIC désarmé pendant l’exécution du firmware. Un témoin EFI autorisé par Secure Boot peut ensuite fermer les maîtres PCI de la fixture, quitter UEFI et activer VT-d avec une racine entièrement vide. Le CPU virtuel s’arrête à ce point. Le NIC ne reçoit aucun bail. La décision d’adopter ce firmware et le transfert du contrôleur à un superviseur NovaOS restent ouverts.

## Entrées et règle d’admission

QEMU 11.1.3 en TCG, Q35, SMM, un vCPU et 256 Mio. Le seul Ethernet est un VirtIO moderne à 0000:00:07.0, requester ID 56, ACCESS_PLATFORM demandé, ATS désactivé, sans vecteur MSI-X ni ROM réseau. Un hub QEMU privé à un seul port isole le NIC. Seul le disque synthétique du témoin est présent, derrière AHCI avec un overlay jetable. Code pflash en lecture seule, écritures de la pflash protégées par SMM et copie de variables spécifique à chaque VM.

Le firmware standard distribué avait fini avec PCI_COMMAND=7 et GSTS=0. Le test comparatif utilise deux constructions issues du même arbre EDK II stable202502, révision fbe0805b2091393406952e84724188f8c1941837, en X64 RELEASE CLANGDWARF, SECURE_BOOT_ENABLE=TRUE, SMM_REQUIRE=TRUE, FD_SIZE_4MB. Les banques de variables vierges sont identiques. À l’intérieur de chaque comparaison, les deux VM reçoivent la même banque enrôlée et le même disque. [Rapport antérieur](virtio-net-pause-dma.md) · [Configuration EDK II](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/OvmfPkg/OvmfPkgX64.dsc)

Le bus PCI du prototype réserve tout le slot 7 du bus 0, segment 0. Avant la sonde des attributs, il exige BME à zéro et exclut le bit bus master du masque effectivement écrit pendant cette sonde. Les requêtes EFI d’activation ou de remplacement d’attributs qui demandent BME sur le slot sont refusées avant tout effet. Les attributs des autres fonctions et le mécanisme de vérification des exécutables UEFI conservent leur implémentation amont. L’adresse du slot n’est valable que pour la topologie fermée du banc. Une position différente exige une autre démonstration. Le générateur vérifie la révision et le contenu des trois fichiers amont avant écriture dans une copie dédiée. [Générateur](../firmware/preparer_ovmf_reseau.py) · [Sonde des attributs PCI amont](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/MdeModulePkg/Bus/Pci/PciBusDxe/PciEnumeratorSupport.c)

## Ce que vérifie le banc

QEMU démarre avec ses vCPU suspendus et la trace PCI active depuis sa création. Le banc lit l’identité et PCI_COMMAND avant la première instruction du firmware via CF8/CFC, puis restitue le sélecteur CF8. L’ECAM n’est pas encore configuré à ce point. Au point terminal, QEMU arrête les vCPU et le banc compare CF8/CFC à l’ECAM. Les écritures de commande entre ces deux points font partie de l’oracle. Une écriture même brève demandant BME invalide la variante. Hypothèses explicites du test : modèle PCI QEMU de confiance, transport moderne, absence de passthrough, de migration, de reset à chaud et de source externe de configuration.

La sonde vérifie SecureBoot=1 et SetupMode=0, puis appelle LoadImage sur le même support pour le PE admis et le PE de contrôle. Elle décharge tout handle obtenu et ne démarre jamais le PE secondaire. Le firmware utilise EFI_ACCESS_DENIED (0x800000000000000f) pour refuser le chargement d’une image sur ce média. Les contrôles utilisent des variables UEFI de laboratoire, d’abord une empreinte dans db, puis un certificat de test distinct de la plateforme. Le test dbx présente le même PE signé à la même adresse de fichier avant et après l’inscription de son empreinte dans dbx. Le test d’intégrité change un octet couvert en conservant le certificat embarqué. Les clés privées de test ne sont pas publiées. [Politique EDK II de vérification](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/SecurityPkg/Library/DxeImageVerificationLib/DxeImageVerificationLib.c) · [Témoin EFI](../tests/ovmf_reseau/temoin.c)

Le témoin terminal exige TCG, les cinq fonctions PCI prévues, le NIC désarmé et une DMAR conforme et unique. Il rejette un DRHD inattendu, une portée ambiguë, une réservation supplémentaire et un contrôleur déjà en service. Il réserve trois pages de 4 Kio, initialise la racine legacy entièrement à zéro, prépare la file d’invalidation et le reçu. Il déconnecte le pilote UEFI du support synthétique, ferme le bus mastering des pairs et recontrôle l’inventaire avant et après ExitBootServices. Il réalise la publication cohérente des tables, attend RTPS et QIES, puis l’invalidation globale des contextes et de l’IOTLB avec drainage et reçu WAIT. Il exige IQH=IQT=48, reçu=1, absence de faute et confirmation de TES. Aucune phase ne demande BME sur le NIC. Les pages restent réservées jusqu’à la destruction de la VM. Une erreur au cours de cette séquence arrête le témoin et conserve ces ressources.

Le banc hôte relit, vCPU arrêté, GSTS, FSTS, RTADDR, IQA, IQH, IQT, le mot de reçu et les 4 096 octets de la **racine effectivement pointée par VT-d**. Il confronte leurs valeurs à la série de diagnostic invitée et aux transitions tracées par le modèle. Une racine vide interdit toute traduction DMA ordinaire par son contenu. Les permissions d’un domaine NIC peuplé, le remappage des interruptions et les échanges Ethernet demandent des essais distincts. [Témoin terminal](../tests/ovmf_reseau/vtd.c) · [Banc Q35](../tests/ovmf_reseau_q35.py) · [Registres du modèle QEMU](https://github.com/qemu/qemu/blob/v11.1.0/hw/i386/intel_iommu_internal.h)

## Résultats de la livraison

| Configuration | Observations |
|---|---|
| EDK II amont, témoin de comparaison | NIC PCI_COMMAND=0 au reset, 3 demandes de BME parmi 12 écritures de commande, commande finale 7 et GSTS=0. Le prérequis échoue. |
| Variante, même disque et mêmes variables | 10 écritures de commande, zéro demande de BME. Commande finale 0, GSTS=0 à ce stade UEFI. Même succès et même refus Secure Boot que le témoin amont. |
| Variante, certificat db et PE signé | Le témoin signé démarre et recharge son propre PE avec succès. Le second PE signé est accepté avant révocation. Le même disque et le même certificat donnent EFI_ACCESS_DENIED après ajout de son empreinte à dbx. |
| Variante, signature altérée | Le PE intact admis par certificat est exécuté. Le second PE, altéré dans une section couverte, est refusé sans handle. |
| Variante, après sortie UEFI (deux cas de refus Secure Boot) | PCI_COMMAND=0 du reset au point terminal, GSTS=0xc4000000, FSTS=0, reçu=1. Racine relue intégralement nulle. Aucun nouvel accès n’est ouvert. |
| Variante, même témoin et même magasin dbx, sans unité Intel | Le témoin signale dmar-refusee. Le NIC reste désarmé et aucune table n’est installée. |

**Mémoire observée.** La flash code garde sa taille de 3 653 632 octets. FVMAIN_COMPACT passe de 1 285 240 à 1 285 272 octets utilisés (+32). Avec le même témoin comparatif, les deux firmwares rapportent 5 776 pages Boot Services, 512 pages Runtime Services et 37 693 pages conventionnelles libres, mesurées au même point avant la sortie. Les 3 pages (12 Kio) du témoin terminal sont une réservation additionnelle pour la racine, la file et le reçu. Les RSS hôtes maximaux **échantillonnés** sur le comparatif final sont 120 352 Kio et 115 424 Kio. Ils incluent QEMU et leur écart ne mesure pas le coût du pilote. Le total mémoire, les IRQ, la restitution en panne et les performances du réseau réel restent à mesurer. Les 92 Kio du contrat constituent toujours un sous-total prévisionnel pour le futur bail.

**Anomalies conservées.** Le premier témoin attendait EFI_SECURITY_VIOLATION, tandis que la politique amont refusait le chargement avec EFI_ACCESS_DENIED. Le second écrivait les caractères barre oblique inverse et n entre la mémoire et le marqueur final. Le banc a refusé ces deux exécutions. Les statuts et journaux originaux restent archivés. La bibliothèque hôte virt-firmware 24.11 signale sur certains PE une section .data sans octets de fichier. Pour les images signées, le contrôle croisé Authenticode, LoadImage et le contrôle différentiel dbx corroborent l’identité des octets acceptés et refusés.

## Conditions d’une éventuelle adoption

La preuve couvre la machine figée ci-dessus jusqu’au point de contrôle. Le refus est obtenu par absence continue de BME tant que la traduction est inactive, puis par BME toujours nul et une racine fermée lorsque TES est acquitté. Le test terminal quitte UEFI avec une politique qui bloque **tous** les endpoints. Il ne transfère aucun domaine vivant au noyau ordinaire. Une adoption requiert un propriétaire superviseur capable de reprendre les tables et leurs pages sans intervalle de bypass, de conserver le stockage légitime, d’isoler la route d’interruption et de prouver les protections des pages NIC avant le tout premier BME. Les écritures d’un OS ultérieur ne sont plus médiées par le protocole EFI d’attributs PCI. Un retour S3, un hotplug, une migration et un reset du contrôleur exigent chacun un contrat de continuité ou un refus.

Sur un PC à firmware constructeur, NovaOS ne peut pas reconstruire l’historique de la maîtrise de bus à partir d’une simple lecture après UEFI. Exiger une protection DMA pré-boot documentée pour les véritables groupes d’isolation, un transfert vérifiable, des protections de remappage des interruptions, puis une preuve de drainage. À défaut, conserver le NIC en découverte sans DMA. Les clés, règles d’enrôlement et mises à jour Secure Boot du constructeur ou de la distribution demeurent de leur ressort. La production ne dépend pas de ce prototype.

## Reproduction du laboratoire

Utiliser un worktree dédié, les temporaires du projet et le verrou commun géré par le banc. Reprendre la recette de construction EDK II du projet pour les sous-modules et BaseTools. En X64 CLANGDWARF sur ce Mac, CLANG_BIN désigne le répertoire LLVM avec barre finale, PATH contient ld.lld, nasm et iasl. Construire d’abord la référence avec la commande suivante dans la copie EDK II après edksetup.sh. Archiver OVMF_CODE.fd et OVMF_VARS.fd sous amont, appliquer le générateur, reconstruire avec les mêmes options et archiver sous variante. Les fichiers sources de NovaOS ordinaire ne sont pas modifiés. [Préparation EDK II sur macOS](installer-v0.15.0.md)

~~~sh
build -a X64 -t CLANGDWARF -b RELEASE -p OvmfPkg/OvmfPkgX64.dsc -n 6 \
  -D SECURE_BOOT_ENABLE=TRUE -D SMM_REQUIRE=TRUE -D FD_SIZE_4MB
~~~

Les commandes suivantes s’exécutent à la racine du worktree avec edk et arch définis sur la copie EDK II et une nouvelle archive distincte. Le générateur se lance entre les deux compilations. Pour les supports, utiliser LLVM clang, lld-link, Python 3.12, virt-firmware 24.11 avec setuptools<81, pefile, qemu-img et osslsigncode 2.10 (8329a14f8b8f69bcffbc3b59b885a42e1256cd08). Les certificats synthétiques sont renouvelés à chaque préparation. Le manifeste de la campagne identifie ceux réellement exercés.

~~~sh
python3 -B novaOS/firmware/preparer_ovmf_reseau.py "$edk"
# Après archivage des deux firmwares dans "$arch/amont" et "$arch/variante".
sh novaOS/tests/ovmf_reseau/preparer.sh "$edk" "$arch/amont" "$arch/pair"
python3 -B novaOS/tests/ovmf_reseau_q35.py "$arch" "$arch/pair" "$arch/comparaison"
sh novaOS/tests/ovmf_reseau/preparer.sh "$edk" "$arch/amont" "$arch/terminal" --vtd
python -B novaOS/tests/ovmf_reseau/certificats.py "$arch/terminal" "$arch/amont/OVMF_VARS.fd" "$arch/certificats"
python3 -B novaOS/tests/ovmf_reseau_q35.py --controle-dbx "$arch" "$arch/certificats/avant-dbx" "$arch/controle"
python3 -B novaOS/tests/ovmf_reseau_q35.py --vtd "$arch" "$arch/certificats/dbx" "$arch/dbx"
python3 -B novaOS/tests/ovmf_reseau_q35.py --vtd "$arch" "$arch/certificats/signature-alteree" "$arch/alteree"
python3 -B novaOS/tests/ovmf_reseau_q35.py --sans-iommu "$arch" "$arch/certificats/dbx" "$arch/absent"
~~~

La relecture AArch64 porte sur l’archive confinement-dxe-02 du point de suspension. Elle confronte les 5 375 traductions aux intervalles, aux droits et au SID du disque, compte 680 MAP et 680 retraits (5 intervalles simultanés au pic). Le DXE déclare séparément 679 octrois et 679 retraits. Aucune équivalence un pour un entre ces deux compteurs n’est supposée. À la ligne 194, une écriture PCI demande 0x27 au NIC, suivie de 0x0. La route IOMMU du NIC était déjà active, sans attachement de son endpoint et avec boot-bypass désactivé. Les deux captures aux extrémités donnent BME=0. La protection observée sur AArch64 repose donc sur la route et l’absence de traduction. La relecture ne donne pas de mesure de trafic réseau. [Contrat AArch64 soumis à arbitrage](virtio-net-contrat-iommu-aarch64.md) · [Relecture hors ligne](../tests/virtio_net_relecture_arm.py)

## Pièces de vérification

Les six démarrages de livraison utilisent le code du commit `99f3ad50718a131c2e9f43576fa40d16e4e18215`. Le dossier de preuves contient leurs entrées hachées, les traces du reset au point d’arrêt, les deux échecs de mise au point et les mesures hôtes. Il conserve aussi la trace AArch64 originale utilisée pour la relecture. Pour le contrôle dbx, les deux supports et leurs deux PE ont des empreintes identiques. La comparaison des variables UEFI retrouve une seule variable modifiée, dbx. [Résultats et dates du verrou](validation/virtio-net/ovmf-q35-20261125/resultats.json) · [Archive des traces](validation/virtio-net/ovmf-q35-20261125/traces.tar.gz) · [Manifeste SHA-256](validation/virtio-net/ovmf-q35-20261125/SHA256SUMS)

Pendant cette reprise, le banc Q35 a exécuté 16 démarrages, mise au point incluse, pour 53,515 secondes cumulées d’exécution hôte. Les six démarrages de livraison représentent 5,296 secondes. L’attente cumulée du verrou pour les 14 acquisitions représente 0,003870 seconde. Chaque acquisition et chaque libération figurent dans le relevé. Les préparations, compilations et analyses hors banc sont comptées séparément. Le [manifeste des pièces locales](validation/virtio-net/ovmf-q35-20261125/archives-locales.sha256) se vérifie depuis `OS-worktrees/archives-virtio-net/`. Les images et journaux de construction conservés localement sont identifiables par leur empreinte.
