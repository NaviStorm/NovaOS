# Audit P9 · Horodatages et migration des images P8

Base examinée le 11 novembre 2026 sur `main` au commit `1eb62f9718a82cf8848386842ed88e7eb1f5e442`. L’audit porte sur NovaFS 0.9.12 (MPL-2.0) et sur les métadonnées attendues par NovaOS 0.13.0 (MIT). Le blocage concerne la continuité des preuves et des clés lors de la migration d’une **lignée chiffrée entière**. Il s’applique déjà à un snapshot et à son unique clone, sur un support sain. [Demande de la CLI native](../../novaOS/docs/cli-v0.14.0.md) · [Reproduction exécutable](../examples/p9-audit.rs)

## Ce que l’image contient

| Couche vérifiée | Conséquence pour P9 |
| --- | --- |
| Enregistrement d’objet | La clé est l’ID persistant. Les 28 octets précédant le nom UTF-8 décrivent type, révision, taille, retrait, parent et longueur du nom. Le décodeur exige la longueur exacte. La révision est un numéro de transaction. L’instant ne peut être reconstitué. [Codec des objets](../src/fs/record.rs) |
| Contenu de volume | Les pages de métadonnées portent la signature `NOVAP0FS`. Le manifeste engage cinq racines et vérifie les octets réservés. Le logiciel P8 ouvre ce contenu autonome ainsi que plusieurs formats de pool. Un simple numéro de version du binaire ne permet pas de choisir le décodeur. [Codec de contenu](../src/codec.rs) · [Format expérimental](novafs-disk-format.md) |
| Transactions P1 à P4.1 | Le moteur épingle tête et prédécesseur, réserve les pages CoW et publie après F1 puis F2. Une erreur de publication peut laisser l’issue incertaine. Le cache et le coordinateur n’établissent aucune date civile. [Durabilité](novafs-p1-durabilite.md) · [Intégrité](novafs-p2-integrite.md) · [Réservations](novafs-p3-espace.md) · [Coordinateur](novafs-p4-coordinateur.md) · [Publication](../src/engine.rs) |
| Vues P5 et compression P6 | Origine, snapshots et clones d’une lignée partagent les identités FS/LV. Les arêtes référencent les unités immuables par adresse et empreinte. Le validateur de transition exige que la racine et la politique d’un snapshot existant restent identiques. Le reçu engage entre autres pool, ID de vue, propriétaire, lignée, type et empreinte de racine. Recréer le catalogue vue par vue change ce graphe. [Transitions](../src/shared_graph.rs) · [Reçus et migration historique](../src/shared.rs) · [LZ4](novafs-p6-compression.md) |
| Pool P7 | Les blocs 0–1 portent l’en-tête, 2–9 les attestations en quatre paires, 10–45 les quatre banques. Dès le bloc 46, l’AEAD engage domaine, position physique, rôle, génération et encodage. Les preuves de vue engagent aussi la génération du catalogue, la racine et les emplacements de déverrouillage. Déplacer, réencoder ou reclasser une unité modifie les preuves à produire. [Disposition et clés](novafs-p7-chiffrement.md) · [Journal](../src/secure_commit.rs) · [Preuves des vues](../src/secure_pool.rs) |
| Scrub P8 et portabilité | Le contrôle structurel peut constater une couverture cryptographique incomplète. Le contrôle complet requiert les clés. Une réparation d’attestation exige sa copie indépendante vérifiée et une nouvelle validation sous exclusion. Le moteur nu utilise déjà `core` et `alloc`. Le temps écoulé du scrub ne fournit aucune origine UTC. [Scrub](novafs-p8-scrub.md) · [Cœur portable](novafs-coeur-portable.md) |

## Trois observations sur les clés et les références

1. **Les deux archives sont valides.** Le banc crée deux domaines indépendants, un fichier compressible de 16 Kio, un snapshot et un clone. Il modifie trois octets du clone à cheval sur deux pages. Il exporte les deux vues puis essaie les deux ordres de restauration dans des images neuves. À chaque fois, la première vue se restaure, passe le contrôle complet et restitue ses octets après remontage. La seconde rencontre `Exists` avant toute écriture ou synchronisation. Chaque restauration provisionne un domaine neuf avec les mêmes IDs FS/LV historiques. Le catalogue refuse deux domaines pour la même lignée. Les octets de la source, les données des deux branches et leurs deux reçus sont revérifiés après remontage. L’export autonome A reste utilisable pour chacune des vues. [Contrat A](novafs-p5-export.md) · [Restauration sécurisée](../src/secure_pool.rs)
2. **Conserver les identifiants ne conserve pas les reçus.** Dans une destination jetable distincte, le banc fait volontairement coïncider l’ID du pool, le numéro de snapshot et tous les champs publics de sa vue avec ceux de la source. La restauration recrée ses unités sous une nouvelle maîtresse. La racine du reçu diffère et `verify_receipt` rejette le reçu source avec `Invalid`. Le contenu relu est exact. Un outil de migration doit distinguer l’attestation de publication P7, le reçu de vue et l’identité logique du contenu. [Calcul du reçu](../src/shared.rs) · [Scellement des unités](../src/secure_unit.rs)
3. **Le budget d’une clé ne se déduit pas du disque.** Sur une copie indépendante, le banc compte uniquement les tirages de 24 octets du vrai CSPRNG. Après une mutation confirmée, il interrompt le renommage suivant au premier appel physique d’écriture, juste après le scellement. Un nonce a été consommé. Après coupure et remontage, tous les octets stables et la génération sont identiques au témoin, l’ancien nom est lisible et le nouveau est absent. Le plafond P7 de 2³² scellements par clé inclut ces essais et toutes les branches. Une copie physique de pool peut conserver les arêtes et les preuves anciennes. La présenter comme une nouvelle branche inscriptible exige toutefois un budget résiduel justifiable pour ces mêmes clés. Le banc ne constate aucune collision de nonce. [Contrat de nonces](novafs-p7-chiffrement.md) · [Ordre du scellement et de l’écriture](../src/secure_pool.rs)

Le migrateur historique `migrate_legacy` parcourt les volumes prêts du catalogue antérieur et crée des origines. `stage_import` part d’une racine vide pour chaque vue. Ces API ne fournissent pas la conversion transactionnelle des deux générations protégées d’un pool chiffré en préservant toutes les arêtes communes. Une validation du seul contenu de chaque fichier laisserait échapper cette perte de partage. [Import des vues](../src/shared.rs) · [Parcours des volumes](../src/pool.rs)

### Décision nécessaire avant d’écrire le format

| Contrat possible | Effet pour l’opérateur et pour le format |
| --- | --- |
| **A · Continuité sous les anciennes clés** | Préserver les anciennes unités, leurs positions logiques dans le support authentifié, leurs racines et les preuves de l’état historique. Définir un profil P9 qui authentifie aussi toute nouvelle métadonnée. Pour ouvrir la destination en écriture, le détenteur doit pouvoir borner le cumul des scellements sur les deux images et les autres branches. En l’absence de cette information, admettre uniquement la lecture vérifiée ou refuser la migration inscriptible. La source reste intacte. |
| **B · Nouveau domaine par lignée** | Concevoir un migrateur de pool entier. Il doit valider les deux générations, transporter chaque unité partagée une fois par identité source, recréer les arêtes et les réservations, provisionner les clés de destination et émettre ses propres attestations. Spécifier un reçu de migration authentifié reliant racines source et destination, son autorité et la remise des moyens de secours. Les preuves historiques restent vérifiables sur la source. Une racine réencodée reçoit une nouvelle preuve. |

Pour prendre en charge aussi une lignée dont le budget historique est inconnu, la recommandation est **B**. Elle change le contrat de continuité cryptographique des sauvegardes et des témoins externes. La décision à prendre est d’autoriser cette nouvelle preuve ou de retenir **A** avec sa restriction d’écriture. Les deux options exigent encore une qualification. Aucun pool n’a été migré vers P9 au cours de cet audit.

## Contrat temporel proposé pour la reprise

Ce contrat permet de préparer les interfaces du moteur sans changer l’ABI NovaOS. Aucun numéro de profil disque n’a été publié.

- **Encodage proposé.** Quatre couples consécutifs `(secondes i64 LE, nanosecondes u32 LE)` dans l’ordre `btime, mtime, ctime, atime`. L’origine est le 1er janvier 1970 à 00 h UTC, avec des jours de 86 400 secondes. La fraction valide va de 0 à 999 999 999. `(0, 0xffffffff)` représente seul l’inconnu. `(0, 0)` est un instant valide. Une seconde négative et une fraction positive représentent les instants antérieurs à l’origine. Les tailles ou fractions non canoniques sont des erreurs d’intégrité. Comparer les couples, et utiliser une différence élargie vérifiée pour les seuils. Les 48 octets par objet sont le coût brut de cette proposition, avant le remplissage des pages, les métadonnées CoW et l’encodage physique.
- **Horloge.** Le cœur reçoit une source portable d’heure civile capable de retourner un instant valide, une valeur indisponible ou une erreur. L’adaptateur Darwin peut lire `SystemTime`. L’adaptateur NovaOS doit déclarer la disponibilité et la résolution réelles de son horloge. L’oracle de test doit accepter les instants négatifs, le recul et les pannes. Une transaction lit l’heure une fois pour tous les objets concernés. Une mesure civile valide peut reculer. La génération continue d’ordonner les publications. Une horloge reconnue invalide ne fournit aucune date. La plausibilité d’une heure civile ne certifie pas son exactitude.
- **Création et mutation.** À la création, initialiser les quatre dates avec l’échantillon disponible, sinon les déclarer inconnues. `btime` est immuable. Une modification effective du contenu change `mtime` et `ctime`. Un changement de nom ou de parent conserve `btime` et `mtime` et change `ctime` de l’objet. Chaque insertion ou retrait change `mtime` et `ctime` des répertoires parents affectés dans la même transaction, même lors d’un déplacement. Un échec avant publication conserve l’ancien état. Une opération effectivement modificatrice dont l’heure manque rend inconnues les dates correspondantes. Les dates non concernées conservent leur valeur.
- **Accès.** Sur une vue inscriptible, `noatime` dispense de la mutation. `strictatime` prend en compte chaque lecture de données non vide et chaque énumération réussie de répertoire. Une lecture de longueur nulle, un constat d’EOF ou un simple `stat` laisse les dates intactes. En `relatime` (défaut), mettre à jour si `atime` est inconnue, si elle précède ou égale une `mtime` ou `ctime` connue, ou si 24 heures civiles se sont écoulées. Une mise à jour isolée d’`atime` ne change pas `ctime`. Si l’horloge est indisponible, signaler ce fait, avec la même représentation inconnue pour l’accès à enregistrer. Éviter une transaction à résultat identique. Prévoir un résultat distinct en cas de refus de mutation ou d’issue F2 incertaine.
- **Lecture seule.** Snapshots, diagnostic, scrub de contrôle, export et montage en lecture seule ne doivent jamais écrire des dates. Le clone reprend celles de la racine capturée puis diverge par CoW. Une source migrée sans heure historique fournit quatre valeurs inconnues jusqu’aux événements qui peuvent légitimement en renseigner une. La date de migration ne décrit aucun de ces événements.

Le banc a aussi verrouillé uniquement le domaine B. A est resté lisible, la lecture de B a été refusée et une mutation de métadonnées de A a rendu `ReadOnly` sans soumission au disque. Le même renommage a réussi après déverrouillage de B. En effet, chaque preuve de vue inclut la génération du catalogue commun. Pour P9, exposer l’état effectivement **en lecture seule** tant que manque une des clés requises pour publier. Un client demandant une lecture devant acquitter un nouvel `atime` doit pouvoir obtenir un refus explicite. Pour un accès inscriptible, réexaminer l’objet et la condition `relatime` sous l’exclusion d’écriture, sans conserver la garde de lecture. [Preuves et disponibilité du pool](../src/secure_pool.rs) · [Coordination](novafs-p4-coordinateur.md)

## Migration, récupération et scrub à qualifier

1. Ouvrir une source stable en lecture seule, reconnaître séparément le contenu autonome, les catalogues et le profil chiffré, vérifier géométrie, générations, graphe et clés requises. Signaler les types bruts, états de préparation ou profils non pris en charge. Contrôler le budget de destination, y compris les réservations, les deux générations et l’espace de travail.
2. Créer exclusivement une image de travail distincte. Refuser une destination existante ou alias de la source. Prévoir un état de préparation qu’un montage ordinaire refuse. Les anciens lecteurs devront refuser le profil P9 avant toute écriture. La version, la politique d’accès et les enregistrements de dates doivent être couverts par la chaîne d’intégrité pertinente, y compris pour les volumes chiffrés.
3. Valider contenu, IDs, répertoires, arêtes partagées, quotas, révisions et valeurs inconnues sous un oracle indépendant. Revalider F1 sur les dépendances et F2 sur la publication. Une erreur F2 exige un remontage et un constat d’issue. Conserver LZ4 avant AEAD et vérifier le tag avant toute décompression. Le scrub doit vérifier les encodages de date même si leurs checksums sont cohérents. Un ordre civil inhabituel peut provenir du recul de l’horloge.
4. Rouvrir et vérifier entièrement la candidate avant publication sans écrasement de son nom final. Synchroniser l’image et, selon le contrat durable du backend hôte, son entrée de répertoire. Au redémarrage, distinguer candidate incomplète, résultat publié et publication de nom incertaine. Une réparation doit conserver les conditions P8 d’exclusion, de relecture et de preuve indépendante. Aucune réparation ne doit fabriquer les dates perdues.
5. Qualifier sur les deux secteurs et architectures les images vides, imbriquées, volumineuses, partagées, compressées, chiffrées, endommagées et interrompues. Inclure origine retirée, deux clones divergents, prédécesseur seul à référencer une unité et clé manquante. Injecter des coupures autour de F1/F2, pendant la migration, un accès horodaté, un renommage et un snapshot. Comparer l’image source octet pour octet et chaque état remonté à l’oracle.

## Résultats exécutés sur la base P8

Mac Apple Silicon, macOS 27.2, Rust stable 1.99.1. Les binaires Darwin du reproducteur ont été identifiés par leurs en-têtes Mach-O. Les tests portent sur des images jetables et le simulateur de stockage.

| Vérification | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Suite standard / avec sondes | 179 / 180 réussis | 179 / 180 réussis |
| Différentiel, 100 graines | 1 000 000 opérations, 25,150 s | 1 000 000 opérations, 40,678 s |
| Publications observées / pénuries injectées / remontages audités | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |
| Coupures P5 / erreurs d’E/S / écritures déchirées, **par secteur** | 342 / 76 / 6 | 342 / 76 / 6 |
| Coupures P7 / erreurs d’écriture ou de flush, par secteur | 291 / 35 | 291 / 35 |
| Coupures pendant la reprise P7.1, par secteur | 200 | 200 |
| Coupures scrub P8 / erreurs d’E/S, par secteur | 27 / 4 | 27 / 4 |
| Scénarios historiques sur fichier / avec backend QEMU, deux remontages par scénario | 24 / 10 | 24 / 10 |
| Rapports CLI scrub P8, secteurs 512 et 4 096 | 12 + 12 | 12 + 12 |
| Nouveau reproducteur, secteurs 512 et 4 096 | 2 géométries conformes | 2 géométries conformes |

Les quatre tests marqués `ignored` dans le lancement général ont été exécutés séparément (différentiel, volumes, extents et interopérabilité avec liblz4). Les tests ciblés P6/P7/P8 ont donné 27 succès supplémentaires par architecture, en recouvrement avec les suites générales. Le banc historique de non-écriture avant réparation a produit 10 observations conformes par architecture et par répétition, sur deux répétitions. Les lectures croisées F01, les fixtures de huit révisions, les huit échanges P6 et les quatre migrations historiques P6 ont également réussi. Quatre échanges P7 ont donné huit restaurations dans les deux sens. Le cœur 0.9.12 avec toutes ses options a compilé pour `x86_64-unknown-none` et `aarch64-unknown-none-softfloat`. Ces exécutions ne couvrent aucune opération P9. [Banc de réparation](validation/p7.1-audit.md)

### Mesures de référence

Le banc P8 a exécuté 1 600 opérations, 100 par combinaison d’architecture, de géométrie et de type de contrôle. Ci-dessous le scrub complet, sur une image de 16 Mio par géométrie. Le RSS est fourni par Darwin pour le processus hôte. Les mesures ont été prises avec le cache macOS actif, sur une machine partagée avec d’autres essais.

| Architecture | Secteur (octets) | p50 (ms) | p99 (ms) | Pic de tas (octets) | RSS maximal (octets) |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 | 512 | 25,901 | 37,812 | 83 760 | 3 407 872 |
| ARM64 | 4 096 | 23,646 | 24,889 | 83 760 | 3 358 720 |
| x86_64 | 512 | 37,625 | 39,672 | 83 328 | 5 222 400 |
| x86_64 | 4 096 | 37,586 | 39,262 | 83 328 | 5 697 536 |

Sur ces quatre lignes, le tas vivant en fin de mesure rejoint son niveau initial (2 160 octets sur ARM64, 2 064 sur x86_64). La création, la modification, le renommage, les accès horodatés, le débit transactionnel, le montage, la migration, les snapshots, le pic mémoire et l’occupation disque **en P9** restent à mesurer. Aucun essai de durabilité physique n’a été réalisé.

## Reproduction

Depuis la racine du monorepo sur macOS Apple Silicon avec Rosetta et les quatre cibles Rust installées. Le premier bloc reproduit les observations qui motivent l’arbitrage.

```sh
set -eu
for isa in aarch64 x86_64; do
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run run --offline --example p9-audit
done
```

```sh
set -eu
out=$(mktemp -d)
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p51-validation-suites.py "$isa" > "$out/suites-$isa.json"
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run test --offline \
        --test p6_compression --test p6_restitution --test p7_secure --test p8_scrub \
        -- --nocapture --test-threads=1
    python3 novaFS/lab/p7-cli.py "$isa"
done
python3 novaFS/lab/test-interop.py
python3 novaFS/lab/p6-historiques.py --support formats > "$out/formats.json"
python3 novaFS/lab/p6-historiques.py --support pannes > "$out/pannes.json"
python3 novaFS/lab/p6-interop.py > "$out/interop-p6.json"
python3 novaFS/lab/p6-lz4-interop.py > "$out/lz4.json"
python3 novaFS/lab/p7-validation.py interop --sortie "$out/interop-p7"
python3 novaFS/lab/p7-no-std.py
python3 novaFS/lab/p8-validation.py cli --sortie "$out/scrub.json"
python3 novaFS/lab/p8-validation.py mesures --repetitions 100 --sortie "$out/mesures.json"
(
    cd novaFS
    export RUSTC="$(rustup which --toolchain stable rustc)"
    for cible in x86_64-unknown-none aarch64-unknown-none-softfloat; do
        rustup run stable cargo build --lib --release --all-features --locked --offline --target "$cible"
    done
)
```
