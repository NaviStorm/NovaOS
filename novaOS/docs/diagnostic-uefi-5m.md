# NovaOS v0.25.0 — Phase 5M — Transfert de confiance UEFI

**Les 16 transitions du premier EFI NovaOS à son arrêt sont admises sur le Q35 de laboratoire.** Avec Secure Boot actif et inactif, le diagnostic accepte le reçu valide et refuse les cinq formes de reçu inutilisables ainsi que les manifestes et ELF invalides. L’observateur relève zéro appel à `GetVariable`, zéro écriture APM, zéro entrée SMM et zéro E/S du namespace interne dans ces 16 intervalles. Les 35 écritures EOI sont toutes attribuées à la minuterie locale de vecteur `0x20`. La politique d’écriture de 5K et le contre-exemple 5L restent applicables. [Preuves et empreintes](validation/diagnostic-uefi-5m/SHA256SUMS) · [Recette](../boot/diagnostic-5m/README.md)

## Autorité et choix du transfert

Le BDS du firmware OVMF 5J a déjà deux fonctions nécessaires. Il contrôle la politique avant le lancement et transmet 32 octets d’options au PE qu’il a chargé depuis son disque RAM. Déplacer `GetVariable` dans un nouveau premier EFI NovaOS aurait conservé les écritures APM dans la frontière à qualifier. Ajouter un nouveau fournisseur au firmware aurait augmenté le code à auditer. Le profil 5M réutilise le firmware existant, ajoute un reçu signé à la fin du support volatile et conserve un seul PE NovaOS. Le PE est identique dans les deux modes. [Étude et observation du service de variables en 5L](diagnostic-uefi-5l.md)

1. Le superviseur de laboratoire détient la VM froide, sa mémoire, la ROM, les deux canaux de contrôle et les entrées PIO. Il vérifie les SHA des outils et des images de qualification, la copie du magasin OVMF, `SecureBootEnable`, la présence de PK et l’absence d’`AuditMode` persistant. Le BDS épinglé exige `SetupMode=0`, lit l’état actif du firmware et journalise le résultat de `LoadImage`. Le superviseur confronte ce résultat au mode attendu au premier RIP, avant de laisser exécuter la première instruction. Pour cette révision OVMF, les lectures 5L de `AuditMode` ont renvoyé `EFI_NOT_FOUND` dans les deux modes. La valeur d’absence codée dans le reçu repose sur ce profil et sur la vérification du magasin. [Firmware et chaîne 5J](diagnostic-uefi-5j.md)
2. L’autorité Ed25519 NovaOS de laboratoire signe un corps de **112 octets**. Il contient le schéma, les états Secure Boot, Setup et Audit, l’origine du support, la position et la longueur du PE, une époque aléatoire de 128 bits, le SHA-256 du fichier PE complet et la clé publique de l’émetteur. La signature détachée porte le reçu à **176 octets**. La clé de réception est épinglée dans le PE et distincte des clés UEFI et du catalogue. La clé privée reste chez le dépositaire de laboratoire. Le PE exige la clé épinglée et la signature stricte, y compris lorsque le canal d’options et un reçu étranger sont cohérents entre eux. [Autorité de reçus](../boot/diagnostic-5m/transfert.py) · [Vérification dans le PE](../boot/diagnostic-5m/entree.rs)
3. Pour chaque VM, l’autorité produit un engagement SHA-256 du corps signé avec un préfixe réservé au transport OVMF 5J et au chemin `EFI/BOOT/BOOTX64.EFI`. Le BDS le transmet dans `LoadOptions`. L’autorité n’accorde la prise du bail qu’une fois, après contrôle du vrai fichier sur le FAT, de la DMAR, des demandeurs et de l’installation effective des domaines. Le reçu est préparé avant le lancement de la VM. La signature seule n’atteste pas que VT-d est déjà installé. Le contrôle de libération du vCPU en apporte la preuve sur le banc. Au premier RIP, l’observateur vérifie les cinq sections du PE en mémoire, y compris les données relocalisées et la zone initialisée à zéro. Il revérifie également les tables installées avant le chargement.
4. Le PE accède au protocole de son image, à la carte mémoire et au chemin de son support. Il exige un disque RAM UEFI volatile, unique dans ce chemin, des étendues de RAM admises et des tailles bornées. Il lit le reçu en mémoire, vérifie son engagement et sa signature, mesure les octets du PE sur le disque RAM et consomme son droit une seule fois. Il utilise ensuite le catalogue, la validation des manifestes et le vérificateur `pkg01` existants. Aucune méthode de disque, de console, de réseau ni de service runtime n’est nécessaire au diagnostic. Il publie son résultat en RAM et attend en HLT en laissant les interruptions actives. [Chemin de disque RAM UEFI](https://uefi.org/specs/UEFI/2.10/10_Protocols_Device_Path_Protocol.html)

Le signataire de reçus est commun à cette livraison de laboratoire. L’époque et le bail sont propres à chaque instance. La fraîcheur dépend de la possession exclusive du canal de transfert, de la VM et de la RAM par l’autorité qui a créé cette époque. Une restauration de snapshot rembobinant aussi l’autorité ou un autre firmware libre de réintroduire un ancien engagement sortent du modèle. En mode inactif, le résultat indique explicitement une chaîne UEFI non authentifiée, même lorsque le reçu, le catalogue et les paquets sont authentiques.

## Arbitrage sur le magasin Secure Boot

La MOE a autorisé l’ajout des seules empreintes Authenticode nécessaires sur une copie jetable du magasin actif 5J. Les trois PE de test sont admis individuellement par empreinte. Le PE nominal fait **233 472 octets** et son SHA-256 complet est `9442959bb9dcfcccba17c8f988684af6615f38e45325fd114c989973ce39a4d7`. Une seconde construction dans un répertoire neuf a produit exactement les mêmes octets. [Manifeste des trois images](../boot/diagnostic-5m/images-laboratoire.json) · [Règles d’autorisation UEFI](https://uefi.org/specs/UEFI/2.10/32_Secure_Boot_and_Driver_Signing.html)

| Variable ou support | Différence vérifiée |
|---|---|
| PK, KEK, dbx, CustomMode et SecureBootEnable | Valeurs, attributs et horodatages identiques. |
| db | Les 791 octets de données d’origine sont conservés. Une liste EFI SHA-256 de 172 octets est ajoutée pour les trois PE. La taille devient 963 octets. L’horodatage et les attributs de db sont conservés. |
| Flash active de 540 672 octets | 312 positions diffèrent, incluant la longueur de db et le déplacement des enregistrements suivants. Le vérificateur reconstruit la flash complète attendue depuis l’original et compare chaque octet. |
| SHA-256 de l’original actif 5J | `e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b` |
| SHA-256 de la copie active 5M | `dd0e83543b732a6b0497e63f106fb064c25b0034591c84d471e585d901a0d671` |
| Magasin inactif | Copie exacte du magasin inactif 5J, SHA-256 `8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0`. |

Une quatrième image de contrôle, absente de db, a provoqué `EFI_ACCESS_DENIED` dans `LoadImage`. Le BDS a rendu ses pages de disque RAM puis atteint sa boucle terminale sans lancer le PE ni une image interne. Cette qualification couvre l’autorisation par empreinte. La signature d’une distribution et son admission par un firmware physique restent ouvertes.

## Matrice des 21 décisions QEMU

Les colonnes donnent les résultats du même scénario dans deux VM froides. Les valeurs numériques sont les statuts opérationnels du PE. Les cinq décisions antérieures à l’entrée du PE sont comptées séparément des 16 transitions exécutées.

| Scénario | Secure Boot actif | Secure Boot inactif |
|---|---|---|
| Reçu actuel, PE nominal | 0, confiance UEFI active | 0, chaîne UEFI non authentifiée |
| Signature du reçu altérée | 25, confiance indéterminée | 25, confiance indéterminée |
| Clé étrangère, signature et engagement cohérents | 25, signataire refusé | 25, signataire refusé |
| Champ réservé non nul, reçu correctement signé | 26, schéma refusé | 26, schéma refusé |
| Reçu absent | 23, confiance indéterminée | 23, confiance indéterminée |
| Reçu valide de l’époque précédente, engagement actuel | 24, époque refusée | 24, époque refusée |
| Ancien reçu et ancien engagement présentés ensemble | Bail refusé avant libération du vCPU | Même refus |
| PE autorisé pour un autre scénario, substitué au nominal | Identité refusée avant libération du vCPU | Même refus |
| Manifeste altéré dans un PE admis | 6, signature interne refusée | 6, signature interne refusée |
| ELF invalide, empreinte attestée par ses manifestes | 18, paquet refusé | 18, paquet refusé |
| PE hors de db présenté à OVMF | `EFI_ACCESS_DENIED`, aucun premier RIP NovaOS | Le contrôle de substitution ci-dessus couvre le mode inactif. |

Les deux cas nominaux exercent également une deuxième demande de prise du même bail dans la même VM. Elle est refusée dans les deux cas. Les huit chemins exécutés par mode gardent les interruptions naturelles. Le PE nominal utilise 59 584 octets de son allocateur pendant le diagnostic. Les refus de confiance en utilisent 2 880 et les refus de catalogue 20 755. Le compteur terminal est de zéro allocation vivante sur chaque chemin. Le moniteur peut lire l’état de confiance, le statut du diagnostic et l’époque dans la RAM de la VM. La lecture humaine sur un affichage physique reste à qualifier.

## Étendue des observations

- **Directement sous QEMU.** Les points d’arrêt d’exécution sont armés depuis le reset pour la première instruction du PE retenu. Le point d’entrée, les cinq sections relocalisées, le RIP et le HLT sont recoupés avec les octets du fichier. Les deux lancements préalables servant à localiser les adresses sont exclus des verdicts. Un point d’arrêt sur la fonction `GetVariable` de la table runtime n’a été atteint dans aucun intervalle. Les traces CPU/APM et de transactions périphériques, les acquittements xAPIC attribués, les compteurs QMP et les reçus RAM se recoupent. La fenêtre se termine après la livraison d’une minuterie naturelle au HLT.
- **Avant le premier EFI.** Le BDS PIO historique contient du code spécifique au laboratoire dans OVMF. Les deux traces nominales enregistrent **343 entrées SMM avant le PE en mode actif et 328 en mode inactif**. Le firmware consulte ses propres variables. La borne de qualification commence au premier PE NovaOS. Le banc n’introduit aucun nouvel étage EFI en amont de cette borne. Le résultat conserve les contre-exemples historiques et ne qualifie aucune politique « zéro SMM depuis le reset ».
- **DMA et stockage.** Une unité VT-d Q35 dessert les trois domaines indépendants NVMe, NIC et EDU. Avant que le vCPU quitte le reset, seuls quatre cadres de données réservés au banc sont mappés, avec des permissions directionnelles. Les pages du PE, du disque RAM, des options et des tables en sont exclues. Les 14 pages de tables restent identiques et le décodeur indépendant retrouve les trois domaines. PCI et les registres VT-d comparés sont stables. Les compteurs du namespace fictif restent à zéro, son SHA avant/après est identique et les compteurs d’écriture flash restent stables pendant les transitions. La garde de coexistence réseau/stockage demeure celle des profils historiques.
- **Contre-lecture hôte.** 44 tests réussis sur ARM64 Darwin natif et 44 sous Rosetta x86_64, dont six contre-exemples 5M. Les 21 décisions sont relues par les deux interpréteurs, avec vérification Ed25519 par OpenSSL 3 natif. L’ABI canonique conserve 48 sélecteurs. Les preuves 5I, 5J, 5K et 5L ont été contrôlées sur leurs manifestes d’origine. [Journal hôte](validation/diagnostic-uefi-5m/hotes.txt) · [Contre-lecture indépendante](../boot/diagnostic-5m/verifier.py)

## Portes avant qualification physique

| Garantie requise | Écart restant |
|---|---|
| Autorité précoce et fraîcheur sur la machine | Identifier une autorité authentifiée antérieure au premier EFI qui lie l’origine, le fichier chargé et l’époque, préserve la propriété de la mémoire et empêche le rejeu des deux canaux ensemble. Le superviseur QEMU, son verrou et son débogueur ne sont pas disponibles sur la machine. |
| Image générique et Secure Boot optionnel | Le même binaire fonctionne dans les deux modes du banc. Il faut un producteur compatible du transfert sur matériel réel, indépendant du modèle et de toute autorité commerciale. En mode actif il faut également établir que la politique du propriétaire admet l’image de distribution exacte. Le refus doit être final si l’autorité ou le support est inconnu. |
| DMA dès avant chargement | Inventorier demandeurs, fonctions sœurs, alias, réservations, transactions en vol, caches et propriétaires. Prouver l’exclusion des pages NovaOS et du reçu depuis avant chargement, avec conservation du domaine de stockage. Une description ACPI ou un bit TES seul sont insuffisants. |
| Windows, disques et démarrage interne | Établir indépendamment les accès du firmware antérieurs à l’entrée, l’absence d’écriture ou de lancement interne imprévu, puis zéro E/S interne jusqu’à l’arrêt. Qualifier le véritable chemin de refus du firmware, le support amovible et la récupération de la machine avec ses protections en place. |
| Restitution du verdict à l’opérateur | Établir un affichage fiable distinguant mode actif, mode inactif, confiance indéterminée et refus de paquet. Son autorité et ses accès doivent respecter la même frontière d’E/S et de DMA. |
| Autorisation | A8 reste fermé. La livraison se limite au laboratoire x86_64, sans support physique. |

Base 5L `c072eeda7e8ce750aecdebeb417c33137ef66771`. Les références au firmware, au modèle Q35 et aux adresses relocalisées appartiennent au banc. Le PE ne contient aucune adresse PCI ni topologie de machine. [Frontière firmware de 5F](diagnostic-uefi-5f.md) · [Protection précoce 5J](diagnostic-uefi-5j.md) · [Critère APIC 5K](diagnostic-uefi-5k.md)
