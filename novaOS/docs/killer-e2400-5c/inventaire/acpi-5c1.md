# MSI-DESKTOP — Collecte ACPI autonome 5C.1

Le relevé Windows communiqué par la MOA identifie le GE62 6QC, le Killer derrière son pont PCIe et le SATA sur une autre branche visible. Le firmware déclare UEFI/Secure Boot actif et SetupMode à zéro. La présence de l’hyperviseur, les indicateurs DeviceGuard et la protection DMA du noyau Windows signalée désactivée restent des observations Windows. La question ouverte est de savoir quelles unités **le firmware décrit** pour ces deux branches.

## Lancer sur le MSI

1. Copier le seul fichier [acpi-5c1.ps1](acpi-5c1.ps1) dans un dossier local privé. Ouvrir **Windows PowerShell 5.1 64 bits** dans ce dossier, avec le compte de l’opérateur. Utiliser une session locale sans transcription ni partage. Vérifier que le profil local et son dossier AppData local ne sont ni synchronisés ni redirigés vers un partage. Le programme prépare son propre sous-dossier sur disque fixe et restreint ses permissions au compte courant.
2. Contrôler l’empreinte avec `(Get-FileHash -Algorithm SHA256 -LiteralPath .\acpi-5c1.ps1).Hash`. Valeur attendue pour cette livraison : `bcd868f7c00f1c1c5c36fae3e4f98453ee8269c74fa18d2b198f9efc493b3d81`.
3. Lancer la commande ci-dessous. Elle utilise la politique PowerShell en vigueur. Si la politique de l’organisation refuse le fichier ou interdit la compilation locale par `Add-Type`, arrêter et faire approuver le fichier par le canal habituel. Le script n’exige aucun module téléchargé. Son adaptateur C# appelle deux fonctions de kernel32. Le rapprochement PCI emploie le fournisseur PnP livré avec Windows ; son indisponibilité laisse les chemins indéterminés.

```powershell
& "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -File ".\acpi-5c1.ps1"
```

4. Ouvrir le dossier affiché, de forme `%LOCALAPPDATA%\NovaOS-ACPI-<identifiant aléatoire>`. **Relire et transmettre uniquement `synthese-moe.json`.** Aucune transmission automatique. Garder localement `local.json` et les fichiers `DMAR.bin`, `MCFG.bin`, `APIC.bin` obtenus. Les octets complets lus sont conservés même si la validation échoue. En cas d’arrêt avant la création de la synthèse, la collecte est incomplète. Un nouvel essai crée un dossier neuf.

## Interpréter

| Pièce | Résultat |
|---|---|
| Tables brutes, privées | Énumération ACPI et lecture en deux appels (taille puis contenu). Les quatre caractères d’identifiant de table sont interprétés en petit-boutiste. Une signature présente plusieurs fois est signalée `multiple` et aucun exemplaire n’est choisi : l’API Windows ne permet de lire que le premier pour cet identifiant. Absence, échec d’énumération et échec de lecture sont distincts. [Énumération Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-enumsystemfirmwaretables) · [Lecture Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getsystemfirmwaretable) |
| DMAR dans `local.json` | Largeur d’adresse hôte, drapeaux, remappage IRQ déclaré. Par DRHD : unité symbolique, segment, base et INCLUDE_PCI_ALL. Par RMRR : segment, intervalle inclusif, scopes. Par ATSR : segment, ALL_PORTS, scopes. Chaque scope conserve type, bus initial, identifiant d’énumération et couples périphérique/fonction décodables. Un type non interprété laisse le décodage partiel. [Spécification Intel VT-d, chapitre 8](https://cdrdv2-public.intel.com/671081/vt-directed-io-spec.pdf) |
| MCFG et APIC dans `local.json` | Pour chaque fenêtre ECAM : base, segment et intervalle de bus. Pour la MADT : base LAPIC et drapeaux, LAPIC/x2APIC, IOAPIC et base GSI, overrides de sources et de base LAPIC, NMI local. Les autres structures MADT sont inventoriées par type et longueur. Ces données ne déterminent pas le routage MSI actif du NIC. [Spécification ACPI](https://uefi.org/sites/default/files/resources/ACPI_Spec_6_5_Aug29.pdf) |
| Synthèse MOE | États des trois tables, nombres d’unités/fenêtres/IOAPIC, unité(s) candidate(s) pour chaque cible, portée RMRR/ATSR potentielle et relation entre les candidats. Les identifiants `U1`, etc. désignent des entrées dans ce relevé. Les adresses, chemins, erreurs système détaillées et octets OEM restent dans le dossier privé |

Le collecteur exige une signature exacte, une longueur d’en-tête identique à la lecture et une somme nulle modulo 256. Il vérifie les limites des sous-structures avant de les décoder, y compris limites de scope, de chemin PCI, de plage réservée et de fenêtres ECAM. Les bornes de travail sont de 1 Mio par lecture, 4 096 entrées DMAR ou MADT, 256 scopes par sous-structure et 256 fenêtres MCFG. Une table rejetée ne participe pas au rapprochement. Le statut `valide` couvre ces vérifications et les champs interprétés.

Le Killer doit correspondre à l’identité et au sous-système de 5C ainsi qu’au chemin visible confirmé `00:1c.3 → 03:00.0`. Le SATA doit être un contrôleur PCI de classe 01/06 unique. Le collecteur remonte les parents PnP jusqu’à la racine PCI ACPI et recherche un segment MCFG compatible avec tous les bus du chemin. Il n’attribue ni segment zéro par convention ni BDF présumé au SATA. Dans le segment candidat, un chemin endpoint doit aboutir à la cible ; un scope de pont couvre le sous-arbre correspondant. Un DRHD explicite est prioritaire sur le DRHD par défaut. Les entrées IOAPIC et HPET ne désignent pas des endpoints réseau ou SATA. ALL_PORTS de l’ATSR concerne des branches de ports racines. Le rapprochement via pont est une indication à corroborer.

`indetermine` couvre notamment une topologie PnP incomplète, un SATA sous RAID/VMD non reconnu comme AHCI, des segments ou DRHD concurrents et un type DMAR non traité. Deux candidats distincts ne prouvent pas une séparation DMA effective. Une unité candidate commune ne prouve pas un domaine commun. La collecte ne lit aucun registre VT-d, BAR, IRQ ou configuration PCI active. Aucune conclusion sur les alias, les mappings réels, la continuité au transfert UEFI ou le drainage sous NovaOS.

## Qualification du collecteur

Base `eefb4d3c9d47dd100613c1bc5016235652292ab3`. Le [banc fictif](essai-acpi-5c1.ps1) appelle les mêmes fonctions d’acquisition et de décodage avec un fournisseur mémoire et des données PnP fictives. Sur Darwin ARM64 avec PowerShell 7.7.0, les déclarations C# P/Invoke ont été compilées, les deux appels de taille/contenu simulés, les octets sauvegardés puis relus, et les associations explicite/par défaut/ambiguë vérifiées. Les essais couvrent les refus de tables absentes, multiples, tronquées, incohérentes ou hors limite et les erreurs API injectées. [Journal des contrôles](qualification-acpi-5c1.txt)

Les 32 contrôles figurent dans le journal. La compatibilité visée est Windows PowerShell 5.1 Desktop x64. La première exécution réelle sur le MSI doit encore confirmer le chargement P/Invoke, l’accès kernel32 et PnP, les permissions NTFS du dossier privé et les tables retournées par ce firmware. Aucun accès au MSI ni démarrage QEMU dans ce lot. Pour rejouer uniquement les vecteurs sur un hôte de test, utiliser `pwsh -NoProfile -NonInteractive -File ./essai-acpi-5c1.ps1` depuis ce dossier.
