#requires -Version 5.1
param([Parameter(Mandatory=$true)][string]$CertificatsPublics)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'synthese.ps1')
$x509=[Guid]'a5c059a1-94e4-4aa7-87b5-ab155c2bf072'
$hashGuid=[Guid]'c1c41626-504c-4092-aca9-41f936934328'
function Esl([Guid]$type,[byte[]]$body) {
    [byte[]]$owner=New-Object byte[] 16
    return ,([byte[]]($type.ToByteArray()+[BitConverter]::GetBytes([int](44+$body.Length))+[BitConverter]::GetBytes([int]0)+[BitConverter]::GetBytes([int](16+$body.Length))+$owner+$body))
}
function Check($ok,$name) { if (!$ok) { throw $name }; "OK $name" }
[byte[]]$all=@()
$map=@{'ms-uefi-2011'='ca2011_der';'ms-uefi-2023'='ca2023_der';'ms-uefi-rom-2023'='rom2023_der';'windows-2011'='windows2011_der';'windows-2023'='windows2023_der'}
foreach ($key in $map.Keys) {
    $pem = [IO.File]::ReadAllText((Join-Path $CertificatsPublics "$key.pem"))
    $der = [Convert]::FromBase64String(([regex]::Match($pem,'(?s)-----BEGIN CERTIFICATE-----(.*?)-----END CERTIFICATE-----').Groups[1].Value))
    $list = Esl $x509 $der
    $row = Read-EslSummary $list
    Check ($row[$map[$key]] -and @($row.Keys | Where-Object { $_ -like '*_der' -and $row[$_] }).Count -eq 1) $key
    $all += $list
}
$tbsGuid=[Guid]'3bd2a492-96c0-4079-b420-fcf98ef103ed'
foreach ($pair in @(@('ca2011_tbs256','9589b8c95168f79243f61922faa5990de0a4866de928736fed658ea7bff1a5e2'),@('ca2023_tbs256','9a35484e640c7592c1ce3c29bf109970242d0b656c38294273bdbeae2f60b9b7'))) {
    [byte[]]$digest=for ($i=0;$i -lt 64;$i+=2){[Convert]::ToByte($pair[1].Substring($i,2),16)}
    $row=Read-EslSummary (Esl $tbsGuid ($digest+(New-Object byte[] 16)))
    Check ($row[$pair[0]] -and $row.tbs256 -and !$row.ca2023_der) $pair[0]
}
$hash=Esl $hashGuid (New-Object byte[] 32)
$r=Get-NovaSummary {param($n) if($n -eq 'db'){@{Bytes=$all}}else{@{Bytes=$hash}}} {$true}
Check ($r.db.ca2011_der -and $r.db.ca2023_der -and $r.dbx.empreintes_images -and !$r.dbx.ca2023_der -and $r.secure_boot -eq 'actif') 'db et dbx distinctes'
$r=Get-NovaSummary {param($n) if($n -eq 'dbx'){@{Bytes=$all}}else{throw 'message-prive'}} {$false}
Check ($r.db.etat -eq 'indetermine' -and $r.dbx.ca2023_der -and $r.secure_boot -eq 'inactif') 'revocation DER et erreur db'
$unknown=Esl ([Guid]::Empty) ([byte[]](1,2,3))
Check ((Read-EslSummary $unknown).autres_types) 'type inconnu'
foreach($bad in @(@{v=[byte[]]@()},@{v=[byte[]]($all+0)},@{v=(New-Object byte[] 29)},@{v=(Esl $hashGuid ([byte[]]@(0)))},@{v=(New-Object byte[] (4MB+1))})) {
    $r=Get-NovaSummary {param($n) @{Bytes=$bad.v}} {throw 'message-prive'}
    Check ($r.db.Count -eq 1 -and $r.db.etat -eq 'indetermine' -and $r.secure_boot -eq 'indetermine') 'refus sans resultat partiel'
}
$r=Get-NovaSummary {throw 'message-prive'} {throw 'message-prive'}
Check (($r |ConvertTo-Json -Compress) -notmatch 'message-prive|[0-9A-Fa-f]{32}') 'synthese expurgee'
