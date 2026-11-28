# MSI — données manquantes après les collectes Windows et ACPI

Les dossiers privés 5C et 5C.1 constituent les entrées. La MOA a déjà identifié le portable, sa version de BIOS, le contrôleur Ethernet, son pont et une branche distincte pour le SATA. DMAR, MCFG et APIC sont valides. Deux DRHD, une fenêtre ECAM, un IOAPIC et le remappage IRQ annoncé ont été relevés. Le NIC et le SATA relèvent de l’unité candidate par défaut U2, sans correspondance RMRR ou ATSR détectée **pour ces deux fonctions**. L’hyperviseur était présent, VBS actif et la protection DMA du noyau Windows déclarée désactivée. UEFI et Secure Boot étaient actifs, SetupMode à zéro. Ces observations décrivent Windows ou le firmware déclaré au moment de la collecte. [Synthèse expurgée 5D](../killer-e2400-5d/README.md) · [Données déjà extraites par 5C](../killer-e2400-5c/inventaire/collecter.ps1) · [Décodage des scopes 5C.1](../killer-e2400-5c/inventaire/acpi-5c1.md)

| Sujet | Complément strictement utile | Source recevable et limite |
|---|---|---|
| USB et xHCI | Dénombrer les contrôleurs visibles, identifier localement segment/bus/fonction et ponts, associer le port envisagé à un contrôleur par la route de hubs. Déterminer interface/protocole et LUN du support candidat lorsque disponibles. | Exploiter d’abord classes et parentés de l’arbre privé 5C. La propriété LocationPaths d’un périphérique **déjà présent** peut compléter la route vue par Windows. Sans périphérique attribuable au port, laisser la correspondance inconnue. Ni insertion de clé ni tentative d’amorçage pour la déduire. |
| VT-d et DMA | Recouper chaque RID candidat xHCI et ses alias avec le segment, les DRHD explicites, les ponts puis la DRHD par défaut du segment. Vérifier RMRR/ATSR sur ce chemin et les autres demandeurs pouvant atteindre RAM/GOP. | Utiliser le fichier privé déjà décodé de 5C.1. Une frontière de segment ou une portée ambiguë garde le résultat indéterminé. BME, TES, tables réellement actives, caches, anneaux et transferts avant chargement nécessitent une autre autorité. Ni VBS ni des domaines différents sous Windows ne ferment cette porte. |
| Amovible UEFI | Confirmer pour la version de firmware recensée le chemin x64 par défaut, l’existence d’un choix externe ponctuel sans changement de configuration, la chaîne d’admission du PE exact et les restrictions OEM. | Examiner documentation OEM de la révision et politique déjà inventoriée. L’outil 5G résume des autorités nommées pour la question historique. Il ne prouve pas que le PE 5R, ou un futur PE NovaOS, serait admis ici. Les entrées Windows actuelles ne prédisent pas la résolution du prochain chemin EFI. |
| Affichage GOP | Identifier le producteur de la console pré-OS, le panneau visé, une surface CPU accessible, format/stride/étendue, propriétaire et exclusion DMA, y compris GPU intégré ou discret s’il intervient. | La classe Display et ses parents donnent des candidats. La résolution du bureau, le GPU utilisé par Windows et une photo du logo ne prouvent aucune adresse GOP ni exclusivité au boot. Demander des preuves de plateforme pour le mode pré-OS et la remise sûre. |
| Repli et watchdog | Documenter l’issue du rejet du premier PE, de l’absence de clé, du timeout, du retour EFI et du reset. Établir qui peut arrêter le firmware et signaler la cause sans charger Windows. | Documentation et qualification indépendante d’un profil représentatif. L’ordre de boot ou le manuel d’une gamme ne constitue pas une trace d’exécution. Décrire « inconnu » si le fabricant ne publie pas cette politique. |
| Disques internes et retour | Déduire du rapport privé tous les disques internes et leurs contrôleurs, repérer les rôles système/boot, la protection BitLocker consignée et la disponibilité d’une procédure de récupération détenue par le propriétaire. | Le relevé 5C couvre ces champs. Garder les données brutes privées. Seuls un contrôle indépendant des commandes et un arrêt attribuable pourront qualifier les futures bornes. L’absence de changement du système de fichiers ne suffit pas. |

### Lecture privée de la copie existante

Windows PowerShell 5.1 x64. Dans une console privée, définir `$fichier5C` comme le chemin local du JSON existant. Le bloc ci-dessous lit cette copie. Il affiche le nombre de candidats xHCI et signale des propriétés de classe manquantes. Un résultat à zéro ou incomplet reste indéterminé. Les objets candidats restent dans la variable locale pour consultation privée.

```powershell
$r = Get-Content -LiteralPath $fichier5C -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
if ($r.schema -ne 'novaos.inventaire-e2400/5c' -or -not $r.arbre) {
    throw 'Inventaire 5C absent ou incompatible.'
}
$pci = @($r.arbre | Where-Object { $_.id -like 'PCI\*' })
$xhci = @($pci | Where-Object {
    $_.CompatibleIds.etat -eq 'observe' -and
    @($_.CompatibleIds.donnees) -contains 'PCI\CC_0C0330'
})
$incomplets = @($pci | Where-Object {
    $_.CompatibleIds.etat -ne 'observe' -or -not $_.CompatibleIds.donnees
})
[pscustomobject]@{
    CandidatsXhciVisibles = $xhci.Count
    ClassesPciIncompletes = $incomplets.Count
    Etat = if ($xhci.Count -eq 0 -or $incomplets.Count -gt 0) { 'indetermine' } else { 'candidats Windows' }
}
```

Pour chaque candidat, consulter dans `$xhci` les propriétés 5C `BusNumber`, `Address`, `LocationPaths` et `parent`. Pour un disque externe **déjà inventorié**, partir de son identifiant PnP dans `disques_pnp`, suivre les parents existants dans `arbre` jusqu’au contrôleur et rapprocher les segments des chemins PCI/USB. Arrêter sur boucle, lien manquant, divergence ou profondeur excessive. Un identifiant Windows d’instance n’est pas une incarnation authentifiée du futur média UEFI. Le segment PCI doit être établi indépendamment en cas d’ambiguïté.

### Propriétés PnP manquantes uniquement

Le complément suivant est facultatif. `$idPresent` désigne **localement** un nœud déjà présent dont la propriété manquait au rapport. Ne transmettre aucune des valeurs `Data`. Une erreur de lecture, un chemin vide ou une propriété absente laisse l’information inconnue. Ce bloc a été relu et analysé syntaxiquement sur l’hôte de laboratoire. Aucune commande PnP n’a été exécutée sur le MSI.

```powershell
Get-PnpDeviceProperty -InstanceId $idPresent -KeyName 'DEVPKEY_Device_LocationPaths' -ErrorAction Stop |
    Select-Object -ExpandProperty Data
Get-PnpDeviceProperty -InstanceId $idPresent -KeyName 'DEVPKEY_Device_Parent' -ErrorAction Stop |
    Select-Object -ExpandProperty Data
```

L’échange avec la MOE se limite au nombre de candidats, à la route directe ou via hub, à la présence d’ambiguïtés, à l’unité candidate symbolique et à la disponibilité des preuves externes. Les chemins complets, numéros de série, GUID d’instance ou de disque, adresses, variables, clés de récupération et captures restent locaux. Aucun nouvel inventaire global, accès au disque brut, changement PowerShell, commande de boot, manipulation BitLocker ou branchement de support n’est nécessaire en 5S. [Propriété LocationPaths](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/devpkey-device-locationpaths) · [Lecture des propriétés PnP](https://learn.microsoft.com/en-us/powershell/module/pnpdevice/get-pnpdeviceproperty)
