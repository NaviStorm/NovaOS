#requires -Version 5.1
# Inventaire local et prive. Seul le JSON est emis sur la sortie standard.
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitProcess) {
    throw 'Utiliser Windows PowerShell 64 bits sur la cible.'
}
function Lire([scriptblock]$Action) {
    try {
        $rows = @(& $Action)
        if ($rows.Count -eq 0 -or ($rows.Count -eq 1 -and $null -eq $rows[0])) { return @{etat='sans_resultat'} }
        return @{etat='observe'; donnees=$rows}
    } catch {
        return @{etat='indetermine'; cause=$_.FullyQualifiedErrorId}
    }
}
$devices = @(Get-PnpDevice -PresentOnly)
$expected = 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10'
$nic = @($devices | Where-Object { $_.InstanceId -like 'PCI\VEN_1969&DEV_E0A1*' } | Where-Object {
    (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName DEVPKEY_Device_HardwareIds).Data -contains $expected
})
if ($nic.Count -ne 1) { throw 'Cible absente ou ambigue. Arret.' }
$id = $nic[0].InstanceId
$compat = @((Get-PnpDeviceProperty -InstanceId $id -KeyName DEVPKEY_Device_CompatibleIds).Data)
if (-not ($compat -match '^PCI\\CC_0200(?:[0-9A-F]{2})?$')) { throw 'Classe Ethernet non confirmee.' }
$report = [ordered]@{schema='novaos.inventaire-e2400/5c'; utc=[DateTime]::UtcNow.ToString('o'); cible=$id}
# Tous les liens de parente permettent de remonter aussi un disque via son bus intermediaire.
$report.arbre = @(foreach ($d in $devices) {
    $node = [ordered]@{id=$d.InstanceId; classe=$d.Class; nom=$d.FriendlyName}
    $node.parent = Lire { (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName DEVPKEY_Device_Parent).Data }
    if ($d.InstanceId -like 'PCI\*') {
        foreach ($key in 'HardwareIds','CompatibleIds','BusNumber','Address','LocationPaths') {
            $node[$key] = Lire { (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName ("DEVPKEY_Device_" + $key)).Data }
        }
    }
    [pscustomobject]$node
})
# Meme parent PCI et meme bus/peripherique. Les fonctions absentes de PnP restent inconnues.
function Emplacement($node) {
    foreach ($key in 'parent','BusNumber','Address') {
        if ($node.$key.etat -ne 'observe' -or $node.$key.donnees.Count -ne 1) { return }
    }
    $addr = [uint32]$node.Address.donnees[0]
    $slot = $addr -shr 16
    $fn = $addr -band 65535
    if ($slot -gt 31 -or $fn -gt 7) { return }
    [pscustomobject]@{pont=$node.parent.donnees[0]; bus=$node.BusNumber.donnees[0]; slot=$slot; fonction=$fn}
}
$source = Emplacement ($report.arbre | Where-Object id -eq $id)
$report.soeurs_visibles = Lire {
    if ($null -eq $source) { throw 'Position PCI inconnue' }
    foreach ($node in $report.arbre) {
        if ($node.id -eq $id -or $node.id -notlike 'PCI\*') { continue }
        $other = Emplacement $node
        if ($null -ne $other -and $other.pont -eq $source.pont -and $other.bus -eq $source.bus -and
            $other.slot -eq $source.slot -and $other.fonction -ne $source.fonction) { $node.id }
    }
}
$report.disques_pnp = Lire { Get-CimInstance Win32_DiskDrive | Select-Object Index,PNPDeviceID,InterfaceType }
$report.machine = Lire { Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer,Model }
$report.pilote = Lire { Get-CimInstance Win32_PnPSignedDriver | Where-Object DeviceID -eq $id |
    Select-Object DriverProviderName,DriverVersion,DriverDate,InfName,IsSigned }
$report.disques = Lire { Get-Disk | Select-Object Number,BusType,IsBoot,IsSystem,PartitionStyle,Path,Location }
$report.bios = Lire { Get-CimInstance Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion,ReleaseDate }
$report.carte = Lire { Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product,Version }
$report.windows = Lire { Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber }
$report.secureBoot = Lire { Confirm-SecureBootUEFI }
$report.variables = @(foreach ($name in 'SecureBoot','SetupMode','PK','KEK','db','dbx') {
    $result = Lire {
        $bytes = (Get-SecureBootUEFI -Name $name).Bytes
        if (-not $bytes.Length) { throw 'Variable vide' }
        if ($name -in 'SecureBoot','SetupMode') { [int]$bytes[0] }
        else {
            $h = [Security.Cryptography.SHA256]::Create()
            try { [BitConverter]::ToString($h.ComputeHash($bytes)).Replace('-','') }
            finally { $h.Dispose() }
        }
    }
    [pscustomobject]@{nom=$name; lecture=$result}
})
$report.bitlocker = Lire { Get-BitLockerVolume | Select-Object MountPoint,VolumeType,VolumeStatus,ProtectionStatus }
$report | ConvertTo-Json -Depth 9
