# NovaOS v0.25.0 — Phase 5N — Image Live USB x86_64

**L’image générique démarre sur USB sous QEMU Q35. L’EFI 5M rend le refus 16 lorsque le firmware USB ordinaire ne lui transmet pas son reçu de confiance.** Le terminal du banc affiche ce verdict. Après arrêt de la VM, le dépositaire peut le conserver dans NOVALOGS. Les huit premiers journaux de la série ont été relus sous macOS, Windows PE et Linux. A8 reste fermé. [Image compressée](validation/diagnostic-uefi-5n/novaos-x86_64.img.gz) · [Reproduction, lecture et procédure dd](../boot/diagnostic-5n/README.md)

## Distribution et capacité

| Élément | Résultat |
|---|---|
| Base 5M | `d3d5bd1066c72001c4d11f4c4721569b4c218232` |
| Image brute | 71 303 168 octets, SHA-256 `c69df9dc41cf5cfff7bf0fb81765f3287111656ad6c8575bdb2db8d94d2e9c92` |
| Image gzip | 204 179 octets, SHA-256 `d3ad87c75cbe58b60502cc14680793602d99aa51045c59656e6c46d4cd6b0b4b` |
| NOVAOS | ESP FAT32 de 33 Mio, à 1 Mio. Chemin amovible x86_64 `EFI/BOOT/BOOTX64.EFI`. Les licences accompagnent le PE sur l’ESP. |
| NOVALOGS | FAT32 de données de 33 Mio, à 34 Mio. Huit répertoires préalloués, trois fichiers de données par répertoire. |
| Géométrie | Secteurs logiques et clusters de 512 octets, deux FAT par volume, 66 512 clusters de données par partition. La taille entière immédiatement inférieure de 32 Mio ne permet pas 65 525 clusters après déduction des FAT. Le reste porte l’alignement et les copies de la GPT. |
| Identité | GUID de format et numéros de volume synthétiques communs aux copies. L’autorité d’écriture repose sur le fichier détenu et l’origine USB observée. |

La même suite d’octets a été écrite avec `dd` dans deux supports virtuels neufs de 68 et 96 Mio. Le préfixe complet a été relu. OVMF démarre les deux. Sur la copie de 96 Mio, la GPT secondaire se trouve encore au dernier secteur de l’image de 68 Mio. Le banc refuse **toute écriture** sur cette géométrie. Une génération à la capacité exacte de 96 Mio place la GPT secondaire au dernier secteur et permet l’export. Le PE, les deux partitions et leur contenu initial sont identiques quelle que soit cette capacité. Les octets propres à la géométrie GPT et au MBR varient. La génération accepte aussi une capacité exacte exprimée en secteurs. Les secteurs physiques ou logiques de 4 Kio et les médias réels restent hors qualification. [Disposition GPT UEFI](https://uefi.org/specs/UEFI/2.10/05_GUID_Partition_Table_Format.html)

## Autorité et accès

L’arbitrage A7 de 5N accorde l’export exclusivement au banc après l’arrêt de la VM et l’attribution du démarrage. Le superviseur crée un fichier régulier neuf dans son répertoire privé, le verrouille et conserve le descripteur d’origine. Il transmet à QEMU un deuxième descripteur ouvert en lecture seule, contrôlé par identité de fichier. Le modèle USB reçoit ce descripteur par fdset. Le moniteur recoupe le port de la ligne BDS, le périphérique QMP, le nombre de supports, les lectures USB, le premier RIP et les cinq sections relocalisées du PE. Le témoin SATA est distinct. La présence de deux copies USB identiques ferme la porte d’export, même si le firmware sait en lancer une.

Au HLT, le banc lit le résultat auto-adressé du PE et relève de nouveau les compteurs. Il termine QEMU, attend la fin effective du processus et vérifie le contenu du SATA fictif. Sur le fichier toujours détenu, il vérifie la propriété, le nombre de liens, les deux GPT, le contenu immuable de NOVAOS, les deux FAT de NOVALOGS et les anciens rapports. L’exporteur déduit lui-même les trois plages autorisées. Il laisse les métadonnées FAT, les répertoires et les autres emplacements intacts. La reconstruction indépendante des SHA des huit **disques complets** avant et après écriture vérifie cette dernière propriété. Le modèle suppose le dépositaire hôte de confiance et l’exclusion des écrivains étrangers. [Exporteur](../boot/diagnostic-5n/journaux.py) · [Contre-lecture](../boot/diagnostic-5n/verifier5n.py)

L’EFI utilise son chemin 5M existant. Sur USB, il s’arrête avant tout accès de système de fichiers par le diagnostic. Le banc lit l’inventaire de classes PCI et présente explicitement sa provenance QMP. Le rapport sépare le mode Secure Boot du firmware de laboratoire et l’état de confiance **indéterminé** rendu par le PE. La phase 5M a établi la protection DMA avant chargement sur son disque RAM avec autorité externe. Le profil USB xHCI de cette phase ne possède pas ce transfert. Le refus ne fournit aucune preuve rétroactive de protection des pages. [Qualification DMA et confiance sur le chemin RAM](diagnostic-uefi-5m.md)

## Publication d’un journal

Chaque démarrage mesuré a un identifiant aléatoire de rapport. Le schéma autorise uniquement le verdict, la raison, les deux états de confiance, les classes PCI, la provenance, la date et les résultats de sécurité. Les dates de la série valent `null`. L’option d’horloge attestée utilise l’UTC du banc au départ de la VM. Le numéro de série, les noms d’hôte ou de personne, les adresses de RAM, les clés et l’époque cryptographique sont exclus du schéma.

| Fichier d’un répertoire BOOT0001 à BOOT0008 | Utilisation |
|---|---|
| `rapport.json` | 2 048 octets. JSON UTF-8, complété par des espaces et un saut de ligne. |
| `JOURNAL.TXT` | 1 024 octets. Verdict lisible à l’écran du terminal du banc et sur le support. |
| `VALIDES.SHA` | 512 octets. Les deux premières lignes donnent les SHA-256 des fichiers complets. |

Les fichiers préalloués sont initialement nuls. Le banc écrit et synchronise le JSON puis le texte, avec relecture à chaque étape. Il publie le sceau en dernier. Une coupure avant le sceau rend l’emplacement indisponible. Une coupure au milieu du sceau entraîne le refus des écritures suivantes. Un emplacement validé ne sert jamais de nouveau. Huit emplacements consommés imposent la conservation du média en lecture seule pour ce protocole. La validité est déterminée au contenu du sceau, indépendamment de la date FAT. La détection par SHA-256 couvre l’intégrité accidentelle. Une réécriture hostile coordonnée des trois fichiers hors de la garde du banc demanderait une authentification durable distincte. Les essais de coupure portent sur un fichier ordinaire et la barrière de synchronisation de l’hôte. Les caches et le contrôleur flash d’une vraie clé restent à qualifier.

## Matrice d’acceptation

| Contrôle | Observation | Portée |
|---|---|---|
| Génération | Deux constructions identiques, GPT primaire et secondaire, deux FAT et FSInfo vérifiés par décodage indépendant. `fsck_msdos -n` a terminé avec code 0 sur les deux partitions extraites. | Fichier image. |
| Écriture avec dd | Copie intégrale relue à 68 et 96 Mio. Refus de l’export si la GPT de secours ne termine pas le média réel. | Fichiers réguliers du banc. |
| Lancement | 20 démarrages USB mesurés, auxquels s’ajoutent les calibrations exclues du compte. Les cinq sections du PE correspondent au premier RIP. Verdict 16 et zéro allocation vivante dans les deux modes. | QEMU 11.1.3, Q35, TCG, xHCI, SATA synthétique, OVMF épinglé. |
| Contrôle de Secure Boot | Le même PE témoin hors db est refusé par le firmware actif et atteint le PE en mode inactif. Le diagnostic refuse alors le transfert absent. | Comparaison de deux copies de magasins 5M. |
| E/S et partitions | Dans les 20 démarrages mesurés, zéro opération interne comptée depuis le reset, zéro commande ATA/AHCI tracée du premier RIP au HLT et SATA intact. L’USB est livré en lecture seule à QEMU. | Le traçage ATA par commande commence à l’entrée EFI. Les compteurs bloc depuis le reset ne couvrent pas toutes les commandes firmware sans données. |
| Refus du PE avant l’entrée | Après rejet du témoin hors db, OVMF a lu **40 fois, 32 768 octets** sur le SATA synthétique. Zéro écriture interne et aucun démarrage de remplacement observé sur ce disque témoin. | Contre-exemple du firmware. Le diagnostic n’a reçu le contrôle dans aucun de ces accès. |
| Persistance | Huit rapports successifs relus. Neuvième export refusé pour saturation. Refus de l’ancien rapport altéré, du FAT corrompu, de NOVALOGS absente et des deux clés ambiguës. Octets complets identiques avant/après chaque refus sans injection. | Export par le dépositaire après arrêt. |
| Coupures | Données seules dans BOOT0001, reprise dans BOOT0002, sceau déchiré dans BOOT0003 puis refus sans mutation. | Quatre démarrages à froid, avec coupure de l’exporteur simulée à deux points précis. |
| Lecteurs | macOS natif et Linux Alpine ARM64 ont recalculé les 24 empreintes. Windows PE ARM64 a comparé les 24 fichiers octet par octet avec un CD de référence indépendant du chemin FAT. | Médias virtuels imposés en lecture seule. |
| Invariants | 9 tests 5N sur Darwin ARM64 et 9 sous Rosetta. Contrôle de l’ABI canonique à 48 sélecteurs. Preuves 5I à 5M conservées. Les deux relectures 5M retrouvent 21 décisions, 16 transitions EFI et 35 EOI. | Relecture des archives historiques, sans étendre leurs garanties au démarrage USB. |
| Affichage | Le terminal du dépositaire affiche le résultat extrait de la RAM. La capture Windows montre le verdict de sa propre contre-lecture. | Affichage autonome du premier EFI sur l’écran physique encore ouvert. |

[Observations par démarrage](validation/diagnostic-uefi-5n/captures/observations.json) · [Contre-épreuve hors db](validation/diagnostic-uefi-5n/modes.json) · [Huit rapports bruts](validation/diagnostic-uefi-5n/captures/rapports.zip) · [Empreintes des preuves](validation/diagnostic-uefi-5n/SHA256SUMS)

## Portes avant A8

| Porte | Condition manquante |
|---|---|
| Origine et confiance | Une autorité disponible avant chargement doit lier l’image générique, son origine amovible, l’époque et le verdict Secure Boot à des preuves vérifiables. En mode actif, l’autorité admise doit appartenir à la politique du propriétaire. |
| RAM et DMA | Démontrer l’exclusion des pages NovaOS et de l’affichage depuis avant chargement, y compris xHCI, alias, transactions en vol et domaines du stockage. Préserver la garde réseau/stockage. |
| Firmware et refus | Qualifier l’absence d’écriture ou d’amorçage interne imprévu, et faire accepter explicitement les lectures préalables propres au profil. Zéro E/S interne à partir du premier EFI demeure obligatoire. |
| Verdict visible | Qualifier un écran local dont le framebuffer, les limites, le format, l’origine et la propriété sont établis par l’autorité précoce. Prévoir une restitution du refus par cette autorité si l’EFI ne peut disposer d’une sortie sûre. Aucun besoin de disque ni de réseau pour ce verdict. |
| Persistance native | Mandat et arbitrage distincts pour établir la propriété exclusive de l’USB amovible, l’unicité du chemin, la portée des commandes et leur drainage. Éprouver les coupures et le comportement des caches du média retenu. |
| Décision physique | Maintenir A8 fermé jusqu’à validation des portes applicables par la MOE. L’autorisation de l’exporteur QEMU se termine au fichier régulier du banc. |
