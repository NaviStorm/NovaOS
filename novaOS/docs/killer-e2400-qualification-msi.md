# Killer E2400 — inventaire Windows et qualification physique du MSI

**État au 26 novembre 2026.** Le matériel n’a pas été accessible pendant la phase 0. Les commandes ci-dessous sont une collecte à exécuter sur MSI-DESKTOP. Le protocole décrit des essais futurs, à autoriser par phase. Aucune campagne QEMU, compilation, installation, modification du firmware ou sollicitation du réseau du MSI n’a été effectuée pour ce dossier.

## 1. Établir l’inventaire Windows

Ouvrir **Windows PowerShell 64 bits en administrateur** sur MSI-DESKTOP. Les blocs affichent les résultats dans la console. Conserver le dossier de preuve sur un support externe si une copie est souhaitée. Ces commandes ne changent aucun réglage de carte, de pilote ou de démarrage. Les identifiants d’instance complets et la MAC restent dans l’inventaire privé du MSI.

### Carte, firmware et état de Secure Boot

```powershell
Get-CimInstance Win32_ComputerSystem |
    Select-Object Manufacturer, Model, TotalPhysicalMemory
Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer, Product, Version
Get-CimInstance Win32_BIOS | Select-Object Manufacturer, SMBIOSBIOSVersion, ReleaseDate
Get-CimInstance Win32_Processor | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber
Confirm-SecureBootUEFI
'SecureBoot', 'SetupMode' | ForEach-Object {
    $variable = Get-SecureBootUEFI -Name $_ -ErrorAction Stop
    [pscustomobject]@{ Nom = $_; Valeur = $variable.Bytes[0] }
}
```

Attendu avant un essai : `True`, `SecureBoot=1` et `SetupMode=0`. Une cmdlet indisponible ou un accès refusé est une observation non concluante. Identifier le jeu de bases actif en conservant séparément les octets complets de `db` et `dbx` si nécessaire à l’analyse locale. Pour lire les octets complets en console, utiliser par exemple `[Convert]::ToBase64String((Get-SecureBootUEFI -Name db).Bytes)` et répéter pour `dbx`. Le calcul suivant donne un identifiant reproductible de chaque base. À lui seul, son résultat ne valide aucun exécutable.

```powershell
'db', 'dbx' | ForEach-Object {
    $variable = Get-SecureBootUEFI -Name $_ -ErrorAction Stop
    $algorithme = [System.Security.Cryptography.SHA256]::Create()
    try {
        $empreinte = [BitConverter]::ToString($algorithme.ComputeHash($variable.Bytes))
        [pscustomobject]@{ Nom = $_; Octets = $variable.Bytes.Length; SHA256 = $empreinte.Replace('-', '') }
    } finally {
        $algorithme.Dispose()
    }
}
```

Dans `msinfo32.exe` (Résumé système), relever **Mode BIOS**, **État du démarrage sécurisé** et **Protection DMA du noyau** avec leur valeur exacte ou « non indiqué ». Ce dernier champ décrit la politique de Windows. La protection du requester E2400 avant et après transfert à NovaOS doit être démontrée indépendamment. [Confirm-SecureBootUEFI](https://learn.microsoft.com/en-us/powershell/module/secureboot/confirm-securebootuefi) · [Get-SecureBootUEFI](https://learn.microsoft.com/en-us/powershell/module/secureboot/get-securebootuefi) · [Protection DMA dans Windows](https://learn.microsoft.com/en-us/windows/security/hardware-security/kernel-dma-protection-for-thunderbolt)

### Instance exacte, chemin PCI, ressources et pilote

Exécuter les blocs suivants dans **la même session**. La sélection se fait sur l’ID matériel complet publié par PnP. Elle reste valable si Windows a renommé l’interface « Ethernet ».

```powershell
$materielAttendu = 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10'
$cartes = @(Get-PnpDevice -PresentOnly -Class Net | Where-Object {
    if ($_.InstanceId -like 'PCI\VEN_1969&DEV_E0A1*') {
        $ids = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop
        $ids.Data -contains $materielAttendu
    }
})
if ($cartes.Count -ne 1) { throw "Nombre de Killer E2400 correspondant au mandat : $($cartes.Count)" }
$instance = $cartes[0].InstanceId
$cartes[0] | Format-List Status, Class, FriendlyName, InstanceId

$champs = 'DEVPKEY_Device_HardwareIds', 'DEVPKEY_Device_CompatibleIds',
          'DEVPKEY_Device_LocationPaths', 'DEVPKEY_Device_LocationInfo',
          'DEVPKEY_Device_BusNumber', 'DEVPKEY_Device_Address', 'DEVPKEY_Device_Parent'
Get-PnpDeviceProperty -InstanceId $instance -KeyName $champs |
    Format-List KeyName, Type, Data
$parentPCI = (Get-PnpDeviceProperty -InstanceId $instance -KeyName 'DEVPKEY_Device_Parent').Data
if ($parentPCI) { Get-PnpDevice -InstanceId $parentPCI | Format-List FriendlyName, InstanceId }
Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceID -eq $instance } |
    Format-List DeviceName, DriverProviderName, DriverVersion, DriverDate, InfName, IsSigned, Signer
```

Sous Windows 11 22H2 ou ultérieur, `/resources` affiche les ressources attribuées par Windows (plages mémoire, lignes ou messages selon disponibilité). Sur les versions antérieures, consulter l’aide locale et l’onglet **Ressources** de la fonction dans le Gestionnaire de périphériques.

```powershell
pnputil.exe /?
pnputil.exe /enum-devices /instanceid $instance /resources /properties /drivers
```

Lire le segment/bus/périphérique/fonction, le chemin via les ponts et les plages allouées. Une IRQ Windows ou une plage mémoire n’indique pas à elle seule la capacité complète du silicium. Les valeurs absentes restent à relever dans l’inventaire PCI de la sonde signée. [Propriétés PnP](https://learn.microsoft.com/en-us/powershell/module/pnpdevice/get-pnpdeviceproperty) · [Disponibilité des options PnPUtil](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax)

### Configuration Ethernet, MAC, lien et compteurs de référence

```powershell
$interfaces = @(Get-NetAdapter -IncludeHidden | Where-Object { $_.PnPDeviceID -eq $instance })
if ($interfaces.Count -ne 1) { throw "Interface NDIS non identifiée de façon unique" }
$nic = $interfaces[0]
$nic | Format-List Name, InterfaceDescription, InterfaceIndex, Status, MacAddress, PermanentAddress,
                   MediaConnectState, MediaDuplexState, ReceiveLinkSpeed, TransmitLinkSpeed, LinkSpeed
Get-NetAdapterHardwareInfo -Name $nic.Name -IncludeHidden | Format-List *
Get-NetAdapterAdvancedProperty -Name $nic.Name -AllProperties |
    Format-Table RegistryKeyword, DisplayName, DisplayValue -AutoSize
Get-NetAdapterStatistics -Name $nic.Name | Format-List *
```

Noter le modèle du pair ou du commutateur, son port, la longueur/catégorie du câble, l’état connecté et la vitesse lue aux **deux extrémités**. Relever à nouveau les compteurs après le trafic de référence si une mesure Windows distincte est autorisée. Une option « Speed & Duplex » décrit la configuration, tandis que les vitesses RX/TX et le statut du pair décrivent le résultat négocié. Une MAC permanente vide, des propriétés MSI absentes ou un pilote NDIS inactif restent des informations manquantes. [Données NDIS](https://learn.microsoft.com/en-us/windows/win32/fwp/wmi/netadaptercimprov/msft-netadapter) · [Ressources matérielles NDIS](https://learn.microsoft.com/en-us/powershell/module/netadapter/get-netadapterhardwareinfo) · [Statistiques](https://learn.microsoft.com/en-us/powershell/module/netadapter/get-netadapterstatistics)

### Tables ACPI et limites de la collecte

Si l’exécutable Windows officiel `acpidump.exe` d’ACPICA est déjà disponible dans le chemin, les commandes ci-dessous affichent uniquement les tables nommées. Conserver leurs octets, révision, identifiant OEM, longueur, somme de contrôle et provenance. En cas d’outil indisponible, laisser ce relevé au diagnostic EFI signé.

```powershell
acpidump.exe -n DMAR
acpidump.exe -n MCFG
acpidump.exe -n APIC
```

DMAR permet d’étudier les unités DRHD, les portées PCI, les réservations RMRR et les indicateurs d’interruptions. MCFG aide à identifier les segments ECAM et APIC les contrôleurs d’interruptions. Il faut encore résoudre les routes ACPI, notamment `_PRT` pour INTx, et lire les capacités **effectives** du silicium. Les cmdlets Windows ne démontrent ni les droits de chaque page VT-d, ni les transactions DMA en vol, ni l’origine NVM exacte de la MAC, ni les registres PHY privés. Ne pas installer de pilote de lecture PCI ou de mémoire arbitraire pour combler ces lacunes. [Outils Windows ACPICA](https://www.intel.com/content/www/us/en/developer/topic-technology/open/acpica/download.html) · [Option de sélection par signature](https://github.com/open-acpica/acpica/blob/master/source/tools/acpidump/apmain.c)

## 2. Parcours d’admission sur matériel physique

Chaque étape ci-dessous correspond à un futur profil identifié. Un refus de sécurité est un résultat à consigner. Il ne vaut pas qualification du pilote.

| Étape | Opérations autorisées dans cette étape future | Preuve de passage ou motif d’arrêt |
|---|---|---|
| A — Référence Windows | Inventaire ci-dessus, machine/modèle/BIOS exacts. Identifier le support USB externe et l’empreinte de son image préparée sur une autre machine. Préparer un pair dédié et une capture hors du MSI | Identité PCI exacte, Secure Boot actif, câblage et équipements connus. Faire l’inventaire des informations absentes |
| B — Démarrage signé sans réseau | Utiliser le menu de démarrage ponctuel du firmware OEM et un support amovible protégé en écriture si possible, avec `\EFI\BOOT\BOOTX64.EFI`. Le témoin et le noyau s’exécutent en RAM. Aucun installateur, montage des disques Windows, pilote bloc actif pour ces disques, ajout d’entrée permanente, écriture de clé ou changement d’ordre de démarrage | Documenter le signataire et le chemin de confiance accepté par la `db` de **cette** machine, sa compatibilité avec `dbx`, puis l’authentification du noyau, de la configuration, du pilote et des modules. Une première image EFI admise qui charge des octets non vérifiés échoue. Vérifier le rejet d’un témoin modifié sur un autre support externe. Sans chaîne admise, arrêter avant le lancement |
| C — Inventaire EFI/NovaOS passif | Depuis le support signé, inventorier le contrôleur sans activer le DMA réseau. Lire identité, classe, ressources, liste de capacités, état PM, BME (Bus Master Enable), MSI/MSI-X, ponts, DMAR et état VT-d aux points de transfert. Relever les autres maîtres, dont le contrôleur USB utilisé au démarrage | Rapprocher les captures avant/sortie des services UEFI et à l’entrée du noyau, leur horodatage et le comportement documenté du firmware. Vérifier le cas PXE/UNDI s’il était propriétaire. Exiger la fin de ses opérations et un passage de propriété sans fenêtre DMA non contrôlée. Un BME à zéro observé tardivement, la seule table DMAR ou l’état Windows ne comblent pas un intervalle inconnu |
| D — Admission VT-d, maître fermé | Vérifier segment, chemin jusqu’au bon DRHD (ou portée globale applicable), SID/alias, P2P/ACS ou isolation équivalente, RMRR, réservations, plage d’adresses, traduction et remappage IRQ actifs. Préserver la protection USB, stockage et iGPU. Réserver des cadres fictifs disjoints et une route IRQ, construire et relire les cartes avec BME réseau désarmé | Preuve écrite du groupe isolable, des droits directionnels, des seules IOVA prévues, de la route, de l’invalidation synchrone et du détachement à vide. Refuser tout domaine en bypass, obstacle firmware, réservation qui impose de partager des pages sensibles ou régime d’IRQ non confinable |
| E — PHY et premier échange | Sur câble direct vers le pair isolé, sans pont vers un LAN, comparer MAC et identité PHY. Négocier 10, 100 et 1 000 Mbit/s en limitant les annonces du **pair** lorsque possible. Après D seulement, ouvrir un bail de test : un TX vers le pair, puis un RX identifié vers le MSI. Une trame L2 expérimentale identifiée par séquence suffit (par exemple EtherType `88B5h`) | État et duplex concordants, octets capturés identiques hors FCS, compteurs de complétion et d’IRQ corrélés aux événements. Vérifier chaque mode IRQ séparément, consigner MSI-X non essayé le cas échéant. Si la négociation ou l’IRQ échoue, fermer le profil et conserver le diagnostic |
| F — Fonctionnement et retrait | Trames courtes et maximales, bourrage nul, RX/TX indépendants puis simultanés. Au moins 100 000 trames par direction pour éprouver plusieurs tours d’anneau. Saturer les crédits, mettre la file en pression, couper/rebrancher le câble, comparer modes et tailles, exercer fermeture et relance en cours de trafic | Séquences et octets comparés au pair, progression après réarmement, pertes expliquées à saturation, zéro perte inexpliquée au débit qualifié. Au moins dix cycles nominaux avec retour aux compteurs de référence et aucune réutilisation de ticket périmé |
| G — Pannes et performances | Tester la mort du pilote à trois frontières (avant publication, transfert en cours, après achèvement avant livraison). Tester le timeout du courtier et les deux issues de retrait sur données fictives. Vérifier les pages sentinelles hors domaine et la cohérence des journaux. Exécuter ensuite les campagnes de débit/RTT/CPU définies dans l’architecture | Au nominal, retrait confirmé et absence de croissance. En cas incertain, compte exact des pages épinglées et refus d’un nouveau bail. Distinguer faute matérielle réellement observée et résultat volontairement masqué par une sonde logicielle. Les contrôles négatifs dangereux ou inobservables sur le MSI restent documentés comme non qualifiés |

La chaîne amovible est une dépendance du projet. Les options à instruire sont une image déjà signée par une autorité admise sur ce MSI, ou un relais admis qui vérifie une politique NovaOS embarquée et la totalité de sa charge. Identifier exactement image, autorité, politique de révocation et prise en compte des arguments/modules. L’admission d’un relais Linux ou d’un certificat de laboratoire par une VM ne démontre rien sur ces fichiers physiques. Aucun enrôlement de clé, firmware personnalisé, OVMF ou correctif QEMU ne fait partie de ce protocole. [UEFI, média amovible](https://uefi.org/specs/UEFI/2.10_A/03_Boot_Manager.html) · [Validation des images](https://uefi.org/specs/UEFI/2.10_A/32_Secure_Boot_and_Driver_Signing.html)

Le Core i5-6300HQ est annoncé compatible VT-d. Un BIOS MSI peut masquer le réglage, omettre une portée, laisser un moteur actif ou limiter l’isolation d’un groupe derrière un pont. Des témoins ponctuels doivent être complétés par une preuve de politique OEM ou une observation suffisamment précoce et continue. En cas d’impossibilité d’établir cette continuité sur firmware standard, le résultat de C reste **inventaire seulement**. La MOE décide alors du devenir de la cible ; ouvrir Bus Master n’est pas une solution de diagnostic. [Intel, capacités du processeur](https://www.intel.com/content/www/us/en/products/sku/88959/intel-core-i56300hq-processor-6m-cache-up-to-3-20-ghz/specifications.html) · [Exigences OEM Microsoft pour la protection pré-boot](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/oem-kernel-dma-protection)

**Preuves et retour à Windows.** Consigner pour chaque passage date, SHA Git et SHA-256 des fichiers effectivement démarrés, BIOS et état Secure Boot, profil, mode IRQ, états de lien, tuple de bail, compteurs avant/pic/après, échéances et résultat. Conserver durablement hors des disques internes les traces du pair (PCAP), mesures et captures console, avec leurs empreintes relues. Avant et après, comparer la liste des supports et les paramètres de démarrage pertinents. Compter dans le témoin les accès bloc : zéro commande d’écriture, de formatage, de FLUSH ou de TRIM vers un support Windows. Pour une preuve d’absence d’écriture indépendante du logiciel, isoler physiquement le disque pour la campagne si le modèle s’y prête, ou employer un bloqueur d’écriture approprié. Qualifier la topologie réellement testée. Après retrait, éteindre, retirer la clé et vérifier le démarrage Windows habituel, Secure Boot actif et la carte de nouveau reconnue. Un retour Windows réussi complète la preuve d’intégrité.

## 3. Phases d’implémentation proposées

| Phase future | Travail borné | Sortie à soumettre |
|---|---|---|
| 1 — Spécification commune | Confronter VirtIO-net, E2400 et I219-V. Fixer droits, contrat L2, transport compatible avec IPC v1 et modèle de restitution | Décision MOA/MOE signée, matrice des trois pilotes et plafond de quatre racines démontrable par acteur |
| 2 — Diagnostic matériel sans DMA réseau | Préparer la chaîne signée pour UEFI OEM, la collecte PCI/ACPI et les preuves de passage. Rester maître fermé | Étapes A à C. Identité MSI, preuves et motif de toute limite firmware |
| 3 — Bail et modèle logiciel | Implémenter après accord les machines d’état Rust `no_std`, tests de format, transitions, échéances et attribution VT-d spécifique à la topologie | Tests avec données fictives, permissions et retrait sans trafic, étape D. Revue des notices et de la provenance des décisions de registre |
| 4 — Pilote E2400 expérimental | PHY/MAC, une file par direction, buffers à copies bornées, IRQ sélectionnée. Activer uniquement sur la cible et le profil admis | Échange E, tests d’intégrité F et retrait contrôlé. Chaque preuve associe registres, compteurs et observation du pair |
| 5 — Qualification et décision | Pannes, saturation, performances, restitution et retour à Windows. Vérifier les profils ordinaires et la compatibilité ABI selon le futur périmètre modifié | Dossier de qualification F/G, résultats par modèle de panne et décision séparée sur toute livraison |

## 4. Registre des risques et arbitrages

| Décideur et point à trancher | Éléments nécessaires | Position de cette phase |
|---|---|---|
| MOA/MOE — chaîne de confiance | BIOS et bases actifs, binaire signé accepté, charge intégralement vérifiée, témoin rejeté | Premier verrou du démarrage physique |
| MOE plateforme — transfert DMA OEM, domaine et IRQ | DMAR complète, topologie PCI et alias, RMRR, identité de source, remappage et preuves avant/après UEFI | Deuxième verrou. Un défaut d’isolement interdit le trafic. Aucune modification des disques Windows pour le résoudre |
| MOE réseau — interface commune et IPC | Dossier Intel I219-V à comparer, résultats VirtIO-net épinglés, propriété des buffers, signification des complétions, vitesse inconnue, politique multicast, budgets de pics et révocation | Choisir fragmentation ou fenêtre de copie, rôles de chaque acteur et droits délégués avant tout développement commun |
| MOE pilote — choix IRQ et révision B0 | Observer MSI sous remappage, interaction INTx Disable, route INTx, fiabilité MSI-X et besoins PHY du sous-système 115a | Un vecteur MSI proposé. MSI-X et les réglages analogiques spécifiques ont leur propre preuve à fournir |
| MOE matériel — recette de fin DMA | Sémantique de l’arrêt matériel, transactions PCIe postées, invalidation avec résultat identifié, protection des autres domaines et témoins hors domaine | Garder en quarantaine en cas d’incertitude, sans réémission automatique. Définir la preuve requise avant toute réutilisation |
| MOA/MOE — budgets et portée de livraison | Mesures E2400 réelles, coût CPU par trame, pression des files, RAM marginale et partagée, IPC au pic, dix retraits | Les [seuils de travail](killer-e2400-architecture.md) restent des propositions. La décision de production est distincte |

Voir le [dossier matériel et les licences](killer-e2400-phase0.md). La phase documentaire se clôt avant toute implémentation du pilote, de l’interface commune ou de la pile IP.
