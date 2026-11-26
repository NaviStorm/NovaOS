# Killer E2400 du MSI-DESKTOP — dossier technique de phase 0

**Décision du 26 novembre 2026 sollicitée auprès de la MOA/MOE.** Le contrôleur dispose de deux pilotes publics qui permettent de préparer un pilote Rust `no_std`. L’ouverture du DMA sur le MSI dépend encore de trois preuves propres à la machine : la chaîne de démarrage signée, la continuité de protection VT-d depuis l’UEFI et une route d’interruption sûre. La phase 0 livre une conception. Il n’existe dans ce dossier aucune mesure NovaOS sur le MSI.

| Pièce | Contenu pour l’arbitrage |
|---|---|
| Présent dossier | Matériel, registres, descripteurs, PHY, séquences, révision, errata, sources et licences |
| [Architecture et budgets](killer-e2400-architecture.md) | Frontière Ring 3, contrat de trames proposé, acquis VirtIO-net, retrait, mémoire et performances |
| [Qualification du MSI](killer-e2400-qualification-msi.md) | Commandes Windows, preuves Secure Boot et VT-d, essais sans écriture sur les disques Windows, phases et décisions |

**Références Git.** Base `origin/main` vérifiée à `5c7f1bfa313a3859ccc0830660438e76d598acb1`. Lecture de `origin/codex/pilote-virtio-net` à `6783dc9c2f3d3bcc13f26281cab05cca5c2c5e36`. Branche documentaire `codex/pilote-killer-e2400`. Les résultats d’autres matériels cités ci-dessous gardent leur propre provenance.

## Identification et niveau de preuve

| Origine | Ce qu’elle établit | Ce qui reste à obtenir |
|---|---|---|
| Inventaire communiqué par la MOA | MSI-DESKTOP, x86_64, Core i5-6300HQ, 16 Gio, démarrage UEFI avec Secure Boot actif. Killer E2400 Gigabit Ethernet, Qualcomm Atheros `1969:e0a1`, sous-système MSI `1462:115a`, révision PCI `10h`. Identifiant `PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10` | Modèle exact de carte mère, version du BIOS, port racine, fonction PCI, firmware réellement chargé et ressources à ce démarrage |
| Linux `alx` v6.12 et FreeBSD `alc` 14.3 | L’identifiant E2400 est admis par les deux pilotes. Ils l’exploitent dans la famille de registres AR816x/AR817x. Intel liste le produit parmi les Killer arrêtés. | Aucune fiche publique de programmation ni liste exhaustive d’errata spécifique à `1462:115a/10h` identifiée dans les sources consultées |
| Relevé PCI publié par un contributeur netdev en 2015 | Un E2400 de même révision sur un **autre** MSI (sous-système `7977`) annonce BAR0 mémoire 64 bits non préchargeable de 256 Kio, BAR2 E/S de 128 octets, INT A, MSI 64 bits masquable (16 possibles, 1 activé), MSI-X (16, désactivé). Lien PCIe x1 à 2,5 GT/s et FLR non annoncé sur cet exemplaire | Relire chaque valeur sur `115a`. Une capacité présente, un mode activé par Windows et une interruption reçue par NovaOS sont trois observations distinctes |

Le code de classe attendu est Ethernet `02/00`. Restreindre la première admission aux cinq valeurs d’identité du mandat, à la classe et au chemin PCI vérifié. Le Wi-Fi Intel AC 3165 demeure hors périmètre. [Linux, table PCI](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/main.c#L2024-L2040) · [FreeBSD, table des cartes](https://github.com/freebsd/freebsd-src/blob/release/14.3.0/sys/dev/alc/if_alc.c#L109-L137) · [Relevé physique du contributeur](https://lists.openwall.net/netdev/2015/09/08/36) · [Avis Intel sur l’E2400](https://www.intel.com/content/www/us/en/support/articles/000059296/wireless.html)

## Configuration PCI et fenêtre MMIO

Dans l’en-tête PCI de type 0, lire vendeur/périphérique à `00h`, commande/état à `04h`, révision/classe à `08h`, BAR à partir de `10h`, sous-système à `2Ch`, pointeur des capacités à `34h` et ligne/broche à `3Ch/3Dh`. Combiner les deux mots d’une BAR 64 bits. Utiliser les ressources allouées et borner chaque accès dans la fenêtre. Parcourir avec contrôle de taille, d’alignement et de cycle les capacités PM, PCIe, MSI (`05h`), MSI-X (`11h`) et, si présentes, les capacités étendues AER/ACS. Les offsets des capacités sont découverts à chaque démarrage. Interroger l’ACS aussi sur les ponts amont. La ligne PCI historique ne suffit pas à déterminer la route ACPI/APIC.

Repères **relatifs à BAR0** employés par les pilotes, en hexadécimal. Le courtier conserve la fenêtre en mémoire d’E/S non cachée, inaccessible aux processus. Aucun balayage par écritures, ni sondage de taille par réécriture des BAR pendant l’inventaire.

| Fonction | Offsets | Interprétation pour le premier profil |
|---|---|---|
| Origine de la MAC | `0204`, `0218`, `1488/148C` | Chargement interne flash/EEPROM, eFuse, puis registres d’adresse de station |
| Contrôle, horloge et état | `12F8`, `1400`, `1408`, `1410` | Gestion d’énergie, reset MAC/DMA, modération IRQ, activité des files et du MAC |
| PHY | `140C`, `1414`, `1448` | Contrôle PHY, accès MDIO et sélection d’un registre étendu |
| Trame et filtres | `1480`, `1490/1494`, `149C` | MAC RX/TX, hachage multicast, longueur maximale sur le contrôleur |
| Bases des anneaux | `1540/1544`, `1550`, `1568`, `1580` | Parties hautes RX/TX, parties basses des RFD, RRD et TPD de priorité 0 |
| Tailles et chargement | `1560/1564`, `1578`, `1584`, `1534` | Nombre de RFD, taille d’un tampon RX, nombres de RRD et TPD, prise en compte des pointeurs |
| Moteurs et index | `1590`, `15A0`, `15C0`, `15E0/15F8`, `15F2/15F6` | Files TX/RX, configuration DMA, index producteur/consommateur RFD et TPD0. Respecter les accès 16 bits des index en trafic, en particulier les index TX voisins |
| Interruptions et statistiques | `1600/1604`, depuis `1700` | Causes/masque IRQ, compteurs MIB accumulés par un lecteur unique car la lecture efface les compteurs concernés |

Valider sur le MSI le type et la taille de BAR0, l’état D0, la géométrie PCIe, les registres à effets de bord, la largeur d’accès et l’attribut mémoire avant tout usage actif. Les offsets de la table MSI-X et de ses bits en attente (PBA) se déduisent de la capacité, jamais des adresses du relevé tiers. [Registres Linux](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/reg.h) · [Registres FreeBSD](https://github.com/freebsd/freebsd-src/blob/release/14.3.0/sys/dev/alc/if_alcreg.h)

## DMA et représentation des trames

Une seule paire de files est proposée. Les descripteurs en mémoire sont en petit-boutiste, les adresses qu’ils contiennent sont des IOVA attribuées par VT-d. L’adresse MAC et les champs Ethernet gardent leur ordre réseau.

| Structure | Taille et sens du point de vue du NIC | Contrôle de progression |
|---|---|---|
| TPD, émission | 16 octets lus. Longueur, options/VLAN, fin de paquet, adresse 64 bits. Premier profil avec une seule zone par trame | Le logiciel publie le producteur après la copie et la barrière DMA. Le consommateur matériel indique les TPD traités. Vérifier l’avance modulo la taille dans la limite des entrées soumises. Cette complétion permet de rendre le tampon, sans attester la réception au pair |
| RFD, mise à disposition RX | 8 octets lus, une adresse 64 bits. La capacité commune des tampons est programmée séparément | Le producteur logiciel publie uniquement des emplacements libres adossés à un tampon. Garder une entrée de séparation entre plein et vide |
| RRD, retour RX | 16 octets écrits. Premier mot : index de départ et nombre de RFD. Dernier mot : mise à jour, erreurs et longueur incluant quatre octets de FCS | Après indication de mise à jour et barrière d’acquisition, exiger l’index attendu, exactement un RFD, une plage dans le tampon et un état cohérent. Rejeter les erreurs CRC/troncature. Pour ce profil, valider 64 à 1 518 octets FCS compris avant de soustraire quatre, copier puis libérer/reposter. Une incohérence de propriété suspend la file |

Les anneaux doivent avoir les bits hauts cohérents. `alx` les alloue ensemble dans une même fenêtre de 4 Gio. NovaOS réserverait des IOVA contiguës dans une seule fenêtre de 4 Gio, avec pages physiques distinctes par permission et vérification de l’adresse du dernier octet. Aucun besoin d’identité IOVA/adresse physique ni de réservation de 4 Gio de RAM. Les tampons de données disposent de leur adresse 64 bits propre. FreeBSD aligne les bases TPD/RFD/RRD selon leurs tailles respectives (16/8/16 octets) et les tampons RX sur 4 octets. Des bases d’anneaux alignées à 4 Kio satisfont ces contraintes. [Allocation et progression Linux](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/main.c) · [Formats de descripteurs](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/hw.h) · [Géométrie FreeBSD](https://github.com/freebsd/freebsd-src/blob/release/14.3.0/sys/dev/alc/if_alcvar.h)

## PHY, adresse permanente et microprogramme

- **PHY.** Sérialiser les échanges MDIO, attendre la fin avec une échéance et consigner un délai expiré. Lire les deux registres d’identité PHY. Les pilotes utilisent le contrôle MII usuel et des registres constructeur étendus. Un réveil à partir de l’hibernation peut demander une horloge MDIO plus lente.
- **Lien cuivre.** Lire les capacités, annoncer par autonégociation les modes autorisés en 10/100 et le 1000BASE-T duplex intégral. Dans le chemin `alx`, la double lecture de BMSR tient compte de l’état mémorisé. Le registre PHY `11h` établit la résolution vitesse/duplex. Appliquer le résultat au MAC, annoncer la perte de porteuse et marquer la vitesse inconnue pendant une renégociation. Vérifier aux deux bouts. Le premier profil écarte EEE, WoL et les déchargements matériels. Fixer explicitement la politique des trames PAUSE lors des mesures.
- **MAC.** `alx` lit `148C` pour les deux premiers octets, puis `1488` pour les quatre suivants dans l’ordre réseau. Si l’adresse est invalide, il commande un chargement depuis l’eFuse, puis depuis l’EEPROM/flash si déclarée présente, avec relecture à chaque étape. Ce sont des lectures de la mémoire non volatile via le moteur interne. Interdire sa programmation. Comparer à la MAC permanente Windows si exposée, distinguer la MAC administrative. Une MAC nulle, multicast ou inexpliquée bloque le profil physique en attendant arbitrage.
- **Firmware.** Les chemins d’attachement étudiés fonctionnent sans chargement d’un fichier de firmware par le système. La carte peut dépendre de données non volatiles, du paramétrage PHY et d’un état laissé par l’UEFI/PXE. Relever cet état à froid et après démarrage Windows. La présence d’une ROM optionnelle ne justifie pas son exécution par NovaOS. [Séquences et PHY Linux](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/hw.c) · [Attachement FreeBSD](https://github.com/freebsd/freebsd-src/blob/release/14.3.0/sys/dev/alc/if_alc.c)

## Révision 10h, interruptions et errata

| Observation vérifiable | Action de conception ou preuve manquante |
|---|---|
| Le calcul de `alx` utilise `10h >> 3 = 2` et sélectionne la branche nommée B0. Le pilote y applique notamment des ajustements PHY suivant vitesse et estimation de longueur de câble. Le correctif additionnel `lnk_patch` cible un AR8161, sous-système `1969:0091`, révision zéro | B0 désigne ici le classement logiciel. Exiger l’identité PHY et des essais à plusieurs vitesses et longueurs. Écarter le réglage réservé à cet autre sous-système |
| `alx` associe l’E2400 au défaut MSI/INTx Disable. Le bit de désactivation INTx dans PCI Command peut empêcher l’émission de MSI. FreeBSD documente aussi cette interaction | Vérifier séparément commande PCI, routage de la broche, MSI effectivement reçue et acquittée et absence d’IRQ double. Conserver le contrôle privilégié de toutes les routes |
| Linux essaie MSI-X puis MSI/INTx. FreeBSD désactive MSI-X par défaut pour la famille Killer après des signalements. Le relevé tiers constate MSI activé et MSI-X désactivé | Proposer **un vecteur MSI remappé** en première qualification si la route et l’identité source sont vérifiables. INTx est un profil distinct soumis à preuve de routage et de médiation. Garder MSI-X expérimental jusqu’à mesure sur `115a`. Un mode non démontré reste fermé |
| `alx` évite un début de tampon RX à l’offset `FC0h` dans une page de 4 Kio en raison d’un risque de débordement RFD. L’allocation des anneaux tient aussi compte des bits hauts communs | Choisir les offsets IOVA `000h` et `800h` pour des tampons de 2 Kio, vérifier le dernier octet, la direction et l’alignement après traduction |
| Le chemin de reset `alx` suit la remise à zéro du producteur RFD et la fin du reset MAC/DMA. Il traite séparément des particularités d’énergie de révision A | Borner l’attente des témoins de fin. Relever toute divergence sur la cible. L’arrêt du MAC à lui seul n’établit pas que les écritures PCIe ont atteint la mémoire |

La sensibilité exacte à ASPM, à la longueur du câble, à la modération d’IRQ et au retour de D3 reste à mesurer. Les registres spécialisés et les constantes analogiques exigent une provenance traçable avant implémentation. [Linux, initialisation et IRQ](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/main.c) · [Linux, reset et réglages PHY](https://github.com/torvalds/linux/blob/v6.12/drivers/net/ethernet/atheros/alx/hw.c) · [FreeBSD, choix MSI-X sur Killer](https://github.com/freebsd/freebsd-src/blob/release/14.3.0/sys/dev/alc/if_alc.c#L1415-L1435)

## Initialisation, arrêt et reprise proposés

1. Après un transfert UEFI protégé, inventorier la fonction, le groupe isolable, les ressources et le domaine existant. La découverte est autorisée sans bail. Une origine DMA héritée incertaine ferme la voie active.
2. Sous le contrôle du superviseur, masquer les interruptions et maintenir Bus Master désarmé. Vérifier D0 et la fenêtre MMIO. Arrêter les files et le MAC, observer l’inactivité des deux files et des deux moteurs puis effectuer le reset MAC/DMA avec échéance. Traiter les besoins du PHY séparément. Enregistrer identité, MAC et état du lien.
3. Préparer le domaine dédié, les permissions, la route IRQ et les descripteurs privés. Renseigner les bases, tailles et index, charger les pointeurs, préposter RX, vérifier la configuration accessible en lecture. Interdire toute activation si une invalidation, l’isolation du groupe ou le contrôle de route est incertain.
4. Une fois le domaine actif et contrôlé, autoriser Bus Master, démarrer les files et le MAC selon le lien résolu, puis armer la notification. Recouper les premières complétions avec le pair. La perte de lien invalide la vitesse et suspend les nouveaux TX selon le contrat.
5. À l’arrêt, geler les soumissions et la génération, masquer et synchroniser les IRQ. Laisser finir les opérations connues sous confinement avec délai, arrêter files et MAC, constater les témoins de repos, désarmer et relire Bus Master. Purger le travail et terminer la révocation VT-d. Une erreur entraîne la quarantaine détaillée dans l’architecture. Un reset PCI de pont pourrait toucher d’autres fonctions et sort du profil.
6. Après retrait confirmé, reprendre avec nouvelle incarnation, génération et anneaux réinitialisés. Un TX d’issue inconnue est signalé comme tel. La récupération après quarantaine exige une preuve indépendante de fin des accès avant réemploi des pages.

## Vérifications de phase 0

Contrôle documentaire par lecture des deux branches avec `git rev-parse` et `git show`, recoupement des chemins d’initialisation Linux/FreeBSD, résolution des liens locaux et des objets Git VirtIO-net, recalcul des tailles et des plafonds de débit. La qualification Windows et les essais de démarrage sont réservés au MSI. Aucun essai invité sur x86_64 ou AArch64, ni essai Darwin/Rosetta : le changement porte uniquement sur trois documents et le mandat exclut les campagnes QEMU.

## Sources et licences

| Sources primaires consultées | Utilisation dans ce dossier et régime |
|---|---|
| Linux v6.12, Johannes Berg et Qualcomm Atheros. [Répertoire alx](https://github.com/torvalds/linux/tree/v6.12/drivers/net/ethernet/atheros/alx) | `main.c`, `hw.c`, `hw.h`, `reg.h` et `alx.h` portent une notice GPL-2.0-or-later et une notice permissive Qualcomm de type ISC pour des travaux incorporés. Synthèse factuelle. Aucun extrait, tableau de constantes C ou traduction de routine intégré à NovaOS |
| FreeBSD 14.3-RELEASE, Pyun YongHyeon. [Répertoire alc](https://github.com/freebsd/freebsd-src/tree/release/14.3.0/sys/dev/alc) | `if_alc.c`, `if_alcreg.h`, `if_alcvar.h` portent BSD-2-Clause. Recoupement indépendant. Une éventuelle reprise de code imposerait la conservation de ses notices |
| Publication netdev du contributeur et fiche d’assistance Intel, liées plus haut | Observation publiée et documentation constructeur. Aucune licence de réutilisation de code revendiquée. Valeurs factuelles et liens |
| [Intel, fiche du processeur](https://www.intel.com/content/www/us/en/products/sku/88959/intel-core-i56300hq-processor-6m-cache-up-to-3-20-ghz/specifications.html) et [spécification VT-d](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf) | Documentation technique Intel sous notices de l’éditeur. Référence pour le processeur, DMAR, la traduction des DMA, les invalidations et le remappage IRQ |
| [UEFI Forum, Boot Manager](https://uefi.org/specs/UEFI/2.10_A/03_Boot_Manager.html), [Secure Boot](https://uefi.org/specs/UEFI/2.10_A/32_Secure_Boot_and_Driver_Signing.html) et [conditions d’accès](https://uefi.org/specs/access) | Spécification 2.10 Errata A, © UEFI Forum. Consultation documentaire selon les conditions du Forum. Aucun texte normatif ou outil redistribué |
| Microsoft Learn et outils ACPICA référencés dans le [protocole Windows](killer-e2400-qualification-msi.md) | Commandes de collecte rédigées pour le mandat. Les pages Microsoft relèvent des [conditions de Microsoft Learn](https://learn.microsoft.com/en-us/legal/termsofuse) et des licences spécifiques signalées. Le paquet binaire ACPICA Windows d’Intel relève de sa [licence de distribution](https://www.intel.com/content/www/us/en/developer/articles/license/acpica-licensing.html). Aucun binaire redistribué |
| [NovaOS](../LICENSE) et publication VirtIO-net épinglée dans l’[architecture](killer-e2400-architecture.md) | MIT pour le code NovaOS étudié. Les modèles et firmwares tiers du laboratoire ne sont ni importés ni requis pour le MSI |

Pour la phase suivante, joindre à chaque hypothèse de registre sa source, sa portée de révision et son observation indépendante. Concevoir les structures Rust et leurs tests depuis ce contrat et les preuves admissibles, sans porter ligne à ligne un pilote tiers.
