#requires -Version 5.1
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'acpi-5c1.ps1')
Initialize-AcpiNative # Compilation seule, aucun appel kernel32.
function Check($ok,$label) {
    if (-not $ok) { throw "ECHEC $label" }
    "OK $label"
}
function Refuse([scriptblock]$action,$label) {
    $failed=$false
    try { $null=& $action } catch { $failed=$true }
    Check $failed $label
}
function Make-Table([string]$sig,[byte[]]$body) {
    $b=New-Object byte[] (36+$body.Length)
    [Text.Encoding]::ASCII.GetBytes($sig).CopyTo($b,0)
    [BitConverter]::GetBytes([uint32]$b.Length).CopyTo($b,4)
    $b[8]=1
    $body.CopyTo($b,36)
    $b[9]=(256-(($b | Measure-Object -Sum).Sum%256))%256
    return ,$b
}
function Struct([int]$t,[byte[]]$body) {
    return ,([byte[]](@([byte]$t,0)+[BitConverter]::GetBytes([uint16](4+$body.Length))+$body))
}
$endpoint=[byte[]](1,10,0,0,0,0,28,3,0,0)
$bridge=[byte[]](2,8,0,0,0,0,28,3)
$h=[byte[]](47,1,0,0,0,0,0,0,0,0,0,0)
$u=(Struct 0 ([byte[]](@(0,0,0,0,0,16,0,0,0,0,0,0)+$endpoint)))
$default=(Struct 0 ([byte[]](1,0,0,0,0,32,0,0,0,0,0,0)))
$rr=Struct 1 ([byte[]](@(0,0,0,0,0,16,0,0,0,0,0,0,255,31,0,0,0,0,0,0)+$bridge))
$ats=Struct 2 ([byte[]](@(0,0,0,0)+$bridge))
$dmar=Make-Table DMAR ($h+$u+$default+$rr+$ats)
$mcfg=Make-Table MCFG ([byte[]](0,0,0,0,0,0,0,0,0,0,0,224,0,0,0,0,0,0,0,255,0,0,0,0))
$apic=Make-Table APIC ([byte[]](0,0,224,254,1,0,0,0,1,12,2,0,0,0,192,254,0,0,0,0,2,10,0,1,9,0,0,0,13,0))
$d=Decode-Acpi DMAR $dmar
$m=Decode-Acpi MCFG $mcfg
$a=Decode-Acpi APIC $apic
Check ($d.Entries.Count -eq 4 -and $d.AddressBits -eq 48 -and $d.Entries[2].Limit -eq '0x0000000000001FFF') 'DRHD RMRR ATSR'
Check ($m.Entries[0].Last -eq 255 -and $a.Entries[1].Gsi -eq 9 -and $a.Entries[0].Id -eq 2) 'ECAM IOAPIC ISO'
$paths=@{Killer=@(@{Bus=0;Pair='1c.3'},@{Bus=3;Pair='00.0'}); SATA=@(@{Bus=0;Pair='1f.2'})}
$r=@{Tables=@{DMAR=$d;MCFG=$m;APIC=$a};Etats=@{DMAR='valide';MCFG='valide';APIC='valide'}}
$s=New-Summary $r $paths
Check ($s.killer.etat -eq 'candidat_explicite' -and $s.killer.rmrr -eq 'portee_candidate' -and $s.killer.atsr -eq 'portee_candidate') 'chemin endpoint et sous-arbre'
Check ($s.sata.etat -eq 'candidat_par_defaut' -and $s.sata.rmrr -eq 'aucune_correspondance' -and $s.relation -eq 'unites_candidates_distinctes') 'SATA sur branche distincte'
Check (($s | ConvertTo-Json -Depth 8) -notmatch '1c\\.3|0x|InstanceId|OEM|\\.bin|LocalApplicationData') 'synthese expurgee'
Check ((Get-Coverage $d $m @()).etat -eq 'indetermine') 'cible inconnue'
$m2=[pscustomobject]@{Entries=@($m.Entries[0],([pscustomobject]@{Segment=1;First=0;Last=255}))}
Check ((Get-Coverage $d $m2 $paths.Killer).etat -eq 'indetermine') 'segments ambigus'
$dm2=Decode-Acpi DMAR (Make-Table DMAR ($h+$u+$u+$default))
Check ((Get-Coverage $dm2 $m $paths.Killer).etat -eq 'indetermine') 'deux unites explicites'
$unknown=Decode-Acpi DMAR (Make-Table DMAR ($h+$u+([byte[]](99,0,4,0))))
Check ($unknown.Partial -and (Get-Coverage $unknown $m $paths.Killer).etat -eq 'indetermine') 'type inconnu conserve et couverture indeterminee'
$bad=[byte[]]$dmar.Clone()
$bad[9]=$bad[9] -bxor 1
Refuse {Decode-Acpi DMAR $bad} 'checksum incorrect'
Refuse {Decode-Acpi MCFG $dmar} 'signature incorrecte'
Refuse {Decode-Acpi DMAR $dmar[0..36]} 'longueur ACPI incorrecte'
Refuse {Decode-Acpi DMAR (Make-Table DMAR ($h+([byte[]](0,0,20,0,0))))} 'DRHD tronquee'
Refuse {Decode-Acpi DMAR (Make-Table DMAR ($h+([byte[]](99,0,0,0))))} 'structure de taille nulle'
$short=$u.Clone()
$short[17]=100
Refuse {Decode-Acpi DMAR (Make-Table DMAR ($h+$short))} 'scope tronque'
Refuse {Decode-Acpi APIC (Make-Table APIC (New-Object byte[] 9))} 'MADT tronquee'
Refuse {Decode-Acpi MCFG (Make-Table MCFG (New-Object byte[] 9))} 'MCFG tronquee'

# Double de l'API en deux appels, copie dans le meme type de tampon que Windows.
$ids=[Text.Encoding]::ASCII.GetBytes('DMARMCFGAPIC')
$series=@{DMAR=$dmar; MCFG=$mcfg; APIC=$apic}
$mode='nominal'
$api={param($enum,$id,$ptr,$size)
    $name=[Text.Encoding]::ASCII.GetString([BitConverter]::GetBytes([uint32]$id))
    if ($enum) { $src=$ids } else { $src=$series[$name] }
    if ($mode -eq 'enum-error' -or ($mode -eq 'get-error' -and $name -eq 'DMAR')) { throw 'Erreur Windows fictive' }
    if ($mode -eq 'zero') { return 0 }
    if ($mode -eq 'limite') { return 1048577 }
    if ($ptr -eq [IntPtr]::Zero) { return $src.Length }
    if ($mode -eq 'course' -and -not $enum) { return ($src.Length+1) }
    [Runtime.InteropServices.Marshal]::Copy([byte[]]$src,0,$ptr,$src.Length)
    return $src.Length
}
$root=Join-Path ([IO.Path]::GetTempPath()) ('novaos-acpi-fictif-'+[guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Path $root
try {
    $o=Get-Tables $api $root
    Check ($o.Etats.DMAR -eq 'valide' -and $o.Etats.APIC -eq 'valide' -and $o.Etats.MCFG -eq 'valide') 'enumeration et lecture uniques'
    Check ([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $root DMAR.bin))) -eq [Convert]::ToBase64String($dmar)) 'octets prives identiques'
    $series.DMAR=$bad
    $o=Get-Tables $api $root
    Check ($o.Etats.DMAR -eq 'invalide' -and $null -eq $o.Tables['DMAR'] -and $o.Etats.MCFG -eq 'valide') 'refus isole sans resultat partiel'
    $ids=[Text.Encoding]::ASCII.GetBytes('DMARDMARAPIC')
    $o=Get-Tables $api $root
    Check ($o.Etats.DMAR -eq 'multiple' -and $o.Etats.MCFG -eq 'absente') 'multiple et absente'
    $ids=[Text.Encoding]::ASCII.GetBytes('DMARMCFGAPIC')
    foreach ($mode in 'enum-error','zero','limite','course','get-error') {
        $o=Get-Tables $api $root
        $attendu=if ($mode -in 'course','get-error') {'lecture_en_echec'} else {'enumeration_en_echec'}
        Check ($o.Etats.DMAR -eq $attendu) $mode
    }
} finally {
    # Seul le dossier GUID cree par ce test contient ses tables fictives.
    [IO.Directory]::Delete($root,$true)
}

# Reconstituer les deux chemins avec des donnees PnP uniquement fictives.
function Get-PnpDevice {
    param([switch]$PresentOnly,$ErrorAction)
    foreach ($id in ('PCI\VEN_1969&DEV_E0A1\TEST','PCI\PONT','PCI\SATA')) {
        [pscustomobject]@{InstanceId=$id}
    }
}
function Get-PnpDeviceProperty {
    param($InstanceId,$KeyName,$ErrorAction)
    $v = switch ($KeyName) {
        DEVPKEY_Device_HardwareIds { 'PCI\VEN_1969&DEV_E0A1&SUBSYS_115A1462&REV_10' }
        DEVPKEY_Device_CompatibleIds {
            if ($InstanceId -eq 'PCI\PONT') { 'PCI\CC_060400' }
            elseif ($InstanceId -eq 'PCI\SATA') {
                if ($sataHidden) { throw 'Propriete fictive inaccessible' }
                'PCI\CC_010601'
            } else { 'PCI\CC_020000' }
        }
        DEVPKEY_Device_Address {
            if ($InstanceId -eq 'PCI\PONT') { 0x1c0003 } elseif ($InstanceId -eq 'PCI\SATA') { 0x1f0002 } else { 0 }
        }
        DEVPKEY_Device_BusNumber { if ($InstanceId -like '*E0A1*') { 3 } else { 0 } }
        DEVPKEY_Device_Parent { if ($InstanceId -like '*E0A1*') { 'PCI\PONT' } else { 'ACPI\PNP0A08\TEST' } }
    }
    [pscustomobject]@{Data=$v}
}
$sataHidden=$false
$pnp=Get-Targets
Check ($pnp.Killer.Count -eq 2 -and $pnp.SATA.Count -eq 1 -and (New-Summary $r $pnp).relation -eq 'unites_candidates_distinctes') 'rapprochement PnP et DMAR'
$sataHidden=$true
$pnp=Get-Targets
Check ($pnp.Killer.Count -eq 2 -and $pnp.SATA.Count -eq 0) 'PnP SATA indisponible conserve le NIC'

$secondSeg=$u.Clone()
$secondSeg[6]=1
$other=Decode-Acpi DMAR (Make-Table DMAR ($h+$secondSeg+$default))
Check ((Get-Coverage $other $m $paths.Killer).etat -eq 'candidat_par_defaut') 'scope explicite dans un autre segment ecarte'
$badRange=$mcfg[36..59] + $mcfg[44..59]
Refuse {Decode-Acpi MCFG (Make-Table MCFG $badRange)} 'fenetres ECAM recouvrantes'

$allRoot=Struct 2 ([byte[]](1,0,0,0))
$withRoot=Decode-Acpi DMAR (Make-Table DMAR ($h+$default+$allRoot))
Check ((Get-Coverage $withRoot $m $paths.Killer).atsr -eq 'portee_candidate' -and
       (Get-Coverage $withRoot $m $paths.SATA).atsr -eq 'aucune_correspondance') 'ATSR ALL_PORTS limite aux branches sous pont'

$notBridge=[pscustomobject]@{Type=2; Bus=3; Path=@('00.0')}
Check (-not (Test-Scope $notBridge $paths.Killer)) 'le scope de pont ne transforme pas un endpoint en pont'
