# Stockage bloc natif — NovaOS v0.10.0

## Transport natif de la v0.10.0

Le service NovaFS émet désormais des requêtes persistantes depuis Ring 3 sur Q35 et EL0 sur QEMU virt. Un client de diagnostic ne possède que les deux capacités IPC de son service NovaFS. Le service NovaFS possède les deux sens de la session avec le service bloc ; ce dernier est le seul client IPC du pilote. Les trois exécutables ont des espaces d’adressage distincts. Le noyau n’interprète aucun secteur NovaFS. Sans disque attribué, le démarrage historique et le service v0.9 restent disponibles.

### Interface et frontières de confiance

| Autorité | Opérations et limites |
|---|---|
| Application | INFO/FIN auprès du service NovaFS ; aucun handle de disque, de pilote ou d’IOMMU. |
| Service NovaFS | INFO, READ, WRITE, FLUSH et CLOSE par des messages de 128 octets au service bloc. Le lanceur choisit un profil de service du [même point d’entrée NovaFS](../user/storage/service.rs). Le diagnostic de persistance produit et compare ses octets dans cette image utilisateur. |
| Service bloc | Valide la version, l’opération, la taille, les champs réservés et la progression du tag. Transmet seulement au pilote publié par le lanceur. |
| Pilote utilisateur | Écrit le header VirtIO little-endian et propose les objets et le sens de la chaîne. Le port privé additif `0x10040` requiert **l’identité du processus et la génération vivante**. Aucun sélecteur, convention, taille de structure ou droit des seize appels natifs antérieurs ne change. |
| Composant DMA | Copie 128 octets depuis la page utilisateur après vérification de toute la plage. Il décode la proposition en privé ; un objet local désigne exclusivement un header, une page entrante, une page sortante ou un statut. Il construit les vrais descripteurs, vérifie la géométrie, publie l’index puis le doorbell après barrière. Pas d’IOVA, de pointeur trans-espace, d’index de file libre ni de registre programmable dans cette interface. |
| Superviseur et HAL | MANAGE : configuration, masquage, mappings, retrait, réinitialisation et redémarrage. Ce droit n’est pas distribuable. READ (bit 0) inclut INFO, WRITE (1), FLUSH (2), CLOSE (3). La propriété RO offerte par le disque retire WRITE, même si le profil le demande. Un PID réutilisant un slot n’hérite pas du bail. |

[Protocole bloc](../abi/block.rs) · [Médiation du bail](../kernel/src/hardware/block/lease.rs) · [Pilote utilisateur](../user/storage/disk.rs) · [Service bloc](../user/storage/relay.rs)

Une seule demande matérielle peut être en vol. READ publie un IN, attend la complétion et livre les morceaux successifs (au plus 96 octets). WRITE assemble le secteur entier dans un cadre privé ; aucun OUT n’est soumis tant que la dernière pièce attendue manque. Les contrôles portent sur le LBA logique, la conversion en unités VirtIO de 512 octets, les bornes et le sens. FLUSH ne prend pas de données. Un succès de WRITE indique l’acceptation, celui de FLUSH la confirmation du backend configuré. Une erreur ou une déconnexion après soumission ne justifie ni un accusé durable ni un rejeu automatique.

### Files, interruptions et retrait

Sept cadres distincts appartiennent à chaque session : descripteurs R, disponible R, utilisé RW, header R, données entrantes W, statut W, données sortantes R (permissions depuis le périphérique). QEMU lit l’index « utilisé » à l’activation : la protection W seule y a produit une faute sur les deux architectures. **Un seul** cadre doit être mixte, sans données client ni descripteurs modifiables. Les objets sont scellés dans la RAM du noyau, indépendamment de la durée de vie du message utilisateur.

Sur Q35, VT-d autorise le SID de la seule fonction de données et une seule route MSI-X remappable. PCI, table MSI-X, PBA, porte de notification et LAPIC restent superviseur ; les ports PCI sont protégés par CPL/IOPL/TSS. Le pilote ne peut choisir ni une destination de DMA ni un format de MSI. L’interruption est comptée uniquement si la demande armée et l’index utilisé concordent. Le chemin de compatibilité du modèle précédent n’est pas délégué ; aucune émission vers un vecteur étranger n’est nécessaire à cette qualification. Les neuf cadres du remappeur sont **permanents** : après retrait, aucune entrée pour le disque, aucune route autorisée, traduction et remappage toujours actifs. Pas de fenêtre de bypass globale. Le contrôle des capacités du matériel exige ici le drainage des lectures et écritures. Sur ARM le pilote prépare les mêmes commandes en EL0 ; le noyau vérifie les fonctions PCI du bus racine, la ligne INTx 37 exclusive, l’absence de MSI et de fenêtre réservée, puis utilise virtio-IOMMU sans bypass et le GICv2.

L’admission précède la soumission, et la mort du processus ferme le bail avant la collecte de son espace. Le retrait attend le retour de l’unique chaîne, effectue le reset, coupe le bus mastering, masque et purge l’interruption, retire les traductions, attend les acquittements et efface les cadres. Un reset MMIO tenté sur une lecture artificiellement suspendue avait bloqué le vCPU. La version livrée attend donc **avant** le reset ; à l’échéance elle conserve l’ancien domaine et les pages épinglées. Une reprise du backend permet au superviseur de réessayer le retrait ; sans preuve de quiescence, pas de réattribution. Sur Q35, la file d’invalidation n’est jamais réactivée avec l’index d’une ancienne incarnation. Sur erreur de session, le superviseur ferme les racines IPC via l’opération qui réveille également les dormeurs. Le retrait brut d’un endpoint avait laissé un dormeur référencer un slot déjà supprimé lors du test de support en lecture seule.

### Qualification et oracles

Hôte macOS ARM64, QEMU 11.1.3 TCG, un CPU, 256 Mio, UEFI/Limine. Quatre profils : deux architectures × secteurs physiques et logiques de 512 ou 4 096 octets. Toutes les données concernent des fichiers raw jetables d’un Mio. Les tests ne touchent aucun disque physique. Le relevé des capacités Q35 donne CAP `0x80d2008c222f0686`, ECAP `0xf00f5a`, DWD=DRD=1. Le profil ARM emploie l’ECAM haut de virt, GICv2, MSI désactivées.

| Contrôles demandés | Vérification exécutée |
|---|---|
| 1, 3 : chaîne valide, modification après validation | Motif préparé hors VM, relecture complète depuis le pilote, statut et longueur utilisés, IRQ réelle. La copie de travail du pilote peut être réutilisée ; les pages de descripteurs et les cadres DMA n’ont aucun alias utilisateur. |
| 2, 4, 5, 6 : objet hors domaine, chaîne cyclique ou indirecte, longueur et dépassement | Propositions à identifiants et champs invalides, longueur altérée, LBA maximal et proposition concurrente : rejet avant publication. L’index disponible relu dans la vraie file et le compteur de notifications de chaque session n’avance que pour la lecture valide. |
| 7, 8, 9 : notification, PCI et MSI non autorisés | Énumération des tables réellement installées du pilote et des services : seuls leurs cadres privés sont adressables ; les branches hautes x86 sont superviseur. Pas de doorbell ni de capacité PCI/MSI. Le second processus appelant le port avec le bon numéro de génération est refusé. Les fautes de privilège CPU sont aussi couvertes par la suite historique. |
| 10 : interruption inattendue | Sur la vraie route, après masquage et complétion, observation de PBA/GIC en attente. Retrait et première IRQ du nouveau propriétaire vérifiés. Autre essai : livraison tardive sur la route d’origine après fermeture du destinataire, acquittée sans réveil. |
| 11, 17 : DMA révoqué et réattribution | Lecture vers le propre tampon du banc passé en R, puis retiré ; faute IOMMU lue dans l’invité, 4 Kio du témoin intacts dans les deux cas, malgré un statut VirtIO 0. Après collecte du pilote, l’allocateur réattribue le même cadre au banc ; son nouveau motif reste intact pendant le transfert de l’incarnation suivante. |
| 12, 13, 16 : panne, redémarrage et reset pendant un transfert | Exception synchrone du pilote sur sa propre instruction de test. Le banc suspend le READ dans le backend et attend la preuve que le pilote est mort avant la complétion. L’invité atteint la quarantaine, ce qui démontre que le vCPU reste disponible. Reprise du backend, retrait acquitté puis nouvelle génération et motif exact. La stratégie refuse précisément de lancer un reset non drainé. |
| 14, 15 : erreurs de lecture, d’écriture et de FLUSH | blkdebug retourne EIO ; le noyau consigne IOERR sur le bon opcode, les quatre processus sont collectés, aucun acquittement durable n’est fourni. Pour READ les octets initiaux correspondent au motif attendu : le test échouerait si l’injection manquait sa cible. WRITE refusé laisse l’image entière intacte. Après FLUSH refusé, seuls les octets extérieurs au secteur sont exigés inchangés : la durabilité de ce secteur est indéterminée. |
| 18, 19, 20 : saturation, IPC malformé, révocation | Second bail et seconde opération en vol refusés ; trame invalide rejetée ; ancien identifiant refusé à la génération suivante, droit fermé à la mort du propriétaire. Le profil manuel découvre le support ; le profil de test ne peut écrire sur un disque annoncé RO. |

**Persistance.** Dans chacune des quatre géométries/architectures, le service NovaFS a créé le motif relatif au LBA et à la position dans le secteur, envoyé WRITE par IPC et obtenu FLUSH. Le banc a arrêté QEMU par SIGKILL, comparé l’image **entière** à un oracle hôte indépendant puis lancé une autre VM sur le même fichier, sans le réinitialiser. Le service a relu et comparé chaque octet. Ces observations concernent VirtIO, le cache writeback de QEMU et la réouverture du fichier sous macOS. Elles n’établissent aucune résistance à une coupure électrique de la machine.

### Mesures natives

**Régressions :** 25/25 scénarios QEMU historiques par architecture. Les dix groupes du banc bloc sont passés dans chacune des deux géométries, soit 20/20 par architecture, plus une campagne de mesure par profil. Les captures de référence v0.10.0 ont été calculées en remplaçant uniquement la bannière dans les cinq images v0.9.0 ayant leur empreinte attendue. L’affichage des deux cibles et la saisie AZERTY x86 ont ensuite été comparés en VM aux nouvelles empreintes. Le banc attend désormais le retour à la ligne série avant de vérifier le bilan RAM : lire dès le préfixe pouvait couper les champs « resident » et « retenu » sous charge. Le total contrôlé inclut explicitement la catégorie DMA.

| Cible / secteur | READ p50 / p99 (µs) | WRITE p50 / p99 (µs) | FLUSH p50 / max (µs) | Séq. W / R (Mio/s) | Dispersé W / R (Mio/s) |
|---|---:|---:|---:|---:|---:|
| Q35 / 512 | 131,26 / 609,86 | 36,91 / 369,52 | 114,85 / 367,51 | 0,54 / 0,57 | 0,58 / 0,59 |
| Q35 / 4096 | 147,02 / 284,15 | 53,09 / 585,26 | 392,05 / 415,76 | 0,83 / 0,83 | 0,85 / 0,84 |
| virt ARM / 512 | 163,84 / 208,00 | 53,25 / 161,01 | 360,45 / 431,01 | 0,44 / 0,46 | 0,47 / 0,50 |
| virt ARM / 4096 | 163,84 / 198,00 | 73,73 / 230,00 | 384,99 / 384,99 | 0,68 / 0,71 | 0,70 / 0,71 |

| Cible / secteur | Validation / copie IN / copie OUT : moyennes (µs) | IPC aller-retour p50 / p99 (µs) | Commutation p50 / p99 (µs) | Retrait (µs) |
|---|---:|---:|---:|---:|
| Q35 / 512 | 2,618 / 0,635 / 0,677 | 98,44 / 246,10 | 3,08 / 4,00 | 561,79 |
| Q35 / 4096 | 2,752 / 0,550 / 0,586 | 98,01 / 163,35 | 3,06 / 4,08 | 623,14 |
| virt ARM / 512 | 4,400 / 0,624 / 0,688 | 131,07 / 245,76 | 5,12 / 6,14 | 541,01 |
| virt ARM / 4096 | 4,144 / 0,656 / 0,624 | 122,88 / 196,61 | 4,10 / 6,14 | 582,00 |

Chaque marche mesure 32 secteurs, profondeur un, pour chaque sens et ordre. Le parcours dispersé visite les 32 mêmes secteurs dans une permutation déterministe. Les débits incluent fragmentation en IPC et copies entre le début de la première opération et la fin de la dernière dans le transport, hors FLUSH. Une marche de lecture vérifie tous les octets côté service NovaFS. Les latences matérielles englobent notification, traitement, vérification de l’IRQ, du registre utilisé, du statut et des fautes. READ inclut également le temps entre les appels utilisateur BEGIN et END. Il y a 64 READ, 64 WRITE et seulement deux FLUSH après écriture par profil : le maximum FLUSH décrit ces deux observations, pas une estimation de queue de distribution. La validation porte sur 130 chaînes. Le coût de copie concerne les fragments de 96 octets (dernier fragment plus court). Les quantiles sont les bornes supérieures des classes de l’histogramme ; maximum exact, compteur natif calibré, temps écoulé incluant les préemptions.

| Cible / secteur | Résident repos / publié / collecté (Kio) | RAM retenue publiée avec vidéo (Kio) | Pic résident (Kio) | Cadres alloués / retenus au premier cycle | Buddy avant / après (octets) |
|---|---:|---:|---:|---:|---:|
| Q35 / 512 | 1284 / 1824 / 1332 | 1824 | 1824 | 135 / 12 | 9280 / 9280 |
| Q35 / 4096 | 1284 / 1824 / 1332 | 1824 | 1824 | 135 / 12 | 9280 / 9280 |
| virt ARM / 512 | 1200 / 1740 / 1212 | 3616 | 1740 | 135 / 3 | 10304 / 10304 |
| virt ARM / 4096 | 1200 / 1740 / 1212 | 3616 | 1740 | 135 / 3 | 10304 / 10304 |

Chaque service a une page de code, une page de données et une page de pile utilisateur ; les piles noyau, tables et cadres IPC sont comptés séparément dans l’annexe. Le pic est calculé avec le minimum de cadres libres sur la fenêtre, outils de mesure compris. La VM possède 256 Mio ; le « résident » est la RAM attribuée par le noyau, pas le RSS de QEMU sur macOS. Au premier cycle trois cadres de tables MMIO restent en place sur chaque cible et neuf cadres supplémentaires gardent VT-d actif sur Q35. Après établissement de ce socle, les cycles chauds du banc retrouvent le même bilan Buddy/cadres ; le témoin réattribué a conservé ses 4 Kio. Aucun plafond artificiel ni extrapolation à une endurance longue.

[Valeurs détaillées et observations série par profil](validation/bloc-natif-v0.10.0.md).

### Reproduction et limites

Depuis la racine du dépôt, avec les deux cibles Rust, QEMU, son firmware EDK2, Python 3 et xorriso :

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test
    make -C novaOS ARCH="$arch" block-test
    make -C novaOS ARCH="$arch" block-perf
done

# Image raw existante, ouverte en lecture seule. Fenêtre QEMU et série sur le terminal.
make -C novaOS ARCH=x86_64 block-run IMAGE=/chemin/absolu/support.raw SECTOR=512
make -C novaOS ARCH=aarch64 block-run IMAGE=/chemin/absolu/support.raw SECTOR=4096
```

[Banc natif](../tests/bloc.py) · [Profil QEMU](../scripts/bloc_qemu.py). Les journaux sont dans `novaOS/build/bloc-<arch>-<secteur>/`. Le banc d’injection utilise QMP pour suspendre/reprendre le backend, jamais pour remplir les anneaux ou écrire les secteurs à la place de l’invité. Les scénarios d’écriture générés par le banc sont **destructifs pour le secteur de test** : ne pas les lancer sur une image à conserver. Le démarrage manuel ne lance pas ces scénarios. L’affichage est disponible sur les deux cibles et le clavier AZERTY sur x86_64.

Qualification limitée à ces deux topologies fixes, à un vCPU et une seule fonction VirtIO moderne ; firmware et modèle du périphérique de confiance. Pas de hotplug, de transfert direct de pages utilisateur, d’ATS, de file indirecte ou de support de contrôleur physique. Le bloc et le pilote sont en espace utilisateur sur les deux architectures ; l’autorité sur la configuration et les copies DMA est privilégiée. Le service exécute une session privée ; l’interface de diagnostic ne constitue pas un montage NovaFS ni une API publique de fichiers. Aucune revendication de durabilité physique ou d’absence générale de fuite n’est déduite de ces essais.

## Audit préalable du composant de confiance, base 83b5c25

Le périphérique de données et le firmware QEMU font partie de la base de confiance. Le pilote, le service bloc et le client peuvent fournir des octets arbitraires ou disparaître pendant un transfert. La mémoire et la configuration de tous les autres périphériques restent hors de leur autorité.

| Chemin | Autorité et contrôle prévus avant le premier kick |
|---|---|
| Demande utilisateur | L’identité du processus et sa capacité bornent l’opération. Copier les paramètres, valider le secteur, la taille, le sens et l’état de la session sur la copie. Un identifiant de fragment ne désigne jamais une adresse machine. |
| Chaîne et TOCTOU | Une seule chaîne directe de deux segments pour FLUSH ou de trois pour READ. Pour une future écriture, trois segments. Le composant construit la chaîne dans ses cadres privés à partir d’objets fixes. Le pilote ne fournit ni index suivant, ni IOVA, ni flags. Les chaînes cycliques et indirectes sont donc hors du langage de soumission. Header, table et anneau disponible sont lisibles seulement par le disque. Le statut, les données entrantes et l’anneau utilisé ont leurs propres cadres. |
| Publication | Une seule opération en vol. Le producteur écrit toutes les descriptions, publie l’index avec une barrière adaptée au transport puis notifie la seule file accordée. Aucun accès utilisateur au doorbell, aux adresses de files, aux vecteurs, à la structure ISR ni au chemin PCI CFG alternatif. Une lecture lit uniquement les octets d’une complétion saine. |
| Interruptions Q35 | CPL3, IOPL zéro, bitmap TSS au-delà de sa limite. Le noyau garde config PCI/ECAM, BAR MSI-X et PBA, LAPIC, IOAPIC et tables VT-d. Il contrôle aussi les destinations d’écriture DMA, y compris les adresses d’anneaux et de statut. Les interruptions de file et de changement de configuration dépendent du transport. Le format de compatibilité de QEMU 11.1.3 rend une délégation directe insuffisante. |
| Interruptions ARM | Le noyau conserve le GIC et l’IOMMU. Le virtio-blk du profil doit être sans MSI, doté d’une seule ligne INTx exclusive et dépourvu de fenêtre MSI réservée. Le pilote EL0 prépare la commande bloc. Les droits aux registres et le cycle de vie du domaine doivent conserver les garanties de la topologie vérifiée. |
| Retrait et crash | Fermer l’admission et la livraison au processus, obtenir la quiescence ou un reset acquitté, retirer la route, traiter les interruptions pendantes et supprimer les mappings. Attendre la fin de l’invalidation et du drainage. Seule une confirmation permet d’effacer et de rendre les cadres. Une ambiguïté retient les ressources en quarantaine. |
| Variante sans MSI Q35 | INTx n’identifie pas intrinsèquement sa source. Il faudrait inventorier et réserver la ligne PCI complète et protéger le routage ainsi que toutes les destinations DMA. Le profil qualifié utilise le transport moderne et un vecteur MSI-X remappable sous contrôle exclusif. |

Les vérifications préalables utilisent des lectures sur des images jetables et des compteurs de complétion, de faute et d’interruption. La persistance depuis le service utilisateur dépend de leur résultat. [Transport PCI et anneaux VirtIO 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

## Historique : qualification de VT-d sur Q35 et d’INTx sur ARM, 11 novembre 2026

**La qualification s’arrête avant l’attribution du disque à un pilote utilisateur.** Sur Q35, QEMU 11.1.3 applique le remappage aux MSI au format remappable. Son traitement du format de compatibilité laisse passer le message sans appliquer la règle de blocage exigée par Intel. Avec ce modèle, le noyau ne pourrait pas garantir l’exclusivité d’une destination APIC ou la révocation de tous les formats avec les files sous le contrôle du pilote. Sur ARM, le disque possède une ligne INTx exclusive dans la topologie testée. La délégation native exige la qualification des deux cibles.

L’audit part du commit `da2cd181e834f6b6852988da30642b577af4b101` sur la branche `main`, vérifiée propre. NovaOS v0.9.0 sous MIT, NovaFS v0.9.12 sous MPL-2.0. L’ABI native contient six appels généraux et dix appels IPC. Les capacités d’endpoint attribuées au service de stockage, le gestionnaire de cadres et les gestionnaires d’IRQ existants ne fournissent pas d’attribution de périphérique. Le superviseur v0.9.0 relance un service IPC. Il ne draine pas des DMA. [ABI](../abi/native.rs) · [IPC](../kernel/src/sched/process/ipc/mod.rs) · [Cadres](../kernel/src/memory/frames.rs) · [Superviseur](../kernel/src/sched/process/storage.rs)

### Machines et périmètre des preuves

Hôte macOS 27.2 ARM64, QEMU Homebrew 11.1.3, images raw jetables d’un Mio, secteurs logiques et physiques de 512 ou 4 096 octets. Q35 utilise `intel-iommu,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off`. ARM utilise `virt,gic-version=2,msi=off` et `virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off`. Le disque moderne `00:06.0` négocie ACCESS_PLATFORM, BLK_SIZE et FLUSH. ATS est désactivé. Q35 expose deux vecteurs MSI-X, ARM zéro. Les débits ci-dessous utilisent le cache de fichier de l’hôte.

- **Démarrages NovaOS.** Quatre démarrages UEFI/Limine avec TCG, un vCPU et 256 Mio. Pour chacune des deux géométries, Q35 démarre sur son CD SATA habituel, ARM sur le CD SCSI. QMP confirme le disque et la géométrie. Les processus Ring 3 et EL0 terminent leur démonstration. Le service envoie `INFO volumes=0 FIN confirmee`. Les quatre images restent vides.
- **Banc matériel.** QTest programme les modèles de périphériques. Sur x86, deux LAPIC sont observables sous TCG. Une ROM témoin exécute sur CPU0 une routine qui incrémente un compteur hors DMA et envoie EOI. CPU1 reste témoin de destination. Sur ARM, QTest lit et acquitte l’interface CPU réelle du modèle GICv2, sans exécuter de routine EL0 ou EL1. La RAM de test est comparée octet pour octet. Ces preuves portent sur QEMU.
- **Reproduction.** Les deux commandes `make -C novaOS ARCH=x86_64 test` et `make -C novaOS ARCH=aarch64 test` exécutent les régressions natives. Les profils, l’assembleur du témoin, les commandes QEMU, les charges et les oracles sont en [annexe exécutable](validation/vtd-intx-v0.10.0.md). Le banc virtio-IOMMU précédent a également été rejoué dans ses trois profils, soit douze instances.

### Matrice de décision

Chaque essai ci-dessous a tourné dans les deux géométries. Les limites s’appliquent au transfert de responsabilité du noyau au pilote.

| Propriété | x86_64 Q35 / Intel VT-d | AArch64 virt / virtio-IOMMU |
|---|---|---|
| Isolation DMA | **Observée pour la RAM testée.** Domaine 1, tables à quatre niveaux, six pages de file et de transfert avec droits directionnels. WRITE et READ reproduisent le motif. Un READ vers une IOVA retirée produit une faute FRCD du SID 48, motif `0x05`. La page de 4 Kio reste intacte. | **Observée pour la RAM testée.** ATTACH sans bypass, MAP explicites et PROBE vide. Le disque accède aux six pages accordées. Après retrait de la page de données, huit enregistrements de faute signalent le SID 48. La page reste intacte malgré le statut bloc 0. |
| Révocation DMA | **Observée.** Suppression de l’entrée, invalidation IOTLB, WAIT écrit en mémoire et tête de file vérifiée. Le retrait final du contexte est aussi invalidé et acquitté. | **Observée.** UNMAP et DETACH acquittés. Une invalidation demandée pour un domaine absent retourne NOENT. Un témoin de lecture confirme la protection après UNMAP. |
| Isolation des interruptions | **Échec de la garantie globale.** Le chemin MSI-X remappable valide le SID et livre au CPU0 configuré. Le format de compatibilité ne respecte pas le blocage requis. Détail ci-dessous. | **Observée pour cette topologie fixe.** Carte PCI fournie par QEMU, fonctions recensées par QMP et IAR du GIC concordent. Disque slot 6, INTA, SPI 5, INTID 37. SCSI slot 1 et IOMMU slot 5 partagent INTID 36, conservé côté noyau. Le disque n’expose ni capacité PCI MSI ni MSI-X. |
| Révocation des interruptions | **Partielle.** IRTE retirée et IEC invalidé avec WAIT. Une complétion remappable ultérieure est refusée. Un vecteur déjà accepté par le LAPIC exige encore son traitement. Le format de compatibilité empêche de qualifier toutes les requêtes. | **Observée au GIC.** Masquage d’une ligne déjà en attente, IAR = 1023. Acquittement de la source, purge du pending, réarmement sans reliquat. Une autre complétion produite pendant le masquage reste interdite à l’interface CPU. La clôture d’un abonnement EL0 reste à réaliser. |
| Protection des pages réattribuées | **Observée entre cadres du banc.** Même IOVA remappée sur un autre cadre, contenu intégral relu. L’ancien cadre garde sa sentinelle. Le test d’EIO en vol répète ce contrôle après reset. | **Observée entre cadres du banc.** Le domaine 2 reprend l’IOVA de l’ancien domaine 1. Seul le nouveau cadre reçoit les octets. L’ancien conserve ses 4 Kio. Également revérifié après EIO en vol. Aucun de ces essais ne réattribue une page à un processus natif. |
| Gestion des pannes | **Partielle.** Un descripteur d’invalidation réservé provoque IQE, bloque la tête de file et empêche le WAIT suivant. Le banc conserve le cadre jusqu’à l’arrêt de la VM. Un transfert suspendu sur EIO est réinitialisé et repris sur un nouveau cadre. | **Partielle.** Commande inconnue refusée avec UNSUPP, domaine absent avec NOENT. Le disque reprend après EIO en vol et reset. La file de fautes avec huit tampons est remplie ; son débordement au-delà de huit et la perte d’une réponse d’invalidation restent non qualifiés. |

**Cycle d’un transfert interrompu.** Le test `en_vol.py` utilise les huit descripteurs d’une file pour quatre FLUSH distincts et vérifie les quatre têtes et leurs quatre statuts. Une lecture suivante rencontre EIO sur le backend jetable. QMP observe `io-status=failed` et l’état `io-error` alors que la tête utilisée et le statut du transfert restent inchangés. Le banc réinitialise la fonction, coupe le bus mastering, retire la route et le mapping, puis change le contenu de l’ancien cadre. Après reprise du backend, une lecture sur un nouveau cadre restitue le secteur attendu. L’ancien cadre et l’image complète restent intacts dans les quatre cas. Cette saturation porte sur les descripteurs effectivement accordés. La mort d’un pilote Ring 3/EL0, la pénurie du futur pool de cadres DMA et les réponses IPC tardives du futur service bloc restent à tester.

Le périmètre DMA porte sur la fonction bloc et ses cadres. L’accès croisé entre deux pilotes natifs affectés simultanément à deux domaines, la panne d’un contrôleur pendant DETACH et la réattribution par le gestionnaire de pages de NovaOS restent à valider. Le contrôleur du CD de démarrage appartient au firmware.

### Qualification des six garanties MSI

En fonctionnement, le banc relit `ECAP=0x00f00f5a`, `GSTS=0xc7000000`, `IRTA=0x02006803`. Les bits de traduction, de file d’invalidation et de remappage sont actifs. La table a été chargée avec EIME à 1 ; CFIS vaut 0. La source exacte est configurée dans l’IRTE. Le banc conserve au disque le même handle remappable pour tous ses essais.

| Garantie demandée | Observation et verdict |
|---|---|
| Livraison autorisée | IRR du LAPIC 0 contient le vecteur 81, IRR du LAPIC 1 ne le contient pas. Après autorisation des IRQ par la ROM, le compteur du gestionnaire avance et EOI est émis. **Vérifiée.** |
| Rejet d’une interruption non autorisée | L’administrateur attribue l’entrée au périphérique témoin `00:07.0`. La complétion habituelle du disque `00:06.0` est refusée, FRCD `0x26`, SID 48, aucun des deux LAPIC ne présente le vecteur. **Vérifiée pour le format remappable.** |
| Protection de la destination APIC | La route autorisée désigne seulement CPU0. La trace confirme cette destination dans le cas positif. La protection de toute destination échoue à l’audit du format de compatibilité. |
| Protection contre l’usurpation de source | Le SID est comparé à la source exacte attendue sur le chemin remappable. Le contrôle négatif ci-dessus l’atteste. Le traitement du format de compatibilité ne consulte pas cette entrée. L’exclusivité globale par périphérique est **non qualifiée**. |
| Rejet après retrait | Retrait de l’IRTE et invalidation IEC acquittée. La complétion suivante produit FRCD `0x22`. Le compteur reste stable. Le réarmement autorise une nouvelle livraison. **Vérifiée pour le format remappable.** |
| Traitement des interruptions en attente | Une interruption déjà dans l’IRR survit au retrait de la route et est traitée une fois par l’ancien témoin. Séparément, le masquage de l’entrée MSI-X avant la complétion met son bit PBA à 1. Après retrait de la route et reset, PBA vaut 0 dans les quatre instances. Le démasquage sous route fermée laisse le compteur inchangé. |

La spécification Intel VT-d, révision 5.0, §5.1.4, prescrit le blocage des interruptions de compatibilité si EIME est actif ou si CFIS est à zéro lorsque le remappage est actif. La source du **tag v11.1.3** traite ce format avant la consultation de l’entrée et renvoie un succès avec le message d’origine. La suite du traitement transmet le résultat à l’APIC. L’inspection statique de la routine dans l’exécutable Homebrew installé retrouve ce chemin. SHA-256 du fichier source `intel_iommu.c` : `c0c72123f5ffa5d49c07e31987c3b3ef12fb5a539f0bb6887e78f82756494ce1`. SHA-256 du binaire testé : `a462b64050825e6adfc88ea14b3896205056b2f0b0090938b536ce69381739ab`. Aucun essai n’a injecté un vecteur étranger en format de compatibilité. La conclusion sur ce format provient de la spécification, des registres relus et du code de cette version. [Intel VT-d, §5.1.4](https://cdrdv2-public.intel.com/831418/vt-directed-io-spec.pdf) · [Remappage dans QEMU v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c#L4377-L4394) · [Livraison au contrôleur](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c#L4488-L4522)

Dans ce profil PCI sans PASID, le modèle Intel expose à la fonction un chemin matériel distinct pour les interruptions. Le refus d’accès à la page témoin valide donc la protection de la RAM, tandis que les interruptions dépendent du filtrage décrit ci-dessus. [Espace d’adressage du périphérique dans QEMU v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c#L1733-L1749)

Sur ARM, le firmware et le laboratoire ont des fenêtres ECAM différentes. Les cartes de routage extraites pour les deux réglages de `highmem` sont identiques. INTA du slot 6 est exclusive parmi les fonctions présentes. Le partage observé sur la ligne 36 montre pourquoi le noyau doit revalider l’inventaire avant chaque attribution et refuser un changement de topologie qui introduit une autre source sur la ligne déléguée. Le noyau garde le contrôle du GIC et des fonctions PCI voisines. [Banc et inventaire](validation/vtd-intx-v0.10.0.md)

### Contrat de capacités matérielles proposé

Chaque droit est attaché à un propriétaire et à l’incarnation de la fonction, du domaine, de la file ou de la route. Une désignation détenue par un client IPC ne crée aucun de ces droits. Les libellés décrivent les opérations demandées au noyau ; aucun numéro de syscall n’est attribué par cet audit.

| Ressource | Autorité minimale envisagée |
|---|---|
| Fonction PCI | Le superviseur possède le droit d’attribuer et de retirer la fonction. Le pilote reçoit un droit d’usage d’une seule fonction, non transmissible. Le noyau garde reset, bus mastering et cycle de vie PCI. |
| MMIO ou configuration PCI | Lecture des seuls champs accordés, écritures limitées aux champs nécessaires. Le noyau garde BAR, configuration MSI/MSI-X, adresses des files et registres privilégiés. Si des champs admis et interdits partagent une page, médiation champ par champ. |
| File VirtIO | Utiliser une file numérotée, dans sa profondeur accordée. Le pilote prépare les requêtes et traite les complétions. La notification est bornée à la file. En cas de médiation, seule une séparation matérielle ou une copie privée immuable peut empêcher une modification après validation. |
| Mapping et révocation DMA | Le pilote peut demander un mapping pour des cadres explicitement épinglés dont il a l’usage, avec un sens, une longueur et une durée. Le noyau contrôle propriété, chevauchements, permissions, fenêtres réservées et compteurs. La révocation d’un mapping ou de tout le domaine est privilégiée et attend l’acquittement matériel. |
| Réception d’IRQ | Le pilote peut recevoir et acquitter une notification logicielle liée à sa seule route et à sa génération. Le noyau contrôle le masquage, la source, la destination, l’acquittement matériel et la purge. |
| Domaine IOMMU | Création, attachement au SID, tables, invalidations, désattachement et destruction réservés au noyau. Le pilote n’obtient jamais d’adresses de tables, d’accès à l’APIC ou au GIC, ni d’autorité sur le domaine voisin. |

### Cycle de vie exigé avant délégation

1. **Initialiser.** Découvrir la topologie, vérifier les capacités et réserver les ressources de contrôle.
2. **Attribuer.** Lier les droits matériels à une seule incarnation et au processus concerné.
3. **Configurer le domaine.** Bypass fermé, ATS désactivé, cadres épinglés, permissions minimales. Attendre le résultat de l’installation.
4. **Configurer les interruptions.** Source, destination et file connues, notifications utilisateur fermées.
5. **Activer.** Autoriser bus mastering et notifications lorsque toutes les protections sont effectives.
6. **Servir.** Borner les requêtes et les cadres en vol, comptabiliser chaque transfert.
7. **Fermer l’admission.** Invalider la session et empêcher toute nouvelle soumission, y compris après mort du pilote.
8. **Arrêter le périphérique.** Obtenir un drainage ou un reset confirmé. Consigner l’issue incertaine des opérations interrompues.
9. **Retirer les interruptions.** Fermer les notifications, masquer la source, retirer la route, acquitter l’invalidation, vider les états périphérique et contrôleur et attendre la fin des gestionnaires de l’ancienne incarnation.
10. **Révoquer le DMA.** Retirer le bus mastering et les mappings. Conserver les cadres épinglés.
11. **Acquitter les invalidations.** Valider leur achèvement et les fautes en attente. En cas d’erreur ou de délai expiré, conserver domaine, route et cadres en quarantaine.
12. **Rendre les pages.** Effacer les cadres avant réattribution, uniquement quand l’ancienne source ne peut plus y accéder.
13. **Détruire le domaine.** Désattacher la fonction, invalider le contexte selon l’architecture et attendre l’acquittement avant de recycler SID logique, identifiants et structures de contrôle.

### Persistance, régressions et mesures

Sur chaque cible et pour chaque géométrie, un processus QEMU écrit un motif identifiable à l’offset 8 192 et confirme FLUSH. Le banc tue QEMU, lance une instance indépendante, relit le secteur par le périphérique, puis compare côté hôte chaque octet de l’image entière. Les quatre couples écriture/relecture réussissent. L’EIO en vol est injectée séparément. La transaction invitée Ring 3/EL0 via INFO, READ, WRITE, FLUSH, CLOSE, ses retours tardifs, ses coupures avant et pendant FLUSH et la durabilité physique restent non validées.

| Régression NovaOS v0.9.0 ou mesure du service existant | x86_64 | AArch64 |
|---|---:|---:|
| Scénarios QEMU réussis par `make test` | 25/25 | 25/25 |
| IPC INFO/FIN, 513 allers-retours, p50 / p99 | 30,621 / 44,911 µs | 45,056 / 53,248 µs |
| RAM résidente au repos / avec serveur seul / après destruction | 1 212 / 1 336 / 1 212 Kio | 1 140 / 1 264 / 1 140 Kio |
| RAM totale retenue aux mêmes étapes, y compris la RAM vidéo comptabilisée | 1 212 / 1 336 / 1 212 Kio | 3 016 / 3 140 / 3 016 Kio |
| Pic dans la fenêtre panne/reprise du service | 1 432 Kio | 1 360 Kio |
| Allocations de cadres dans cette fenêtre / blocs Buddy après collecte | 161 / 3 | 161 / 3 |

Les journaux `novaOS/build/serie-<arch>-storage-test.log` expriment la RAM en cadres de 4 Kio. Le pic correspond au résident de base plus les 55 cadres supplémentaires observés au minimum de cadres libres. Le retour à la valeur initiale porte sur cette fenêtre. Les suites historiques couvrent ABI, IPC, capacités, isolation utilisateur, interruptions, mémoire, pénurie, relance du service et restitution de ses ressources. La mémoire et les fuites éventuelles du futur pilote natif restent à mesurer.

La campagne matérielle ci-dessous utilise des transferts de 4 Kio à profondeur 1. Pour chaque profil, 128 opérations par direction et par parcours, ordre pseudo-aléatoire reproductible. Les latences incluent soumission QTest, notification, lecture de la complétion et du statut. Les débits incluent la préparation ou la comparaison de tous les octets via QTest. Le coût de cette relecture hôte domine le débit READ. Les mesures sont des temps muraux de laboratoire, sensibles à la charge hôte. [Charge et calcul des percentiles](validation/vtd-intx-v0.10.0.md)

| Architecture / secteur | WRITE séq. p50 / p99 (µs) | READ séq. p50 / p99 (µs) | Débit séquentiel W / R (Mio/s) | Débit aléatoire W / R (Mio/s) | FLUSH après écritures séq. / aléatoires (µs, 1 par charge) |
|---|---:|---:|---:|---:|---:|
| x86_64 / 512 | 147,7 / 240,6 | 170,9 / 306,5 | 17,18 / 1,79 | 19,68 / 1,96 | 468,9 / 510,9 |
| x86_64 / 4 096 | 132,8 / 176,1 | 161,0 / 254,2 | 19,56 / 1,82 | 21,19 / 1,93 | 462,2 / 257,6 |
| AArch64 / 512 | 126,2 / 176,4 | 126,4 / 161,2 | 20,43 / 1,99 | 20,44 / 1,84 | 483,4 / 515,4 |
| AArch64 / 4 096 | 123,1 / 165,9 | 125,9 / 205,1 | 20,64 / 2,00 | 20,45 / 2,02 | 475,9 / 528,6 |

| Architecture / secteur | MAP / UNMAP p50 (µs) | Retrait / installation IRQ p50 (µs) | FLUSH sans nouvelle écriture p50 / p99 (µs) |
|---|---:|---:|---:|
| x86_64 / 512 | 99,5 / 99,5 | 98,5 / 98,1 | 112,2 / 151,4 |
| x86_64 / 4 096 | 88,6 / 88,8 | 87,1 / 89,5 | 114,6 / 170,3 |
| AArch64 / 512 | 123,2 / 121,3 | 26,3 / 25,7 | 105,5 / 140,6 |
| AArch64 / 4 096 | 121,8 / 122,6 | 25,8 / 25,8 | 105,6 / 123,2 |

Trente-deux échantillons par opération de contrôle et 64 FLUSH sans nouvelle écriture. Sur Q35, MAP et UNMAP incluent l’invalidation IOTLB et son WAIT, le changement d’IRTE inclut IEC et WAIT. Sur ARM, ce sont des commandes virtio-IOMMU acquittées et des masquages ou démasquages GIC relus. La latence complète de révocation d’un pilote inclurait aussi arrêt des soumissions, drainage, traitement des interruptions déjà acceptées et récupération des cadres. Aucun chiffre natif v0.10.0 ne peut être déduit de ces mesures.

### Arbitrage

1. **Conserver le pilote et ses files en espace utilisateur.** Qualifier une version corrigée ou un autre modèle VT-d dont la conformité au blocage des deux formats peut être vérifiée. Exiger les tests de source, destination, retrait et interruptions déjà en attente avant l’attribution.
2. **Autoriser une médiation du transport sur Q35.** Conserver Intel VT-d et le pilote bloc utilisateur, mais permettre à un composant de confiance de valider et de sceller les descripteurs et de garder le contrôle de toutes les destinations d’interruption. Définir les accès CPU et DMA aux descripteurs pendant tout le transfert. Cette option élargit la base de confiance et nécessite sa propre qualification.

Le profil ARM retenu demeure virtio-IOMMU avec MSI désactivé. La reprise du stockage natif dépend de cet arbitrage et de la validation de la solution choisie.

---

## Historique de la qualification virtio-IOMMU sur les deux cibles

**Décision au commit de départ `547502d96c889e4fcd19250506171bbeb622d0da`, le 11 novembre 2026.** L’accès à une page de RAM démappée est effectivement refusé par virtio-IOMMU sur Q35 et sur ARM `virt`. Sur Q35, le contrôleur annonce aussi au disque une fenêtre MSI vers l’APIC que le noyau ne peut pas retirer avec `UNMAP`. Avec les descripteurs matériels sous le contrôle du pilote Ring 3, la plateforme testée ne fournit pas de preuve que ses accès au contrôleur d’interruptions se limitent à l’IRQ attribuée. Le §19 demande l’arrêt avant de déléguer le périphérique. Le dépôt était propre sur `main`. NovaOS reste en v0.9.0 (MIT) et NovaFS en v0.9.12 (MPL-2.0).

### Matériel observé

Hôte macOS 27.2 ARM64, QEMU Homebrew 11.1.3. Les démarrages UEFI/Limine utilisent TCG, un vCPU et 256 Mio. Le banc matériel distinct utilise QTest pour émettre les transactions PCI d’un pilote sur les vrais modèles QEMU. Aucun code invité ne s’exécute dans QTest. Les disques d’essai sont des images raw temporaires.

| Observation | Q35 x86_64 | ARM `virt`, GICv2 |
|---|---|---|
| Transport qualifié | VirtIO moderne PCI, configuration PCI et BAR | VirtIO moderne PCI, configuration ECAM et BAR |
| IOMMU et disque | `1af4:1057` en 00:05.0 et `1af4:1042` en 00:06.0 | Mêmes identités dans cette topologie |
| Fenêtres BAR4 après firmware, adresses de base observées avec `info mtree` | IOMMU `0xc000000000`, disque `0xc000004000` sur le profil CD SCSI | IOMMU `0x8000004000`, disque `0x8000008000` sur le profil d’origine |
| Interruptions | INTx disponible avec `vectors=0`. La région `apic-msi` demeure | INTx disponible. `msi=off` retire la région GICv2m |
| PROBE du disque, domaine créé sans bypass | Sous-type MSI, `0xfee00000..0xfeefffff` inclus | Sous-type MSI, `0x08020000..0x08020fff` inclus par défaut. Aucune région annoncée avec `msi=off` |

Le banc a négocié `VERSION_1`, les fonctions IOMMU INPUT_RANGE, DOMAIN_RANGE, MAP_UNMAP, PROBE, BYPASS_CONFIG et, côté disque, ACCESS_PLATFORM, BLK_SIZE et FLUSH. Il a relu la valeur de bypass à zéro, le granule minimal de 4 Kio, les IOVA de 0 à 2⁴⁸−1 et la plage de domaines de 0 à 2³²−1. ATS est désactivé. Le SID du disque vaut 48. Les quatre pages de BAR4 contiennent dans l’ordre la configuration commune, ISR, la configuration propre au périphérique et la notification. Le multiplicateur de notification se lit dans la capacité PCI. Ces adresses dépendent du placement par le firmware. Le banc sans firmware attribue lui-même les BAR et vérifie les capacités PCI. Un pilote NovaOS devrait découvrir les ressources et les valider à chaque attribution.

**Amorçage.** Sur le CD SATA du lanceur Q35 actuel, aucune bannière NovaOS en 60 s avec `boot-bypass=off` et le disque de données présent. La trace rapporte des traductions du disque avant son attachement. Le même firmware atteint NovaOS en plaçant le CD en lecture seule sur un contrôleur VirtIO SCSI distinct, SATA désactivé, tout en gardant `boot-bypass=off` et `iommu_platform=on` sur le disque de données. Le contrôleur du CD sert au firmware et reste hors de l’attribution au pilote. Sur ARM, ce CD est déjà sur VirtIO SCSI. La bannière, les processus EL0 et `INFO volumes=0 FIN confirmee` sont observés avec et sans MSI. Ce message provient du service v0.9.0. Il ne découvre aucun disque. Le lancement modifié reste une recette d’audit.

### Ce qui empêche la délégation sur Q35

`PROBE` décrit la fenêtre MSI même avec les vecteurs MSI-X du disque désactivés. L’arbre mémoire de la machine l’associe à `apic-msi`. Dans le modèle amont, Q35 impose cette région et la traduction traite son type MSI avant les mappings ordinaires du domaine. L’exception concerne le contrôleur d’interruptions. Le refus d’accès à la RAM privée reste observable. Aucune interruption étrangère n’a été injectée. [Région imposée par Q35](https://github.com/qemu/qemu/blob/master/hw/i386/pc.c) · [Traduction virtio-IOMMU](https://github.com/qemu/qemu/blob/master/hw/virtio/virtio-iommu.c) · [Définition normative de RESV_MEM](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Trois contrôles permettent de départager les configurations.

- Employer INTx et réserver l’accès CPU au BAR MSI-X au noyau laisse la propriété MSI visible par le disque. Une liste d’IOVA tenue par NovaOS ne filtre pas les descripteurs qu’un pilote peut modifier après publication.
- Sur le binaire Q35 installé, la tentative de remplacer cette région par une région interdite a été refusée dès la configuration, car la propriété `reserved-regions` était fixée deux fois. En retirant l’APIC du modèle CPU, la fenêtre `apic-msi` disparaît de l’arbre mémoire, mais le firmware x86_64 fourni n’atteint même pas la bannière en 60 s, y compris lors du contrôle avec bypass de démarrage activé. Ce contrôle ne constitue pas un profil de production.
- Sur ARM, `virt,gic-version=2,msi=off` conserve le démarrage et la file en INTx. QTest confirme la disparition de la propriété MSI et le fonctionnement des E/S et d’UNMAP dans ce profil. Il reste à valider le routage de l’IRQ vers un pilote EL0 et sa révocation par NovaOS. [Configuration MSI de la machine ARM](https://github.com/qemu/qemu/blob/master/hw/arm/virt.c)

### Essais de persistance et de révocation

Pour chacun des trois profils et des deux tailles de secteur, un premier processus QEMU traduit six pages distinctes pour le disque, écrit un motif à l’offset 8 192, confirme WRITE puis FLUSH. Le banc vérifie le statut, la longueur de complétion et le bit d’interruption de file, puis tue QEMU par SIGKILL. Un autre processus QEMU reconstruit les files, ouvre la même image et relit le motif. Après chaque arrêt, le lecteur hôte compare l’image entière de 1 Mio à un oracle indépendant des lectures du périphérique. Les octets hors du secteur doivent tous rester nuls. Ce contrôle porte sur le backend de fichier avec `cache=writeback` et propagation des erreurs. Il n’y a pas eu de coupure électrique de l’hôte. [Protocole QTest](https://www.qemu.org/docs/master/devel/testing/qtest.html)

Dans la seconde instance, le banc obtient le reset du disque, attend la réponse OK à UNMAP, écrit un témoin dans les 4 Kio de l’ancien cadre et réinitialise la file. Une lecture bloc utilisant l’ancienne IOVA de test déclenche un événement matériel `FAULT_R_MAPPING`, SID 48, première adresse `0x08004000`. Après le second reset, les 4 Kio du témoin sont intacts. Le banc retire ensuite les cinq autres mappings et vérifie DETACH. Il s’agit d’une réutilisation contrôlée de la page par le banc. La mort d’un processus NovaOS et le recyclage par son allocateur restent à qualifier.

| Profil QTest | Secteurs 512 / 4 096 | Relance après FLUSH et image entière | Faute et témoin intact |
|---|---|---|---|
| Q35 | 2 couples écriture/relance | 2/2 | 2/2 |
| ARM `virt`, MSI par défaut | 2 couples | 2/2 | 2/2 |
| ARM `virt,msi=off` | 2 couples | 2/2 | 2/2 |

**Deux résultats distincts pour la même requête négative.** Dans les six essais, le disque a rendu le statut bloc 0, tandis que l’IOMMU refusait la traduction et que le tampon conservait son témoin. Les huit tampons d’événements offerts ont été remplis et QEMU a signalé l’absence de tampon supplémentaire. Le statut de la file bloc seul serait une fausse confirmation de livraison. L’absence d’événement reçu ne prouve pas non plus le succès d’une traduction. La spécification autorise la perte d’un rapport quand la file d’événements est vide. Le futur service devra maintenir valides les mappings de toute requête admise jusqu’à sa fin, retirer les confirmations d’une génération en révocation et mettre en quarantaine une session en faute ou dont les événements sont perdus. [Rapports de fautes VirtIO](https://github.com/oasis-tcs/virtio-spec/blob/master/device-types/iommu/description.tex)

### Contrat de sécurité à conserver à la reprise

- **Autorité.** Le lanceur de confiance détient MANAGE et REVOKE sur le domaine. Il attribue au seul pilote USE_DEVICE et une autorité MAP sur des tampons dédiés. Les droits DMA_READ et DMA_WRITE décrivent le sens vu du périphérique. Le service bloc détient les droits de session sur les secteurs. Le client reçoit des droits IPC limités à son endpoint. Un identifiant numérique d’endpoint, de PID ou d’IOVA ne constitue pas une capacité matérielle. Les droits matériels du premier profil sont locaux, liés au PID, au SID et à une incarnation, sans transfert implicite par IPC. Les seize sélecteurs existants sont conservés. Toute porte matérielle ultérieure devra porter une version et vérifier le type et les droits avant les arguments mémoire. [ABI](abi.md) · [Contrat IPC](ipc-security.md)
- **Pages.** Le noyau attribue et efface des cadres dédiés. Chaque mapping porte une identité de domaine et une génération, la page épinglée, les bornes IOVA et le sens autorisé. Vérifier largeur d’adresse, alignement, additions et chevauchements avant MAP. Une soumission garde les pages épinglées et les identités stables. Le mapping passe par les états réservé, actif après MAP acquitté, en retrait, puis libéré après confirmation, ou en quarantaine. Un handle d’une ancienne incarnation reste invalide après relance du pilote. Des tampons intermédiaires alignés reçoivent les fragments IPC. Le descripteur, l’en-tête et l’anneau disponible n’ont besoin que de lecture DMA. Dans ce modèle, le disque lit aussi l’index de l’anneau utilisé à l’initialisation. Cet anneau occupe une page dédiée RW. Le statut est dans une page distincte inscriptible. [Gestion des cadres](memoire.md) · [Destruction des espaces utilisateur](processus.md)
- **Ordonnancement des accès.** Le pilote doit publier le contenu et les descripteurs avant l’index disponible, puis l’index avant la notification MMIO. Après observation d’un index utilisé, appliquer une barrière d’acquisition adaptée au DMA avant la lecture du statut et du tampon. Sur le profil ARM cohérent, prévoir un BAR de type Device, une barrière de publication vers le domaine externe partagé (par exemple `dmb oshst`) et une barrière d’acquisition après observation de la complétion (`dmb oshld`). Une séquence store puis load de contrôle de notification exige une barrière complète. Sur x86, séparer RAM cohérente et BAR non cacheable, avec les accès volatils et barrières de compilateur et de transport appropriés. Sur un transport non cohérent, la gestion explicite des caches s’ajoute à ces barrières. QTest sérialise les commandes de l’hôte. Il ne qualifie pas l’ordre des instructions d’un futur pilote CPU.
- **Retrait.** Fermer l’admission et retirer la capacité et les notifications CPU avant d’arrêter le disque. Obtenir un reset achevé ou un drainage prouvé, sans rendre de tampon au client pendant cette transition. Émettre UNMAP pour les intervalles exacts, contrôler les identités et les réponses sur la file IOMMU, puis détacher avec bypass global fermé. Dans le profil visé, ATS reste désactivé. Une invalidation TLB du CPU n’acquitte pas la révocation DMA. Conserver les cadres et la fonction en quarantaine si reset, réponse ou invalidation restent incertains. Le noyau reste propriétaire de la commande PCI de bus master. Sa désactivation seule ne prouve pas que les transferts antérieurs sont drainés. Le collecteur de processus doit pouvoir rendre la pile et les pages ordinaires sans rendre les cadres DMA retenus.
- **Persistance.** L’acceptation d’un WRITE par IPC réserve une opération. Sa complétion bloc valide achève le transfert. Seul un FLUSH ordonné et confirmé, pour une session saine et après toutes les écritures concernées, permet d’annoncer la durabilité du backend. Une faute, un timeout ou une réponse tardive laissent l’issue indéterminée. Aucune donnée de lecture partielle ne doit être publiée. Les opérations et bornes LBA du contrat ci-dessous restent applicables, ainsi que les deux barrières de NovaFS lors d’une intégration ultérieure.

### Couverture et mesures

| Périmètre | x86_64 | AArch64 |
|---|---:|---:|
| Régression NovaOS v0.9.0, démarrages QEMU validés par `make test` | 25/25 | 25/25 |
| RAM avant/après collecte et avec le service v0.9.0 résident | 1 212 / 1 212 / 1 336 Kio | 1 140 / 1 140 / 1 264 Kio |
| Pic lors du scénario de reprise du service v0.9.0, allocations de cadres | 1 432 Kio, 161 | 1 360 Kio, 162 |
| IPC INFO/FIN v0.9.0, 513 allers-retours, p50 / p99 | 30,732 / 45,073 µs | 53,248 / 106,496 µs |
| Commutations directes chronométrées v0.9.0, nombre, p50 / p99 | 1 028, 3,073 / 4,097 µs | 1 027, 6,144 / 8,192 µs |

Les régressions ont tourné simultanément sur les deux cibles. Les chiffres IPC varient avec la charge TCG. Les bilans viennent de `build/serie-<architecture>-storage-test.log` sous `novaOS/`. Aucun service bloc ni pilote utilisateur v0.10.0 n’a été lancé. Leurs latences d’E/S, débits, temps de mapping, d’invalidation et de révocation, RAM et fuites demeurent non mesurés.

| Qualification demandée | Portée réellement atteinte |
|---|---|
| Sécurité DMA §14 | Mappings positifs, reset, retrait acquitté d’une page, essai matériel sur l’ancienne IOVA et détachement d’un domaine sous QTest. Restent à vérifier dans NovaOS les capacités, les bornes et permissions, plusieurs domaines simultanés, la révocation en vol, la mort, le redémarrage, les erreurs d’invalidation, les requêtes invalides, la saturation et le recyclage de cadres entre processus. |
| Stockage §15 | Démarrage et régressions de l’invité existant. Géométrie, WRITE, FLUSH, READ après redémarrage et signalement d’interruption sur les modèles PCI sous QTest. Restent à vérifier en Ring 3 et EL0 le service bloc, ses cinq opérations via IPC, les droits, erreurs, pannes, redémarrages, fuites et la persistance de bout en bout. |

### Arbitrage du précédent audit

1. **Conserver virtio-IOMMU sur les deux cibles** et autoriser une médiation générique des descripteurs sur Q35. Le noyau scellerait la liste vérifiée des segments autorisés et les adresses des files jusqu’à la complétion. L’admission exclurait les fenêtres d’interruptions. Les pages de description seraient protégées des modifications CPU et DMA. Le pilote utilisateur conserverait la négociation bloc, les commandes, la géométrie et la gestion du disque. Cette variante étend la partie de transport à laquelle le noyau doit faire confiance.
2. **Conserver les files sous le contrôle du pilote utilisateur** et autoriser pour Q35 la qualification d’un autre contrôleur assurant aussi le remappage des interruptions par source, par exemple Intel VT-d. QEMU installé expose `intel-iommu,intremap`. Il faudrait démontrer le filtrage du SID, y compris le traitement des interruptions au format de compatibilité, et toutes les invalidations. Sur ARM, poursuivre avec virtio-IOMMU, INTx et `msi=off`. Cette variante ajoute un backend IOMMU x86. Sa sécurité reste à qualifier.

La seconde option conserve la frontière de confiance retenue pour le pilote. Dans les deux cas, la disparition d’un droit IPC et une complétion bloc positive ne suffisent pas pour rendre une page DMA au gestionnaire mémoire.

### Reproduction de la qualification

Depuis la racine du dépôt sur macOS, avec les deux QEMU Homebrew. Les deux premières commandes reconstruisent également les ISO de la v0.9.0.

```sh
make -C novaOS ARCH=x86_64 test
make -C novaOS ARCH=aarch64 test
```

Le banc d’amorçage et le banc QTest sont intégralement reproduits en [annexe de qualification](validation/dma-qemu-v0.10.0.md). Les anciens contrôles sans pilote restent décrits ci-dessous.

---

## Historique du premier audit, base v0.9.0

**Décision du 11 novembre 2026.** Arrêt avant implémentation selon le §15 de la mission. Sur la base `66f7b028446fe429da7d1903d1562569c6d16748`, le noyau peut retirer un endpoint à un processus et restituer ses pages. Il ne possède aucun domaine d’adressage de périphérique qui lui permette de retirer les accès DMA avant cette restitution. Accorder le contrôle d’une virtqueue à un pilote Ring 3 ou EL0 dans les configurations actuelles laisserait des accès mémoire hors de l’autorité accordée au processus. Le choix de la frontière de confiance requiert un arbitrage.

Le dépôt était propre sur `main` au début de l’audit. NovaOS reste en v0.9.0 sous MIT et NovaFS en v0.9.12 sous MPL-2.0. [Manifeste NovaOS](../kernel/Cargo.toml) · [Licence MIT](../LICENSE) · [Manifeste NovaFS](../../novaFS/Cargo.toml) · [Licence MPL-2.0](../../novaFS/LICENSE)

## Préconditions vérifiées dans le code

| Domaine | État de la v0.9.0 | Conséquence pour le service bloc |
|---|---|---|
| Processus | Le lanceur charge des images plates RX de 4 Kio maximum, avec DATA et pile privées RW/NX, un PID et des tables propres. Le superviseur exécute déjà deux processus, traite une mort et relance un service. | La chaîne application → NovaFS → service bloc demande une troisième identité et une seconde paire d’endpoints. Le lanceur peut provisionner un groupe. La chaîne à trois n’existe pas dans le service actuel. |
| IPC | Six appels natifs et dix sélecteurs IPC sont implémentés. Un nœud de capacité désigne un endpoint. SEND, RECEIVE, GRANT et MANAGE sont contrôlés par PID et incarnation. GIVE, révocation transitive, fermeture et réveil des attentes sont effectifs. | Ces droits permettent une session interservices. Le droit SEND seul ne distingue pas une opération READ d’une opération WRITE sur un même endpoint. |
| Mémoire | Les copies utilisateur vérifient les segments avant accès. Les pages de `UserSpace` sont des cadres RAM gérés. Sa destruction démape et restitue les cadres. | Aucune API d’épinglage pour E/S en vol, de prêt révocable de page, de traduction IOVA ou de quarantaine sur échec de reset. Des fragments IPC de 128 octets suffisent pour le transport entre processus. |
| Matériel | Le noyau gère le timer sur les deux cibles et le clavier sur Q35. Les accès aux ports x86 et les mappings ARM du GIC et de l’UART sont réservés au superviseur. | Il manque l’attribution exclusive d’une fonction PCI, d’une fenêtre de registres et d’une source d’IRQ à un pilote. Une première file pourrait être scrutée avec un délai borné. Cela résout l’attente de complétion, sans résoudre le confinement DMA. |
| Bloc et durabilité | Q35 démarre sur CD. Sur ARM, le CD dépend de VirtIO SCSI utilisé par le firmware. Le serveur utilisateur accepte INFO et FIN et annonce zéro volume. Le moteur macOS utilise un trait Device de 4 Kio et une synchronisation explicite. | NovaOS ne dispose d’aucun chemin de lecture, d’écriture ou de flush du disque de données. La capacité du firmware à charger l’ISO prend fin avec cette phase du démarrage. |

Sources auditées. [Chargement et fin de processus](../kernel/src/sched/process.rs) · [Droits, nettoyage et lanceur](../kernel/src/sched/process/ipc/mod.rs) · [Descripteurs et copie IPC](../kernel/src/sched/process/ipc/traffic.rs) · [ABI](../abi/native.rs) · [Mappings privés](../kernel/src/memory/user.rs) · [Ports x86](../kernel/src/arch/x86_64/ports.rs) · [Mappings ARM](../kernel/src/arch/aarch64/paging.rs) · [Serveur NovaFS actuel](../user/storage/service.rs) · [Configuration QEMU](../scripts/qemu.sh) · [Contrat Device](../../novaFS/src/lib.rs) · [Adaptateur fichier](../../novaFS/lab/device.rs)

## Options examinées lors du premier audit

**A — VirtIO Block PCI moderne avec domaine DMA, option recommandée.** Les deux QEMU installés acceptent le même modèle de disque et le même modèle `virtio-iommu-pci`. Cela permettrait de conserver les commandes bloc, la géométrie et les erreurs dans le pilote utilisateur. Le noyau porterait l’autorité générique sur les ressources matérielles. Le protocole VirtIO distingue la négociation de `VIRTIO_F_ACCESS_PLATFORM` de la configuration effective des traductions. Le banc a vérifié la présence des deux objets émulés avec le CPU arrêté. La création d’un domaine, sa politique par défaut, son attachement au bon identifiant PCI et ses invalidations restent à développer dans l’invité. [VirtIO 1.2, §§ 2.7, 5.2, 5.13 et 6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Ordre minimal proposé pour l’option A.

1. Le lanceur identifie la fonction dédiée sur le bus PCI et conserve les opérations sensibles de configuration, de reset et d’activation du bus master. Il remet au pilote une autorité matérielle locale, bornée à cette fonction et à son incarnation. Les clients ordinaires reçoivent seulement leurs endpoints de session. Vérifier aussi les fenêtres MMIO à la granularité de la page et la maîtrise des interruptions MSI avant toute délégation de registres.
2. Attacher un domaine initialement fermé. Y inscrire uniquement les pages du ring et les tampons dédiés, dans le sens d’accès nécessaire. Les pages restent épinglées jusqu’à complétion ou arrêt démontré du périphérique. Le pilote pourra manipuler les IOVA autorisées sans connaître les adresses physiques des autres processus. Activer le périphérique seulement après contrôle de la négociation et de la configuration du domaine.
3. À la révocation ou à la mort du pilote, fermer les admissions et invalider la génération de session. Arrêter la file, obtenir le reset ou le drainage confirmé, retirer les mappings CPU et DMA et attendre les invalidations avant tout recyclage. Sur délai expiré, conserver les pages en quarantaine, déclarer le périphérique indisponible et interdire sa réattribution tant que sa quiescence reste incertaine.
4. Qualifier ces mécanismes, y compris la mort pendant une opération et la conservation de cadres témoins extérieurs au domaine, avant la première écriture applicative. Ajouter ensuite le pilote, le service bloc et la liaison de NovaFS. Le moteur P8 et les politiques de système de fichiers demeurent en espace utilisateur.

**B — Transport VirtIO médié par le noyau.** Le superviseur serait seul propriétaire des registres de configuration et des descripteurs matériels. Il construirait des chaînes bornées à partir de tampons autorisés et copiés, avec métadonnées immuables pendant l’E/S, barrières de publication et de complétion, et la même discipline de reset. Cette option réduit les prérequis IOMMU et augmente le code de transport de confiance. Une simple vérification des descripteurs laissés ensuite inscriptibles au pilote introduirait une course entre contrôle et usage. Le choix entre A et B détermine qui peut programmer le périphérique et qui doit prouver le confinement.

**Alternative sans DMA étudiée.** Le modèle PIIX3 IDE figure dans le QEMU x86_64 installé. Le QEMU AArch64 installé refuse `-device piix3-ide,help`. SDHCI PCI est proposé des deux côtés. Pour le chemin SD ordinaire, le code amont examiné journalise côté hôte des erreurs de lecture et d’écriture et ne fournit pas, dans ces routines, de confirmation de flush exploitable par le client. Il faudrait qualifier la propagation des erreurs et la barrière du modèle exact avant de le retenir. Aucun de ces constats ne justifie d’exposer des ports globaux, l’espace de configuration PCI ou un CD d’amorçage comme interface de données. [Modèle PIIX de QEMU](https://github.com/qemu/qemu/blob/master/hw/ide/piix.c) · [Modèle SD de QEMU](https://github.com/qemu/qemu/blob/master/hw/sd/sd.c)

## Interface bloc proposée lors du premier audit

Le chemin proposé est **application → NovaFS → service bloc → pilote utilisateur → disque de données QEMU**. Le protocole bloc est indépendant du format NovaFS. Seul le pilote reçoit les ressources matérielles. Le lanceur publie séparément le canal du client vers NovaFS et celui de NovaFS vers le service bloc. Le service bloc associe **à l’endpoint** l’identité du périphérique, l’intervalle logique délégué, les droits de lecture, d’écriture et de synchronisation et une génération de session. Chaque client obtient un canal de réponse privé. Les PID, adresses et numéros de session fournis dans les charges utiles ne font pas autorité. Le pilote se voit confier un disque de données dédié. Si plusieurs domaines de confiance doivent partager un disque, l’IOMMU protège la RAM et le service bloc de confiance reste responsable des bornes LBA. Un pilote autorisé sur le disque entier appartient à la base de confiance pour son contenu. Si les bornes LBA doivent aussi résister à une faute de ce pilote, attribuer un backend matériel distinct par plage ou imposer la vérification des commandes dans un composant de confiance indépendant.

| Opération proposée | Validation et réponse |
|---|---|
| INFO | Géométrie logique et physique, taille de la vue déléguée, mode lecture seule, capacité de flush. |
| READ | Droit de lecture et un secteur logique entier dans la vue. Livrer les fragments après complétion du pilote. Sur erreur, le client abandonne toute réponse incomplète. |
| WRITE | Droit d’écriture, secteur dans la vue, longueur exacte et fragments complets avant soumission. Un succès indique que le périphérique a accepté toute la requête. |
| FLUSH | Droit de synchroniser, file ordonnée, écritures antérieures terminées et confirmation positive du périphérique. Une plateforme incapable de confirmer le flush doit refuser l’engagement durable. |
| CLOSE | Retirer les droits de la session, régler le sort de la requête en vol et fermer les endpoints. La fermeture n’est pas une barrière de persistance. |

Premier profil proposé, sans changement des seize sélecteurs ABI. Un échange en vol par session, une file matérielle et un tampon borné à un secteur logique (512 ou 4 096 octets). Un en-tête little-endian de 24 octets peut porter version, opération, phase et drapeaux (4 × u8), longueur du fragment et statut (2 × u16), identifiant de requête (u64), position et longueur totale (2 × u32). Il reste 104 octets par message, soit cinq fragments pour 512 octets et quarante pour 4 096. BEGIN porte l’adresse logique. Les fragments doivent couvrir chaque octet, dans l’ordre, avant COMMIT. Contrôler les additions, conversions de secteurs, limites de vue et de disque avant tout accès. Pour VirtIO Block, l’unité d’adressage du champ matériel reste 512 octets, même si le secteur logique annoncé vaut 4 096. [Limites IPC existantes](ipc-abi.md) · [Format des requêtes VirtIO Block](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Distinguer capacité absente ou révoquée, droit insuffisant, cible invalide, hors plage, dépassement arithmétique, taille ou alignement invalide, trame invalide, indisponibilité, erreur d’E/S, échec de synchronisation et service interrompu. Une déconnexion avant soumission abandonne le tampon. Après soumission, un timeout, une erreur ou la perte de la réponse peuvent laisser l’issue d’une mutation indéterminée. Une nouvelle session utilise de nouveaux endpoints. Écarter les réponses tardives et proscrire le rejeu automatique des mutations incertaines.

### Domaine de persistance et oracle de reprise

Pour le banc, choisir une image raw ordinaire, `cache=writeback`, `rerror=report,werror=report`, sans surcouche éphémère pour les données. QEMU doit transmettre les flushs au backend (`cache.no-flush=off`). Le pilote doit négocier le support FLUSH, ordonner la soumission et vérifier le statut final. Le domaine de persistance s’arrête à la confirmation rendue par QEMU et le stockage macOS. Une coupure de VM laisse subsister les caches de l’hôte. La future intégration de NovaFS exigera F1 après données et métadonnées, puis publication de l’ancrage et F2 avant confirmation au client. [Modes de cache QEMU](https://www.qemu.org/docs/master/system/invocation.html) · [Contrat de durabilité P1](../../novaFS/docs/novafs-p1-durabilite.md)

Le futur banc devra conserver hors VM le numéro de génération, le motif et la somme du dernier contenu confirmé durable. Sur image jetable, injecter les coupures avant soumission, en vol, après WRITE, pendant FLUSH et après son acquittement. Tuer le processus QEMU, en lancer un nouveau sur **la même image**, puis relire via le service invité et comparer aussi depuis un lecteur hôte indépendant. Vérifier les sentinelles et les secteurs durables non visés. Avant confirmation, qualifier l’ancien contenu, le nouveau contenu et toute écriture partielle observable suivant l’atomicité effectivement documentée. Une déchirure doit être rapportée explicitement. Après confirmation, tout écart au contenu durable attendu fait échouer le test. Injecter séparément les erreurs de lecture, d’écriture et de flush.

## Observations du premier audit

- Hôte Darwin ARM64, QEMU 11.1.3, TCG, un vCPU, 256 Mio, UEFI/Limine. Une image raw de 4 Mio initialisée côté hôte a été jointe à Q35 et à `virt`. Pour **chaque architecture et chaque géométrie** (512/512 puis 4 096/4 096), deux processus QEMU indépendants ont démarré sur la même image. Huit démarrages au total. QMP a confirmé l’image inscriptible, la taille, les deux propriétés de géométrie, la transmission des flushs configurée et `iommu_platform=false`. Dans les huit invités, NovaFS a répondu `INFO volumes=0 FIN confirmee`. Après chaque arrêt, l’empreinte de l’image entière restait `2b07811057df887086f06a67edc6ebf911de8b6741156e7a2eb1416a4b8b1b2e`.
- Deux essais distincts, CPU arrêté avant le firmware, ont associé `virtio-iommu-pci,boot-bypass=off` et `virtio-blk-pci,iommu_platform=on`. QMP a relu ces deux propriétés sur chaque architecture. Il reste à démontrer l’attachement et l’isolation par un pilote d’IOMMU dans NovaOS.
- **Backend hôte seul.** Pour des transferts de 512 et 4 096 octets à l’offset 8 192, `qemu-io` a écrit un motif, demandé un flush et relu le motif dans un autre processus. Un lecteur Python a vérifié l’image entière, sentinelles comprises. Les trois injections `blkdebug` ont donné un code de sortie 1 et une erreur d’E/S à chaque taille. L’échec de vidage était notamment visible à la fermeture du backend. Ces contrôles qualifient l’oracle hôte. [Injection d’erreurs QEMU](https://www.qemu.org/docs/master/devel/testing/blkdebug.html)
- **Stockage bloc invité.** Les cinq opérations du nouveau service, leurs droits sur les secteurs, les erreurs renvoyées au client, les pannes du pilote, la relecture d’une écriture invitée après redémarrage et la durabilité restent **non validées**. Latences READ/WRITE/FLUSH, débits séquentiel et aléatoire, allocations, pics et fuites propres au futur service bloc restent à mesurer. Le démarrage vérifié, les mises à jour atomiques et le portage du moteur P8 restent hors de ce jalon.

### Régression et mémoire du service existant

Les deux commandes `make test` ont terminé avec le code 0, chacune après 25 démarrages UEFI. Le scénario `storage-test` a revérifié le client sans bootstrap, les droits inversés, les trames invalides, la perte du serveur, la seconde session, la pénurie de cadres et la restitution du Buddy. Les images testées ont les mêmes SHA-256 que dans la qualification v0.9.0. [Référence v0.9.0](novafs-integration-v0.9.0.md)

| Mesure du scénario v0.9.0 dans cet audit | x86_64 | AArch64 |
|---|---:|---:|
| RAM résidente avant lancement et après collecte | 1 212 Kio | 1 140 Kio |
| RAM avec le serveur en attente après collecte du client | 1 336 Kio | 1 264 Kio |
| RAM à la publication des deux processus | 1 428 Kio | 1 356 Kio |
| Pic dans la fenêtre panne, reprise et second client | 1 432 Kio | 1 360 Kio |
| Allocations de cadres dans cette fenêtre | 162 | 161 |
| Blocs Buddy vivants après collecte | 3 | 3 |
| Échantillons d’aller-retour IPC | 513 | 513 |
| Latence IPC p50 / p99 | 30,68 / 53,19 µs | 45,06 / 57,34 µs |
| Commutations directes chronométrées | 1 027 | 1 026 |

Ces mesures concernent INFO et FIN en mémoire. La qualification publiée donnait 49,07 et 53,25 µs au p99, avec des campagnes successives. Les suites de cet audit ont tourné simultanément. Le pic et le bilan final des cadres sont identiques à ceux de la référence.

## Reproduction du premier audit

```sh
make -C novaOS ARCH=x86_64 test
make -C novaOS ARCH=aarch64 test
qemu-system-x86_64 -device virtio-blk-pci,help
qemu-system-aarch64 -device virtio-blk-pci,help
```

Les commandes suivantes contrôlent l’attachement des images via QMP et attendent l’échange série entre les deux processus invités. Les images de test sont exclusivement temporaires.

```sh
python3 - <<'PY'
import hashlib, os, subprocess, sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path("novaOS/tests").resolve()))
from demarrage import qmp, wait_demo, wait_storage

with tempfile.TemporaryDirectory() as tmp:
    disk = Path(tmp) / "audit.raw"
    disk.write_bytes(bytes(range(256)) * 16384)
    digest = lambda: hashlib.sha256(disk.read_bytes()).hexdigest()
    initial = digest()
    for arch in ("x86_64", "aarch64"):
        for sector in (512, 4096):
            for boot in (1, 2):
                log = Path(tmp) / f"{arch}-{sector}-{boot}.log"
                args = ["sh", "novaOS/scripts/qemu.sh", "-display", "none",
                        "-monitor", "none", "-serial", f"file:{log}", "-qmp", "stdio",
                        "-drive", f"if=none,id=data,file={disk},format=raw,cache=writeback,werror=report,rerror=report",
                        "-device", f"virtio-blk-pci,drive=data,id=audit,disable-legacy=on,logical_block_size={sector},physical_block_size={sector}"]
                with subprocess.Popen(args, env={**os.environ, "NOVA_ARCH":arch},
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0) as vm:
                    try:
                        qmp(vm, "qmp_capabilities")
                        wait_demo(vm, log, "Ring3" if arch=="x86_64" else "EL0")
                        wait_storage(vm, log)
                        node = next(d["inserted"] for d in qmp(vm, "query-block") if d["device"] == "data")
                        assert node["image"]["virtual-size"] == 4*1024**2 and not node["ro"]
                        assert not node["cache"]["no-flush"]
                        for prop in ("logical_block_size", "physical_block_size"):
                            assert qmp(vm, "qom-get", path="/machine/peripheral/audit", property=prop) == sector
                        assert not qmp(vm, "qom-get", path="/machine/peripheral/audit", property="iommu_platform")
                        qmp(vm, "quit")
                        assert vm.wait(5) == 0
                    finally:
                        if vm.poll() is None:
                            vm.kill()
                            vm.wait()
                assert digest() == initial
                print(arch, sector, boot, "INFO volumes=0", initial, flush=True)
PY
```

Pour reproduire le contrôle de topologie IOMMU sur chaque architecture, exécuter depuis la racine le même lanceur avec le processeur arrêté, puis interroger ses propriétés à l’accueil QMP. Ces commandes sont indépendantes de l’image jetable utilisée plus haut.

```sh
for arch in x86_64 aarch64; do
    printf '%s\n' \
        '{"execute":"qmp_capabilities"}' \
        '{"execute":"query-status"}' \
        '{"execute":"qom-get","arguments":{"path":"/machine/peripheral/remap","property":"boot-bypass"}}' \
        '{"execute":"qom-get","arguments":{"path":"/machine/peripheral/disk","property":"iommu_platform"}}' \
        '{"execute":"quit"}' |
        NOVA_ARCH="$arch" sh novaOS/scripts/qemu.sh -S -display none -monitor none -serial none -qmp stdio \
            -device virtio-iommu-pci,id=remap,boot-bypass=off \
            -blockdev driver=null-co,node-name=topologie,size=4194304 \
            -device virtio-blk-pci,drive=topologie,id=disk,disable-legacy=on,iommu_platform=on
done
```

Le backend nul de cette commande sert uniquement à instancier les objets de la topologie CPU arrêté. Pour le contrôle de persistance **hôte**, utiliser des fichiers raw neufs. Chaque invocation suivante de `qemu-io` ouvre l’image dans un processus distinct.

```sh
python3 - <<'PY'
import errno, subprocess, tempfile
from pathlib import Path

def io(image, commands):
    args = ["qemu-io", "-t", "writeback", "-f", "raw"]
    for cmd in commands:
        args += ["-c", cmd]
    return subprocess.run([*args, image], capture_output=True, text=True)

with tempfile.TemporaryDirectory() as tmp:
    for size in (512, 4096):
        disk = Path(tmp) / f"disk-{size}.raw"
        disk.write_bytes(bytes(1024 * 1024))
        assert io(str(disk), [f"write -P 0x5a 8192 {size}", "flush"]).returncode == 0
        assert io(str(disk), [f"read -P 0x5a 8192 {size}"]).returncode == 0
        expected = bytearray(1024 * 1024)
        expected[8192:8192 + size] = bytes([0x5a]) * size
        assert disk.read_bytes() == expected
        for event, commands in [
            ("read_aio", [f"read 8192 {size}"]),
            ("write_aio", [f"write 8192 {size}"]),
            ("flush_to_disk", [f"write -P 0x5a 8192 {size}", "flush"]),
        ]:
            config = Path(tmp) / "fault.conf"
            config.write_text(f'[inject-error]\nevent = "{event}"\nerrno = "{errno.EIO}"\n')
            result = io(f"blkdebug:{config}:{disk}", commands)
            assert result.returncode == 1 and "Input/output error" in result.stdout + result.stderr
            print(size, event, "EIO")
PY
```
