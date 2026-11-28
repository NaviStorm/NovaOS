# NovaOS v0.25.0 — Réception du transfert USB 5Q par le premier EFI

Sur le profil Q35 qualifié, le premier EFI chargé par xHCI termine son inventaire dans les deux modes Secure Boot et affiche le statut 0. Il reconnaît un support USB authentifié par l’autorité du banc. Le catalogue sélectionne le pilote fictif IOAPIC et signale un conflit entre les deux candidats HPET. Le diagnostic s’arrête sur HLT. La base est `22ea086542421a24b78557270e985d33fa032b62`. [Preuves de la campagne](validation/diagnostic-uefi-5r/observations.zip) · [Rapports JSON et texte](validation/diagnostic-uefi-5r/rapports.zip)

## Autorité et chemin effectivement exercé

1. À froid, le superviseur QEMU installe et relit les traductions VT-d avant de libérer le premier vCPU. Le modèle possède une unité commune et quatre domaines distincts pour le stockage, le réseau, la sonde et xHCI. Les mappings USB sont publiés avant utilisation et leurs permissions dépendent du rôle. Les pages du PE, de la pile, du tampon de transit et de la surface graphique restent hors des cadres prêtés. Les 18 pages de tables sont comparées avant LoadImage, à l’entrée du PE et à l’arrêt. [Protection et limites 5Q](diagnostic-uefi-5q.md)
2. Le firmware 5Q lit l’ESP du support GPT par xHCI. Il prépare un tampon de 33 Mio pour l’ESP et 16 Kio pour les reçus de laboratoire. Le BDS vérifie l’enveloppe USB avant StartImage. Le premier EFI retrouve dans la RAM disque l’image exacte qui a été lue. Le banc compare les cinq sections relogées avant StartImage et sur la première instruction du PE.
3. L’EFI vérifie le reçu Ed25519 5M, l’ancre LoadOptions, le signataire connu, le PE entier et l’usage unique dans cette invocation. Il prend une copie privée du suffixe de 16 Kio. Il vérifie le bail 5O et un bail 5R distinct qui lie le droit de présenter un verdict en lecture seule à l’époque, au PE, à la géométrie, au contrat initial et à l’empreinte de l’image USB entière mesurée par le superviseur. Le droit graphique se limite aux 544 × 120 pixels de la fenêtre sur une surface GOP dont les descripteurs concordent. Le refus de surface intervient avant le premier pixel.
4. L’EFI vérifie lui-même la signature de l’enveloppe 5Q, la fraîcheur, les octets de l’ESP, le PE et sa section de code en mémoire, le chemin UEFI brut, le contrôleur, le port, l’interface, le LUN et l’incarnation. Il compare les compteurs de soumissions et de complétions et reconstruit le hachage des 18 pages VT-d attendues à partir des prêts signés. Les domaines réels sont relus indépendamment par QEMU. L’enveloppe utilisée doit être identique, octet par octet, à celle acceptée par le BDS. Les deux copies relues par le moniteur à la première instruction puis à HLT sont archivées. L’argument de continuité pendant cet intervalle repose aussi sur les traductions, les droits de DMA, l’unique vCPU et le code de ce firmware qualifié.
5. Le catalogue commun vérifie les signatures des manifestes et les empreintes des ELF. Les descripteurs ACPI et chemins PCI des handles firmware sont parcourus avec des bornes. Le plan d’inventaire est publié en une fois après tous les contrôles. Aucun protocole de lancement de pilote n’est invoqué. [Code de réception](../boot/diagnostic-5r/entree.rs) · [Contrat USB](../boot/diagnostic-5r/contrat.rs) · [Inventaire](../boot/diagnostic-5r/inventaire.rs)

Le SHA-256 du PE est `7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5`. Deux compilations dans deux répertoires cibles distincts donnent des fichiers identiques. Le SHA-256 de l’image USB synthétique de 68 Mio est `9440a1030b530174d97e85e4e55cb6566c514dfd3a143a68fff77ae8b5f1b404`.

## Matrice QEMU

| Cas | Secure Boot | Observation et décision |
|---|---|---|
| Nominal et transaction réellement observée en vol puis achevée | Actif et inactif | Statut 0, sept fonctions PCI, huit tables ACPI, un IOAPIC, un HPET et une unité DRHD. Un plan IOAPIC. Un conflit HPET. Zéro activation. |
| Contrat absent | Les deux | Refus 50 affiché. |
| Signature altérée ou autre autorité | Les deux | Refus 51 affiché. |
| Époque incohérente ou reçu intégral d’un démarrage antérieur | Les deux | Refus 52 affiché. Le juge compare le reçu rejoué à l’original archivé. |
| Identité du PE contradictoire dans le contrat | Les deux | Refus 53 affiché. |
| Origine USB incohérente | Les deux | Refus 54 affiché. |
| Droits DMA contradictoires dans un reçu signé | Les deux | Refus 55 affiché. |
| Transaction déclarée inachevée dans un reçu signé | Les deux | Refus 56 affiché. |
| Second contrat valide et cohérent dans la même époque, différent de l’original | Les deux | Refus 57 affiché. Les deux signatures sont valides. |
| Manifeste ou ELF invalide | Les deux | Refus 6 ou 18 affiché. Origine USB établie, aucune sélection publiée. |
| Géométrie de surface contradictoire | Les deux | Refus 44 en RAM. Avant et après, tous les pixels sont identiques. |
| PE 5O réellement substitué sur l’ESP | Les deux | L’autorité arrête le transfert avant StartImage. |
| Deux supports identiques ou reconnexion au même port | Les deux | Refus d’admission du clone ou quarantaine de l’incarnation perdue. |
| Transaction xHCI réellement en vol, retrait demandé | Les deux | Le modèle garde tous les prêts en quarantaine. Les compteurs du firmware et la trace d’un transfert asynchrone sont capturés avant l’arrêt QMP. |
| PE 5R présenté au magasin 5O original | Actif | OVMF renvoie EFI_ACCESS_DENIED à LoadImage. La première instruction du PE reste inexécutée. |

La campagne archivée comprend 39 démarrages. Le PE s’exécute 30 fois, dont quatre inventaires achevés et 26 refus. Les neuf autres arrêts précèdent l’entrée du PE. Le vérificateur contrôle les signatures avec OpenSSL, les traductions réellement utilisées, les interruptions, les compteurs bloc et les pixels QEMU. Les deux captures nominales permettent de lire le même écran que l’opérateur. [Écran actif](validation/diagnostic-uefi-5r/verdict-actif.png) · [Écran inactif](validation/diagnostic-uefi-5r/verdict-inactif.png)

## Secure Boot et préservation des preuves

Le PE a pour empreinte Authenticode `64686cc9127b09aeb1f17ec171f50ba544d4eeb5f6783e6637601802fccbdb1f`. L’original 5O et sa copie ont chacun 540 672 octets. La sérialisation de la copie change 189 octets. Le décodage des magasins retrouve l’unique empreinte supplémentaire dans db. Toutes les autres variables ainsi que leurs attributs et horodatages restent identiques. Après chaque arrêt actif, le banc recompare PK, KEK, db et dbx au magasin choisi pour cet essai. Cette politique d’autorisation par empreinte concerne le laboratoire. En mode inactif, Ed25519, l’ancre et les vérifications internes s’appliquent toujours. [Différence des magasins](validation/diagnostic-uefi-5r/variables.json)

Sur les 39 démarrages, le périphérique interne synthétique conserve son empreinte et ses compteurs d’E/S à zéro. L’image USB reste identique et ses compteurs d’écriture restent à zéro. Dans les 30 intervalles allant de l’entrée EFI à HLT, le juge APIC attribue les EOI au seul vecteur 0x20 de la minuterie qualifiée. La trace couvre les écritures APM, les soumissions de stockage USB, les notifications VirtIO et les écritures de registres VT-d. Les documents et archives 5I à 5Q conservent leur provenance. Le contre-exemple APIC 5I y reste consultable. Les contrôles hôtes couvrent les deux architectures Darwin et les 48 sélecteurs ABI. [Résultats hôtes](validation/diagnostic-uefi-5r/hotes.txt)

## Mesures nominales

Une observation instrumentée par mode et par version figure ci-dessous. Le témoin historique 5Q a été redémarré sous le même QEMU et a affiché le refus 40 attendu dans les deux modes. Chaque durée EFI est un temps mural hôte entre la fin du contrôle à la première instruction et l’observation de HLT, avec le coût du moniteur et des interruptions. L’échantillon ne caractérise aucun percentile.

| Mesure | Secure Boot actif | Secure Boot inactif |
|---|---:|---:|
| Installation initiale VT-d 5R | 272,03 ms | 269,72 ms |
| Chargement de l’ESP 5R | 1,667 s | 1,610 s |
| Émission du contrat 5R par le banc | 3,710 ms | 3,567 ms |
| Diagnostic EFI 5R complet | 1 647,362 ms | 1 396,882 ms |
| Témoin 5Q, jusqu’au refus 40 | 68,196 ms | 68,447 ms |
| EOI du minuteur attribués pendant le diagnostic 5R | 128 | 111 |

Les deux démarrages nominaux ont chacun lu 34 714 624 octets en 554 opérations USB. Le contrat atteste 1 685 transactions soumises et achevées. Les 13 prêts représentent 27 pages de 4 Kio, soit 108 Kio, et les 18 pages de tables représentent 72 Kio. Le rectangle graphique autorise 261 120 octets dans une surface réservée de 3 Mio. Le PE 5R occupe 382 976 octets dans l’ESP et 4 657 152 octets une fois chargé, arène fixe de 4 Mio comprise. L’arène atteint 181 755 octets contre 59 416 pour 5Q. À l’arrêt, le compteur d’allocations vivantes vaut zéro. Le tampon de transit de 33 Mio, son suffixe et la copie privée de 16 Kio ont des usages distincts. La copie est comprise dans l’arène.

Le chemin nominal 5R demande davantage de temps et de mémoire. Il relit notamment les 33 Mio de l’ESP pour leur SHA-256, vérifie les reçus, reconstruit les tables attendues, parcourt le catalogue et l’inventaire puis dessine le résultat. La mesure agrège ces opérations. La campagne ne détermine pas le coût individuel de chacune. [Mesures brutes et témoins](validation/diagnostic-uefi-5r/mesures.json)

## Portée et conditions avant A8

- La conclusion positive repose sur le Q35 monocœur avec VT-d legacy, un contrôleur xHCI sur le bus racine, un support au port observé, sans hub, et le LUN 0. Les identités des périphériques et du support viennent du contrat et du chemin observé. Les limites du profil sont vérifiées dans l’EFI. Le disque est synthétique, le réseau est inactif. Le contrôle du stockage démontre la conservation de ses mappings et de ses octets pendant ce diagnostic.
- L’autorité externe connaît l’époque, les prêts, l’origine du média et la protection de la surface. Sur matériel physique, il faut une autorité précoce de confiance qui établisse ces faits avant le chargement, identifie tous les demandeurs et alias, préserve les domaines de stockage et atteste les transactions xHCI. Une observation après l’entrée EFI ne couvre pas cet intervalle antérieur. Il faut également établir l’origine exclusive du support et son incarnation en cas de reconnexion.
- La politique physique devra établir la confiance dans le premier EFI selon le mode choisi par le propriétaire. L’autorité publique du banc et la copie de db ne qualifient aucune politique OEM. Il reste à prouver la propriété et les limites du framebuffer, la possibilité d’un arrêt sûr, l’absence de lancement interne imprévu, la frontière des E/S firmware acceptée par la MOE et la préservation de Windows. Le verdict doit pouvoir rester silencieux lorsque l’affichage lui-même n’est pas sûr.
- Les rapports JSON et texte sont préparés hors VM. Chaque rapport indique si le PE a pu s’exécuter, qui a rendu la décision et si le verdict a pu être affiché. L’exporteur hors VM 5N est conservé. Toute écriture USB native, tout drainage physique ou toute nouvelle capacité nécessite son arbitrage propre. Les ressources incertaines restent prêtées ou en quarantaine jusqu’à la destruction de la VM. Le prototype ne revendique aucune restitution physique.
- A8 reste fermé et la garde réseau/stockage reste en vigueur. Les suites noyau, NVMe et Ethernet générales n’ont pas été rejouées. Le périmètre modifié est le premier EFI x86_64 et son banc. [Procédure de reproduction](../boot/diagnostic-5r/README.md)
