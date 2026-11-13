# NovaOS v0.16.6 — Éditions par plages et prêt multipage

La campagne suivante mesure le regroupement des requêtes physiques, les interruptions et la restitution des cadres dans QEMU et UTM. [Qualification VirtIO-blk multipage](es-v0.16.6-dma.md)

## Portée et mesure

Qualification du prototype issu de `1719e7440de772d2f40b274936d7316a0cccb07c`, sur `codex/es-groupees-0166`. Le résultat porte sur la création d'une copie vers un **nom libre** avec confirmation dans le vrai `nsh`. L'image livrée reste NovaOS v0.16.5 et son lecteur NovaFS v0.9.13 P9. Les fixtures sont appliquées dans des copies du dépôt. [Audit précédent](es-v0.16.6-qualification.md) · [Relevés par exécution](validation/es-v0.16.6-plages.json)

Le fichier de référence contient 4 194 304 octets. Chaque commande localisée insère un scalaire UTF-8, supprime un octet ou remplace un caractère. Le cas multiple exerce les trois positions dans la même édition. Le cas « nouveau » écrit 4 Mio entièrement nouveaux. Le cas « ajout » insère 4 Mio dans une source de 4 Mio et publie **8 Mio**. Les variantes croisées insèrent ces 4 Mio au milieu, à la fin, ou au milieu après une suppression en fin puis ajoutent Ω en tête. Pour les grandes insertions, une fixture de clavier prépare le contenu avant la mesure. Elle utilise le même gap buffer et la même publication que l'éditeur. Les VM disposent d'un vCPU et de 256 Mio. Les deux invités sont construits par Rust 1.99.1 (58cf8f9c3). QEMU direct 11.1.3 utilise TCG et une image brute en writeback. Le profil de développement AArch64 utilise GICv2 avec VirtIO-IOMMU et celui de x86_64 utilise Q35 avec Intel-IOMMU. UTM 5.0.8 sur macOS 27.2 (26B123) utilise son moteur 10.0.12, Cortex-A72 émulé, QCOW2, UEFI privé, TPM CRB, GICv3 sans ITS et VirtIO-IOMMU sans bypass.

Le chrono hôte part à l'envoi de `w copie` et s'arrête au message de succès. Le banc envoie OUI dès la demande. Le chrono invité va de la construction du paquet dans `vi` jusqu'à son retour après F2 et libération du prêt. Les quatre liaisons sont comptées dans cette fenêtre, dons acquittés compris. Le plafond conservateur ajoute les créations, fermetures et contrôles locaux du prêt, y compris la tentative finale du propriétaire sur un prêt déjà libéré. La sonde publie aussi le total en comptant chaque copie locale de littéraux. Les appels locaux au portail bloc et les requêtes physiques sont ventilés séparément. Le délai d'un scénario refusé est un temps de refus.

## Chemin de publication

L'ancien chemin reconnaissait une insertion ASCII en tête. Une substitution au milieu faisait transmettre à nouveau le tampon à `nsh`, puis au moteur par fragments de 88 octets. L'ancienne validation groupée rejetait aussi des scalaires UTF-8 valides et les CRLF. Une édition individuelle dépassant le budget d'historique pouvait faire tourner la boucle d'éviction à vide. Le prototype conserve alors le texte et signale la troncature de l'historique suivant la politique de l'éditeur.

1. Le gap buffer conserve les intervalles issus de la source. Chaque plage encode trois entiers u64 little-endian, position dans la source, nombre d'octets retirés et longueur ajoutée. Le paquet commence par le nombre de plages, puis leurs triplets et les octets ajoutés dans le même ordre. Les zones conservées proviennent de l'objet épinglé. La liste survit à l'éviction de l'historique et suit les annulations. À 2 048 intervalles ou en cas d'allocation refusée, le préfixe déjà chargé peut être représenté littéralement. Une limite de paquet ou de mémoire produit une erreur explicite avant publication.
2. Le mode est négocié à l'ouverture sur le volume P9 du profil expérimental. Jusqu'à 64 octets, une trame privée transporte intégralement le plan. Pour un paquet plus long, `vi` réserve des pages privées entières et les remplit avant de les sceller. Le budget de qualification est **8 Mio par prêt**, en-têtes inclus. Le noyau conserve une référence de génération au canal, un tag monotone, les limites et le propriétaire. Il vérifie toutes les feuilles, les passe en lecture seule, invalide les traductions et interdit le retrait de la plage du tas jusqu'à la restitution. Une écriture utilisateur sur la seconde page provoque une faute matérielle sur les deux architectures.
3. Seul le courtier propriétaire du canal peut transmettre le prêt au descendant RECEIVE exact du moteur désigné. Le moteur lit par fenêtres de 1 Mio maximum. Le noyau vérifie les droits et **les deux plages entières** avant d'écrire le premier octet, puis copie directement entre cadres privés sous verrou. Ni le courtier ni le pilote n'obtiennent le droit de modifier les littéraux. Le nombre de prêts simultanés est borné à un dans ce profil mono-CPU. Le destinataire ne reçoit aucun mapping DMA de l'éditeur.
4. Le moteur vérifie la délégation de destination, l'identité, la révision et la longueur de la source, le tri des plages, leur non-chevauchement et la somme des littéraux. Chaque page dense de la source, y compris une partie supprimée, est relue et vérifiée avec l'empreinte P9. Le texte source et celui de la candidate sont contrôlés à travers les frontières de page et de lot. Une vérification par mots accélère l'ASCII imprimable. Les scalaires Unicode, CRLF et contrôles suivent le même contrat que `vi`. L'espace alloué doit suffire pour un nouvel objet complet.
5. La transaction réserve des pages CoW privées, construit leurs extents et vérifie le nom libre dans la transaction. Pour le prêt multipage, un retrait acquitté avant l'admission empêche F1. Le plan court voyage, lui, dans la trame de la délégation nommée. Une fois la dernière lecture terminée et le droit de publier acquis, un retrait concurrent obtient `WOULD_BLOCK` jusqu'à la fin de la décision. La file bloc contrôle aussi le bail au moment du kick et attend la complétion des commandes déjà en vol. Une issue de transport incertaine interdit d'annoncer une sauvegarde réussie. F1 synchronise la préparation, F2 synchronise l'ancre. Deux FLUSH sont émis pour chaque sauvegarde mesurée.
6. L'état normal du système continue d'utiliser ses anciennes trames. P9, le protocole PC1, l'ABI publique et le remplacement d'un fichier existant gardent leurs contrats publiés. Les sélecteurs de prêt, l'opération privée 30 et les lots de 256 pages sont expérimentaux. La forme 29 et les anciennes opérations restent disponibles. [Fixture invitée](../tests/fixtures/io-0166-plages.patch) · [Préparation CoW expérimentale](../../novaFS/lab/es-groupees/preparation.rs)

## QEMU direct

Les 80 publications de référence, les 12 insertions volumineuses croisées (3 parcours × 2 architectures × 2 géométries) et les quatre refus de saturation ont chacun un passage sans compteur et un second passage instrumenté. Chaque cellule est le temps mural sans sonde en millisecondes. La dernière colonne est l'enveloppe sur les quatre mesures instrumentées correspondantes. La table conserve les identifiants du banc pour permettre le rejeu exact.

| Édition | AArch64 512 | AArch64 4096 | x86_64 512 | x86_64 4096 | IPC avec contrôles |
|---|---:|---:|---:|---:|---:|
| ajout-4m-milieu | 414,3 | 415,0 | 451,1 | 447,0 | 54 |
| ajout-4m-fin | 357,4 | 376,7 | 415,8 | 414,4 | 54 |
| ajout-4m-multiple | 347,1 | 344,3 | 381,9 | 385,5 | 56 |
| inserer-debut | 283,9 | 281,7 | 291,9 | 286,6 | 41 |
| inserer-milieu | 321,4 | 293,6 | 305,7 | 271,5 | 43 |
| inserer-fin | 390,9 | 255,1 | 295,6 | 287,1 | 43–44 |
| inserer-multiple | 256,2 | 270,9 | 322,6 | 263,0 | 48 |
| supprimer-debut | 349,8 | 267,7 | 278,6 | 275,6 | 39 |
| supprimer-milieu | 336,0 | 235,0 | 304,0 | 256,1 | 41 |
| supprimer-fin | 333,5 | 262,0 | 278,5 | 275,7 | 41–42 |
| supprimer-multiple | 330,4 | 260,5 | 325,3 | 324,5 | 46 |
| remplacer-debut | 412,0 | 255,4 | 285,0 | 278,9 | 41 |
| remplacer-milieu | 306,2 | 254,2 | 288,9 | 278,8 | 43 |
| remplacer-fin | 297,8 | 260,2 | 292,0 | 279,1 | 43 |
| remplacer-multiple | 288,0 | 247,1 | 266,7 | 262,6 | 48 |
| neuf-4m | 186,7 | 163,2 | 205,3 | 194,3 | 36 |
| ajout-4m | 433,7 | 397,6 | 533,3 | 496,5 | 52 |
| debut | 293,2 | 255,2 | 277,2 | 281,7 | 41 |
| milieu | 322,2 | 336,6 | 267,0 | 300,8 | 41 |
| fin | 250,2 | 244,1 | 302,5 | 282,1 | 43 |
| multiple | 274,6 | 257,7 | 295,8 | 311,3 | 48 |
| index | 265,9 | 239,7 | 287,6 | 282,5 | 41 |
| fragmente | 441,0 | 377,1 | 406,6 | 408,2 | 41 |
| sature | 68,7 | 68,1 | 58,1 | 58,1 | refus |

La substitution historique au milieu passe de **52 307 à 41** échanges avec contrôles, soit 267 à 337 ms au lieu de 3 109 à 3 878 ms sur les quatre configurations. Le cas `multiple` passe de 52 334 à 48. Pour 4 Mio entièrement nouveaux, 36 échanges incluent les contrôles du prêt. Sur AArch64/4096, l'ajout de 4 Mio dans une source de 4 Mio publie 8 Mio en 43 allers-retours interservices et dons, 52 avec contrôles, 58 en incluant les six copies locales du prêt. Le compteur historique de toute la session relève 2 242 tours. Le pilote effectue également 3 128 copies de page, 3 128 contrôles de position et 3 130 soumissions locales dans la fenêtre de sauvegarde. Ces appels au noyau et les requêtes physiques sont ventilés dans le JSON.

Les insertions de 4 Mio **au milieu et à la fin** consomment 54 échanges avec contrôles, ou 60 en incluant chaque copie locale du prêt. L'édition combinée avec retrait en fin et ajout Unicode en tête consomme 56 échanges, et exactement **64** avec les huit lectures locales du prêt. L’oracle indépendant reconstruit la destination à partir des positions d’origine et la compare octet par octet pour chaque couple architecture/géométrie.

Trente-deux répétitions complètent les mesures sans sonde. Les deux géométries et trois passages donnent six valeurs par architecture et scénario, en millisecondes.

| Scénario | AArch64 min–max | Médiane | x86_64 min–max | Médiane |
|---|---:|---:|---:|---:|
| neuf-4m | 161,1–186,7 | 164,6 | 190,2–205,3 | 192,8 |
| ajout-4m | 392,7–3 264,7 | 406,1 | 467,2–1 458,0 | 497,3 |
| milieu | 285,6–336,6 | 302,7 | 267,0–300,8 | 275,0 |
| multiple | 235,8–274,6 | 249,0 | 295,8–1 255,2 | 300,6 |

Trois répétitions sur 32 dépassent la seconde avec les autres campagnes actives, sur secteur 4096. Chaque destination a été intégralement relue. La colonne suivante donne les trois sauvegardes chronométrées du même cas lorsque le banc détient le verrou exclusif des VM de qualification.

| Architecture / scénario | Dépassement (ms) | Rejeux VM unique (ms) |
|---|---:|---:|
| aarch64 / ajout-4m | 3 264,7 | 384,3 / 406,7 / 382,5 |
| x86_64 / ajout-4m | 1 458,0 | 451,1 / 461,9 / 465,1 |
| x86_64 / multiple | 1 255,2 | 278,4 / 280,3 / 289,6 |

Le périphérique virtuel, les octets et l’oracle sont identiques dans ces contre-épreuves. Les trois passages lents ne disposaient d’aucune sonde de planification hôte. Ces mesures établissent une variabilité du délai de bout en bout. Elles ne permettent pas d’attribuer la totalité du retard à l’ordonnanceur du Mac.

La source fragmentée contient 983 extents P9, contre 128 dans l'image dense. Le volume presque plein laisse 145 blocs de 4 Kio disponibles. Le refus conserve la source, le tiers et l'absence de destination. Les ancres après refus sont identiques en génération à celles d'un témoin ayant seulement ouvert la source et quitté sans sauvegarder. Le changement de génération observé à l'ouverture dépend d'atime.

## Application UTM

Les temps du profil de référence incluent le vrai dialogue de confirmation et le rendu du message de succès dans le terminal UTM. Un redémarrage autonome, puis une exportation et une comparaison octet par octet accompagnent chaque succès retenu. Les ESP de la campagne sont relues pour identifier par SHA-256 le noyau effectivement embarqué. Les essais antérieurs à la dernière correction de délai sont conservés séparément.

| Édition | UTM 512 (ms) | UTM 4096 (ms) | IPC avec contrôles |
|---|---:|---:|---:|
| ajout-4m-milieu | 1 709,8 | 3 306,7 | 54 |
| ajout-4m-fin | 1 361,6 | 1 767,7 | 54 |
| ajout-4m-multiple | 1 675,7 | 2 859,3 | 56 |
| inserer-debut | 1 277,0 | 1 222,7 | 41 |
| inserer-milieu | 1 086,2 | 1 220,0 | 43 |
| inserer-fin | 1 635,4 | 1 474,4 | 43 |
| inserer-multiple | 840,6 | 1 049,8 | 48 |
| supprimer-debut | 627,3 | 2 237,9 | 39 |
| supprimer-milieu | 1 787,4 | 1 107,0 | 41 |
| supprimer-fin | 1 002,1 | 1 790,3 | 41 |
| supprimer-multiple | 1 185,9 | 1 267,8 | 46 |
| remplacer-debut | 1 607,4 | 2 107,7 | 41 |
| remplacer-milieu | 1 599,9 | 1 098,4 | 43 |
| remplacer-fin | 1 240,7 | 1 048,8 | 43 |
| remplacer-multiple | 1 483,0 | 1 232,2 | 48 |
| neuf-4m | 700,1 | 609,4 | 36 |
| ajout-4m | 1 452,1 | 2 423,4 | 52 |
| multiple | 1 111,2 | 886,4 | 48 |

**5 des 36 sauvegardes** sans sonde respectent la seconde. Les 36 passages instrumentés comptent de 36 à 56 échanges avec contrôles. Chacun contient exactement deux FLUSH. Les noyaux des deux séries ont leur propre SHA-256 dans les relevés.

En QCOW2/4096, quatre passages sans sonde donnent les distributions suivantes.

| Scénario | Quatre mesures (ms) | Médiane (ms) |
|---|---:|---:|
| ajout-4m | 2 423,4 / 2 098,2 / 1 828,9 / 1 610,4 | 1 963,5 |
| remplacer-milieu | 1 098,4 / 1 117,2 / 1 050,6 / 1 047,2 | 1 074,5 |

Le témoin brut utilise la même application UTM, le même firmware, le même ELF, 4 096 octets par secteur et les mêmes traces IOMMU/VirtIO. GPT et P9 ont été relus après redémarrage pour les quatre VM. Chaque cellule correspond à un seul passage.

| Scénario | Brut (ms) | QCOW2 (ms) |
|---|---:|---:|
| supprimer-milieu | 880,5 | 961,6 |
| ajout-4m | 1 590,4 | 1 753,1 |

Avec les deux traces QEMU de lectures/écritures désactivées, les quatre mesures de substitution au milieu et de modifications multiples donnent 1 095,6–1 409,8 ms. Les quatre dépassent la seconde. Les traces IOMMU restent activées. Le dépassement existe également sur le brut pour l’ajout de 4 Mio. Ces témoins ne déterminent pas à eux seuls le coût de QCOW2 ou du journal de traces.

Sur QCOW2/4096, la suppression au milieu instrumentée donne **41 échanges**, F2 à **2 042,736 ms** et affichage du succès à **2 323,400 ms**. Les lots de lecture occupent 614,490 ms et les lots d'écriture 759,284 ms. Les vérifications SHA-256 de la source occupent 99,225 ms. La construction des empreintes et extents de destination prend 190,745 ms, et les contrôles de texte 41,547 ms. Les lots comprennent leurs appels noyau, leurs copies, la soumission et l'attente VirtIO. Pour l'ajout de 4 Mio, 1 041 lectures, 2 087 écritures et deux FLUSH aboutissent à F2 en **1 864,023 ms**. La préparation représente 1 845,327 ms, les lots de lecture 332,152 ms et ceux d'écriture 881,782 ms. Une requête physique suit la soumission du descripteur jusqu'à sa complétion. Les compteurs de lots englobent les requêtes de données du lot. Les opérations de métadonnées et les deux FLUSH ont leurs propres compteurs. L'intervalle hôte et les phases invité sont consignés séparément.

Le pilote utilise une page DMA privée de 4 Kio dans chaque sens. Pour chaque page, il vérifie le droit, soumet puis recueille la complétion et l'IRQ avant la page suivante. Le regroupement interservices a réduit les messages. Il reste plus de deux mille admissions, soumissions et complétions physiques pour une copie dense de 4 Mio. Les durées résiduelles comprennent les copies, l'ordonnancement et le traitement du backend. Le banc mesure leur temps cumulé. Les références d'un extent autonome partagé entre générations doivent conserver le même propriétaire, le même offset logique et la même empreinte. Créer un autre objet en lui attribuant directement les pages source contournerait ces contrôles. [Vérification P9 des pages partagées](../../novaFS/src/fs/check.rs)

Avant correction du délai, deux VM exécutées en parallèle des régressions ont signalé une erreur d'E/S lors d'une insertion au milieu et d'un ajout de 4 Mio. Sur les deux exports identifiés par UUID, GPT et P9 ont été vérifiés, les générations 66/67 sont lisibles, la source est exacte et la destination absente. Les traces des services comportent le statut 8 (`TIMED_OUT`). Le relais et le moteur attendaient une réponse de lot avec le délai habituel d’un secteur, soit 250 ticks à 250 Hz. La fixture conserve ce délai pour l'ancienne interface et borne celui de la réponse du lot à 250 ticks plus un tick par page, soit 2,024 s au maximum. Une expiration échoue sans acquittement. La cible d'une seconde reste mesurée à l'horloge hôte, indépendamment de ce délai. Le banc sérialise maintenant les VM de mesure.

## Récupération, concurrence et mémoire

La vérification à froid contrôle les deux générations par `verify`, puis extrait la source, la destination et, le cas échéant, le fichier tiers avec un oracle indépendant. UTM redémarre réellement sur son disque, rouvre la copie dans `vi`, exporte le QCOW2, vérifie les deux GPT et compare les octets P9. Toute candidate complète doit aussi contenir le dernier octet prévu.

Les huit points QMP couvrent les pages, le retrait d'une commande en vol, les deux côtés des lots F1 et F2, et une suspension effective du FLUSH par `blkdebug`. Les quatre couples architecture/géométrie donnent 32 arrêts avec transfert de 4 Mio. Après F2, la copie complète est obligatoire. Entre l'écriture de l'ancre et l'acquittement de F2, un état complet antérieur ou nouveau est admissible. Un essai séparé de coupure simulée après acquittement impose la génération nouvelle. Le banc Darwin exerce 117 injections sur l'insertion en tête et 126 sur trois plages avec littéraux multipages, pour chaque architecture et géométrie, soit **972 injections**. Une ancre déchirée ambiguë demeure un refus de montage explicite. Ces essais ne coupent pas l'alimentation physique du Mac.

UTM a exécuté six arrêts forcés sur QCOW2. Le banc stoppe la VM au repère du moteur, avant tout acquittement de `vi`, exporte l’image éteinte, la vérifie, redémarre normalement et vérifie un second export. La table donne les générations disponibles immédiatement après coupure puis après redémarrage. La source de 4 Mio est exacte dans les douze exports.

| Frontière du moteur | 512 | 4096 | Destination aux deux relectures |
|---|---:|---:|---:|
| pages | 66/67 → 66/67 | 66/67 → 66/67 | absente |
| f1 | 66/67 → 66/67 | 66/67 → 66/67 | absente |
| f2 | 67/68 → 68/69 | 67/68 → 68/69 | 8 Mio exacts |

La suspension à l’intérieur du FLUSH a été exercée par `blkdebug` sur QEMU direct. Dans UTM, les repères sont les retours de commandes du moteur. Le Mac reste alimenté pendant les deux campagnes.

La qualification des capacités exerce des acteurs distincts pour l'éditeur, le courtier, le moteur et un tiers. Elle vérifie la copie traversant deux pages, les plages mal alignées ou hors bornes, la destination intacte après un refus, le rejet d'un ancien tag et la concurrence entre copie et retrait. Après admission, la faute d'écriture du propriétaire sur sa seconde page termine son processus. Le moteur garde la décision et libère le prêt. Les bancs bloc exercent le retrait avec deux commandes en attente, l'absence de kick après acquittement du retrait, le conflit de deux écrivains et la modification, la suppression et la recréation de la source. Les traces de révocation et les oracles froids sont dans les relevés.

Le moteur utilise deux fenêtres effacées à la restitution, de 1 Mio chacune au plus, et dimensionne leur allocation sur les octets utiles. Une source absente n'alloue aucune fenêtre d'entrée. Un fichier de 4 Mio n'impose aucun tampon de 4 Mio au courtier. `G` conserve une fenêtre finale éditable si le préfixe est intact et si la fin contient assez de lignes. La recherche s'arrête au premier résultat. Un passage entre fenêtres modifiées ou une ligne sans contexte suffisant peut matérialiser l'intervalle manquant pour conserver l'historique. Les économies obtenues en fin de fichier ne s'étendent pas à tous les parcours multipoints.

Les mesures par PID, relevées à la fin du processus, donnent 392 Kio à `vi` pour une édition en tête, 456 Kio en fin, 6 600 Kio au milieu et jusqu'à 17 096 Kio avec plusieurs petites modifications. La création du fichier neuf de 4 Mio occupe 12 872 Kio. Pour une insertion de 4 Mio dans une source de même taille, on relève 8 776 Kio en fin, 16 968 Kio au milieu et **33 608 Kio** dans le cas combiné avec une suppression en fin et une insertion en tête. Ce dernier parcours réunit l'intervalle manquant et agrandit le gap buffer. Le prêt lui-même reste borné. Le maximum de session atteint alors **9 673 cadres AArch64 et 9 683 cadres x86_64** de 4 Kio, soit 37,8 Mio, contre 5 537/5 547 cadres dans la première matrice. Le maximum observé du moteur est de 3 796/3 820 Kio et celui de `nsh` de 460/476 Kio. Le banc Darwin du moteur a relevé 14 085 allocations sur l’insertion en tête dans le profil instrumenté, contre 33 934 par l’ancienne voie. Les relevés par PID ne représentent pas un pic d’allocateur par fonction. Le franchissement de fenêtres modifiées reste coûteux pour l'éditeur. [Mémoire mesurée avant cette extension](es-v0.16.6-qualification.md#mémoire)

## Régressions et décision

- **NovaOS, sur chaque architecture.** Les 25 démarrages historiques, les 24 groupes bloc, les trois contrôles ELF, processus et IPC, les courses IPC, les fichiers et espaces de noms et les quatre profils CLI ont réussi. Les suites `nsh`, scripts, terminal, `vi`, écran et grands volumes ont aussi réussi. Le banc mémoire de `vi` compte huit tests, dont celui du plan après éviction et annulation, et la progression de montage trois. Les essais ABI et IPC à 128 et 256 Mio ont réussi. À 64 Mio, le firmware AArch64 exige 128 Mio et le chargeur x86_64 épuise son allocateur haut avant l’entrée du noyau. Le banc de performance bloc a aussi vérifié 128 transferts par géométrie et par architecture. Les deux bancs de performance IPC à 256 Mio ont franchi leurs seuils historiques. Le stress court est compris dans les démarrages. L'endurance optionnelle de deux heures reste hors de cette campagne.
- **NovaFS, ARM64 natif et x86_64 sous Rosetta.** Chaque architecture a exécuté 196 tests standards et 198 instrumentés avec succès. Le contrôle des octets UTF-8 par voie rapide s'ajoute aux 197 instrumentés historiques. Six cas sont marqués ignorés dans chacune des suites ordinaires. Les campagnes dédiées ont exécuté 1 000 000 d'opérations différentielles et 2 304 injections historiques sur les ancres par architecture. Les snapshots, clones, migrations, le chiffrement et la compression figurent dans les régressions. Quatre échanges interarchitectures en 512/4096 ont vérifié les sources et les alias.
- **DMA et interruptions.** Les 11 profils DMA/GICv3 et les 23 profils DMA séquentielle ont été rejoués avec les noyaux et firmwares de qualification. Dans les VM UTM de la campagne de sauvegarde, le terminal a aussi contrôlé le rattachement du SID 56 et l'égalité des droits accordés et retirés à la sortie d'UEFI.
- **Limites de couverture.** Le protocole expérimental traite les fichiers texte denses d'un volume P9 autonome et crée un objet à un nom libre. Il refuse explicitement une lacune dans les extents de la source. Les pools chiffrés en tant que périphériques du service invité et la coupure physique de l'alimentation du Mac restent à qualifier. Aucun banc électrique dédié n'était disponible.

Le seuil IPC et l'intégrité ont été vérifiés sur les scénarios décrits. Le délai de sauvegarde exigé sous UTM demeure un **critère de qualification non satisfait**. Le prototype reste désactivé par défaut sur sa branche. L'intégration dépend d'une nouvelle validation MOE/MOA. Pour l'essai suivant, proposer un lot **physique** de pages contiguës ou des descripteurs scatter/gather avec une arène DMA privée et bornée. Continuer à contrôler les LBA autorisés, les directions, les quotas, la révocation avant chaque admission et le drainage des commandes déjà en vol avant de libérer les pages. Mesurer les kicks, les IRQ, les empreintes et le temps jusqu'à F2. Cette option concerne le pilote et les mappings IOMMU. Elle requiert sa propre qualification sur 512/4096, les deux architectures, le brut et QCOW2. Le coût SHA-256 en mode logiciel reste mesurable. L'éventuelle accélération matérielle devrait préserver les mêmes domaines et octets, disposer d'une détection des instructions et qualifier la sauvegarde du contexte SIMD.

## Reproduire

Depuis la racine du dépôt, créer une archive privée de la branche du prototype. Les exemples suivants utilisent un répertoire neuf. Appliquer les fixtures **exclusivement dans cette archive**.

```sh
depot="$PWD"
essai=$(mktemp -d)
git archive HEAD | tar -x -C "$essai"
cd "$essai"
for nom in io-0166-prototype io-0166-plages io-0166-charge-4m
do
    patch -p1 -F0 < "$depot/novaOS/tests/fixtures/$nom.patch" || exit 1
done
novaFS/lab/run build --offline --bin novafs
novaFS/lab/run build --offline --features prototype-es-groupees --example es-groupees
generateur="$essai/novaFS/target/aarch64-apple-darwin/release/examples/es-groupees"
make -C novaOS ARCH=aarch64 kernel
python3 -B novaOS/tests/es_groupees_elargies.py aarch64 4096 ajout-4m "$generateur" "$essai/mesure.json" --sans-sonde
```

Répéter avec `ARCH=x86_64`, la valeur `x86_64` en premier argument du banc, et 512/4096 en deuxième argument. Pour la sonde, appliquer `io-0166-prototype-sonde.patch` et `io-0166-plages-sonde.patch` après `io-0166-plages.patch`, puis la fixture de charge. Pour la course, ajouter `vi-conflit-source.patch` et `io-0166-course-nom.patch` dans une autre copie, puis lancer `es_groupees_conflits.py ARCH SECTEUR rapport.json`. Pour les huit coupures, ajouter `io-0166-plages-coupure.patch`, `io-0166-barrieres.patch` et la charge, puis lancer `coupure_cow_groupee.py ARCH SECTEUR rapport.json --barrieres --taille 4194304 --multipage`.

Pour UTM, ajouter `io-0166-systeme-qualification.patch` et construire dans `novaOS` avec `RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --offline --release --locked --target aarch64-unknown-none-softfloat --features systeme-gpt`. Le banc `es_groupees_utm.py PROFIL.utm ELF GENERATEUR SORTIE_NEUVE SECTEUR --cas CAS` emprunte la configuration et le firmware du profil de qualification. Ajouter `--sans-sonde` pour l'invité sans compteurs. Les essais d'une campagne doivent partager le même répertoire parent, où le banc prend un verrou pour qu'une seule VM UTM de mesure s'exécute à la fois. L'option `--format raw` permet le témoin brut dans la même application. Pour les arrêts UTM, utiliser l'ELF système construit avec les fixtures de coupure et ajouter `--cas ajout-4m --sans-sonde --coupure pages`, puis rejouer avec `f1` et `f2`, en conservant un dossier neuf à chaque fois. L'option témoin `--sans-traces-bloc` omet les deux journaux QEMU par E/S et conserve les traces IOMMU. [Banc QEMU](../tests/es_groupees_elargies.py) · [Banc UTM](../tests/es_groupees_utm.py) · [Procédures de régression](es-v0.16.6-qualification.md#reproduire-et-décider)
