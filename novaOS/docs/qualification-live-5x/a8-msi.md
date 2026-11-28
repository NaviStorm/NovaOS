# Fiche A8 · Un démarrage STANDARD sur MSI GE62 6QC

**Statut au 28 novembre 2026 — candidature préparée, A8 fermé.** Le profil visé est un inventaire passif et terminal. Le dernier état communiqué du MSI indique UEFI et Secure Boot actifs, SetupMode à zéro. Le chemin recommandé est de conserver la politique actuelle et d’y établir l’admissibilité de l’exécutable exact. À défaut, différer l’essai. [Rapport et empreintes 5X](../diagnostic-uefi-5x.md)

## Support et image à lier au futur mandat

| Champ de l’autorisation | Valeur séparément vérifiable |
|---|---|
| Machine, portée | MSI GE62 6QC, BIOS inventorié E16J5IMS.11D. Une personne sur place, un seul démarrage ponctuel sur USB. |
| Exécutable | PE STANDARD x86_64 générique de 297 984 octets. SHA-256 brut `da718b3944579c3b53e82c1f489eeecd7899ea5097b8a899bb5f884e49665355`. |
| Disque de référence | [Image compressée](../validation/diagnostic-uefi-5w/novaos-standard-68mio.img.gz) · SHA-256 du disque décompressé de 71 303 168 octets `79ee5d200f5e90bca19be8f06011dd8cd0b080f086e4f300005a338b55e20060`. |
| Clé dédiée | **À renseigner en privé** par le propriétaire lors d’une préparation ultérieure autorisée. Identification non ambiguë de la clé entière que le propriétaire accepte d’effacer, bus externe USB, capacité exacte, secteurs logiques de 512 octets. |
| Disque destiné à cette clé | **À figer avant autorisation d’écriture.** Si sa capacité diffère, générer une image ordinaire à cette capacité et y relire la GPT secondaire au dernier LBA, les deux FAT32 et le PE exact. Consigner capacité et SHA-256 du disque brut dans l’autorisation. Le SHA des 68 Mio ne couvre que cette géométrie. |
| Contrôle après écriture future | Condition préalable au démarrage. Relire tout le support autorisé, comparer sa taille et son SHA-256 à l’image figée et confirmer l’identité de la clé jusqu’à l’éjection. Un changement de cible, un remontage qui modifie la FAT ou une lecture partielle invalide la préparation. [Procédure macOS 5W](../qualification-live-5w/operateur.md) |

## Décision Secure Boot et sauvegarde Windows

- **Actif sur le MSI inventorié.** Le PE livré ne porte aucun certificat Authenticode embarqué. L’autorisation par empreinte sous OVMF utilise le digest Authenticode `9309d587a9f6886a33d21bb7da49b2baa4a6c84d1e41012d5d32015f8cc38834`. Réunir une preuve applicable à ce digest, à la politique effective d’amorçage amovible du MSI et aux révocations `dbx`. Le nom d’une autorité dans `db`, le SHA du fichier ou un succès OVMF ne suffisent pas. Si l’image est refusée, l’EFI NovaOS ne reçoit pas le contrôle. Aucune clé à enrôler pour cet essai. [UEFI 2.11, validation et révocation des images](https://uefi.org/specs/UEFI/2.11/32_Secure_Boot_and_Driver_Signing.html)
- **Inactif sur une plateforme déjà dans cet état.** Une décision MOA spécifique peut accepter STANDARD avec la chaîne de démarrage déclarée non authentifiée. Les signatures internes restent vérifiées par le PE exécuté. Une substitution du PE lui-même pourrait aussi remplacer ce vérificateur. Le passage du MSI en mode inactif ne fait pas partie de cette candidature.
- **Préparation Windows par le propriétaire.** Vérifier une sauvegarde récupérable et la disponibilité d’une procédure de récupération Windows. Pour chaque volume chiffré concerné, vérifier en privé que la clé BitLocker correspondante peut être retrouvée depuis un autre appareil ou hors ligne. Inscrire seulement « récupération vérifiée » dans la fiche. Garder la clé, les identifiants et les sauvegardes hors des preuves partageables. [Récupération BitLocker](https://support.microsoft.com/en-us/windows/security/encryption/find-your-bitlocker-recovery-key)

## Décisions explicites de la MOA avant A8

| Acceptation à consigner indépendamment | Portée et critère d’abandon |
|---|---|
| Lectures du firmware avant l’EFI | Accepter ou refuser que le firmware lise le disque interne depuis le reset en examinant ses options. Le témoin nominal QEMU compte déjà 6 lectures. Leur nombre et leur contenu sur le MSI sont inconnus. Si ces lectures sont interdites par la MOA, différer. |
| Rejet USB et repli | Choisir explicitement si une tentative de lecture interne et un lancement éventuel de Windows ou de sa récupération sont acceptables après rejet, avec leurs E/S possibles. Le témoin QEMU a fait 58 lectures depuis le reset sur un SATA vierge. Une arrivée dans Windows indique un repli possible et ne démontre aucune absence d’accès interne. Si ce comportement est inacceptable et ne peut être exclu par la politique existante, différer. |
| Confiance STANDARD | Accepter la protection DMA préchargement indéterminée, la confiance placée dans la console et les services firmware et le caractère volatil des résultats. Aucune autorité de laboratoire QEMU n’est disponible sur le MSI. |
| Abort et autorité opérateur | Nommer en privé la personne habilitée à abandonner. Fixer avant l’essai le délai d’attente d’écran et la conduite applicable à un écran noir d’origine indéterminée. Prévoir une alimentation stable. Un incident termine l’unique tentative. |

## Déroulement réservé au futur mandat

1. Terminer les travaux Windows et obtenir un arrêt normal. Vérifier la correspondance entre la clé, le SHA du disque relu, le PE et la fiche approuvée. Si la sauvegarde, la récupération, la politique d’admission ou l’identité du support restent inconnues, s’arrêter avant amorçage.
2. Employer le menu de démarrage ponctuel du MSI. Vérifier l’unique entrée UEFI correspondant à la clé approuvée. En présence d’entrées ambiguës, quitter le menu. Conserver les paramètres permanents et l’ordre de démarrage existants. [Procédure opérateur de référence](../qualification-live-5w/operateur.md)
3. Sur l’écran NovaOS, relever le titre STANDARD, le mode Secure Boot attendu, l’origine USB déclarée, l’état DMA, les catégories de contrôleurs, le nombre d’activations (zéro), la dernière ligne et tout code de refus. L’attendu nominal se termine par « Inventaire termine ». Relever manuellement ou photographier uniquement le texte utile avec accord du propriétaire. Le journal RAM disparaît à l’extinction. NOVALOGS reste vide.
4. Abandonner sur refus, origine indéterminée, confiance inattendue, écran illisible, diagnostic incomplet, activation non nulle, redémarrage ou apparition d’une récupération imprévue. Un écran noir n’atteste ni un refus 17 ni l’arrêt CPU. Si Windows a déjà démarré, le laisser rejoindre un état sûr puis effectuer un arrêt normal. Pour un écran indéterminé appliquer la procédure décidée avec le propriétaire. Aucune réparation de volume, saisie de clé dans un écran inconnu ou tentative USB supplémentaire.
5. Une fois l’arrêt local NovaOS identifié, éteindre suivant la procédure convenue. Retirer la clé une fois la machine éteinte, puis démarrer volontairement Windows. Relever le retour à la session habituelle ou confier la récupération au propriétaire. Distinguer ce démarrage volontaire d’un repli intervenu avant le diagnostic.

**Avis MOE.** Le candidat logiciel est prêt pour instruction de la demande. La signature de distribution, l’autorisation du digest par le MSI, la propriété DMA antérieure, le comportement réel de repli et les effets indirects de son firmware ne sont pas qualifiés. Pour le profil STANDARD, la MOA peut accepter l’incertitude DMA et les risques firmware décrits. L’admission avec Secure Boot actif doit respecter la politique effective de la machine. Restent à consigner le support exact, la preuve de récupération et les deux décisions relatives aux lectures et au repli. Toute exécution physique relève d’un mandat distinct.
