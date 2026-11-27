#requires -Version 5.1
# ASCII pour Windows PowerShell Desktop. Aucune ecriture ni interrogation reseau.
function Read-EslSummary([byte[]]$Bytes) {
    if (!$Bytes -or $Bytes.Length -gt 4MB) { throw 'taille' }
    $result = [ordered]@{ etat='lu'; ca2011_der=$false; ca2023_der=$false; windows2011_der=$false; windows2023_der=$false; rom2023_der=$false; autres_x509=$false; empreintes_images=$false; tbs256=$false; ca2011_tbs256=$false; ca2023_tbs256=$false; autres_types=$false }
    $offset = 0
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        while ($offset -lt $Bytes.Length) {
            if ($Bytes.Length - $offset -lt 28) { throw 'entete' }
            $type = [Guid]::new([byte[]]$Bytes[$offset..($offset+15)])
            [long]$size = [BitConverter]::ToUInt32($Bytes,$offset+16)
            [long]$header = [BitConverter]::ToUInt32($Bytes,$offset+20)
            [long]$step = [BitConverter]::ToUInt32($Bytes,$offset+24)
            $payload = $size - 28 - $header
            if ($size -gt $Bytes.Length-$offset -or $step -le 16 -or $payload -lt $step -or $payload % $step) { throw 'liste' }
            switch ($type.ToString()) {
                'a5c059a1-94e4-4aa7-87b5-ab155c2bf072' {
                    if ($header -ne 0) { throw 'x509' }
                    for ($pos=$offset+28; $pos -lt $offset+$size; $pos+=$step) {
                        $hash = [BitConverter]::ToString($sha.ComputeHash($Bytes,$pos+16,$step-16)).Replace('-','')
                        switch ($hash) {
                            '48E99B991F57FC52F76149599BFF0A58C47154229B9F8D603AC40D3500248507' { $result.ca2011_der=$true }
                            'F6124E34125BEE3FE6D79A574EAA7B91C0E7BD9D929C1A321178EFD611DAD901' { $result.ca2023_der=$true }
                            'E8E95F0733A55E8BAD7BE0A1413EE23C51FCEA64B3C8FA6A786935FDDCC71961' { $result.windows2011_der=$true }
                            '076F1FEA90AC29155EBF77C17682F75F1FDD1BE196DA302DC8461E350A9AE330' { $result.windows2023_der=$true }
                            'E5BE3E64C6E66A281457ECDECE0D6D0787577AAD2A3A0144262C10C14BA8D8F1' { $result.rom2023_der=$true }
                            default { $result.autres_x509=$true }
                        }
                    }
                }
                'c1c41626-504c-4092-aca9-41f936934328' {
                    if ($header -ne 0 -or $step -ne 48) { throw 'sha256' }
                    $result.empreintes_images=$true
                }
                '3bd2a492-96c0-4079-b420-fcf98ef103ed' {
                    # EFI_CERT_X509_SHA256: condensat TBSCertificate + EFI_TIME.
                    if ($header -ne 0 -or $step -ne 64) { throw 'tbs' }
                    $result.tbs256=$true
                    for ($pos=$offset+28; $pos -lt $offset+$size; $pos+=$step) {
                        $digest=[BitConverter]::ToString($Bytes,$pos+16,32).Replace('-','')
                        switch ($digest) {
                            '9589B8C95168F79243F61922FAA5990DE0A4866DE928736FED658EA7BFF1A5E2' { $result.ca2011_tbs256=$true }
                            '9A35484E640C7592C1CE3C29BF109970242D0B656C38294273BDBEAE2F60B9B7' { $result.ca2023_tbs256=$true }
                        }
                    }
                }
                default { $result.autres_types=$true }
            }
            $offset += $size
        }
    } finally { $sha.Dispose() }
    return $result
}

function Get-NovaSummary([scriptblock]$ReadVariable,[scriptblock]$Confirm) {
    $r = [ordered]@{ schema='novaos.secureboot/5g'; secure_boot='indetermine'; admissibilite='non_etablie' }
    try {
        $active = & $Confirm
        if ($active -is [bool]) {
            if ($active) { $r.secure_boot='actif' } else { $r.secure_boot='inactif' }
        }
    } catch { }
    foreach ($name in 'db','dbx') {
        try {
            $variable = & $ReadVariable $name
            $r[$name] = Read-EslSummary $variable.Bytes
        } catch {
            $r[$name] = @{etat='indetermine'}
        }
    }
    return $r
}

if ($MyInvocation.InvocationName -ne '.') {
    Get-NovaSummary { param($name) SecureBoot\Get-SecureBootUEFI -Name $name -ErrorAction Stop } {
        SecureBoot\Confirm-SecureBootUEFI -ErrorAction Stop
    } | ConvertTo-Json -Depth 3
}
