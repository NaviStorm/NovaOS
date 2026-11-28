# Contrôle local des blocs publiés sur un inventaire fictif. Aucun fournisseur PnP.
param([Parameter(Mandatory=$true)][string]$Sortie)
$ErrorActionPreference = 'Stop'
$fiche = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'inventaire.md'))
$blocs = [regex]::Matches($fiche, '(?ms)^\x60{3}powershell\r?\n(.*?)^\x60{3}\s*$')
if ($blocs.Count -ne 2) { throw 'Deux blocs PowerShell attendus.' }
foreach ($bloc in $blocs) {
    $jetons = $null
    $erreurs = $null
    [void][Management.Automation.Language.Parser]::ParseInput($bloc.Groups[1].Value, [ref]$jetons, [ref]$erreurs)
    if ($erreurs.Count -ne 0) { throw 'Erreur de syntaxe dans la fiche.' }
}
$lecture = [scriptblock]::Create($blocs[0].Groups[1].Value)
$dossier = New-Item -ItemType Directory -Path $Sortie
$fichier5C = Join-Path $dossier.FullName 'fictif.json'
function Noeud($id,$classe,$etat) {
    [pscustomobject]@{id=$id; CompatibleIds=@{etat=$etat; donnees=@($classe)}}
}
$a = Noeud 'PCI\DEMO_A' 'PCI\CC_0C0330' 'observe'
$b = Noeud 'PCI\DEMO_B' 'PCI\CC_0C0330' 'observe'
$c = Noeud 'PCI\DEMO_C' 'PCI\CC_010601' 'observe'
$d = Noeud 'PCI\DEMO_D' $null 'indetermine'
function Ecrire($schema,$noeuds) {
    $json = @{schema=$schema;arbre=@($noeuds)} | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($fichier5C,$json)
}
function Verifier($noeuds,$nombre,$manquants,$etat) {
    Ecrire 'novaos.inventaire-e2400/5c' $noeuds
    $resultat = & $lecture
    if ($resultat.CandidatsXhciVisibles -ne $nombre -or $resultat.ClassesPciIncompletes -ne $manquants -or
        $resultat.Etat -ne $etat) { throw 'Résultat de tri incorrect.' }
}
Verifier @($a,$c) 1 0 'candidats Windows'
Verifier @($a,$b) 2 0 'candidats Windows'
Verifier @($c) 0 0 'indetermine'
Verifier @($a,$d) 1 1 'indetermine'
foreach ($invalide in @(@{schema='autre';arbre=@($a)}, @{schema='novaos.inventaire-e2400/5c';arbre=@()})) {
    Ecrire $invalide.schema $invalide.arbre
    $refuse = $false
    try { $null = & $lecture } catch { $refuse = $_.Exception.Message -eq 'Inventaire 5C absent ou incompatible.' }
    if (-not $refuse) { throw 'Faux inventaire accepté.' }
}
"PowerShell $($PSVersionTable.PSVersion) — 2 blocs analysés, 6 cas fictifs conformes. Aucun appel PnP."
