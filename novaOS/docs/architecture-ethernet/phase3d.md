# NovaOS v0.25.0 — Phase 3D — retrait DMA multipériphérique

Le Q35 expérimental peut rendre les **26 cadres privés du NIC** après la mort de son pilote Ring 3, pendant que le NVMe conserve son contexte, ses six cadres et son activité. Le superviseur rend les cadres après le reçu matériel correspondant au retrait de cette incarnation. La non-livraison du reçu laisse les 26 cadres épinglés. Sur le profil ARM existant, un seul RX consommé sur seize permet de rendre huit cadres et impose d’en retenir quatorze.

Les références sont la clôture 3B `32f372820af79f7f0669a6dae731c0e56beffa39` et la clôture 3C `0169c75148c8359c9235f3f19359133a0e002115`. Le noyau de qualification 3D provient de `58e387267ea900f18e4c0620a4b737721ac3eb39`. Le constructeur vérifie les quinze modules du superviseur historique à la révision `56db7d5c0d04490cd284b91567cbbd4736534266`, puis applique le raccord privé dans une extraction temporaire. Le manifeste de chaque construction enregistre les objets d’origine et les objets effectifs. La branche du pilote conserve sa révision auditée. Le jalon documentaire v0.25.0 conserve le manifeste interne v0.20.0. [Constructeur](../../tests/ethernet_v1/port_construire_q35.py) · [Raccord des modules audités](../../tests/ethernet_v1/retrait_duo.patch) · [Implémentation du reçu](../../kernel/src/hardware/vtd_lab/retrait_individuel.rs)

## Preuve du retrait sur Q35 double

1. Le courtier ferme l’autorité de soumission. Il observe le désarmement PCI du NIC, le reset de fonction, les deux files désactivées et les entrées utilisées cohérentes avec les buffers de cette incarnation. Le chemin de décès exige la collecte des trois acteurs, la disparition de leurs racines IPC, la cause INTx attendue et la fermeture de la ligne.
2. Le superviseur associe le jeton privé `Drain` au PID du pilote et à sa génération. Il vérifie l’identité PCI, les 26 cadres distincts, les permissions de leurs feuilles, l’absence d’alias avec les huit ancres et les six cadres NVMe. Il relit le contexte et les traductions du stockage actif.
3. Il efface l’entrée de contexte de `00:07.0`, SID 56, puis publie sur la QI commune une invalidation de contexte sélective pour ce requester, une invalidation IOTLB du DID 3 avec DR et DW et un WAIT avec écriture du cookie. La confirmation vérifie le cookie, la tête, la queue et l’état de faute. Le SID 48 et le DID 1 du NVMe conservent leurs traductions. La racine du DRHD et la page QI restent installées.
4. Le reçu privé garde le titulaire, la génération, le cookie et la position de fin. Sa collecte exige un NIC toujours désarmé, une file commune saine et les tables et droits attendus. L’effacement du reçu et la restitution sont à usage unique. Une demande avec un ancien numéro de génération, un autre titulaire ou un autre cookie laisse le reçu intact. Toute admission pendant la rétention est refusée.
5. Le dernier cookie commun peut appartenir au disque. Le cas `duo-postdisque` laisse le reçu NIC 4 en attente jusqu’au retrait normal du NVMe. La tentative de restituer le NIC avec le cookie disque 5 échoue. Le cookie NIC 4 rend ensuite les 26 cadres et un doublon est refusé. Le retrait du NIC avait laissé le stockage actif pendant toute la transaction.

La spécification VT-d décrit le drainage des DMA déjà traités par le remappeur et encore en attente dans le complexe racine. Le WAIT ordonne les opérations pertinentes pour le domaine sélectionné. Le profil ferme la source de nouveaux DMA et exclut les ponts, alias, ATS, Device-TLB et mode scalable. Ces préconditions bornent l’usage de cette preuve. Les observations portent sur les modèles de laboratoire. [Intel VT-d, rév. 4.1, §§ 6.5.2 et 6.5.4](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf)

## Matrice QEMU

| Scénario | Preuve observée | Ressources à la fin |
|---|---|---|
| Q35, décès et retrait individuel | Quatre trames par sens. Cookie 4 du NIC collecté après une CQE NVMe. 31 commandes de stockage après ce WAIT. | 0 cadre NIC, 8 ancres du contrôleur |
| Q35, livraison différée | 26 cadres conservés jusqu’à la 30ᵉ CQE. READ, WRITE et FLUSH aboutissent pendant la réconciliation. | 0 cadre NIC, 8 ancres |
| Q35, reçu non livré | La QI confirme le retrait du NIC. Son contexte est vide. Le superviseur conserve les pages et refuse la nouvelle admission. | 26 NIC + 8 ancres |
| Q35, preuve de reset retenue | Aucun jeton de retrait. Le contexte du NIC désarmé et ses permissions sont relus sous QTest. Le NVMe termine sa transaction. | 26 NIC + 8 ancres |
| Q35, livraison après le WAIT du disque | Le reçu du disque est refusé pour le NIC, puis l’ancien reçu valide du NIC est accepté une seule fois. | 0 cadre NIC, 8 ancres |
| Q35, deux retraits concurrents | Deux demandes distinctes sont en attente. L’ordre NIC puis NVMe et l’ordre inverse sont chacun exercés avec deux acquittements ciblés. | 0 cadre de bail, 8 ancres |
| Q35, WAIT matériel commun absent ou tardif | Les deux périphériques ont terminé et désarmé leur BME. La QI s’arrête à IQH 176, IQT 192. Un cookie tardif après expiration ne rouvre aucun bail. Les deux demandes restent refusées. | 26 NIC + 6 NVMe + 8 ancres |
| Q35, réouvertures après restitution | Huit décès, sept réouvertures et huit transactions. 56 PID distincts et 33 WAIT sur une seule racine. | 0 cadre de bail à chaque fin, 8 ancres |
| ARM, RX partiel | Un seul RX rendu, aucun TX en vol. Huit pages TX sont libérées. Les six pages de files et les huit pages RX restent retenues. | 14 NIC + 9 du contrôleur |
| ARM, RX rendus après arrêt du client | Les seize identifiants utilisés sont validés sans délivrer de reçu applicatif artificiel. Les 22 pages sont rendues après UNMAP et DETACH. | 0 cadre DMA au terme du profil |
| ARM, reçu IOMMU différé ou absent | Lectures et FLUSH du disque pendant l’attente. La collecte exacte permet la réouverture. L’absence de livraison maintient la quarantaine. | 0 si collecté, sinon 22 NIC + 9 contrôleur |

Les deux instantanés QTest pris en cours de réconciliation sont des démarrages distincts des mesures de latence. Ils relisent le registre PCI des deux périphériques, la racine, la page de contexte et les deux arbres de traduction. Le NIC a BME à zéro et un contexte absent. Le NVMe a BME à un et conserve les seules feuilles autorisées du DID 1. Les 40 cadres recensés sont distincts. Les quatre tables et les 22 pages NIC sont encore attribuées pendant la conservation du reçu. Le pair Ethernet vérifie les octets dans les deux sens. La relecture hors VM contrôle également les PCAP. [Oracle QTest, QI et NovaFS](../../tests/ethernet_v1/retrait_duo.py)

## Endurance et ressources

| Mesure par invité | AArch64 VirtIO-IOMMU | x86_64 Q35 double |
|---|---:|---:|
| Cycles nominaux successifs dans un démarrage | 256 | 256 |
| Générations du NIC | 4 à 259 | 1 à 256 |
| Trames vérifiées par sens | 36 864 | 36 864 |
| Pages IPC et processus utilisateur résiduels après chaque cycle | 0 et 0 | 0 et 0 |
| Cadres libres au relevé comparable de chaque cycle | 61 858 | 24 518 |
| DMA après le retrait dans chaque cycle | 16 pour le stockage et son contrôleur | 8 ancres de la racine commune |
| DMA final après la clôture du profil | 0 | 8 ancres |

Sur ARM, le relevé des cadres libres suit la collecte des trois acteurs. Le relevé DMA par cycle suit les UNMAP et la lecture du disque témoin. Sur Q35, le relevé final de chaque cycle suit la restitution des deux baux. Le banc vérifie les compteurs de pages, les tables, le tas, les piles et les PID des sept acteurs. Les 1 792 PID des 256 cycles sont distincts. La trace contient 10 487 commandes de stockage achevées. Le lecteur indépendant retrouve les deux dernières ancres et la génération 259, contre 3 au départ. Les 1 025 groupes CC/IOTLB/WAIT produisent douze tours complets de la file de 256 descripteurs. L’oracle décode toute la trace QEMU et reconstruit les 4 096 octets de la dernière image circulaire, y compris les triplets à cheval sur sa fin. Le dernier cookie vaut 1 025, IQH et IQT valent 48.

Le contrat Ethernet conserve les 48 sélecteurs ABI, quatre racines maximum par acteur, huit crédits, le transport PUT/TAKE groupé et le repli DATA négocié à l’ouverture. Les essais de repli explicite et d’épuisement des boîtes exercent les deux architectures. Le plafond de huit boîtes de 4 Kio représente 32 Kio au total. Le superviseur et ses deux emplacements de demande restent privés. Le registre généré, les manifestes NovaOS, le moteur NovaFS, les composants et la distribution gardent leurs objets Git de phase 3C. Les tests hôtes donnent 20 réussites par architecture de processus Darwin, 12 pour Ethernet et ses droits, trois pour les demandes et cinq pour le format VT-d. Les neuf contrôles ABI réussissent. L’exécution x86_64 hôte utilise Rosetta. [Tests hôtes](../../tests/ethernet_v1/hote_retrait.py)

## Continuité et latence NovaFS

Sur le namespace Q35 jetable à secteurs de 512 octets, la trace indépendante associe chaque CID au READ, WRITE ou FLUSH et à sa CQE. Pour chaque transaction de l’endurance, elle vérifie les écritures, F1, la publication de l’ancre, F2 et l’acquittement applicatif. Après arrêt de la VM, le lecteur NovaFS portable remonte le volume, contrôle les deux générations d’ancres, le contenu exact de `/sortie` et la page de garde. Les instantanés de chaque génération intermédiaire ne sont pas conservés sur l’hôte. Sur ARM, le banc vérifie les READ/FLUSH du disque indépendant et l’empreinte du volume de référence.

| Variante, médiane de cinq démarrages | READ p50 / p95, ms | WRITE p50 / p95, ms | FLUSH p50 / p95, ms |
|---|---:|---:|---:|
| Décès témoin, 26 cadres conservés | 4,442 / 5,072 | 3,713 / 4,963 | 3,148 / 3,743 |
| Retrait individuel immédiat | 4,047 / 5,047 | 3,753 / 5,141 | 3,532 / 3,912 |
| Collecte différée à la 30ᵉ CQE | 3,602 / 5,096 | 3,424 / 5,275 | 3,099 / 3,792 |

Le p95 des écritures augmente de 0,178 ms avec restitution immédiate et de 0,312 ms avec collecte différée. Le p95 des lectures reste dans un intervalle de 0,05 ms entre les trois variantes. Le p50 des FLUSH monte de 0,384 ms sur la variante immédiate.

Les démarrages de latence utilisent les mêmes périphériques, le même contenu initial et le même chronométrage de la soumission jusqu’à la CQE. Le groupe témoin conserve les 26 cadres après le décès. Chaque démarrage produit 22 READ, 7 WRITE et 3 FLUSH. Le p95 d’un démarrage représente donc son maximum pour WRITE et FLUSH. L’effectif des FLUSH et la variabilité QEMU limitent toute conclusion sur les queues de distribution. Le cas historique `BUSY (24)` du mode fichiers reste une réserve distincte.

## Régression Ethernet Q35

| Variante, médiane de cinq démarrages | Débit TX utile, Mbit/s | p99 du lot, ms | IPC par trame |
|---|---:|---:|---:|
| Image archivée 3B | 37,38 | 2,896 | 14,96 |
| Image archivée 3C | 35,83 | 3,070 | 14,97 |
| Témoin 3C reconstruit | 36,21 | 3,114 | 14,95 |
| Clôture en fonction froide | 37,23 | 3,046 | 14,96 |

Sur les images archivées, l’écart médian actuel atteint −4,13 % de débit et +6,00 % de p99. Les écarts publiés en phase 3C étaient de −3,71 % et +5,52 %. Le nombre d’IPC par trame reste proche de 15.

La contre-épreuve porte exclusivement sur l’écho de lots de huit trames de 1 514 octets. Chaque ELF dispose de cinq démarrages et de 1 024 lots mesurés par démarrage. Le témoin 3C reconstruit et l’image 3C archivée ont la même section `.text`. Dans la variante expérimentale, l’adresse d’entrée de `Port::demande` est identique à celle du témoin reconstruit. L’extraction du traitement de clôture fait passer le corps de `Port::demande` de 3 304 à 2 864 octets. Le débit augmente dans les cinq paires, le p99 baisse dans quatre. Le p99 médian reste au-dessus de la tolérance de 1 % par rapport au témoin 3B. Le correctif de disposition reste une contre-épreuve archivée. Cette expérience décrit une sensibilité au code généré par le compilateur et exécuté sous TCG. La contribution exacte de chaque bloc traduit à la régression 3C reste ouverte. [Critères fixés avant l’essai](criteres-phase3d.md) · [Modification diagnostique](../../tests/ethernet_v1/cloture_froide.patch)

## Portée des garanties et arbitrages

Le profil AArch64 exécute QEMU 10.0.12, `virt-10.0,gic-version=3,its=off`, Cortex-A72. Le Q35 utilise QEMU 11.1.3 expérimental, SMM, SATA désactivé et support OVMF de démarrage PIO de 34 Mio. Les profils utilisent un vCPU et 256 Mio. Les essais de WAIT absent ou tardif vérifient un second modèle QEMU et les préconditions historiques de confinement et de démarrage signé. Ils exercent une incertitude commune après confirmation de la transaction du disque. La poursuite de nouvelles E/S après une défaillance commune de l’IOMMU reste hors du résultat observé. Les empreintes du firmware, des modèles et des ELF sont conservées. La reproductibilité binaire intégrale de chaque ELF et la reprise sur machine physique restent ouvertes.

VirtIO 1.3 distingue le reset global du périphérique (§ 2.4) et le reset individuel de virtqueue (§§ 2.6.1.1 et 4.1.4.3). Le premier interdit les nouvelles interactions avec les files après acquittement. L’UNMAP du virtio-iommu retire les traductions demandées (§ 5.13.6.6). Le banc ne possède pas, par ces deux observations seules, un reçu identifiant les anciennes transactions RX en aval. Le second mécanisme autorise explicitement la restitution des ressources d’une file après vérification de son reset. Sous PCI, il suppose de négocier `VIRTIO_F_RING_RESET`, d’écrire `queue_reset` et de relire `queue_reset=0` et `queue_enable=0` pour la file sélectionnée. [Spécification OASIS VirtIO 1.3](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)

La lecture ECAM indépendante sur ARM trouve la capacité commune dans le BAR 4, offset 0, longueur 4 096 octets. Le champ de deux octets à l’offset 58 est dans ses bornes. Le NIC offre le bit 40, absent du masque négocié `0x320000020`. Le courtier historique ne possède aucune opération de reset individuel de file. L’arbitrage demandé au titre du § 8 porte sur l’ajout de cette opération privée au seul profil expérimental. En attendant cet arbitrage, les 14 cadres du cas RX partiel restent en quarantaine. Une qualification ultérieure devra associer les acquittements des deux files à la bonne incarnation, conserver les reçus IOMMU et exercer un acquittement manquant sans inventer une complétion Ethernet.

Pour un futur gestionnaire de pilotes, il reste à attribuer et conserver l’identité du groupe isolable, la propriété du contrôleur commun et les reçus pendant un redémarrage du superviseur. Il devra gérer l’épuisement des générations, les demandes concurrentes au-delà des deux emplacements du banc et le traitement d’un contrôleur commun en panne. Les sept réouvertures après décès ont lieu dans le superviseur vivant, avec un retrait normal du bail stockage entre les générations. La réadmission dynamique d’un nouveau NIC au milieu du même bail NVMe reste à qualifier. Cette phase livre le raccord du profil expérimental.

## Incidents du banc

- La première instrumentation appelait sur Q35 l’étalonnage de fréquence dans la section critique du courtier NVMe. L’étalonnage attendait le tick qu’empêchait cette section. Le courtier lit désormais le TSC et le lanceur utilise la fréquence déjà étalonnée dans cette session.
- Au deuxième cycle de l’essai initial, le service de fichiers a reconnu l’écriture du même contenu et aucun WRITE durable n’a eu lieu. L’oracle a refusé ce cycle. Le client du banc alterne deux contenus de 4 231 octets pour imposer une transaction vérifiable à chaque génération. Les droits et le moteur NovaFS gardent leur comportement.
- Le premier décompte hôte des commandes après désarmement incluait l’intervalle entre la publication du contexte NIC et le premier BME. Le consolidateur repère le front PCI de désarmement dans la trace d’origine. Les rapports bruts restent consultables sous leur révision de lanceur.

## Reproduction

Les archives brutes résident dans `OS-worktrees/archives-ethernet-phase3d`. L’inventaire couvre 756 fichiers et 56 démarrages retenus. Il relie par SHA-256 les ELF, PCAP, traces, namespaces, arguments et rapports. Les 364 pièces du paquet compact comprennent les journaux des démarrages et les deux traces des instantanés actifs. Les révisions intermédiaires et les échecs de mise au point restent séparés des résultats de qualification. [Matrice et mesures par démarrage](../validation/ethernet-v1-phase3d/bilan.json) · [Inventaire](../validation/ethernet-v1-phase3d/inventaire.json) · [Pièces compactes](../validation/ethernet-v1-phase3d/preuves.tar.gz) · [SHA-256](../validation/ethernet-v1-phase3d/SHA256SUMS)

Depuis le worktree Ethernet, avec le Python 3.12 du laboratoire et les dépendances historiques identifiées. Chaque construction et chaque démarrage de la recette acquiert le verrou partagé. La destination d’un nouveau rejeu doit être libre.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
banc=novaOS/tests/ethernet_v1
sortie=../archives-ethernet-phase3d-rejeu
for lot in images-duo duo instantanes latences arm capacite transports ethernet hote
do
  python3 -B "$banc/rejouer_phase3d.py" "$sortie" "$lot" || exit 1
done
python3 -B "$banc/bilan_retrait_duo.py" "$sortie" "$sortie/bilan"
```
