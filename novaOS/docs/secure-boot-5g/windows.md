# Inventaire local db/dbx sous Windows

**Résultat attendu.** Une synthèse JSON indiquant si les deux CA applicatives Microsoft connues apparaissent dans db ou dbx, et si le firmware Windows rapporte Secure Boot actif. L’inventaire ne prononce jamais l’admissibilité du MSI. Le fichier livré au dépôt est le collecteur, sans résultat de machine physique.

## Procédure opérateur

1. Depuis la révision 5G vérifiée, placer uniquement `synthese.ps1` dans un répertoire local privé et non synchronisé. Comparer son SHA-256 à celui du fichier de cette révision, par exemple avec `Get-FileHash -Algorithm SHA256 .\synthese.ps1`. Utiliser Windows PowerShell Desktop 5.1 **64 bits**, lancé en administrateur pour la lecture des variables protégées. Les commandes appartiennent au module Windows intégré SecureBoot.
2. Depuis ce répertoire, exécuter la commande ci-dessous. La politique d’exécution existante doit permettre le script vérifié. Si elle le bloque, consigner « indéterminé » et arrêter. Les bases restent en RAM. Le script écrit uniquement le JSON sur la sortie standard. Il ne crée aucun fichier, n’effectue aucune requête réseau et n’envoie aucun résultat.
3. Relire le JSON affiché. Pour la MOE, copier **uniquement cet objet**. Exclure capture d’écran, transcription de console, nom d’utilisateur, chemin et contenu des variables. Une lecture impossible ou une base malformée doit conserver son état `indetermine`. La procédure nécessite une exécution locale ultérieure par l’opérateur. [Get-SecureBootUEFI](https://learn.microsoft.com/en-us/powershell/module/secureboot/get-securebootuefi) · [Confirm-SecureBootUEFI](https://learn.microsoft.com/en-us/powershell/module/secureboot/confirm-securebootuefi)

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -File ".\synthese.ps1"
```

## Interprétation

| Champ dans db ou dbx | Signification |
|---|---|
| `etat=lu` | Structure EFI_SIGNATURE_LIST parcourue jusqu’au dernier octet. `indetermine` couvre notamment variable vide ou absente, accès refusé, plateforme non prise en charge, taille supérieure à 4 Mio ou structure invalide. Aucun résultat partiel pour cette base. |
| `ca2011_der`, `ca2023_der` | Correspondance exacte du SHA-256 des octets DER d’une entrée X.509 avec Microsoft Corporation UEFI CA 2011 ou Microsoft UEFI CA 2023. Dans dbx, un résultat positif impose une analyse de révocation. |
| `windows2011_der`, `windows2023_der`, `rom2023_der` | Entrées connues de Windows Production PCA 2011, Windows UEFI CA 2023 et Microsoft Option ROM UEFI CA 2023. Leurs usages sont distincts de la signature d’applications tierces. |
| `tbs256`, `ca2011_tbs256`, `ca2023_tbs256` | Présence d’entrées EFI_CERT_X509_SHA256 et correspondance éventuelle du condensat de la partie TBSCertificate pour les deux CA applicatives. La date EFI_TIME et la validité d’un horodatage ne sont pas évaluées. |
| `empreintes_images`, `autres_x509`, `autres_types` | Présence d’empreintes SHA-256, d’octets X.509 non reconnus ou d’autres formats (dont les révocations TBS SHA-384/512). Aucune empreinte locale, identité d’éditeur inconnue, date ni GUID propriétaire n’est publié. |
| `admissibilite=non_etablie` | Valeur systématique, y compris lorsque Secure Boot est actif et la CA 2023 figure dans db. |

Un `false` concerne uniquement le type et les octets publics identifiés par ce champ. Une révocation portant sur le PE, une feuille ou un intermédiaire peut bloquer la chaîne alors que les champs relatifs à la CA sont négatifs dans dbx. La synthèse ne vérifie pas la fraîcheur de dbx, la politique OEM complète, l’état DMA ni le résultat d’un chargement. Seul un contrôle privé avec l’artefact signé définitif et l’ensemble de la politique pourrait instruire l’admission. [Format des listes EFI](https://uefi.org/specs/UEFI/2.11/32_Secure_Boot_and_Driver_Signing.html)

## Qualification du collecteur

**Exécuté le 27 novembre 2026** sur Darwin ARM64 natif, PowerShell 7.7.0, avec les cinq certificats publics Microsoft fournis par virt-firmware 26.10 et des listes EFI fabriquées en mémoire. Les 16 contrôles passent. Ils couvrent les cinq identités DER et leur séparation, les deux identités TBS SHA-256, des db/dbx différentes, une révocation DER, l’échec d’une lecture, un type inconnu, cinq refus de structure ou de taille, et la suppression du détail d’erreur. Le point d’entrée réel exécuté sur l’hôte sans module SecureBoot produit la seule synthèse indéterminée attendue. Un vecteur a détecté une inversion de chiffres dans l’empreinte Option ROM pendant le développement. L’empreinte a été corrigée avant le rejeu complet.

Depuis le dossier du collecteur, avec un répertoire contenant les PEM publics de `virt/firmware/certs/microsoft.com`, la commande suivante rejoue les essais sans module Windows ni accès firmware.

```powershell
pwsh -NoProfile -NonInteractive -File ./essai-synthese.ps1 -CertificatsPublics "/chemin/vers/virt/firmware/certs/microsoft.com"
```

**À vérifier sur Windows.** L’exécution sous Desktop 5.1 x64, les privilèges effectifs, la disponibilité du module Windows et les valeurs du MSI restent non vérifiés. Aucun démarrage, essai physique ou rejeu QEMU en 5G. [Vecteurs de test](essai-synthese.ps1) · [Collecteur](synthese.ps1)
