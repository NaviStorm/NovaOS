# Fiche opérateur Windows — MSI-DESKTOP

**Autorisation actuelle A8**. L’opérateur présent devant le MSI peut interroger Windows. Ce dossier ne prépare aucune clé et ne demande aucun redémarrage. Les données brutes restent sur la machine. Utiliser la console locale hors transcription, partage d’écran et journalisation distante. Si la politique locale impose une transmission, reporter la collecte.

## Collecter

1. Examiner les deux sources. Utiliser Windows PowerShell 64 bits avec les droits de lecture des modules PnP, BitLocker et SecureBoot. Garder la politique d’exécution existante. Depuis le dossier livré, lancer la commande ci-dessous. Si Windows refuse son exécution, consigner le refus. Le script affiche un JSON privé sur la console. Il vérifie la présence d’une unique fonction portant l’identifiant complet attendu et une classe Ethernet.
2. Pour le complément ACPI, utiliser le [collecteur Windows PowerShell autonome de phase 5C.1](acpi-5c1.md). Il ne nécessite aucune installation. Le script Python initial reste conservé pour la provenance de 5C.
3. Dans `msinfo32`, relever manuellement le mode BIOS, l’état du démarrage sécurisé et la protection DMA du noyau Windows. Relever la date et la version de Windows. Une erreur d’accès reste une observation manquante. Pour conserver les résultats, choisir au préalable un dossier privé local non synchronisé, à accès restreint. Utiliser des noms libres et `Out-File -NoClobber -Encoding utf8` en sortie de la commande. Ne joindre au dépôt ni JSON, ni capture, ni identifiant, ni table brute.

```powershell
.\collecter.ps1
powershell.exe -NoProfile -NonInteractive -File .\acpi-5c1.ps1
```

## Exploiter localement

| Champ | Vérification à faire |
|---|---|
| `cible`, `HardwareIds`, `CompatibleIds` | Confirmer PCI 1969:e0a1, sous-système 1462:115a, révision 10h et classe 02/00. Distinguer le chemin d’instance et l’identité du modèle |
| `arbre`, `BusNumber`, `Address`, `LocationPaths`, `parent` | Remonter chaque parent jusqu’à la racine, consigner les cycles, parents absents ou multiples. Pour une fonction PCI, décoder l’adresse Windows en numéro de périphérique dans le mot haut et de fonction dans le mot bas. Contrôler la liste calculée `soeurs_visibles`. Une liste vide ne couvre que les fonctions PnP visibles et renseignées. Rechercher les autres fonctions sous le même pont, sur le même bus au même numéro de périphérique. Recouper le chemin pour distinguer les racines. Un segment indéterminé reste inconnu |
| `arbre`, `disques` | Repérer SCSIAdapter, HDC, fonctions PCI de classe 01 et disques Windows. Joindre le numéro de disque à `disques_pnp.Index`, puis `PNPDeviceID` au nœud d’arbre. Remonter aussi les périphériques intermédiaires RAID/VMD, USB ou virtuels. Identifier le contrôleur des disques système et de démarrage et les ancêtres communs avec le NIC. Une association de disque ambiguë interdit de conclure à l’indépendance |
| `bios`, `carte`, `pilote` | Consigner modèle, version et date firmware, pilote lié, fournisseur, INF et statut signé. Compléter les ressources PnP de la cible avec les seules requêtes d’énumération de la fiche 5B |
| `secureBoot`, `variables`, `bitlocker` | Recouper SecureBoot=1, SetupMode=0 et état de protection des volumes. Les empreintes PK/KEK/db/dbx servent à détecter un changement entre relevés. Elles ne constituent pas une preuve de confiance dans un chargeur spécifique. Ne relever aucun secret de récupération. [Variables Secure Boot](https://learn.microsoft.com/en-us/powershell/module/secureboot/get-securebootuefi) |
| DMAR, MCFG, APIC | Conserver les octets validés localement. Faire examiner DRHD et scopes, unités par segment, INCLUDE_ALL, RMRR, ATSR éventuels et remappage IRQ. Rapprocher chaque chemin du matériel présent, y compris fonctions sœurs et stockage. Les tables décrivent le firmware. Leurs octets ne prouvent ni l’état actif de VT-d, ni la liste complète des alias, ni le drainage DMA |

La synthèse partageable contient uniquement, par rubrique, « confirmé », « indéterminé » ou « divergent », la version des outils et la raison technique expurgée. Garder les valeurs détaillées pour l’arbitrage local. En cas d’écart avec l’identité historique, suspendre la préparation ciblée et demander une nouvelle identification.
