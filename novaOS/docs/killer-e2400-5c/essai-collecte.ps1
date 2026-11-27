# Banc local sur donnees fictives. Les fonctions ci-dessous remplacent les cmdlets Windows.
$ErrorActionPreference = 'Stop'
$casE2400 = 'nominal'
function Get-PnpDevice {
    param([switch]$PresentOnly)
    $n = [pscustomobject]@{InstanceId='PCI\VEN_1969&DEV_E0A1\TEST'; Class='Net'; FriendlyName='Fictif'}
    $n
    if ($casE2400 -eq 'multiple') { $n }
    [pscustomobject]@{InstanceId='PCI\STOCKAGE'; Class='SCSIAdapter'; FriendlyName='Fictif'}
}
function Get-PnpDeviceProperty($InstanceId,$KeyName) {
    $data = switch ($KeyName) {
        DEVPKEY_Device_HardwareIds { 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10' }
        DEVPKEY_Device_CompatibleIds { if ($casE2400 -eq 'classe') { 'PCI\CC_0108' } else { 'PCI\CC_020000' } }
        DEVPKEY_Device_Address { if ($InstanceId -eq 'PCI\STOCKAGE') { 65537 } else { 65536 } }
        DEVPKEY_Device_Parent { 'PCI\RACINE' }
        default { 0 }
    }
    [pscustomobject]@{Data=$data}
}
function Get-CimInstance($ClassName) { [pscustomobject]@{DeviceID='PCI\VEN_1969&DEV_E0A1\TEST'; DriverVersion='TEST'} }
function Get-Disk { [pscustomobject]@{Number=1; BusType='test'} }
function Get-BitLockerVolume { throw 'Lecture indisponible' }
function Confirm-SecureBootUEFI { $false }
function Get-SecureBootUEFI($Name) { [pscustomobject]@{Bytes=[byte[]]@(1)} }
$original = $env:OS
try {
    $env:OS = 'Windows_NT'
    $file = Join-Path $PSScriptRoot 'inventaire/collecter.ps1'
    $d = (& $file) | ConvertFrom-Json
    if ($d.arbre.Count -ne 2 -or $d.secureBoot.donnees[0] -ne $false -or
        $d.soeurs_visibles.donnees[0] -ne 'PCI\STOCKAGE' -or
        $d.bitlocker.etat -ne 'indetermine' -or $d.variables.Count -ne 6 -or
        $d.pilote.donnees[0].DriverVersion -ne 'TEST') { throw 'Collecte incomplete' }
    foreach ($casE2400 in 'multiple','classe') {
        $refus = $false
        try { & $file | Out-Null } catch {
            $attendu = if ($casE2400 -eq 'multiple') { 'Cible absente ou ambigue. Arret.' } else { 'Classe Ethernet non confirmee.' }
            if ($_.Exception.Message -ne $attendu) { throw }
            $refus = $true
        }
        if (-not $refus) { throw "Refus attendu pour $casE2400" }
    }
    'OK - inventaire fictif, acces refuse, Secure Boot inactif, deux identites et classe divergente'
} finally { $env:OS = $original }
