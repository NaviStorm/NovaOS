# Killer E2400 — inventaire MSI, phase 5B

Fiche de lecture locale pour MSI-DESKTOP. L’arbitrage A8 de phase 5B couvre cet inventaire non destructif. Les commandes PowerShell sont réservées à l’opérateur présent sur le MSI. La préparation de cette fiche a été réalisée sur le Mac de développement. Aucun accès au MSI n’a eu lieu. Afficher les résultats à la console. Si l’opérateur les conserve, choisir un dossier privé local hors dépôt, sauvegarde partagée et synchronisation. Le rapport public peut indiquer « observé », « absent » ou « indéterminé » et la version des outils. Les identifiants d’instance, MAC, topologie nominative et tables ACPI brutes restent locaux.

## Sous Windows

Dans la même session Windows PowerShell 64 bits, utiliser les droits de lecture requis par les cmdlets. La première sélection exige une seule fonction présente qui annonce l’identité prévue. La propriété CompatibleIds permet de recouper la classe PCI 02/00. Un refus d’accès ou une identité différente arrête la qualification de cette cible. [Étude historique](https://github.com/NaviStorm/NovaOS/blob/00335af793e456b6fa6546514f65305f9c114165/novaOS/docs/killer-e2400-qualification-msi.md)

```powershell
$expected = 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10'
$candidates = @(Get-PnpDevice -PresentOnly -Class Net | Where-Object {
    if ($_.InstanceId -like 'PCI\VEN_1969&DEV_E0A1*') {
        (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop).Data -contains $expected
    }
})
if ($candidates.Count -ne 1) { throw "Identité E2400 absente ou ambiguë." }
$id = $candidates[0].InstanceId
$keys = 'DEVPKEY_Device_HardwareIds','DEVPKEY_Device_CompatibleIds','DEVPKEY_Device_BusNumber','DEVPKEY_Device_Address','DEVPKEY_Device_LocationPaths','DEVPKEY_Device_Parent'
Get-PnpDeviceProperty -InstanceId $id -KeyName $keys | Format-List KeyName,Data
Get-CimInstance Win32_PnPSignedDriver | Where-Object DeviceID -eq $id |
    Select-Object DriverProviderName,DriverVersion,DriverDate,InfName,IsSigned
```

Relever la chaîne des parents, les fonctions sœurs présentes et la position des contrôleurs de stockage. Lorsque PnPUtil propose `/relations` et `/resources` dans l’aide locale, les requêtes ci-dessous donnent les relations et les ressources attribuées par Windows. Les versions sans ces options permettent la consultation de la vue « Périphériques par connexion » du Gestionnaire de périphériques. Une fonction sœur ou un pont partagé compte comme dépendance à examiner. [Syntaxe et versions de PnPUtil](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax)

```powershell
$cursor = $id
$seen = @{}
while ($cursor -and !$seen.ContainsKey($cursor)) {
    $seen[$cursor] = $true
    Get-PnpDevice -InstanceId $cursor -ErrorAction Stop | Select-Object Class,FriendlyName,InstanceId
    $cursor = (Get-PnpDeviceProperty -InstanceId $cursor -KeyName 'DEVPKEY_Device_Parent' -ErrorAction SilentlyContinue).Data
}
Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('SCSIAdapter','HDC') } |
    Select-Object Class,FriendlyName,InstanceId
pnputil.exe /?
pnputil.exe /enum-devices /instanceid "$id" /relations
pnputil.exe /enum-devices /instanceid "$id" /resources
```

Identifier l’UEFI, Secure Boot et la protection effective des volumes. Garder uniquement les colonnes ci-dessous. La collecte ne nécessite aucun identifiant de protecteur ni mot de passe de récupération. Contrôler aussi dans `msinfo32.exe` le mode BIOS et l’état affiché de la protection DMA du noyau Windows. [État BitLocker](https://learn.microsoft.com/en-us/powershell/module/bitlocker/get-bitlockervolume) · [Lecture Secure Boot](https://learn.microsoft.com/en-us/powershell/module/secureboot/confirm-securebootuefi)

```powershell
Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product,Version
Get-CimInstance Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion,ReleaseDate
Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber
Confirm-SecureBootUEFI
'SecureBoot','SetupMode' | ForEach-Object {
    [pscustomobject]@{ Name = $_; Value = (Get-SecureBootUEFI -Name $_ -ErrorAction Stop).Bytes[0] }
}
Get-BitLockerVolume | Select-Object MountPoint,VolumeType,VolumeStatus,ProtectionStatus
```

Si l’utilitaire ACPICA de lecture `acpidump.exe` est déjà présent et de provenance vérifiée, ces requêtes affichent les tables. Vérifier les longueurs, sommes de contrôle et la version OEM. En cas d’outil absent, relever « indisponible sous Windows ». Les API Windows permettent aussi à un collecteur local préalablement approuvé de lire les tables firmware. [API Windows](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getsystemfirmwaretable) · [Options ACPICA](https://github.com/open-acpica/acpica/blob/master/source/tools/acpidump/apmain.c)

```powershell
acpidump.exe -n DMAR
acpidump.exe -n MCFG
acpidump.exe -n APIC
```

## Ce qui relève d’un démarrage contrôlé ultérieur

| Information Windows disponible | Preuve supplémentaire nécessaire avant une attribution |
|---|---|
| IDs PnP, ressources Windows, pilote et chaîne de ponts | Réénumération complète et datée après transfert firmware, BAR réellement attribuées, toutes les fonctions, dépendances et alias émetteurs DMA |
| DRHD, RMRR, MCFG et APIC si leurs tables sont accessibles | Résolution au bon segment et à la bonne unité de chaque requester, état effectif des traductions, isolation entre pairs, portée des resets et routage/remappage IRQ |
| État du démarrage sécurisé et BitLocker | Chaîne EFI du candidat déjà acceptée et état du firmware avant et après transfert. La collecte Windows ne démontre pas la continuité de la propriété du NIC |
| Contrôleurs de stockage présents | Démonstration séparée de la séparation des domaines du stockage et du NIC à cette époque. La garde de réadmission s’applique également |
| Identité Ethernet et pilote Windows | Identifiants PHY, état réel des files, arrêt de la source, retrait des traductions et retour de propriété par page |

Au moindre doute, conserver le statut d’inventaire incomplet et refuser l’attribution. Les étapes d’un éventuel démarrage NovaOS, les accès MDIO, BAR, IRQ ou DMA et la qualification au câble exigent les arbitrages physiques correspondants. Le protocole 5B ne programme aucun périphérique. [Protocole physique réservé](killer-e2400-protocole-msi-5a.md)
