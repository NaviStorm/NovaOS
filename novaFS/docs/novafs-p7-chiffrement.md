# NovaFS P7 · chiffrement authentifié des volumes · v0.9.9

Le format P7 permet de faire cohabiter des volumes clairs et chiffrés dans le même pool. Le service ouvre le pool avec son secret administratif. Le propriétaire déverrouille chaque domaine chiffré avec sa phrase ou sa clé de récupération. Les images et les archives de cette version sont expérimentales. Le code du prototype et sa licence MPL-2.0 se trouvent dans le paquet NovaFS. [Moteur du pool](../src/secure_pool.rs) · [Licence](../LICENSE)

## Contrat de sécurité et décision de nonces

Un lecteur hors ligne peut conserver, lire, comparer, tronquer, déplacer ou réécrire les octets du support et tenter des phrases hors ligne. La confidentialité couvre les noms, le contenu, les extents et les métadonnées internes d'une vue chiffrée. La géométrie, les identités publiques, la parenté observable via les références, l'occupation, les positions, les longueurs et le choix de compression restent visibles. Le service, ses clés, son espace mémoire déverrouillé, le CSPRNG et le code de validation appartiennent au périmètre de confiance. La destruction du support peut empêcher la lecture.

**Décision A.** XChaCha20-Poly1305 utilise une clé de 256 bits, un nonce de 192 bits et un tag de 128 bits. Chaque appel de scellement tire 24 octets indépendants, y compris pour les enveloppes, les objets abandonnés avant publication et les réessais. Une référence CoW qui reprend les mêmes octets ne déclenche aucun chiffrement. Le contexte disque et la génération servent à authentifier l'unité. Un défaut d'entropie fait échouer l'opération. Le pilote hôte utilise `getrandom::fill` et le backend Darwin `getentropy`. La façade accepte une implémentation remplaçable de `Entropy`. Un futur fournisseur NovaOS devra obtenir un état frais, y compris après duplication de l'hôte en fonctionnement. [Primitives](../src/crypto.rs) · [Construction XChaCha20-Poly1305](https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction) · [Source Darwin de getrandom](https://docs.rs/getrandom/0.4.3/getrandom/#supported-targets)

Pour deux tirages, la probabilité de collision est 2⁻¹⁹². Pour q tirages uniformes et indépendants sous une même clé, l'union des q(q−1)/2 paires donne la borne q(q−1)/2¹⁹³. À q = 2³², elle vaut 2⁻¹²⁹ × (1 − 2⁻³²), soit moins de 1,47 × 10⁻³⁹. Cette borne concerne les nonces. La sécurité de la phrase et la résistance de l'authentificateur ont d'autres limites.

**Plafond d'exploitation de 2³² scellements par clé effective**, en cumulant les branches, les reprises et les tentatives. Le disque seul ne connaît pas le nombre de copies ni leurs usages. Le détenteur doit allouer des budgets aux écrivains et provisionner un nouveau domaine avant le plafond. Si le cumul ne peut plus être borné, retirer l'ancienne clé de l'écriture. La migration disponible consiste à figer un snapshot, produire et contrôler son archive, puis vérifier sa restauration dans un pool neuf doté d'une nouvelle clé maîtresse. Confirmer la relecture et sauvegarder le nouveau secours avant de retirer l'ancienne copie. Une telle migration porte sur une vue à la fois. Elle ne recrée pas la topologie de partage et n'efface pas les sauvegardes anciennes. Une nouvelle enveloppe de phrase sous une même maîtresse laisserait les clés de données identiques. P7 ne réalise aucune conversion automatique.

L'audit de `0d72ebc` avait mesuré deux branches P6 produisant des contenus différents à la même génération 4, au même LBA 9 et au même bloc physique 22. Le scénario a été rejoué sur `ea00a755` en natif et sous Rosetta dans les deux géométries. Le banc P7 compare les nonces effectivement persistés sur des écritures issues de deux copies autonomes. Cette observation teste le câblage. La borne provient de l'hypothèse sur la source d'entropie. [Tests du pool et des copies](../tests/p7_secure.rs)

## Autorités, clés et mémoire

- Une clé maîtresse administrative aléatoire est enveloppée par une sous-clé dérivée du secret de provisionnement externe de 32 octets. La clé de publication est dérivée de cette maîtresse et de l'identité du pool. Le propriétaire conserve hors du support l'identité attendue et le secret administratif.
- Chaque origine chiffrée reçoit une maîtresse aléatoire de 32 octets et un domaine aléatoire de 16 octets. HKDF-SHA-256 sépare les usages données, métadonnées et engagement de vue. Les libellés ont un préfixe de protocole et une longueur. La dérivation et le calcul des MAC utilisent RustCrypto. [HKDF](https://www.rfc-editor.org/rfc/rfc5869)
- Les deux emplacements de 72 octets enveloppent indépendamment la même maîtresse. Chacun contient un nonce, un tag et 32 octets chiffrés. Le profil de 16 octets et un sel frais de 16 octets précèdent les emplacements. Le type, le domaine, l'indice et le profil sont engagés. Argon2id v19 utilise m = 65 536 Kio, t = 3, p = 4. Le second emplacement dérive sa clé d'un secret de récupération aléatoire indépendant de 32 octets. La clé du domaine authentifie également l'ensemble des emplacements autorisés. Les paramètres et longueurs sont contrôlés avant allocation. Un seul Argon2 à la fois, avec retour `Busy` en contention. [Profil Argon2id](https://www.rfc-editor.org/rfc/rfc9106.html#section-4) · [Dépendances et vecteurs](novafs-p7-verification.md)
- Un snapshot et ses clones utilisent la même maîtresse, même si l'origine est supprimée. Le détenteur de la clé de domaine peut lire ses descendants hors ligne. Le service applique séparément les droits sur chaque vue. Le verrouillage supprime les sous-clés résidentes du domaine. Une lecture ciblée peut continuer sur un autre domaine. Une mutation ou un contrôle intégral exige les clés de tous les domaines retenus par les deux catalogues. `encrypted_domains()` les énumère, y compris celles d'une origine en cours de retrait.
- Les secrets possédés, le tampon Argon2, les pages privées du moteur et les entrées du cache sont effacés à leur libération. Le verrouillage exige un emprunt exclusif. Les appels de la façade ne conservent aucun ticket de lecture ni cache clair déverrouillé entre opérations. L'appelant possède les octets déjà rendus par `read_file`. Les registres, les copies temporaires du compilateur, les diagnostics de l'hôte et le swap limitent l'effacement vérifiable. La perte de tous les secrets de déverrouillage pour un domaine rend son contenu irrécupérable. Le prototype ne dépend d'aucun trousseau externe.

## Disposition du support et vérification des références

Les entiers sont little-endian. Un bloc fait 4 096 octets. Le premier ou le second en-tête P7 permet d'identifier la famille. Le lecteur exige les deux copies canoniques, la révision 7, le bit de fonctionnalité 1 et la géométrie exacte. Le formateur se limite aux images explicitement neuves avec zone de contrôle vide. Les lecteurs historiques gardent leur contrat P0/P5/P6. L'ancien écrivain P6 a été présenté à des images P7 sur les deux architectures. [Échanges et refus de l'ancien écrivain](validation/v0.9.9/p7-echanges.json)

| Adresses physiques | Contenu |
| --- | --- |
| 0, 1 | En-tête `NP7POOLH`. Aux offsets 16 et 32 figurent l'identité du pool et sa taille. Les octets 40 à 111 contiennent le nonce, le tag et la maîtresse administrative enveloppée. |
| 2 à 9 | Quatre paires d'attestations indépendantes. À la génération g, les indices sont 2 + 2(g mod 4) et le suivant. Chaque HMAC engage le pool, la base, l'emplacement, g, le digest de la banque et celui du parent. |
| 10 à 45 | Quatre banques de neuf blocs. Chacune porte le répertoire des domaines, les sept blocs de contrôle du graphe interne, puis les HMAC des vues de la tête et du prédécesseur. L'ordre est couvert par le digest de banque signé après F1. |
| À partir de 46 | Chaque unité réserve deux blocs contigus, en-tête canonique et corps de 4 Kio. L'indice CoW b ≥ 7 désigne le bloc physique 46 + 2(b − 7). Le dernier bloc isolé éventuel reste hors capacité. |

Le répertoire accepte jusqu'à 17 domaines retenus. Le graphe admet 16 vues vivantes et conserve également les références du prédécesseur. Le HMAC de vue lie pool, domaine, identités FS/LV, génération du catalogue, identifiant, propriétaire, classe, politique, quota, racine physique, empreinte et emplacements de déverrouillage. Le crédit P3 s'applique à la capacité interne de 7 + ⌊(blocs physiques − 46)/2⌋. Chaque unité allouée consomme deux blocs, y compris sur un volume clair. Le coût du sidecar entre ainsi dans l'admission, même pour des pages incompressibles ou partagées.

L'en-tête `NP7UNIT1` a une version explicite. Les 88 premiers octets forment les données associées. Ils fixent la suite, le type, l'encodage, le pool, le domaine, la portée logique, la position physique, la génération de naissance, les longueurs et le bitmap des slots. Les offsets 88–111 et 112–127 contiennent le nonce et le tag. Le reste est nul. Le SHA-256 du préfixe de séparation et de la paire complète identifie cette **instance immuable** dans sa référence parente. Une nouvelle écriture à la même adresse et à la même génération sur une branche sœur produit une autre référence. Une copie physique à une autre position ne vérifie pas les données associées attendues.

La construction d'un pack LZ4 précède le scellement. Les positions d'un pack décrivent sa portée d'origine, même si un clone ne garde qu'un seul slot. Le vérificateur compare d'abord contexte et empreinte de la paire, vérifie le tag, déchiffre, confronte les longueurs et le bitmap au répertoire et contrôle les extents. Le contenu brut peut commencer par une signature LZ4. Seul le type de référence autorise le décodeur. [Enveloppe d'unité](../src/secure_unit.rs) · [Graphe et pin des blocs](../src/shared_graph.rs)

## Publication, perte de fraîcheur et traitement des erreurs

1. L'écrivain exclusif épingle les deux générations. Il prépare les nouveaux objets hors de leurs adresses, puis la nouvelle banque et les preuves de vue. Les opérations du graphe sur son préfixe privé restent en RAM. **F1** synchronise les dépendances et la banque.
2. Après le succès de F1, le journal de NovaFS produit deux HMAC de publication pour deux blocs disjoints. **F2** précède le succès rendu au client. Dès qu'une attestation entière a persisté, la transaction peut être visible après une interruption, même si son appelant n'a reçu aucun acquittement. Une banque préparée seule ne choisit jamais la tête.
3. Le montage opérationnel sélectionne la plus haute attestation authentifiée et exige son prédécesseur lié. Deux attestations valides incompatibles d'une même génération bloquent la reprise. Un échec sur le catalogue sélectionné ou ses dépendances interdit son utilisation. La perte d'une copie complète laisse la seconde disponible. Avant une réparation, l'écrivain draine, relit le support et vérifie les données des graphes vivants. Le contrôle hors ligne ne répare aucun octet. [Journal et tests](../src/secure_commit.rs)
4. Un échec après l'émission d'une attestation rend le résultat indéterminé. Un échec après une écriture matérielle interdit le réemploi dans cette instance. Réouvrir le même support exclusif, réexaminer la tête et comparer le reçu client. Les résultats supposent des écritures confinées à la bonne adresse et des flushs fidèles au domaine de persistance déclaré.

Un adversaire qui rejoue **les deux blocs d'attestation** peut reproduire l'état observable d'avant publication si l'ancienne banque et ses données existent encore. Il suffit de rétablir un ancien *état logique* valide, parfois en changeant très peu d'octets physiques. Deux branches légitimes dupliquées peuvent aussi avoir des têtes authentiques distinctes. La fraîcheur absolue exige un témoin récent indépendant qui lie identité, génération et racine. Un reçu conservé dans la même image peut reculer avec elle. Les essais sur simulateur, VM et images Darwin ne démontrent aucune persistance après coupure électrique d'un SSD.

## Sauvegarde autonome et restauration

Le détenteur fournit une clé de sauvegarde indépendante. À chaque export, le CSPRNG génère une identité d'archive et une maîtresse de transport fraîches. L'enveloppe protège cette dernière, puis des sous-clés séparées authentifient le manifeste, les enregistrements et la clôture. Les données associées de chaque page incluent le digest du contexte, son rang et son LBA. La clôture lie tous les octets précédents, leur ordre et le nombre de pages. Une fin exacte est exigée. Le manifeste reprend les identités de lignée et le reçu P5/P6 de la vue, en plus du domaine source.

L'importeur conserve uniquement du chiffré dans son image temporaire. Il vérifie l'archive, la couverture exacte des deux générations FS et revalide chaque page relue. La restauration crée une **nouvelle maîtresse et un nouveau domaine**, y compris lors d'une deuxième restauration du même export. La destination réencode ses propres unités et conserve les identités FS/LV du contrat A. Aucune clé maîtresse source ni aucun pool source n'est nécessaire. La perte de la clé de sauvegarde rend cette archive inutilisable. Un détenteur de cette clé peut fabriquer une archive authentique. La preuve de provenance ou de fraîcheur exige un reçu externe de confiance. [Export authentifié](../src/secure_export.rs) · [Contrat A](novafs-p5-export.md)

## Commandes et reproduction

Les transformations de la CLI écrivent l'image résultante sur stdout. Pour conserver l'image d'entrée, rediriger stdout vers un **autre** fichier. Le provisionneur transmet la trame binaire `NP7KEYS1` par le pipe anonyme FD3. Elle contient l'identité attendue, le secret administratif et les moyens de déverrouillage des domaines nécessaires. Les commandes suivantes utilisent ces descripteurs hérités. Le parcours Python configure les pipes sous macOS et génère les secrets de test en mémoire. [Encodage de FD3 et codes de retour](novafs-p7-verification.md) · [Exemple complet de provisionnement](../lab/p7-cli.py)

| Commande | Données supplémentaires sur FD4 |
| --- | --- |
| `pool-format BLOCS SECTEUR POOL_ID p7 --cles-fd3` | Aucune |
| `volume-create BLOCS SECTEUR OWNER FS LV TAILLE none\|lz4 clair\|chiffre --cles-fd3` | Pour `chiffre`, longueur de phrase u16 LE, 1 à 1 024 octets de phrase, puis 32 octets de secours indépendants et EOF |
| `file-write BLOCS SECTEUR OBJET CHEMIN OFFSET --donnees-fd4 --cles-fd3` | Jusqu'à 65 536 octets de contenu du fichier |
| `export BLOCS SECTEUR OBJET --cles-fd3` | Clé de sauvegarde de 32 octets et EOF |
| `check-export --cle-fd4` | Même clé de sauvegarde. L'archive arrive sur stdin |
| `restore BLOCS SECTEUR NOUVEAU_POOL OWNER none\|lz4 --cles-fd3` | Clé de sauvegarde suivie des éléments de provisionnement du nouveau domaine |

Les mêmes options de pool s'appliquent à `file-create`, `file-read`, `snapshot`, `clone`, `drop-view`, `advance`, `set-compression` et `list-views`. Le formatage P7 et l'enrôlement sont explicites. `check-pool` et `check` sans clé rendent un rapport structurel. Avec `--cles-fd3`, ils contrôlent également l'authenticité. L'archive P7 se contrôle indépendamment avec sa seule clé de sauvegarde.

```sh
resultats=$(mktemp -d)
for isa in aarch64 x86_64; do
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run test --offline --lib --test p7_secure --test p7_cli -- --test-threads=1
    python3 novaFS/lab/p7-cli.py "$isa"
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "$resultats/historique-$isa.json"
done
python3 novaFS/lab/p6-lz4-interop.py > "$resultats/lz4.json"
python3 novaFS/lab/p6-interop.py > "$resultats/interop-p6.json"
python3 novaFS/lab/p6-historiques.py --support formats > "$resultats/anciens-formats.json"
python3 novaFS/lab/p6-historiques.py --support pannes > "$resultats/pannes-support.json"
python3 novaFS/lab/p7-no-std.py
python3 novaFS/lab/p7-validation.py tout --sortie "$resultats"
```

Le protocole et le script exacts du contre-exemple P6 se lisent avec `git show 0d72ebc:novaFS/docs/novafs-p7-chiffrement.md`, section 8. Ils peuvent être rejoués dans une extraction de `ea00a755`. Les commandes de vérification d’archives historiques construisent certains binaires via Git et les essais de VM requièrent QEMU.

## Qualification sur ARM64 et x86_64 sous Rosetta

Les deux exécutables Darwin ont tourné sur le Mac Apple Silicon, avec des secteurs de 512 et 4 096 octets. Les scénarios ci-dessous identifient l'oracle et la portée de chaque résultat. Les essais cryptographiques et les tests de pannes constituent une validation du prototype par ses auteurs. [Rapport par architecture](validation/v0.9.9/integration-p7.json) · [Qualification des primitives](novafs-p7-verification.md)

| Exigences | Vérifications réalisées |
| --- | --- |
| 1–3, 33 | Suites historiques P0 à P6, scénarios ignorés lancés par leur banc, un million d'opérations différentielles par ISA, lectures des anciens formats, reprise des campagnes de panne et de saturation. |
| 4–6 | Reproduction du contre-exemple P6. Bifurcation P7 sur deux copies et comparaison de nonces sous le CSPRNG réel. Six échecs d'entropie pendant le CoW par architecture. Validation par vecteurs indépendants sur la même source. |
| 7–12 | Origines indépendantes de contenu identique, volume clair dans le même pool, phrase et récupération, mauvais secrets, verrouillage d'un seul domaine, remontage et lecture exacte. |
| 13–19 | Snapshots et clones, suppression de l'origine, écritures divergentes et partielles, relecture des survivants, LZ4 et données incompressibles, campagnes à graines fixes avec oracle indépendant. |
| 20–25, 36 | 30 altérations physiques par ISA, dont chiffrés, nonces, tags, longueurs, domaines et références. Une banque republiée sous une authentification administrative valide échoue si la preuve du domaine est fausse. Trames de clés et structures de taille hostile refusées. |
| 26–29 | Préservation des crédits CoW à saturation. Par ISA, 582 variantes de coupure et 70 erreurs d'écriture ou de synchronisation sur le pool chiffré. Relecture des deux états admissibles, contrôle du prédécesseur et nouvelles écritures après reprise. |
| 30–32, 34 | Export, destruction de la source et huit restaurations croisées au total. Clé erronée, clôture absente, données modifiées et enregistrements substitués rejetés. Les lecteurs anciens ont refusé la révision P7. |
| 35 | Deux campagnes par ISA, 7 488 opérations chronométrées et 96 processus mesurés. Tas de départ retrouvé après destruction dans les 96 processus. Pic tas + pile hors KDF de 2,26 Mio en natif et 2,28 Mio sous Rosetta. |

| Campagne finale par architecture | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Tests Cargo standard / instrumentés, zéro échec | 163 / 164 | 163 / 164 |
| Opérations différentielles / durée réelle | 1 000 000 / 25,670 s | 1 000 000 / 40,233 s |
| Pénuries injectées / remontages du modèle | 54 643 / 1 000 | 54 643 / 1 000 |
| Coupures P7 / erreurs d'écriture et de synchronisation P7 | 582 / 70 | 582 / 70 |
| Commandes de bout en bout formatage → export → restauration, géométries réussies | 2 / 2 | 2 / 2 |

Les suites standard et instrumentées se recouvrent. Le banc dédié a exercé les quatre scénarios ignorés dans la commande Cargo standard, dont le million différentiel. Le rapport conserve également les essais sur fichiers et les coupures de VM des protocoles antérieurs. [Résultats historiques complets](validation/v0.9.9/suites-historiques.json.gz)

Sur le clone LZ4 à données répétitives, le p99 d'écriture P6/P7 chiffré passe de 12,34 à 34,84 ms sur ARM64 et de 12,28 à 69,47 ms sous Rosetta. Le prototype réinspecte la vue sur le chemin de lecture et rouvre le graphe sur le chemin de mutation. Il soumet deux blocs physiques par unité et neuf blocs de banque. Le coût existe aussi sur un volume P7 clair. Le pic du déverrouillage Argon2id, tas et pile inclus, atteint 66,06 et 66,05 Mio respectivement. Les distributions et les observations individuelles permettent de comparer par architecture, profil, opération, géométrie et répétition. [Comparaison P6/P7 et reproduction](novafs-p7-mesures.md)
