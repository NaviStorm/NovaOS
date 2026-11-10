# NovaFS P6 · compression transparente LZ4

## Audit de la base 3ca74ee et décisions de format

P0 décrit les extents par LBA de contenu, génération de naissance et SHA-256 logique. P1 stabilise par F1 puis publie par F2. P2 vérifie les deux générations, leurs B+Trees, les propriétaires des extents et les octets récupérés. P3 réserve la divergence des vues inscriptibles au coût maximal des données brutes. P4 et P5.1 conservent un cache borné de pages logiques contrôlées. P5 tient le registre de partage dans les références persistantes des feuilles, puis reconstruit en RAM les deux graphes durables. L'export autonome émet les pages logiques nécessaires aux deux générations. [Format P5](novafs-p5-format.md) · [Qualification P5](novafs-p5-validation.md) · [Mesures P5.1](novafs-p5.1-performance.md)

Un pack occupant plusieurs blocs pourrait rester épinglé par un seul LBA après divergence. Ce cas rendrait insuffisant le crédit historique d'une page physique par LBA. P6 choisit un bloc physique de 4 Kio par unité compressée, avec 2 à 16 pages de données non nulles dans une même feuille de traduction. Chaque page de 4 Kio a son propre flux LZ4 block. Une unité couvre donc de 8 à 64 Kio logiques. Le coût du descripteur et des longueurs est inclus dans les 4 Kio. Les pages isolées, incompressibles et les métadonnées suivent le chemin brut. Un trou reste implicite. Les tailles de secteur admises divisent exactement la taille physique des unités.

La politique de la vue règle l'encodage des nouvelles données. Une écriture partielle alloue un nouveau LBA FS sans modifier les octets de l'ancienne unité. La tête et son prédécesseur gardent toutes leurs références. Un snapshot ou un clone peut donc conserver certains membres d'une unité après réécriture des autres. Un pack ne libère son bloc physique qu'à la disparition de sa dernière référence. Les garanties CoW et les quotas logiques ne dépendent pas du taux de compression.

Les flux logiques d'export conservent leur format autonome. Une restauration vérifie l'archive puis réencode les pages de données selon la politique de destination. Les deux formats de pool sont sélectionnés explicitement au formatage. Les images historiques conservent leur lecteur. La licence propre au répertoire NovaFS est la MPL-2.0. [Texte officiel intégral](../LICENSE) · [Source Mozilla](https://www.mozilla.org/media/MPL/2.0/index.815ca599c9df.txt)

Le fichier de licence conserve les 16 726 octets du texte officiel, y compris une espace en fin de ligne. Son SHA-256 est `fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85`. Le manifeste Cargo déclare la même licence.

## Contrat binaire du pool P6

Les entiers sur disque sont little-endian. Le bloc logique NovaFS et le bloc physique du pool font 4 096 octets. Un bloc occupe huit secteurs de 512 octets ou un secteur de 4 096. La sûreté de publication ne dépend pas de l'atomicité d'un bloc de 4 Kio. Les deux en-têtes de pool, le sélecteur, les quatre emplacements de catalogue et les nœuds de traduction conservent leurs positions P5. Leurs nouvelles signatures sont `NP6POOLH`, `NP6POOLS`, `NP6POOLC`, `NP6FSROT` et `NP6FSLEF`. Les octets 8–9 portent la révision 2, les octets 10–11 le masque obligatoire 0x0001. Un lecteur vérifie la combinaison exacte, y compris les champs réservés. Un pool P6 dont toutes les vues utilisent `none` exige lui aussi un lecteur P6.

L'octet 57 du **descripteur de vue** enregistre la politique, 0 pour `none`, 1 pour `lz4`. Les 64 bits à l'offset 24 d'une **feuille de traduction** indiquent quelles références désignent des unités compressées. Le bit i s'applique à la référence i. Une référence contient le numéro de bloc physique sur 8 octets et son SHA-256 sur 32 octets. Le masque est couvert par l'empreinte de la feuille. Toute référence marquée doit exister, désigner une unité de la même lignée et y trouver son LBA. Un bloc de données brutes reste arbitraire, même si son contenu commence par une signature NovaFS. La racine, les B+Trees, les ancres, les manifestes et les cartes d'allocation restent bruts. L'admission recoupe les rôles avec les extents de fichiers des générations protégées.

| Offset dans une unité | Taille | Valeur ou règle |
| ---: | ---: | --- |
| 0 | 8 | ASCII `NP6LZ4PK` |
| 8 | 2 | Version d'unité 1 |
| 10 | 1 | Algorithme 1, LZ4 block indépendant sans dictionnaire |
| 11 | 1 | Nombre N de pages, de 2 à 16 inclus |
| 12 | 4 | Taille logique égale à N × 4 096 |
| 16 | 2 | Somme exacte des longueurs des flux, au plus 4 000 octets |
| 18 | 2 | Taille physique allouée, exactement 4 096 |
| 20 | 12 | Réservé, zéro |
| 32 | 64 | Seize couples (LBA u16, longueur u16), N utilisés et ordonnés, reste nul |
| 96 | variable | Flux contigus dont les bornes se déduisent des longueurs, puis remplissage nul |

Les LBA d'une unité appartiennent à la même feuille de 64 positions. L'unité peut grouper des LBA non adjacents. Chaque longueur est positive, les LBA sont strictement croissants et inférieurs à la capacité du volume. Le lecteur vérifie toutes les tranches, les extensions de longueurs LZ4 et les distances de copie avant utilisation. Un flux doit restituer exactement 4 096 octets et une page entièrement nulle s'encode en trou. Un défaut de décodage remonte comme erreur de corruption. Le moteur encode et décode en Rust sûr. La table de recherche de l'encodeur occupe 2 Kio, le tampon d'essai 4 000 octets. Le groupe garde une unité et une page brute de repli. Une lecture décode vers une seule page de sortie. Les tailles venant du support ne déterminent aucune allocation du codec. [Codec et tests](../src/compression.rs) · [Assemblage](../src/shared/packing.rs)

## Publication, intégrité et récupération

1. Le moteur de contenu prépare les pages privées et identifie les nouvelles données par les références de ses extents. Le pool CoW écrit les données, les unités terminées, les traductions modifiées puis le catalogue candidat. Un pack qui ne fait gagner aucun bloc revient au stockage brut. La préparation FIFO est revalidée au moment de publier, y compris sa politique.
2. F1 synchronise toutes les dépendances du candidat. Le pool publie ensuite le sélecteur de catalogue et exécute F2. Les pages logiques en cache deviennent admissibles après le succès de F2. Le reçu P6 lie entre autres la politique, l'identité de la vue et sa racine.
3. La rotation détache les arêtes qui n'appartiennent plus aux deux catalogues protégés. Plusieurs références d'une feuille peuvent nommer la même unité. Chacune la retient mais son poids physique est compté une seule fois. Une réécriture partielle laisse l'ancienne unité intacte, même si une autre vue en utilise un seul membre. Aucun travail de recompression en arrière-plan n'est nécessaire.
4. En cas de publication indéterminée, le prochain écrivain stabilise le support et vérifie le sélecteur, les générations et leur filiation avant de réemployer des adresses. Cela peut ajouter des synchronisations à la reprise. Une mutation normale d'un pool déjà stabilisé comporte exactement deux synchronisations. Les tests de coupure comparent les octets de chaque vue à un oracle indépendant et vérifient deux remontages.

L'empreinte physique d'une unité est SHA-256 du domaine `NovaFS.P6.lz4`, de son adresse u64 little-endian et de ses 4 096 octets. Chaque extent du FS conserve le SHA-256 logique P0, lié aux identités FS et LV, au LBA, à la naissance et à la page décompressée. Le montage valide le graphe, les deux générations et les membres des unités. Il détecte aussi des métadonnées empaquetées alors que les sommes physiques et les octets logiques sont valides. Lors de l'import, le prédécesseur relu doit encore correspondre à la tête montée. La copie est remontée et son reçu et sa carte de données sont comparés avant publication. Cela couvre une source qui renverrait successivement deux générations authentiques différentes.

`check-pool BLOCS SECTEUR --full` contrôle le pool entier. `check BLOCS SECTEUR POOL_ID OBJET EMPREINTE --full` exige en plus le reçu donné. Les deux chemins sont en lecture seule. Sur les pools partagés, le contrôle par défaut est déjà exhaustif. Le coût comprend la lecture du graphe accessible et le décodage des membres de ses unités, y compris ceux que d'autres slots d'une unité gardent physiquement en vie. Le contrôle du contenu des deux générations et de plusieurs vues peut relire des blocs partagés. Le rapport de mesures distingue le remontage du pool et le contrôle supplémentaire du contenu de chaque vue. [Publication et registre physique](../src/shared_graph.rs) · [Vérificateur hors ligne](../src/bin/novafs/check.rs)

Les garanties supposent un écrivain exclusif, des écritures confinées à leur adresse, le modèle de déchirure décrit en P5 et un support qui honore les barrières. Le backend Darwin utilise `File::sync_all`. La perte du cache volatil est injectée dans le simulateur. Les essais sur images et les arrêts de VM ne qualifient pas la persistance d'un SSD après une coupure électrique. Une substitution cohérente de tout le support exige un témoin externe pour détecter un retour en arrière. Les SHA-256 n'authentifient pas un auteur capable de reconstruire tout le graphe.

## Politique, espace et export

La politique se fixe à la création ou par `set-compression` sur une origine ou un clone. Le snapshot est immuable. Le clone hérite de la politique du snapshot, puis peut la changer. Les anciennes pages gardent leur encodage tant qu'elles sont référencées. La limite d'une écriture, la frontière d'une feuille et les pages à forte entropie peuvent produire un singleton brut. À l'inverse, une page à fort taux de compression au milieu d'un groupe peut partager un bloc sans élargir le périmètre d'une future réécriture.

`compression-space` détaille la somme des tailles de fichiers, le quota logique, les LBA comprimés mappés, les unités physiques distinctes, les octets de flux et les octets alloués. L'économie locale vaut 4 096 × (LBA mappés − unités distinctes). Les octets de flux comprennent les membres conservés mais devenus inaccessibles depuis cette vue. Cette économie est un écart avec le stockage brut des mêmes LBA. Pour comparer les pools, utiliser leur occupation dédupliquée, qui inclut les deux générations, les métadonnées et les sept pages fixes. Additionner les économies de deux snapshots partagerait deux fois le même gain.

La limite logique d'une vue s'applique avant le choix d'encodage. La réservation d'une vue inscriptible couvre sa divergence au coût brut, même après un excellent gain de compression. Les snapshots facturent leurs blocs physiques accessibles selon la règle P5 de déduplication des racines identiques. Une politique plus efficace n'augmente aucun quota. Dans le profil actuel, une vue contient de 256 à 1 024 blocs logiques, avec au plus 16 vues dans le catalogue. [Admission et quotas](../src/shared.rs)

`migrate-view` relit une vue P5/P6 et produit **un nouveau pool P6 sur stdout**. Le pool source reste disponible et doit être conservé pour retrouver ses autres vues et son historique de partage. Le formatage par défaut sans argument reste P5. L'export complet version 1 continue de transporter les pages logiques indépendantes du pool et leur identité. Son en-tête garde le reçu de la source, qui peut différer entre deux encodages du même contenu. `restore` avec une politique explicite matérialise un nouveau pool P6 et revalide la vue. La comparaison interarchitectures vérifie séparément les enregistrements logiques et l'en-tête d'archive. [Échanges binaires et migrations](validation/v0.9.8/interop-p6.json)

## Vérifications fonctionnelles

Les deux exécutables sont des Mach-O distincts. Le laboratoire contrôle leurs identifiants CPU, exécute ARM64 en natif et x86_64 sous Rosetta. Une même image est produite d'un côté, relue de l'autre et restaurée dans une destination neuve. Les fichiers sont des images temporaires. Les oracles comparent des octets et l'état attendu des vues après remontage.

| Exigences P6 | Scénarios et critères d'acceptation |
| --- | --- |
| 1–4, suites P0 à P5.1 | Suites Cargo standard et instrumentées, million différentiel, ordres de suppression, restitution, saturation, FIFO et contrôle du cache après F2. Coupures P5 à toutes les frontières, erreurs d'E/S et sélecteurs déchirés. |
| 5–7, encodage et remontage | Les pages répétitives forment des unités, les pseudo-aléatoires reviennent au stockage brut. Vérification des octets après remontage et comparaison bidirectionnelle avec le codec de référence. |
| 8–9, écritures partielles et frontières | Modifications de 1 octet, 4 Kio et 16 Kio à l'intérieur et à cheval sur les limites, dont le décalage de 15 pages + 37 octets. Comparaison du clone, du second clone et du snapshot après chaque remontage. |
| 10–11, saturation et quotas | Crédit de l'origine identique après réécriture compressible, refus d'admettre un clone supplémentaire et d'étendre un fichier au-delà du quota logique. Génération et octets protégés comparés après refus. |
| 12–15, CoW et libération | Snapshots immuables, plusieurs clones, suppression de l'origine, conservation du dernier LBA d'une unité et libération après rotation du prédécesseur. L'occupation revient aux sept blocs fixes après retrait des vues. |
| 16–19, corruption | Répertoires non canoniques, taille produite invalide, distances LZ4 invalides, versions ou fonctionnalités inconnues, LBA absent ou mauvaise lignée. Variantes rescellées pour exercer les contrôles sémantiques même si les SHA physiques concordent. Les CLI renvoient l'incohérence et le fichier source conserve son SHA-256. |
| 20–24, export et compatibilité | Huit pools P6 répartis sur les politiques, ISA et secteurs. Seize restaurations P6 au codage opposé et huit avec le binaire P5.1 archivé après destruction des sources. Quatre migrations explicites P5.1 vers P6. Les huit archives ont la même empreinte de contenu logique. Huit refus d'écriture par l'ancien outil. |
| 25, restitution | Mesure du tas vivant après chaque série complète de créations, divergences, suppressions et remontages, avec et sans erreur F2. Le seuil d'acceptation est l'égalité exacte avec la mesure initiale. |
| Campagnes P6 complémentaires | Deux graines par secteur combinent compression, variations d'entropie, snapshots, clones, changements de politique, coupures et échecs de flush. Une campagne distincte teste chaque frontière de publication d'une mutation compressée. Une source changeante pendant l'import est refusée avant publication. |

| Résultats sur les sources finales, deux géométries cumulées | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Suites Cargo standard / toutes sondes | 137 / 138 réussis | 137 / 138 réussis |
| Million différentiel, temps écoulé | 24,660 s | 40,177 s |
| Publications observées / pénuries injectées / remontages audités | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |
| P5, coupures / E/S injectées / sélecteurs déchirés | 684 / 152 / 12 | 684 / 152 / 12 |
| P6, frontières d'une écriture compressée / coupures / EIO d'écriture | 78 / 234 / 40 | 78 / 234 / 40 |
| P6, campagnes déterministes supplémentaires, actions / coupures / échecs de flush | 96 / 48 / 48 | 96 / 48 / 48 |
| P6, cycles de restitution dont la moitié avec erreur F2 | 128 | 128 |
| Tas vivant avant et après chaque série P6 | 5 520 → 5 520 octets | 5 360 → 5 360 octets |

Les suites standard et instrumentées se recouvrent. Le million différentiel est le test historique du moteur de contenu. Les campagnes CoW/LZ4 sont comptées séparément. Les quatre scénarios normalement ignorés par Cargo ont des lanceurs dédiés pour le différentiel, les volumes, le coût des extents et l'oracle LZ4. L'égalité du tas vivant porte sur les séries mesurées. [Détail des suites ARM64](validation/v0.9.8/suites-aarch64.json) · [Détail des suites Rosetta](validation/v0.9.8/suites-x86_64.json)

Les 256 pages du corpus LZ4 sont encodées par liblz4 1.11.0 et décodées par chaque NovaFS, puis encodées par chaque NovaFS et décodées par liblz4. Le SHA-256 des flux émis est identique sur les deux ISA. Cette bibliothèque sert uniquement d'oracle au laboratoire. Les tests de formats exécutent les lecteurs et producteurs de huit révisions, de P0 à P5.1. Les essais historiques P1 ont effectivement interrompu 24 processus sur image et dix VM par architecture de l'outil hôte, avec deux remontages concordants par essai. L'invité est x86_64 dans les deux campagnes. [Oracle LZ4](validation/v0.9.8/lz4-interop.json) · [Formats historiques](validation/v0.9.8/historiques.json) · [Interruptions de fichier et VM](validation/v0.9.8/pannes-p1.json)

## Méthode de mesure

Le même client de laboratoire est compilé contre le commit 3ca74ee et le moteur P6. Les trois configurations sont P5.1 brut, P6 `none` et P6 `lz4`. Sur chaque architecture et secteur, deux campagnes alternent l'ordre des configurations, avec une nouvelle image par profil. Les positions et tailles d'appel sont appariées entre les trois phases. Une passe supplémentaire avec sondes relève les temps CPU du codec sur chacune des quatre combinaisons architecture/secteur. Le client utilise des fichiers de 4 Kio et 512 Kio, avec des pages répétitives, mixtes (2 560 premiers octets répétitifs par page) et pseudo-aléatoires. Les copies de référence restent dans le banc pour comparer les octets lus.

Les mesures de latence sont murales et individuelles. Pour un groupe de N appels, le quantile p est l'échantillon de rang supérieur ou égal à N × p, trié par durée croissante. La colonne débit est la somme des octets utiles divisée par la somme des durées. Le CPU du thread, les lectures/écritures achevées au fichier image, leurs durées, les temps F1 et F2 et le temps CPU du codec sont enregistrés séparément. L'amplification vaut 4 096 × nombre de blocs écrits / octets demandés. Toute mutation chronométrée doit effectuer deux synchronisations et toute lecture, ouverture ou vérification comptée doit en effectuer zéro.

Les lectures dites froides suivent la restitution du cache NovaFS et l'ouverture du chemin. Les lectures chaudes répètent immédiatement le parcours. Le cache macOS reste sous contrôle de l'hôte. Une lecture physique au sens du banc est une demande au fichier image, susceptible d'être servie par le cache de macOS. Les séquences quantifient le comportement du prototype sur cette machine, y compris la traduction Rosetta et les variations de synchronisation de l'hôte.

Le compteur de tas inclut l'arrondi de l'allocateur Darwin ainsi que les tampons de référence du client. Le budget de travail additionne le pic de ce tas à toute la pile réservée du thread de travail. La RSS maximale, comptée par `/usr/bin/time -l`, inclut davantage de mémoire hôte. Les journaux conservent les versions, les empreintes des sources et des exécutables, les bornes UTC des passages et l'étalonnage des horloges.

Le profil historique P5.1, recompilé avec les sondes P6, a également produit les 28 enregistrements attendus par architecture. Ses 384 écritures ont déclenché 768 synchronisations, ses 256 lectures de lots aucune écriture ou synchronisation. Dans chaque géométrie, le témoin brut de 128 mutations conserve les 1 660 écritures de blocs et 256 synchronisations historiques. [Profil ARM64](validation/v0.9.8/profil-p51-aarch64.jsonl.gz) · [Profil x86_64](validation/v0.9.8/profil-p51-x86_64.jsonl.gz)

## Résultats comparés à P5.1

Le banc a enregistré 11 808 appels sans sonde et 1 968 appels instrumentés sur un Apple M4 Max, macOS 27.2. Sur le fichier de 512 Kio, l'origine seule et son prédécesseur occupent 162 blocs après le premier remplissage en P5.1 et en P6 `none`. La table donne le résultat avec `lz4`. Le ratio porte sur les 128 pages de données initiales et le gain net sur l'occupation physique du pool entier.

| Contenu du fichier de 512 Kio | Blocs occupés, P6 LZ4 | Économie physique | Ratio logique/physique des données | Amplification d'écriture séquentielle, P5.1 → P6 LZ4 |
| --- | ---: | ---: | ---: | ---: |
| Répétitif | 46 | 464 Kio | 10,667× | 1,641× → 0,734× |
| Mixte | 99 | 252 Kio | 1,969× | 1,641× → 1,148× |
| Pseudo-aléatoire | 162 | 0 | 1,000× | 1,641× → 1,641× |

Le fichier de 4 Kio occupe 26 blocs de pool dans les trois configurations. Son unique page ne peut économiser un bloc physique entier. Sur les données répétitives, le débit séquentiel d'écriture de 64 Kio passe de 5,92 à 6,40 Mio/s sur ARM64 et de 5,76 à 5,77 sous Rosetta. Sur les données pseudo-aléatoires, il passe de 6,17 à 6,08 Mio/s et de 5,67 à 5,43 Mio/s. Le coût CPU moyen d'un essai de compression de 64 Kio pseudo-aléatoires atteint 190 µs sur ARM64 et 186 µs sous Rosetta, pour un gain physique nul.

| Mesure | ARM64 natif, P5.1 → P6 LZ4 | x86_64 sous Rosetta, P5.1 → P6 LZ4 |
| --- | ---: | ---: |
| Lecture séquentielle froide NovaFS, 512 Kio, trois entropies | 160,56 → 155,56 Mio/s | 116,38 → 110,69 Mio/s |
| Même lecture, cache NovaFS chaud | 1 159,03 → 1 122,07 Mio/s | 850,38 → 842,44 Mio/s |
| Lecture aléatoire froide NovaFS, fichier de 512 Kio | 163,94 → 164,10 Mio/s | 119,44 → 115,78 Mio/s |
| p99 de toute mutation, 336 appels par cellule | 11,926 → 12,389 ms | 12,724 → 13,264 ms |
| Maximum de toute mutation sur les mêmes appels | 15,264 → 152,625 ms | 13,396 → 13,681 ms |
| Pic tas + pile de travail, essais sans sondes | 3,688 → 3,688 Mio | 3,676 → 3,676 Mio |
| Pic RSS du processus, essais sans sondes | 5,125 → 5,234 Mio | 10,367 → 10,402 Mio |

La pointe ARM64 de 152,625 ms avec LZ4 concerne le petit fichier mixte en secteurs de 4 096. F1 a pris 74,117 ms et F2 77,927 ms, pour 0,810 ms de CPU du thread. Le mode P6 `none` présente aussi une pointe de 146,316 ms sur une réécriture de clone. Aucune mutation chronométrée n'a atteint les 263 ms observées en P5.1 sur une autre campagne. Deux répétitions ne suffisent pas à exclure de nouvelles pointes ni à attribuer au codec les fluctuations de l'hôte. Le lecteur cache les pages logiques. À froid, plusieurs pages d'une unité peuvent encore provoquer plusieurs demandes de lecture du même bloc physique. À chaud, le temps de décompression mesuré sur ces lectures est nul.

Le pic toutes configurations et toutes sondes comprises est de 3 866 976 octets pour le tas et la pile de travail, sous le plafond de 7 340 032 octets des charges de référence. Le remontage et les vérifications exhaustives relisent l'intégrité des vues. Pour le grand fichier répétitif, le temps moyen de montage est passé de 98,36 à 94,29 ms sur ARM64 et de 139,78 à 144,24 ms sous Rosetta. Le contrôle supplémentaire du contenu des trois vues a pris respectivement 85,84 et 133,21 ms avec LZ4. Ces durées dépendent de la taille du graphe et du support. [Tableaux détaillés, coûts CPU, E/S et distributions](validation/v0.9.8/mesures/README.md) · [Appels individuels](validation/v0.9.8/mesures/echantillons.jsonl.gz) · [Distributions avec effectifs](validation/v0.9.8/mesures/distributions.json.gz) · [Empreintes et mesures mémoire](validation/v0.9.8/mesures/campagnes.json)

## Reproduire

Depuis la racine du monorepo, sur un Mac Apple Silicon avec Rosetta, Rust stable et les deux cibles Darwin, utiliser des fichiers temporaires ordinaires. Les dépendances du Cargo.lock doivent être en cache pour le mode hors ligne. Le corpus interversions utilise les objets Git locaux. Les essais VM P1 utilisent QEMU x86_64, EDK II, Clang et le linker Rust. L'oracle LZ4 accepte `--bibliotheque /chemin/vers/liblz4.dylib`. Les mesures RSS demandent un accès aux compteurs Darwin.

```sh
set -eu
resultats=$(mktemp -d)
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "$resultats/suites-$isa.json"
done
python3 novaFS/lab/p6-lz4-interop.py > "$resultats/lz4-interop.json"
python3 novaFS/lab/p6-interop.py > "$resultats/interop-p6.json"
python3 novaFS/lab/p6-historiques.py --support formats > "$resultats/historiques.json"
python3 novaFS/lab/p6-historiques.py --support pannes > "$resultats/pannes-p1.json"
python3 novaFS/lab/p51profil.py --ref travail --sortie "$resultats/profil-p51"
python3 novaFS/lab/p6-mesures.py --repetitions 2 --sortie "$resultats/mesures"
```

Exemple de création et inspection d'une image jetable. Les commandes de transformation lisent une image et émettent leur résultat dans **un autre fichier**. La dernière commande rend le détail de l'occupation en JSON.

```sh
set -eu
image=$(mktemp -d)
pool=13131313131313131313131313131313
domaine=35353535353535353535353535353535
fs=57575757575757575757575757575757
lv=68686868686868686868686868686868
outil=novaFS/lab/run
"$outil" run --offline -- pool-format 2048 4096 "$pool" p6 > "$image/vide.raw"
"$outil" run --offline -- volume-create 2048 4096 "$domaine" "$fs" "$lv" 256 lz4 \
    < "$image/vide.raw" > "$image/volume.raw"
"$outil" run --offline -- file-create 2048 4096 1 /exemple \
    < "$image/volume.raw" > "$image/fichier.raw"
bloc=$(python3 -c 'print("41" * 8192)')
"$outil" run --offline -- file-write 2048 4096 1 /exemple 0 "$bloc" \
    < "$image/fichier.raw" > "$image/donnees.raw"
"$outil" run --offline -- check-pool 2048 4096 --full < "$image/donnees.raw"
"$outil" run --offline -- compression-space 2048 4096 1 < "$image/donnees.raw"
```
