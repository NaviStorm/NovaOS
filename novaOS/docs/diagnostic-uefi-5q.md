# NovaOS v0.25.0 — Chargement USB sous protection DMA précoce

**Le même démarrage Q35 établit VT-d avant la première instruction du vCPU, lit le premier EFI NovaOS sur xHCI, contrôle son transfert signé, puis affiche le motif de refus du PE historique.** Les modes Secure Boot actif et inactif ont chacun été exercés. Base Git `12411d98d8c309c2331d6df1301bbd10ccaead95`. L’image x86_64 et le PE sont identiques à ceux de 5O et 5P. [Recette et contre-lecture](../boot/diagnostic-5q/README.md) · [Campagne](validation/diagnostic-uefi-5q/observations.zip) · [Rejeu après reconstruction](validation/diagnostic-uefi-5q/reconstruction.zip)

| Artefact contre-vérifié | SHA-256 |
|---|---|
| Image USB x86_64 de 68 Mio | `a4dd3d96739b0821f955a4e4e14524516b100e557d333739a6cecda23d31d7cb` |
| Premier EFI NovaOS | `f9577af2b719e0e0bafeec7179bada2f3039fa92cdb3a4592bca80c013f66971` |
| OVMF_CODE.fd de laboratoire, deux constructions comparées octet par octet | `b2c10fe2d90f8324c350f655a90d0b5bb716cacb636cf017e55e2173b5009f5d` |

## Du reset au premier RIP

1. Le superviseur possède une VM Q35 froide et suspendue. Il découvre le RID du contrôleur de classe xHCI par QMP, vérifie la couverture DMAR et le placement de la réserve RAM privée. Il active le remappeur en mode legacy et ajoute un contexte USB vide. Les domaines NIC, NVMe et sonde conservent leurs quatre feuilles directionnelles. La contre-lecture part de la racine active. Les pages de données autorisées et celles des tables sont disjointes. Le modèle qualifié utilise le canal firmware PIO.
2. Le pilote xHCI d’OVMF demande un prêt avant de communiquer une adresse au contrôleur. Le superviseur vérifie le rôle, la taille, le quota et la séquence, publie uniquement des feuilles vierges puis les relit avant son accord. Chaque prêt conserve son adresse jusqu’à la fin de l’époque. Le profil exige l’absence de cache de traductions négatives. Le témoin VT-d retrouve pour les 27 pages la traduction réellement utilisée après publication.
3. Le BDS relie le contrôleur à la partition EFI par son chemin UEFI. Le superviseur recoupe le RID, le port, l’interface, le LUN, la GPT et le backend ouvert en lecture seule. La partition entière passe par xHCI, par blocs de 64 Kio au plus. Le firmware copie chaque transfert achevé dans une allocation CPU exclue du DMA. Le témoin compare les **34 603 008 octets** de cette allocation à l’ESP de la clé virtuelle avant `LoadImage`.
4. Le firmware enregistre cette copie comme disque RAM privé et charge `EFI/BOOT/BOOTX64.EFI` avec les services UEFI. C’est l’unique provenance des octets du PE. Le `DeviceHandle` final désigne le disque RAM. Aucun PE ni contenu d’ESP n’est fourni par `fw_cfg`. Une seconde lecture compare en RAM les cinq sections relogées du PE avant `StartImage`, puis à sa première instruction. [Services UEFI LoadImage et StartImage](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html)
5. Le superviseur signe le contrat canonique et ses champs fixes. Le vérificateur Rust dans le BDS vérifie Ed25519, l’époque de démarrage, les empreintes de l’ESP, du PE entier, du chemin USB et du contrat, ainsi que les seize valeurs comparées au firmware. Le contrat lie l’incarnation, le port, la route directe, le LUN, les droits et pages, les compteurs de transferts, les traductions, le mode Secure Boot et la surface GOP. Le BDS compare notamment le port extrait du chemin réellement rencontré. Le PE vérifie ensuite ses reçus historiques de la même époque.
6. Depuis la première instruction du vCPU, un plugin TCG observe les écritures CPU vers les 18 pages des tables de traduction. Il enregistre aussi les passages avant et après `LoadImage`, l’acceptation de la signature et le départ vers `StartImage`. La trace matérielle surveille les écritures aux registres VT-d. Les tables sont relues avant chargement, avant lancement, au premier RIP et après le verdict. Les protections couvrent la RAM de staging, l’image et ses données, la pile d’entrée, les reçus et le framebuffer. Le signataire privé s’exécute hors de la VM. [Interface d’observation TCG](https://www.qemu.org/docs/master/devel/tcg-plugins.html)

## Droits et propriété

| Objet dédié | Accès du contrôleur | Vérification |
|---|---|---|
| DCBAA, table des segments d’événements, anneaux de commandes et de transferts, contexte d’entrée | Lecture | Pages réservées au rôle et à l’unique RID admis |
| Anneau d’événements, données USB IN | Écriture | Complétion du transfert et bornes avant toute copie dans la RAM CPU |
| Données USB OUT | Lecture | Copie CPU bornée avant publication du transfert |
| Contexte de sortie | Lecture et écriture | Cadre exclusif, utilisé par le contrôleur pour consulter et mettre à jour le contexte |
| RAM du PE, tables IOMMU, boîte CPU, pile et surface GOP | Aucune feuille DMA vers ces cadres | Décodage exhaustif des quatre domaines, contrôle des traductions mises en cache et témoin CPU |
| NVMe et NIC | Un cadre de lecture dans chacun de leurs domaines | DID distincts, feuilles inchangées, compteurs et traces NVMe contrôlés indépendamment |

Le firmware sérialise les transferts de données. Le tampon directionnel peut servir de nouveau après confirmation de la requête précédente du même titulaire. Le tampon USB IN grandit par extensions contiguës jusqu’à seize pages. Le tampon OUT est plafonné à une page. Chaque extension publie uniquement les feuilles supplémentaires requises. Les copies CPU restent limitées à la longueur réelle du transfert. La clôture constate la fin des transferts de cette lecture. Elle ne transfère aucun cadre à un nouveau propriétaire. Le dispositif refuse les scratchpads, les propriétaires, les géométries et les chemins qui sortent du profil.

## Matrice de qualification

Chaque ligne a été exercée avec les deux magasins OVMF. Deux campagnes indépendantes couvrent les mêmes seize cellules. [Empreintes](validation/diagnostic-uefi-5q/SHA256SUMS) · [Tests hôtes](validation/diagnostic-uefi-5q/hotes.txt)

| Cas | Observation directe et décision |
|---|---|
| Nominal | 554 lectures USB, cinq sections identiques au PE de référence. Le BDS accepte le contrat, puis le PE affiche le code 40. Les projections 800 × 600 sont identiques, pixel par pixel, aux références du mode correspondant en 5O. |
| USB en vol | Une requête asynchrone de données du slot 1, endpoint 3, reste ouverte alors que le compteur firmware des soumissions dépasse celui des complétions. Le registre refuse la clôture. Les mêmes pages restent prêtées puis le chargement aboutit après complétion. |
| Rôle de mapping inconnu | Injection dans la demande au courtier. Refus avant la première feuille USB et arrêt du pilote xHCI constaté à la boîte de contrôle. |
| Clone | Deux instances du même média émulé. Le superviseur refuse l’admission avant de libérer le vCPU. |
| Reconnexion | Après la lecture de l’ESP, déconnexion observée, puis nouvelle instance aux mêmes octets et sur le même port. L’autorité retient la VM avant `LoadImage`. Les 27 pages restent réservées et la nouvelle instance reçoit zéro commande bloc. |
| Signature altérée | Un bit de signature modifié. Le BDS refuse avant `StartImage`. La surface reste intacte. |
| Reçu contradictoire ou périmé | Signatures valides avec un RID contradictoire ou une autre époque. Chaque variante est refusée indépendamment avant `StartImage`. |
| Interdiction DMA vers le PE et ses données | Les tables réellement actives excluent ces cadres pour tous les RID. Le juge rejette aussi un cliché synthétique pointant une feuille vers le PE et un autre élargissant ses droits. Aucune tentative d’écriture destructive sur le PE en exécution. |
| Stockage, GOP, interruptions | Zéro E/S NVMe, canari de 4 Mio intact. Les compteurs USB et NVMe sont stables du premier RIP du PE à l’arrêt. Le rendu reste dans le rectangle 544 × 120. Le juge 5K attribue chaque EOI au vecteur 0x20. |
| NOVALOGS et ABI | SHA-256 des 68 Mio de la clé identique après chaque cas. Les deux partitions et les rapports existants sont préservés. Les preuves 5N de l’export hors VM et le registre des 48 sélecteurs ont été contre-vérifiés. |

Le code **40** signifie « origine USB non attestée » pour le vérificateur du PE 5O. Le BDS 5Q atteste le chemin USB et vérifie le contrat complet avant le saut. Le diagnostic historique ne consomme que ses reçus 5M/5O et voit un disque RAM. Le verdict conservateur et son affichage sont donc attendus.

## Mesures

Campagne archivée dans `observations.zip`. Un démarrage nominal par mode dans ce tableau. Les durées sont des temps muraux hôte instrumentés, incluant les suspensions et les contrôles du témoin pour les intervalles concernés.

| Mesure | Secure Boot actif | Secure Boot inactif |
|---|---:|---:|
| Installation initiale VT-d et contre-lecture | 271,881 ms | 257,720 ms |
| Publication des 13 prêts, temps cumulé | 3,665 ms | 3,295 ms |
| Du premier `cont` au contrôle de l’ESP avant `LoadImage` | 1,728 s | 1,558 s |
| Intervalle terminal instrumenté après capture initiale du GOP | 70,283 ms | 68,510 ms |
| Octets lus sur le backend USB, partition et métadonnées comprises | 34 714 624 | 34 714 624 |
| Transferts de données soumis et achevés | 1 685 / 1 685 | 1 685 / 1 685 |
| Taille du reçu 5Q signé | 3 031 octets | 3 033 octets |

Au rejeu avec le firmware reconstruit, l’ESP était prête après 1,639 s en mode actif et 1,683 s en mode inactif. Le volume lu et la géométrie des 27 pages sont identiques.

Le contrôleur reçoit 13 prêts, soit **27 pages et 108 Kio** pour 96 824 octets de structures et de capacité de tampons. Dix pages portent les structures, seize les données entrantes et une les données sortantes. La réserve privée du banc fait 2 Mio, dont 18 pages de tables VT-d (72 Kio, quatre pages pour la branche xHCI) et une page de dialogue CPU. Les 108 Kio sont inclus dans cette réserve. Le staging coûte **33 Mio + 16 Kio** de RAM CPU, en plus du PE relogé et de la surface GOP de 3 Mio. Le tampon maximal d’une opération de données USB est de 64 Kio. Le coût supplémentaire démontré est la lecture et la copie de toute l’ESP. Les profils séparés de 5P n’offrent pas de référence chronométrique équivalente pour un pourcentage de régression. [Comparaison avec 5P](diagnostic-uefi-5p.md)

## Frontière physique et arbitrages

Le superviseur connaît l’état froid du modèle, l’unique remappeur, tous les demandeurs et la provenance de la réserve avant la première instruction. Il contrôle les branchements et la clé de signature de laboratoire. Le firmware du banc coopère à chaque prêt. Ces conditions limitent la portée du résultat à Q35/OVMF, avec un contrôleur, un média direct et un LUN. Le Secure Boot actif utilise l’empreinte du PE déjà autorisée dans le magasin 5O. Le mode inactif conserve les vérifications internes et le reçu du laboratoire. Aucune signature de distribution ou autorité physique n’a été qualifiée.

**A8 reste fermé.** Un firmware ou premier étage physique devra donner avant tout chargement une preuve authentifiée de couverture DMA, de propriété des pages et de fermeture des transactions héritées. Il faudra attribuer les vrais RID et alias, les réservations, l’incarnation USB et la surface graphique, conserver le domaine du stockage et qualifier un arrêt sûr si l’un de ces éléments échoue. L’image ne contient aucune personnalisation du modèle de machine.

L’exporteur hors VM 5N reste la référence. Toute écriture USB native dans NOVALOGS demande un arbitrage A7 distinct et un courtier capable de lier les commandes à l’unique média amorcé, de vérifier ses bornes, de préserver les journaux existants, puis de prouver FLUSH, drainage et invalidations avant restitution. Une reconnexion rompt le bail. [Garanties de l’exporteur 5N](diagnostic-uefi-5n.md) · [Préconditions du courtier USB](diagnostic-uefi-5p.md)
