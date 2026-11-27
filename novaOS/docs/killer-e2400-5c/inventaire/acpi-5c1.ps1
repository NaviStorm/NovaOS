#requires -Version 5.1
# Source ASCII pour Windows PowerShell 5.1. Dot-sourcing reserve aux tests locaux.
[CmdletBinding()]
param()
Set-StrictMode -Version 2.0

function Initialize-AcpiNative {
    if ('NovaOs5C1.Firmware' -as [type]) { return }
    Add-Type -ErrorAction Stop -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace NovaOs5C1 {
    public static class Firmware {
        const uint Acpi = 0x41435049;
        [DllImport("kernel32.dll", ExactSpelling=true, SetLastError=true)]
        static extern uint EnumSystemFirmwareTables(uint provider, IntPtr buffer, uint size);
        [DllImport("kernel32.dll", ExactSpelling=true, SetLastError=true)]
        static extern uint GetSystemFirmwareTable(uint provider, uint table, IntPtr buffer, uint size);
        public static uint Read(bool enumerate, uint table, IntPtr buffer, uint size) {
            uint n = enumerate ? EnumSystemFirmwareTables(Acpi, buffer, size)
                               : GetSystemFirmwareTable(Acpi, table, buffer, size);
            if (n == 0) throw new Win32Exception(Marshal.GetLastWin32Error());
            return n;
        }
    }
}
'@
}

function Read-Firmware([scriptblock]$Api, [bool]$Enum, [uint32]$Id) {
    $size = & $Api $Enum $Id ([IntPtr]::Zero) 0
    if ($size -le 0 -or $size -gt 1048576) { throw 'Taille API invalide ou superieure a 1 Mio' }
    $ptr = [Runtime.InteropServices.Marshal]::AllocHGlobal([int]$size)
    try {
        $actual = & $Api $Enum $Id $ptr $size
        if ($actual -ne $size) { throw 'Reponse API incomplete ou taille modifiee' }
        $bytes = New-Object byte[] $size
        [Runtime.InteropServices.Marshal]::Copy($ptr,$bytes,0,$size)
        return ,$bytes
    } finally { [Runtime.InteropServices.Marshal]::FreeHGlobal($ptr) }
}

function U16($b,$i) { [BitConverter]::ToUInt16($b,$i) }
function U32($b,$i) { [BitConverter]::ToUInt32($b,$i) }
function U64($b,$i) { [BitConverter]::ToUInt64($b,$i) }
function Hex64($b,$i) { '0x{0:X16}' -f (U64 $b $i) }

function Read-Scopes($b, [int]$i, [int]$end) {
    $count=0
    while ($i -lt $end) {
        if (++$count -gt 256) { throw 'Trop de scopes dans une structure' }
        if ($end-$i -lt 6) { throw 'Entete de scope tronquee' }
        $type = $b[$i]
        $len = $b[$i+1]
        if ($len -lt 6 -or $i+$len -gt $end) { throw 'Longueur de scope invalide' }
        $path = @()
        if ($type -in 1,2,3,4,5) {
            if (($len-6)%2 -ne 0 -or ($type -ne 5 -and $len -lt 8)) { throw 'Chemin PCI mal forme' }
            $path = @(for ($j=$i+6; $j -lt $i+$len; $j+=2) {
                if ($b[$j] -gt 31 -or $b[$j+1] -gt 7) { throw 'Couple PCI hors limites' }
                '{0:x2}.{1}' -f $b[$j],$b[$j+1]
            })
        }
        [pscustomobject]@{Type=$type; Bus=$b[$i+5]; Enumeration=$b[$i+4]; Flags=$b[$i+2]; Path=$path}
        $i += $len
    }
}

function Read-Dmar($b) {
    if ($b.Length -lt 48) { throw 'DMAR tronquee' }
    $rows = New-Object 'System.Collections.Generic.List[object]'
    $partial = $false
    for ($i=48; $i -lt $b.Length; $i+=$len) {
        if ($rows.Count -ge 4096 -or $b.Length-$i -lt 4) { throw 'Liste DMAR incomplete ou trop grande' }
        $type = U16 $b $i
        $len = U16 $b ($i+2)
        $head = switch ($type) { 0 {16} 1 {24} 2 {8} default {4} }
        if ($len -lt $head -or $i+$len -gt $b.Length) { throw 'Sous-structure DMAR invalide' }
        $r = [ordered]@{Type=$type; Length=$len}
        if ($type -in 0,1,2) {
            $r.Segment = U16 $b ($i+6)
            $r.All = ($type -ne 1 -and ($b[$i+4] -band 1) -ne 0)
            $r.Scopes = @(Read-Scopes $b ($i+$head) ($i+$len))
            foreach ($scope in $r.Scopes) { if ($scope.Type -notin 1,2,3,4,5) { $partial=$true } }
            if ($type -eq 0) {
                $r.Base = Hex64 $b ($i+8)
                $r.Unit = 'U{0}' -f (1+$rows.Count)
                if ((U64 $b ($i+8)) -eq 0 -or ((U16 $b ($i+8)) -band 4095)) { throw 'Base DRHD invalide' }
            }
            if ($type -eq 1) {
                if (-not $r.Scopes.Count) { throw 'RMRR sans scope' }
                $r.Base = Hex64 $b ($i+8)
                $r.Limit = Hex64 $b ($i+16)
                if ((U64 $b ($i+8)) -gt (U64 $b ($i+16)) -or ((U16 $b ($i+8)) -band 4095) -ne 0 -or
                    ((U16 $b ($i+16)) -band 4095) -ne 4095) { throw 'Intervalle RMRR invalide' }
            }
        } else { $partial = $true }
        $rows.Add([pscustomobject]$r)
    }
    if ($b[36] -gt 63) { throw 'Largeur adresse hote impossible' }
    [pscustomobject]@{AddressBits=1+[int]$b[36]; Flags=$b[37]; Partial=$partial; Entries=$rows.ToArray()}
}

function Read-Mcfg($b) {
    if ($b.Length -lt 60 -or ($b.Length-44)%16 -ne 0 -or ($b.Length-44)/16 -gt 256) { throw 'Allocations MCFG invalides' }
    $seen = @{}
    $entries = @(for ($i=44; $i -lt $b.Length; $i+=16) {
        $seg = U16 $b ($i+8)
        $first,$last = $b[$i+10],$b[$i+11]
        if ($first -gt $last -or (U64 $b $i) -eq 0 -or ((U32 $b $i) -band 0xfffff)) { throw 'Fenetre ECAM invalide' }
        foreach ($bus in $first..$last) {
            $key="$seg/$bus"
            if ($seen.ContainsKey($key)) { throw 'Fenetres MCFG superposees' }
            $seen[$key]=$true
        }
        [pscustomobject]@{Segment=$seg; First=$first; Last=$last; Base=(Hex64 $b $i)}
    })
    [pscustomobject]@{Entries=$entries}
}

function Read-Madt($b) {
    if ($b.Length -lt 44) { throw 'MADT tronquee' }
    $count=0
    $items = @(for ($i=44; $i -lt $b.Length; $i+=$len) {
        if ($b.Length-$i -lt 2 -or ++$count -gt 4096) { throw 'Entete MADT tronquee ou limite atteinte' }
        $type,$len = $b[$i],$b[$i+1]
        $min = switch ($type) { 0 {8} 1 {12} 2 {10} 4 {6} 5 {12} 9 {16} default {2} }
        if ($len -lt $min -or $i+$len -gt $b.Length) { throw 'Sous-structure MADT invalide' }
        $r = [ordered]@{Type=$type; Length=$len}
        switch ($type) {
            0 { $r.Id=$b[$i+3]; $r.Flags=U32 $b ($i+4) }
            1 { $r.Id=$b[$i+2]; $r.Address=U32 $b ($i+4); $r.GsiBase=U32 $b ($i+8) }
            2 { $r.Bus=$b[$i+2]; $r.Source=$b[$i+3]; $r.Gsi=U32 $b ($i+4); $r.Flags=U16 $b ($i+8) }
            4 { $r.Uid=$b[$i+2]; $r.Flags=U16 $b ($i+3); $r.Lint=$b[$i+5] }
            5 { $r.Address=Hex64 $b ($i+4) }
            9 { $r.Id=U32 $b ($i+4); $r.Flags=U32 $b ($i+8); $r.Uid=U32 $b ($i+12) }
        }
        [pscustomobject]$r
    })
    [pscustomobject]@{Lapic=U32 $b 36; Flags=U32 $b 40; Entries=$items}
}

function Decode-Acpi([string]$Name,[byte[]]$Bytes) {
    if ($Bytes.Length -lt 36 -or [Text.Encoding]::ASCII.GetString($Bytes,0,4) -cne $Name -or
        (U32 $Bytes 4) -ne $Bytes.Length) { throw 'Signature ou longueur ACPI invalide' }
    $sum=0
    foreach ($v in $Bytes) { $sum+=[int]$v }
    if ($sum % 256) { throw 'Somme ACPI invalide' }
    switch ($Name) { DMAR {Read-Dmar $Bytes} MCFG {Read-Mcfg $Bytes} APIC {Read-Madt $Bytes} }
}

function Pnp-Value($id,$property) {
    (Get-PnpDeviceProperty -InstanceId $id -KeyName ("DEVPKEY_Device_"+$property) -ErrorAction Stop).Data
}

function Get-Chain($device) {
    $chain = @()
    $seen = @{}
    $id = $device.InstanceId
    while ($id -like 'PCI\*') {
        if ($seen.ContainsKey($id) -or $chain.Count -ge 32) { throw 'Cycle PCI' }
        $seen[$id]=$true
        if ($chain.Count -and -not (@(Pnp-Value $id CompatibleIds) -match 'CC_0604')) { throw 'Pont inconnu' }
        $bus = Pnp-Value $id BusNumber
        $addr = Pnp-Value $id Address
        if ($null -eq $bus -or $null -eq $addr) { throw 'Adresse PCI absente' }
        $slot = [uint32]$addr -shr 16
        $fn = [uint32]$addr -band 65535
        if ($slot -gt 31 -or $fn -gt 7 -or [uint32]$bus -gt 255) { throw 'Adresse PCI invalide' }
        $chain = @([pscustomobject]@{Bus=[int]$bus; Pair=('{0:x2}.{1}' -f $slot,$fn)}) + $chain
        $id = Pnp-Value $id Parent
    }
    if ($id -notmatch '^ACPI\\PNP0A(03|08)\\') { throw 'Racine PCI non corroboree' }
    $chain
}

function Get-Targets {
    $targets = @{Killer=@(); SATA=@()}
    $devices = @(Get-PnpDevice -PresentOnly -ErrorAction Stop | Where-Object InstanceId -like 'PCI\*')
    try {
        $nic = @($devices | Where-Object InstanceId -like 'PCI\VEN_1969&DEV_E0A1*' | Where-Object {
            @(Pnp-Value $_.InstanceId HardwareIds) -contains 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10'
        })
    } catch { $nic=@() }
    if ($nic.Count -eq 1) {
        try {
            $path = @(Get-Chain $nic[0])
            if ($path.Count -ge 2 -and $path[-1].Bus -eq 3 -and $path[-1].Pair -eq '00.0' -and
                $path[-2].Bus -eq 0 -and $path[-2].Pair -eq '1c.3') { $targets.Killer=$path }
        } catch { $targets.Killer=@() }
    }
    try {
        $sata = @(foreach ($dev in $devices) {
            if (@(Pnp-Value $dev.InstanceId CompatibleIds) -match '^PCI\\CC_0106(?:[0-9A-F]{2})?$') { $dev }
        })
    } catch { $sata=@() }
    if ($sata.Count -eq 1) {
        try { $targets.SATA=@(Get-Chain $sata[0]) } catch { $targets.SATA=@() }
    }
    $targets
}

function Test-Scope($scope,$chain) {
    if ($scope.Type -notin 1,2) { return $false }
    for ($i=0; $i -lt $chain.Count; $i++) {
        if ($scope.Bus -ne $chain[$i].Bus) { continue }
        $end=$i+$scope.Path.Count
        if ($end -gt $chain.Count -or ($scope.Type -eq 1 -and $end -ne $chain.Count) -or
            ($scope.Type -eq 2 -and $end -eq $chain.Count)) { continue }
        $ok=$true
        for ($j=0; $j -lt $scope.Path.Count; $j++) {
            if ($chain[$i+$j].Pair -ne $scope.Path[$j]) { $ok=$false; break }
        }
        if ($ok) { return $true }
    }
    return $false
}

function Get-Coverage($dmar,$mcfg,$chain) {
    $result=[ordered]@{etat='indetermine'; unites=@(); rmrr='indetermine'; atsr='indetermine'}
    if (-not $chain -or -not $mcfg -or -not $dmar) { return $result }
    $segments = @($mcfg.Entries | ForEach-Object Segment | Sort-Object -Unique | Where-Object {
        $seg=$_
        $uncovered=@(foreach ($node in $chain) {
            if (-not @($mcfg.Entries | Where-Object {
                $_.Segment -eq $seg -and $node.Bus -ge $_.First -and $node.Bus -le $_.Last
            }).Count) { $node }
        })
        $uncovered.Count -eq 0
    })
    if ($segments.Count -ne 1) { return $result }
    $explicit=@()
    $fallback=@()
    $rmrr=$false
    $atsr=$false
    foreach ($r in $dmar.Entries) {
        if ($r.Type -notin 0,1,2 -or $r.Segment -ne $segments[0]) { continue }
        $matchesScope=@($r.Scopes | Where-Object { Test-Scope $_ $chain }).Count -gt 0
        if ($r.Type -eq 0) {
            if ($matchesScope) { $explicit+=$r.Unit }
            if ($r.All) { $fallback+=$r.Unit }
        } elseif ($r.Type -eq 1 -and $matchesScope) { $rmrr=$true }
        elseif ($r.Type -eq 2 -and $chain.Count -gt 1 -and ($matchesScope -or $r.All)) { $atsr=$true }
    }
    $result.unites=$fallback
    $result.etat='candidat_par_defaut'
    if ($explicit.Count) { $result.unites=$explicit; $result.etat='candidat_explicite' }
    if ($result.unites.Count -ne 1 -or $fallback.Count -gt 1 -or $dmar.Partial) { $result.etat='indetermine' }
    if (-not $dmar.Partial) {
        $result.rmrr=if ($rmrr) {'portee_candidate'} else {'aucune_correspondance'}
        $result.atsr=if ($atsr) {'portee_candidate'} else {'aucune_correspondance'}
    }
    $result
}

function Get-Tables([scriptblock]$Api,[string]$Folder) {
    $states=[ordered]@{}
    $decoded=@{}
    $errors=@{}
    try {
        [byte[]]$ids=Read-Firmware $Api $true 0
        if ($ids.Length%4) { throw 'Liste des identifiants tronquee' }
        $counts=@{}
        for ($i=0; $i -lt $ids.Length; $i+=4) {
            $key=[BitConverter]::ToUInt32($ids,$i)
            $counts[$key]=1+[int]$counts[$key]
        }
    } catch { $counts=$null; $errors.Enumeration=$_.Exception.Message }
    foreach ($name in 'DMAR','MCFG','APIC') {
        $key=[BitConverter]::ToUInt32([Text.Encoding]::ASCII.GetBytes($name),0)
        if ($null -eq $counts) { $states[$name]='enumeration_en_echec'; continue }
        if (-not $counts.ContainsKey($key)) { $states[$name]='absente'; continue }
        if ($counts[$key] -ne 1) { $states[$name]='multiple'; continue }
        try { [byte[]]$data=Read-Firmware $Api $false $key }
        catch { $states[$name]='lecture_en_echec'; $errors[$name]=$_.Exception.Message; continue }
        [IO.File]::WriteAllBytes((Join-Path $Folder ($name+'.bin')),$data)
        try {
            $decoded[$name]=Decode-Acpi $name $data
            $states[$name]='valide'
            if ($name -eq 'DMAR' -and $decoded[$name].Partial) { $states[$name]='partielle' }
        } catch { $states[$name]='invalide'; $errors[$name]=$_.Exception.Message }
    }
    @{Etats=$states; Tables=$decoded; Erreurs=$errors}
}

function Invoke-Collecte {
    $ErrorActionPreference='Stop'
    if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitProcess -or -not [BitConverter]::IsLittleEndian) {
        throw 'Utiliser Windows PowerShell x64 sur le MSI.'
    }
    $base=[Environment]::GetFolderPath('LocalApplicationData')
    if (-not $base -or $base -notmatch '^[a-zA-Z]:\\' -or
        [IO.DriveInfo]::new([IO.Path]::GetPathRoot($base)).DriveType -ne 'Fixed') {
        throw 'Profil local sur disque fixe requis'
    }
    for ($parent=[IO.DirectoryInfo]::new($base); $null -ne $parent; $parent=$parent.Parent) {
        if ($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Profil redirige : arret' }
    }
    Initialize-AcpiNative
    $dir=Join-Path $base ('NovaOS-ACPI-'+[guid]::NewGuid().ToString('N'))
    $null=New-Item -ItemType Directory -Path $dir -ErrorAction Stop
    # Restreindre le dossier neuf avant toute lecture de table.
    $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl=New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true,$false)
    $rule=[Security.AccessControl.FileSystemAccessRule]::new($owner,'FullControl','ContainerInherit,ObjectInherit','None','Allow')
    $acl.AddAccessRule($rule)
    Set-Acl -LiteralPath $dir -AclObject $acl -ErrorAction Stop
    $check=Get-Acl -LiteralPath $dir -ErrorAction Stop
    $rules=@($check.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
    if (-not $check.AreAccessRulesProtected -or $rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $owner.Value) {
        throw 'Dossier prive non confirme'
    }
    $api={param($enum,$id,$ptr,$size) [NovaOs5C1.Firmware]::Read($enum,$id,$ptr,$size)}
    $r=Get-Tables $api $dir
    try { $paths=Get-Targets }
    catch { $paths=@{Killer=@(); SATA=@()}; $r.Erreurs.PnP=$_.Exception.Message }
    $private=@{decodage=$r.Tables; chemins=$paths; erreurs=$r.Erreurs}
    $private | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $dir 'local.json') -Encoding UTF8 -ErrorAction Stop
    $summary=New-Summary $r $paths
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $dir 'synthese-moe.json') -Encoding UTF8 -ErrorAction Stop
    Write-Output ("Dossier prive : "+$dir)
    Write-Output 'Seul synthese-moe.json est prevu pour transmission apres relecture.'
}

function New-Summary($r,$paths) {
    $d=$r.Tables['DMAR']
    $m=$r.Tables['MCFG']
    $a=$r.Tables['APIC']
    $nic=Get-Coverage $d $m $paths.Killer
    $disk=Get-Coverage $d $m $paths.SATA
    $relation='indetermine'
    if ($nic.etat -like 'candidat*' -and $disk.etat -like 'candidat*') {
        $relation=if ($nic.unites[0] -eq $disk.unites[0]) {'meme_unite_candidate'} else {'unites_candidates_distinctes'}
    }
    [ordered]@{schema='novaos.acpi/5c.1'; tables=$r.Etats; killer=$nic; sata=$disk; relation=$relation;
        drhd=$(if ($d) { @($d.Entries | Where-Object Type -eq 0).Count });
        remappage_irq_declare=$(if ($d) { [bool]($d.Flags -band 1) });
        fenetres_ecam=$(if ($m) { $m.Entries.Count }); ioapic=$(if ($a) { @($a.Entries | Where-Object Type -eq 1).Count });
        isolation_dma_effective='non_mesuree'; etat_vtd='non_lu'}
}

if ($MyInvocation.InvocationName -ne '.') { Invoke-Collecte }
