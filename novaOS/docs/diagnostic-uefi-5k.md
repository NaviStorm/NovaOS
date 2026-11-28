# NovaOS v0.25.0 — Phase 5K — Retrait DMA individuel

Le retrait du demandeur EDU est qualifié sur le Q35 de laboratoire avec VT-d legacy, une racine commune, trois domaines exclusifs et ATS désactivé. Dans le profil NovaOS VirtIO-net/NVMe, les 26 cadres du NIC redeviennent disponibles après le décès du pilote, la confirmation matérielle et la collecte de son reçu. Le stockage continue ses lectures, écritures et FLUSH. Le refus de restitution fait aussi partie du résultat qualifié. **L’intervalle EFI complet reste refusé** dans les deux modes Secure Boot. [Preuves et empreintes](validation/diagnostic-uefi-5k/SHA256SUMS) · [Recette et vérificateur](../boot/diagnostic-5k/README.md)

## Autorité et portée de la preuve

L’autorité 5K vit dans le processus hôte qui possède exclusivement une VM neuve et son canal de contrôle. Elle reprend le modèle de confiance de 5J. Elle vérifie la DMAR, les RID, la provenance de sa réserve volatile, la racine libre avant installation et les traductions effectives. Le bail contient l’époque de démarrage, le titulaire, le RID, le DID, l’incarnation, la génération et les deux pages. Le diagnostic EFI signé de 5J est identique dans les deux modes. Le noyau, le courtier NIC/NVMe, la garde d’admission et l’ABI à 48 sélecteurs gardent leurs versions de référence. [Base 5J](diagnostic-uefi-5j.md)

1. La porte privée enregistre un transfert EDU aller-retour et refuse les nouvelles soumissions dès la clôture. Le modèle exécute la lecture sur la première page et l’écriture sur la seconde. La dernière commande est observée en vol. Le drainage exige simultanément la fin de RUN et la présence sur la page destination du défi de cette époque. La mort d’un acteur, BME à zéro, le temps écoulé ou une absence de trafic ne délivrent aucun reçu.
2. À vCPU arrêté, le courtier relit les tables et leur identité. Il efface les deux feuilles privées et le seul contexte du RID retiré. La racine commune reste installée. La file reçoit une invalidation du cache de contexte ciblée par RID et DID, une invalidation IOTLB du seul DID avec drainage des lectures et écritures et un WAIT clôturé par une barrière. Le DID est exclusif. Le profil ne dispose d’aucun cache de traduction périphérique actif.
3. Le courtier lie le cookie et la position cible avant de publier la queue. Il exige la valeur écrite par le remappeur, les positions IQH/IQT attendues, l’absence de faute et la conservation des autres tables. L’autorité consomme une seule fois son objet reçu. Un autre bail, une copie de l’objet, un reçu périmé ou contradictoire échouent. Le lot entier reste indisponible jusqu’à l’effacement et à la relecture intégrale des deux pages. Une faute de file, une divergence des tables ou un échec d’effacement maintient la quarantaine.
4. Après la réaffectation, l’observateur exerce encore le modèle EDU avec BME actif. La trace identifie une faute de contexte pour le RID retiré à l’ancienne IOVA. Le motif du nouveau titulaire reste intact. Les 14 pages de tables sont comparées octet par octet, avec les seules différences autorisées au contexte et aux deux feuilles. Les états PCI voisins, le pointeur de racine, TES, les compteurs du namespace et son empreinte sont relus séparément. [Spécification du modèle EDU](https://www.qemu.org/docs/master/specs/edu.html) · [Spécification Intel VT-d](https://www.intel.com/content/www/us/en/content-details/774206/intel-virtualization-technology-for-directed-i-o-architecture-specification.html)

## Qualification du stockage

Les cinq démarrages du témoin emploient les ELF de la révision de construction `58e387267ea900f18e4c0620a4b737721ac3eb39`, avec le banc de la base 5K `5890a1194cb758ffbe345023af86a90b7db62c3f`. QEMU 11.1.3, les ELF et le lecteur P9 ont leurs empreintes distinctes dans chaque rapport. Le modèle modifié de rétention du WAIT sert exclusivement aux deux dernières lignes. [Oracle matériel du témoin](../tests/ethernet_v1/retrait_duo.py)

| Scénario Q35 NIC/NVMe | Résultat matériel et résultat du superviseur | Stockage |
|---|---|---|
| Décès du pilote avec retrait nominal | 26 cadres libérés, collecte en double refusée | 31 commandes après le WAIT du NIC |
| Collecte différée | 26 cadres retenus jusqu’à la collecte. Le cliché intermédiaire montre BME NVMe actif et BME NIC éteint | 31 commandes après le WAIT du NIC, READ/WRITE/FLUSH pendant la réconciliation |
| Preuve de reset manquante | 26 cadres NIC en quarantaine | 31 commandes après désarmement du NIC |
| WAIT commun absent | Cookie 0, IQH 176, IQT 192. Deux retraits refusés | Transaction P9 antérieure intacte |
| WAIT commun tardif | Cookie 4 et IQH/IQT 192 après le constat d’incertitude. Le reçu demeure refusé | Transaction P9 antérieure intacte |

Chaque image contient une transaction avec 32 commandes et un fichier de 4 231 octets. Le lecteur indépendant vérifie les deux ancres, les générations, le contenu restauré et le secteur de garde. Sur les deux pannes de la file commune, 40 cadres restent comptabilisés, dont 26 NIC, 6 NVMe et 8 de contrôleur. La poursuite de nouvelles E/S après une panne commune reste non démontrée. Les scénarios individuels et le scénario de panne commune ont des résultats différents pour la disponibilité du disque.

## Minuterie du firmware

La décision MOE autorise uniquement l’EOI local xAPIC de la minuterie du firmware qualifié. Sur chaque démarrage instrumenté, les points d’arrêt d’exécution du débogueur couvrent l’entrée du premier EFI jusqu’au retour à l’attente terminale après une interruption. Le vCPU conserve ses interruptions. Le banc valide en RAM les instructions des PE épinglés et relève le vecteur passé au gestionnaire, le PC de l’instruction, l’adresse de retour de l’appelant, l’adresse effective et la valeur. L’observateur QEMU fournit indépendamment la livraison de la minuterie locale, ISR 32, LVTT périodique 32, l’écriture 32 bits à zéro en 0xFEE000B0 sur le vCPU 0 et l’acquittement du registre 0x0b. Les événements concurrents réseau, NVMe ou de livraison externe invalident l’attribution. [Gestionnaire EDK II épinglé](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/OvmfPkg/LocalApicTimerDxe/LocalApicTimerDxe.c)

| Mode | EOI attribués au gestionnaire | Autres écritures observées | Verdict de l’intervalle |
|---|---:|---|---|
| Secure Boot actif | 3 | 3 paires PIO sur 0xb3 et 0xb2, modèle APM | Refus |
| Secure Boot inactif | 3 | 3 paires identiques | Refus |

Les six écritures APM par démarrage restent hors exception, y compris lorsqu’elles sont effectuées par un service firmware appelé pendant le diagnostic. Elles n’ont produit aucune E/S du namespace. Le verdict porte sur l’intervalle et les événements observés par ce modèle instrumenté. Une observation depuis l’entrée EFI ne qualifie pas rétroactivement une période antérieure. Le contre-exemple 5I conserve son statut initial. [Observation 5I](diagnostic-uefi-5i.md)

## Délais et ressources

Durées en millisecondes, mesurées par horloge monotone hôte sur une VM TCG mono-vCPU. Chaque ligne correspond à un démarrage indépendant du lot archivé.

| Profil EDU | Drainage | Invalidation ou constat IQE | Restitution et réaffectation | Pages retenues à la fin |
|---|---:|---:|---:|---:|
| Secure Boot actif | 100,477 | 0,123 | 68,672 | 0 |
| Actif avec observation APIC | 101,701 | 0,134 | 65,311 | 0 |
| Secure Boot inactif | 101,020 | 0,151 | 68,810 | 0 |
| Inactif avec observation APIC | 101,755 | 0,130 | 66,036 | 0 |
| Faute QI explicite, Secure Boot actif | 100,685 | 0,159 | Refus | 2 |

Le drainage inclut le délai d’exécution du modèle EDU, la suspension QMP et la relecture du défi. L’invalidation va de la publication IQT aux lectures IQH/IQT/WAIT. Le dernier délai comprend la relecture indépendante des tables, la vérification des reçus, l’effacement vérifié et la première écriture du nouveau propriétaire. Les coûts de l’observateur et du transport QTest sont inclus. Ces échantillons ne donnent pas de percentile ni de borne pour un périphérique physique.

La réserve volatile de 2 Mio contient **80 Kio utilisés**. Les 14 pages de tables prennent 56 Kio, les quatre cadres de données 16 Kio et la file QI avec son statut 8 Kio. Deux cadres de données, soit 8 Kio, font l’objet du retrait. L’injection d’un descripteur réservé provoque IQE, IQH 16, IQT 48, WAIT nul et la rétention de ces deux cadres. Le test ne réinitialise pas le remappeur commun. Le banc prend séparément un cliché de 256 Mio de RAM invitée pour retrouver le reçu EFI. La RSS haute du processus Python Darwin atteint 357,8 Mio sur la séquence complète. Cette mesure exclut le processus QEMU et inclut le coût du cliché.

## Contrôles et conditions d’entrée en 5L

Sur chaque hôte logique, Darwin ARM64 natif et Darwin x86_64 sous Rosetta, 13 tests 5K, 10 tests 5J et 6 tests des domaines 5I réussissent. La contre-lecture des dix démarrages publiés réussit sur les deux architectures. Le lecteur P9 natif a en outre remonté les cinq images conservées. Le contrôle de génération ABI et la contre-lecture des 12 cellules archivées de 5J réussissent. [Journal hôte](validation/diagnostic-uefi-5k/hotes.txt)

La qualification couvre un seul remappeur legacy, des RID sans alias, des domaines exclusifs, une géométrie connue et des canaux de commande privés à la VM. Elle établit la restitution pour EDU et pour le profil historique NIC/NVMe testé. Le coût d’un retrait intégré à un futur gestionnaire, la continuité après faute de la file commune, l’invalidation d’un cache ATS actif, les domaines partagés entre demandeurs et les périphériques physiques restent ouverts. Le contrôle d’accès Q35 et l’état fermé d’A8 demeurent applicables.

Pour soumettre 5L, la MOE doit préciser le profil visé et la décision attendue sur les écritures APM si l’acceptation de l’intervalle EFI complet est requise. Une cible possédant des alias ou des caches périphériques nécessite l’attribution de tous les demandeurs concernés et une preuve de complétion de chacune des invalidations applicables. Toute prétention de continuité sur faute commune exige un oracle de nouvelles transactions après la faute. Le passage en 5L relève d’un mandat distinct.
