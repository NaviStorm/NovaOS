# NovaFS P9 · Horodatages natifs et migration cryptographique

NovaFS 0.9.13 introduit un profil de contenu horodaté et un profil de pool authentifié. La migration se fait hors ligne vers une **nouvelle image**. Le service de NovaOS 0.13.0 conserve son contrat de montage historique. La bibliothèque portable fournit les opérations de dates et de migration aux intégrateurs. [Source du moteur](../src/engine.rs) · [Migration du pool](../src/secure_pool/migration.rs)

## Décisions et preuves de départ

La base est `f2742e1f7621eb4b732e81bd472a7156b1f48ecf`. L’audit précédent a établi qu’un scellement interrompu peut tirer un nonce puis laisser le disque identique à sa génération précédente. En P8, deux exports valides d’un même snapshot et de son clone pouvaient aussi produire `Exists` au second import dans un pool commun. La décision B crée une maîtresse indépendante par lignée. [Reproducteur du budget et des archives](../examples/p9-audit.rs) · [Contrat des clés P7](novafs-p7-chiffrement.md)

Deux historiques, l’un avec le snapshot 2 comme parent du clone, l’autre avec le snapshot 3, ont produit **3 000 blocs et six reçus identiques** après remontage. Le retrait du parent maintient cette indistinguabilité. Un témoin avec contenu divergent produit bien des octets différents. Le même phénomène s’observe pour deux snapshots issus de clones frères. Le reçu inscrit **« parent historique inconnu »** pour une dérivation importée de P8. Il engage les vues, la lignée et les arêtes effectivement vérifiées. Toute nouvelle dérivation P9 conserve l’ID exact de son parent, y compris après son retrait. Les identités locales sont qualifiées par l’ID du pool. [Banc des deux histoires](../examples/p9-lignees.rs) · [Preuve des vues et catalogue](../src/secure_pool.rs)

Cargo résout la version de la dépendance locale même en compilation verrouillée. L’arbitrage autorise la mise à jour de la seule entrée `novafs` du verrou du service. Les 19 appels de l’ABI NovaOS et les deux licences gardent leur contrat. [Verrou du service](../../novaOS/user/novafs/Cargo.lock) · [Licence MPL-2.0](../LICENSE) · [Licence MIT](../../novaOS/LICENSE)

## Temps civil, format et accès

Une date occupe douze octets little-endian. Les huit premiers codent les secondes signées depuis le 1er janvier 1970 à 00 h UTC, les quatre suivants la fraction en nanosecondes entre 0 et 999 999 999. Le codage suit les secondes de l’horloge système sans représenter séparément une seconde intercalaire. La paire `(0, 0xffffffff)` signifie *inconnue*. L’époque réelle `(0, 0)` reste distincte. Un suffixe non canonique, une fraction excessive ou une longueur incorrecte rend l’enregistrement invalide. La comparaison des instants utilise un intermédiaire signé de 128 bits. [Codec temporel](../src/time.rs)

| Élément persistant | Encodage et contrôle |
| --- | --- |
| Manifeste de contenu | Le marqueur `NP9TIME1` à l’octet 400 et la politique à l’octet 408 entrent dans l’empreinte du manifeste. La politique vaut 1 pour `noatime`, 2 pour `relatime`, 3 pour `strictatime`. Les anciens champs réservés à zéro identifient le contenu historique. |
| Objet fichier ou répertoire | Après les 28 octets du descripteur et le nom UTF-8, les objets écrits en P9 portent 48 octets dans l’ordre `btime, mtime, ctime, atime`. Les objets historiques gardés par CoW peuvent conserver leur forme courte et leurs quatre dates inconnues. Les anciens lecteurs refusent une forme ou un manifeste incompatible. |
| Pool | `NP9POOLH`, version 9, deux copies identiques. Le bit de présence obligatoire du reçu se trouve à l’octet 12, dans les données associées à l’enveloppe administrative. `NP9DIR01` identifie les banques. Les blocs 2 à 9 gardent les quatre paires d’attestations, 10 à 45 les quatre banques de neuf blocs. Les unités physiques commencent au bloc 46 et occupent deux blocs de 4 Kio. |
| Filiation | `NP9PAR01` dans la moitié supérieure du bloc de preuves. Chaque filiation exacte utilise 32 octets pour l’enfant, le parent et le pool d’origine. Le domaine conserve aussi l’origine authentifiée des imports d’archives. Le décodeur vérifie les bornes, l’ordre canonique, la cohérence avec le catalogue, les deux générations, les empreintes et les MAC applicables. |

La génération transactionnelle garde son rôle d’ordre de publication. Une horloge reculant peut donc produire une date civile antérieure. L’adaptateur Darwin lit `SystemTime`. Un intégrateur injecte une `Clock` et peut déclarer une horloge indisponible ou jugée incorrecte. Le défaut bare-metal produit une date inconnue. La précision effective dépend de la source. [Manifeste](../src/codec.rs) · [Enregistrements](../src/fs/record.rs)

| Opération confirmée | Effet sur les dates |
| --- | --- |
| Création, y compris la racine d’un nouveau volume | Les quatre dates prennent l’instant disponible. Le parent change de `mtime` et de `ctime` lors de l’insertion de l’entrée. |
| Modification effective des octets ou de la longueur | Le fichier change de `mtime` et de `ctime`. Une réécriture identique à taille constante conserve les dates. |
| Renommage, déplacement dans le volume | L’objet conserve `btime` et `mtime`. Son `ctime` et les deux dates de modification des répertoires concernés changent dans la même transaction. |
| Suppression | Mise à jour du parent. Un objet en collecte conserve sa date de création et reçoit le `ctime` de retrait. |
| Réglage explicite des dates | `set_times` accepte des valeurs d’accès et de modification. Un changement effectif actualise `ctime`. La création est immuable. |
| Lecture ou énumération | `read_access`, `list_access`, `SecurePool::read_file` et `list_dir` portent la sémantique applicative. Une lecture à EOF ou de zéro octet garde les dates. L’énumération demandée avec une limite positive compte comme accès au répertoire, même vide. |
| Capture, clone, export, vérification | Le snapshot conserve les dates capturées. Le clone les reçoit puis diverge par CoW. Les vérifications, extractions forensiques et lectures d’une vue immuable conservent son état. |

Les appels portables `times_id`, `set_times_id`, `read_access_id` et `list_access_id` prennent l’identité persistante déjà obtenue par résolution relative. Un renommage conserve cette identité. Une suppression rend sa consultation invalide. Le service appelant reste responsable de la vérification de la capacité. [Interface par objet](../src/fs.rs)

`relatime` est la politique par défaut. Elle actualise un accès inconnu, un accès antérieur ou égal à `mtime` ou `ctime`, ou un accès datant d’au moins 24 heures. `strictatime` enregistre tout nouvel instant observé. La politique `noatime` dispense les lectures de transaction. Un instant déjà identique dispense d’une nouvelle écriture. Si un événement modifie une date alors que l’horloge est indisponible, la valeur devient inconnue. Une date forcée ou un recul de l’horloge ne sert jamais à modifier la filiation. L’accès qui doit publier une date revérifie la tête et la politique sous exclusion. Une erreur d’écriture est transmise au lecteur. Certaines mutations exigent toutes les clés retenues par les deux générations du pool. Un lecteur hors ligne peut utiliser `inspect_file`. [Accès du moteur](../src/fs.rs) · [CoW des fichiers](../src/fs/data.rs) · [Façade du pool](../src/secure_pool.rs)

## Migration et domaines

| Source lue par la version P8 | Migration admise |
| --- | --- |
| Contenu autonome historique | Même géométrie et mêmes identités FS/LV. Copie des blocs protégés, conversion des deux manifestes et des deux ancres dans la candidate. La vérification porte sur les empreintes et les deux générations. Ce profil clair n’a pas d’autorité administrative pour signer un reçu de pool. |
| Catalogue ancien de volumes prêts | Import logique des LV prêts à travers le migrateur historique, dans un espace de travail distinct. Les volumes bruts et les états de préparation sont refusés. Le reçu engage le SHA-256 de la source originale. Le catalogue ancien n’enregistrait pas d’UUID de pool. Ce chemin transporte les LV prêts du catalogue actif. |
| Pool partagé P5 ou P6 | Le témoin doit fournir une empreinte obtenue indépendamment et une clé de signature HMAC. La conversion vérifie les deux catalogues vivants, la classe des pages, les packs et les réservations. Une lignée claire demeure claire. Prévoir au moins 46 blocs physiques plus deux blocs par unité au-delà du préfixe logique de sept blocs. |
| Pool P7 validé par P8 | Les secrets de récupération ou phrases source doivent ouvrir toutes les lignées protégées, y compris celles retenues par le prédécesseur. Les deux copies de chaque attestation protégée doivent être valides avant migration. Le candidat reçoit un pool neuf, une clé administrative neuve et une maîtresse aléatoire neuve par lignée chiffrée. Chaque lignée reçoit ses propres emplacements phrase et secours. |

Le scan parcourt les catalogues du plus ancien au plus récent. Une table temporaire, indexée par adresse de l’unité immuable, mémorise la paire FS/LV, le rôle et les empreintes avant et après conversion. Le migrateur authentifie l’unité sous l’ancienne clé, conserve son encodage LZ4, scelle le contenu sous la nouvelle clé puis le relit. Les pages d’index reçoivent les nouvelles empreintes de leurs enfants. Une arête partagée est réutilisée seulement après contrôle de l’identité source et du rôle. Les quotas, propriétaires, numéros de vues, trous dans les numéros et compteurs d’attribution sont transportés. Les générations internes de destination sont 1 et, si un prédécesseur est protégé, 2. Le reçu conserve le point d’origine. Après migration, la première modification d’un ancien contenu intégré inscrit son manifeste P9. Les racines gelées des snapshots peuvent garder le codage historique. [Parcours des références](../src/shared_graph/migration.rs) · [Conversion des volumes autonomes](../src/engine/migration.rs)

Les données et les métadonnées chiffrées utilisent des clés HKDF-SHA-256 distinctes et XChaCha20-Poly1305. La vérification du tag précède l’interprétation du clair ou la décompression. Le CSPRNG hôte fournit les maîtresses de 256 bits, identifiants, sels et nonces de 192 bits. La migration rejette tout identifiant de domaine déjà présent et tout second tirage d’un même nonce d’unité dans la candidate, avant le scellement. Les essais interrompus sont abandonnés. Une nouvelle tentative exige une image vierge et tire de nouvelles clés. Une réplique de VM doit disposer d’une entropie renouvelée. Le moteur natif sans source cryptographique refuse la création des secrets. [Primitives et emplacements](../src/crypto.rs) · [Vérification des unités](../src/secure_unit.rs) · [Construction XChaCha20-Poly1305](https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction)

Le reçu comptabilise les scellements effectivement effectués par clé d’unité de destination au moment de la migration. Les anciennes clés de chiffrement des unités servent uniquement à leur ouverture AEAD. La clé administrative source authentifie le reçu. Après livraison du pool, le plafond opérationnel P7 reste de 2³² scellements par clé effective, tentatives et copies incluses. Sous des tirages indépendants uniformes, la probabilité d’une collision sur une clé est bornée par q(q−1)/2¹⁹³. La comptabilité des écrivains après migration relève du détenteur de clé. Une copie du fichier ne mesure pas ce cumul. [Budget et frontières de confiance](novafs-p7-chiffrement.md)

### Reçu, vérification et restauration

Les octets 112–113 des en-têtes contiennent la longueur little-endian du reçu `NP9MIG01`. Sa partie fixe occupe 195 octets. Elle engage les pools, les générations, la racine source, la racine précédente ou l’empreinte de l’image claire, la racine de destination, le nombre d’unités, le condensat des correspondances et le profil d’autorité. Suivent 80 octets par lignée, 11 par vue et deux HMAC-SHA-256 de 32 octets chacun. L’autorité source signe le corps. La nouvelle autorité signe le corps **et** la signature source. L’étiquette de vérification publique sans clé reste structurelle. Une phrase, un secours ou une maîtresse n’apparaît jamais dans le reçu. [Codec et vérification](../src/secure_pool/migration.rs)

`migration_receipt` valide la MAC de destination. `verify_migration_source` vérifie l’autorité et le point source. Lorsque les deux images correspondent encore au point de migration, `verify_migration_pair` réauthentifie les graphes protégés, compare les clairs et les enregistrements des vues et recalcule le condensat des arêtes. Pour les catalogues clairs, `verify_clear_source` exige l’empreinte externe et le secret du témoin. Les homologues d’une vue sont identifiés par le couple (pool, numéro de vue). Le reçu reste consultable après de nouvelles transactions. La comparaison du point historique nécessite les images de ce point.

Les archives P9 utilisent `NV9EXPRT`, révision 2, et ajoutent au manifeste authentifié les 24 octets de la référence du parent lorsque cette référence est connue. Un pool P9 peut réunir deux exports autonomes authentifiés sous la même autorité de transport. Il vérifie aussi la provenance par pool, domaine et identités FS/LV, ainsi que les secrets d’enrôlement de destination avant de joindre la seconde vue. Chaque restauration reçoit un nouvel ID local. L’origine exacte exportée garde son ID qualifié par le pool source. Les pages identiques éligibles du même domaine peuvent être partagées après relecture. Le transfert d’une seule vue garde le contrat d’export autonome. [Archive](../src/secure_export.rs) · [Deux restaurations sans source](../tests/p9_archives.rs)

### Publication et récupération

Le migrateur exige une destination vierge, distincte et assez grande. Deux marqueurs `NP9PREP1` précèdent les données de préparation. Avant la première persistance du marqueur, une coupure peut laisser une image vierge ou un secteur illisible. Après cette première barrière, le marqueur identifie la préparation jusqu’à la publication. Le montage normal exige les deux en-têtes définitifs identiques. Les en-têtes définitifs du pool sont conservés en mémoire pendant la vérification de la candidate. Chaque génération est publiée selon le protocole CoW. Les graphes, banques, attestations, reçus et source sont relus avant d’exposer la destination. F1 stabilise les dépendances et F2 acquitte la publication. Un échec de soumission des en-têtes ou de F2 renvoie une issue incertaine. Un nouveau lecteur doit établir l’issue en inspectant le support. Une tête antérieure au point signé de migration est rejetée. Les mécanismes historiques de drainage et de réparation de réplique conservent leurs propres barrières. [Protocole de publication](../src/secure_commit.rs) · [Reprise P7.1](novafs-p7.1-consolidation.md)

L’outil Darwin ouvre un fichier source ordinaire en lecture seule avec refus des liens symboliques. L’outil vérifie la capacité hôte disponible pour l’image entière et propage toute pénurie ultérieure. La candidate est privée dans le répertoire de destination. L’outil compare le SHA-256 de la source relue, celui de la copie réellement migrée et celui de la candidate relue. Il demande `F_FULLFSYNC` sur le fichier, crée le nom final par lien exclusif, puis synchronise le répertoire. Une destination ou un alias déjà présent entraîne un refus. En cas d’arrêt de l’outil avant création du lien, un fichier temporaire privé peut rester à supprimer. Après une erreur de synchronisation du répertoire, vérifier le nom final avant toute reprise. Ces garanties supposent un répertoire sous contrôle de l’opérateur, un support honorant ses barrières et une source hors ligne. Aucun essai ici ne démontre la résistance à une coupure électrique du matériel ou au rollback physique de tout le pool sans témoin externe. [Outil de publication](../lab/migrate-p9.py)

Le scrub examine le profil, les encodages temporels, la chaîne des catalogues et les références. Il garde le contrat P8 de réparation contrôlée. Une attestation manquante peut être réécrite après authentification de sa réplique indépendante et revalidation exclusive. Un enregistrement temporel incohérent, une unité altérée ou une paire d’attestations entièrement perdue ne permettent pas de fabriquer une réparation. [Scrub](novafs-p8-scrub.md) · [Injections P9](../tests/p9_pannes.rs)

## Résultats de qualification

Les essais du 11 novembre 2026 ont utilisé macOS 27.2 (26B123), Rust stable 1.99.1 et les secteurs logiques de 512 et 4 096 octets. Le banc vérifie le type Mach-O de l’exécutable avant d’attribuer ses résultats à ARM64 natif ou à x86_64 sous Rosetta. Les essais de migration utilisent des images jetables.

| Exécution Darwin | Tests standards | Tests instrumentés | Opérations différentielles | Publications / pénuries injectées / remontages vérifiés |
| --- | ---: | ---: | ---: | ---: |
| ARM64 natif | 192 réussis | 193 réussis | 1 000 000 | 345 183 / 54 643 / 1 000 |
| x86_64 sous Rosetta | 192 réussis | 193 réussis | 1 000 000 | 345 183 / 54 643 / 1 000 |

Aucun échec dans ces deux campagnes finales. Les quatre essais marqués optionnels dans le passage standard sont rejoués par leurs bancs dédiés, dont le différentiel et la comparaison LZ4. Les deux compilations complètes du cœur `no_std + alloc` utilisent les backends cryptographiques logiciels configurés dans le workspace NovaFS. Les deux constructions verrouillées de NovaOS v0.13.0 ont lié le service. Les licences MPL-2.0 et MIT ont conservé leurs empreintes Git. [Résultats par architecture](validation/v0.9.13/qualification.json)

Pendant la qualification, un appel Cargo depuis la racine du monorepo a omis la configuration de cible du paquet. La compilation de Poly1305 sur x86_64 nu a alors échoué. Les commandes de reproduction partent de `novaFS/` pour le cœur nu. Un premier passage sous Rosetta a aussi détecté une modification du préfixe de `novafs --help`. Le préfixe historique a été rétabli et les tests ont été rejoués avec leur assertion d’origine.

| Parcours exécuté | Résultat observé pour chaque architecture et chaque secteur |
| --- | --- |
| Contenu autonome | Montage des volumes vide et peuplé, fichiers et répertoires imbriqués, lecture octet pour octet. Les dates historiques sont inconnues. Aux 48 points de coupure, toute image montée contenait le contenu complet et vérifié. |
| Pool chiffré | 26 points de coupure de migration, sept échecs de synchronisation injectés et une écriture déchirée. Les contrôles ont relu la source et les unités de destination. Le reçu comptait exactement les 21 unités chiffrées de ce témoin. |
| Clés et reçus | Le test a interrompu un scellement, puis volontairement reproduit le même nonce lors d’un essai muni d’une nouvelle clé. Le domaine et le tag ont changé. Un nonce répété dans la même candidate a provoqué un refus. Un ciphertext source altéré, un reçu tronqué et des signatures modifiées ont également entraîné un refus. |
| Transactions en P9 | 492 combinaisons de coupure et d’ordre de persistance pour le renommage, le réglage d’une date, le snapshot et le clone. Chaque nouvel accès au support a vérifié les octets, les dates, les références, les identités et l’état admis par l’oracle avant et après F1/F2. |
| Scrub et réparation | Une copie d’attestation valide a permis une réparation d’un bloc. Après perte des deux répliques, aucune écriture. Le vérificateur a aussi rejeté un horodatage rendu non canonique dans des métadonnées dont les empreintes avaient été recalculées. |
| Horloge | Les essais injectent des dates antérieures à 1970, un recul, une horloge indisponible et les extrêmes du type. Les trois politiques d’accès, la modification identique, le déplacement, le démontage et le remontage conservent les valeurs attendues. |

Dans les quatre échanges entre architectures, un exécutable compilé depuis la révision P8 a produit la source. L’autre architecture a relu le pool P9, validé deux lignées puis modifié trois octets à cheval sur une frontière de bloc du clone. Les 59 017 octets du snapshot et ceux du clone modifié ont été comparés à leur oracle. Les deux archives restaurées ensemble ont reçu des identités différentes. La source et son lien physique avaient la même empreinte avant et après les essais. L’éditeur historique a refusé P9. Une destination existante et une destination de capacité insuffisante ont été refusées. [Échange de fichiers et publication](../lab/p9-interop.py) · [Tests du reçu et des graphes](../tests/p9_migration.rs) · [Restauration autonome](../tests/p9_archives.rs)

Les contrôles historiques ont relu des images produites par huit révisions. Chaque architecture a exercé 24 coupures sur fichiers et 10 sur le support QEMU du laboratoire, avec deux remontages par cas. Les huit échanges P6, quatre migrations P6, huit restaurations P7 et 48 rapports de scrub P8 complètent les suites Cargo. La comparaison avec liblz4 1.11.1 a vérifié 256 flux dans chaque sens sur chaque architecture. Dans NovaOS v0.13.0, le scénario historique sur fichiers a confirmé deux transactions, puis relu le volume dans une nouvelle VM en lecture seule, sur les deux architectures et les deux géométries. Ce scénario utilise le profil de contenu historique.

## Performances et mémoire

Les quatre campagnes ont tourné successivement sur des fichiers temporaires Darwin. Chacune a migré 24 fois une même source vers une nouvelle image. La source de 3 000 blocs contenait un fichier compressible de 256 Kio, un snapshot et un clone avec écriture partielle. Les latences utilisent le rang supérieur. Avec 24 observations, le p99 et le maximum coïncident. Le cache hôte a pu servir les lectures. Le test de débit isole 1 Mio d’ouverture, de rescellement et de vérification AEAD en mémoire, sans E/S. Le montage et ce débit ont été mesurés une fois par campagne.

| Architecture et secteur | Migration p50 / p95 / p99 (ms) | Réchiffrement isolé (Mio/s) | Snapshot / restauration du clone p50 (ms) | Montage (ms) | Transactions/s¹ |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 · 512 | 275,3 / 284,5 / 285,6 | 66,0 | 45,8 / 101,6 | 0,35 | 18,1 |
| ARM64 · 4 096 | 270,7 / 284,3 / 285,0 | 67,3 | 44,0 / 100,0 | 0,31 | 18,3 |
| Rosetta x86_64 · 512 | 427,4 / 447,1 / 450,9 | 43,6 | 66,8 / 167,8 | 0,61 | 12,1 |
| Rosetta x86_64 · 4 096 | 427,4 / 435,6 / 443,4 | 44,1 | 66,0 / 168,6 | 0,61 | 11,8 |

¹ Débit calculé sur la durée cumulée des créations, écritures et renommages P9 acquittés. La migration complète inclut l’enrôlement, les barrières et les relectures de vérification.

| Architecture et secteur | Création P8 / P9 (ms) | Écriture P8 / P9 (ms) | Renommage P8 / P9 (ms) | Lecture noatime / strictatime (ms) | Création volume clair / chiffré (ms) | Contrôle du reçu (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ARM64 · 512 | 58,0 / 43,9 | 55,8 / 58,9 | 57,2 / 58,9 | 70,5 / 131,8 | 9,3 / 71,6 | 0,32 |
| ARM64 · 4 096 | 57,8 / 42,9 | 56,2 / 57,1 | 56,9 / 57,9 | 66,4 / 126,8 | 9,2 / 70,6 | 0,32 |
| Rosetta x86_64 · 512 | 92,6 / 66,8 | 89,7 / 89,7 | 91,8 / 88,0 | 106,4 / 194,3 | 10,0 / 120,5 | 0,57 |
| Rosetta x86_64 · 4 096 | 88,1 / 65,0 | 86,8 / 86,1 | 88,2 / 87,4 | 106,2 / 196,5 | 9,6 / 120,4 | 0,55 |

Ces valeurs sont des médianes de 24 opérations par colonne. Les échantillons des deux profils suivent l’ordre du banc et incluent les effets de cache et de vérification propres à chaque état. Le test P8 s’exécute sur une copie de la source. Le coût du volume chiffré inclut Argon2id et les deux emplacements de déverrouillage. Le contrôle du reçu comprend la relecture de la tête et la vérification du HMAC de destination.

| Architecture et secteur | Tas au pic de migration (Mio) | Allocations sur 24 migrations | RSS maximum du processus (Mio) | Tas avant / après le banc (octets) |
| --- | ---: | ---: | ---: | ---: |
| ARM64 · 512 | 64,10 | 645 015 | 69,58 | 1 936 / 1 936 |
| ARM64 · 4 096 | 64,10 | 645 014 | 69,61 | 1 936 / 1 936 |
| Rosetta x86_64 · 512 | 64,10 | 645 019 | 72,35 | 1 840 / 1 840 |
| Rosetta x86_64 · 4 096 | 64,10 | 645 015 | 72,19 | 1 840 / 1 840 |

Argon2id réserve 64 Mio pendant la dérivation. Le compteur du tas inclut les arrondis de malloc Darwin. Le RSS couvre toute la vie du processus, code et traduction Rosetta compris. Le banc enregistre également la pile réservée par Darwin. La profondeur réellement utilisée de cette pile n’a pas été mesurée. Le tas est revenu à son niveau d’entrée dans les quatre processus.

Sur cette charge, le reçu contenait 372 octets pour une lignée et trois vues et figurait dans les deux en-têtes existants. Les 19 unités partagées du snapshot sont restées partagées. La migration a transformé 28 unités immuables distinctes. Chaque horodatage ajoute 12 octets et chaque enregistrement d’objet étendu ajoute 48 octets avant empaquetage. Darwin a comptabilisé 12 288 000 octets pour chacune des deux images. Conserver l’original et la destination exige donc 12 288 000 octets supplémentaires dans ce cas. Les mesures détaillées incluent le nombre d’allocations et les distributions de vérification, d’enrôlement et de restauration. [Résultats mesurés](validation/v0.9.13/qualification.json)

## Reproduction sur macOS Apple Silicon

Exécuter depuis la racine du dépôt. Les deux cibles Darwin, Rosetta et les cibles Rust nues doivent être disponibles. Les campagnes créent leurs images de laboratoire dans des fichiers temporaires.

```sh
set -eu
out=$(mktemp -d)
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p51-validation-suites.py "$isa" > "$out/suites-$isa.json"
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run test --offline --test p9_temps \
        --test p9_migration --test p9_archives --test p9_pannes -- --nocapture --test-threads=1
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run run --offline --example p9-audit
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run run --offline --example p9-lignees
    python3 novaFS/lab/p7-cli.py "$isa"
done
python3 novaFS/lab/p9-interop.py
python3 novaFS/lab/p6-historiques.py --support formats > "$out/formats.json"
python3 novaFS/lab/p6-historiques.py --support pannes > "$out/pannes.json"
python3 novaFS/lab/p6-interop.py
python3 novaFS/lab/p6-lz4-interop.py
python3 novaFS/lab/p7-validation.py interop --sortie "$out/p7"
python3 novaFS/lab/p8-validation.py cli --sortie "$out/scrub.json"
python3 novaFS/lab/p7-no-std.py
(
    cd novaFS
    export RUSTC="$(rustup which --toolchain stable rustc)"
    for cible in x86_64-unknown-none aarch64-unknown-none-softfloat; do
        rustup run stable cargo build --lib --release --all-features --locked --offline --target "$cible"
    done
)
make -C novaOS ARCH=x86_64 kernel
make -C novaOS ARCH=aarch64 kernel
(
    cd novaOS
    for isa in x86_64 aarch64; do
        NOVA_ARCH="$isa" python3 tests/fichiers.py --only normal
    done
)
```

Pour les compilations nues, lancer Cargo depuis `novaFS/` afin que Cargo lise la configuration des backends cryptographiques logiciels des deux cibles. Pour les mesures, lancer les quatre processus successivement, hors des suites précédentes. Les fichiers JSON contiennent les distributions en nanosecondes. Le second fichier de chaque campagne contient les compteurs du tas et les mesures Darwin. [Configuration des cibles](../.cargo/config.toml) · [Banc de mesure](../examples/p9-mesures.rs)

```sh
out=$(mktemp -d)
for isa in aarch64 x86_64; do
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run build --offline --example p9-mesures
    for secteur in 512 4096; do
        /usr/bin/time -l "novaFS/target/$isa-apple-darwin/release/examples/p9-mesures" "$secteur" 24 \
            > "$out/$isa-$secteur.json" 2> "$out/$isa-$secteur.log"
    done
done
```

Pour une image sécurisée, le descripteur 3 reçoit `NP7KEYS1` et les secrets de déverrouillage source dans un pipe anonyme. Le descripteur 4 reçoit `NP9ENRL1`, le secret administratif de destination (32 octets), le nombre de lignées chiffrées (u8), puis pour chacune, l’ancien domaine (16 octets), la longueur de phrase (u16 little-endian), la phrase et le secours (32 octets). Pour les catalogues clairs, FD4 reçoit le secret du témoin et le secret administratif (32 octets chacun). Le contenu autonome utilise les IDs FS/LV externes. La CLI `restore-into` reçoit sur FD4 la clé d’archive, la phrase préfixée par sa longueur, le secours, puis le flux d’archive complet. Le banc d’échange fournit un exemple intégral avec des secrets éphémères. [Banc des pipes et de publication](../lab/p9-interop.py)

```sh
python3 novaFS/lab/migrate-p9.py --help
novaFS/lab/run build --offline --bin novafs
outil=novaFS/target/aarch64-apple-darwin/release/novafs
# Les pipes 3 et 4 proviennent de l'appelant.
python3 novaFS/lab/migrate-p9.py source.raw volume-p9.raw --profil p7 --secteur 4096 --outil "$outil"
```
