# Export autonome NovaFS P5, version 1

Le pool et le reçu d’une vue permettent de contrôler sa génération dans le pool. L’export matérialise une vue hors ligne pour pouvoir la restaurer après perte de tout le pool. Il accepte les snapshots et les clones. Une origine inscriptible peut d’abord produire un snapshot. Le lecteur du flux n’accède à aucun autre support source.

## Encodage

Le flux commence par 160 octets, suivis de `n` enregistrements de 4 104 octets et d’une clôture de 40 octets. Les entiers sont non signés, encodés en little-endian sans alignement dépendant du CPU. Les identités et empreintes sont des suites d’octets.

| Octets, fin exclue | Champ |
| --- | --- |
| 0–8 | ASCII `NV5EXPRT` |
| 8–10, 10–12 | Version 1, longueur d’en-tête 160 |
| 12–16 | Masque de fonctionnalités requises, zéro |
| 16–20, 20–24 | Bloc de 4 096 octets, secteur source de 512 ou 4 096 octets |
| 24–32 | Capacité logique de la vue en blocs, égale à 256, 512, 768 ou 1 024 |
| 32–48, 48–56 | Identité du pool source et numéro persistant de vue dans ce pool |
| 56–64 | Type de vue, 1 pour un snapshot, 2 pour un clone |
| 64–80 | Domaine propriétaire source |
| 80–96, 96–112 | Identités FS et LV de la lignée de contenu |
| 112–144 | Empreinte du reçu de branche dans le pool source |
| 144–152, 152–160 | Nombre `n` de pages transportées, puis huit octets réservés nuls |

Chaque enregistrement contient un numéro de bloc logique sur 8 octets et sa page complète sur 4 096 octets. Les numéros croissent strictement et restent dans la géométrie déclarée. Le lecteur refuse les versions, fonctionnalités et champs réservés inconnus, les numéros dupliqués et les enregistrements en trop.

Le parcours d’intégrité P2 des deux générations de contenu produit le bitmap des pages à transporter. Il inclut les ancres, les métadonnées, les extents atteignables et les obligations de collecte du FS. Il faut conserver le prédécesseur pour vérifier la filiation du contenu. Une sauvegarde peut ainsi contenir des octets de la génération précédente ou d’un fichier en cours de collecte. Elle demande les mêmes protections d’accès qu’une sauvegarde de volume. Les pages libres non lues par la vérification et les données des vues sœurs restent hors du flux. Le bitmap est borné à 128 octets pour 1 024 blocs logiques.

La clôture est la chaîne ASCII `NV5XEND!` suivie du SHA-256 de la concaténation de `NovaFS/export/complet/v1`, d’un octet nul, de l’en-tête, des enregistrements complets et de `NV5XEND!`. La fin du flux doit suivre immédiatement le digest. Un échec ou une interruption du producteur peut laisser un préfixe de l’archive sur sa sortie. Tant que la clôture ou un octet annoncé manque, le lecteur refuse la restauration.

## Vérification et publication

Le lecteur vérifie les limites avant de matérialiser les pages dans un fichier temporaire anonyme, creux et limité à quatre Mio. Les numéros omis y valent zéro. Il recalcule le SHA-256, monte les deux générations en lecture seule et compare le bitmap réellement parcouru avec celui reçu. Une page superflue ou une page indispensable absente invalide le flux, même avec une somme externe correctement recalculée. Le contrôle écrit uniquement dans son espace de travail temporaire. Il préserve le fichier d’archive et les supports de destination.

La restauration valide entièrement le flux avant toute mutation du pool destinataire. Elle importe son contenu dans une nouvelle vue transactionnelle, conserve le type et les identités de lignée, et l’impute au domaine explicitement choisi à destination. L’identité publique est le couple (pool, numéro de vue). Un numéro local peut coïncider entre deux pools différents. La restauration émet le nouveau reçu, utilisable avec le nouveau pool. La disparition du pool source devient alors sans effet.

Une sauvegarde en flux transmet le reçu source à titre de provenance. Le checksum détecte les corruptions accidentelles. Un producteur qui refabrique délibérément toute l’archive peut aussi recalculer ses sommes non secrètes. Pour attester la provenance après transport, conserver une empreinte de l’archive complète dans une source de confiance indépendante. La réussite de `Write::flush` concerne le transport. Pour un fichier d’archive, l’appelant gère la synchronisation durable du fichier et de son nom sur le support de sauvegarde.

## Commandes

Depuis la racine du dépôt sur le Mac, fournir des chemins de **fichiers ordinaires distincts**, les géométries réelles et le reçu conservé. Pour les commandes de transformation, stdin contient l’image d’origine et stdout reçoit une image neuve. Le shell ouvre la sortie avant de lancer le programme.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
outil="${CARGO_TARGET_DIR:-novaFS/target}/aarch64-apple-darwin/release/novafs"
"$outil" list-views 2048 512 < pool.raw
"$outil" check 2048 512 "$pool_source" "$vue" "$empreinte" < pool.raw
"$outil" export 2048 512 "$vue" < pool.raw > sauvegarde.nvx
"$outil" check --export < sauvegarde.nvx
"$outil" restore 2048 4096 "$pool_destination" "$domaine_destination" < sauvegarde.nvx > nouveau.raw
"$outil" check-pool 2048 4096 < nouveau.raw
```

La CLI `restore` crée un **nouveau pool** sur sa sortie. Son reçu de vue est émis sur stderr. L’API `restore_view` importe également dans un pool déjà ouvert après validation complète. Dans les deux cas le destinataire doit disposer du crédit physique requis. Le recours à un fichier temporaire pour intégrer un flux non repositionnable borne le tas. Les allocations et I/O du support temporaire comptent dans le temps de restauration.

## Reproduction

```sh
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    NOVAFS_TARGET="$cible" novaFS/lab/run test --offline --test p5_export --test p5_cli_migration --test tool
done
python3 novaFS/lab/p5-export.py
```

Le banc croisé vérifie les en-têtes Mach-O des deux binaires Darwin. Pour chaque producteur et chaque géométrie, il écrit une modification à cheval sur deux pages du clone, supprime l’origine et le snapshot, exporte, supprime le fichier du pool source, puis restaure avec l’autre architecture dans chacune des deux géométries. Il compare tous les octets du fichier, vérifie les reçus et refuse une archive tronquée. [Code de l’export](../src/shared_export.rs) · [Scénarios de corruption](../tests/p5_export.rs) · [Échange entre exécutables](../lab/p5-export.py)

## Résultats macOS sur la v0.9.6

Les deux exécutables ont produit quatre archives de 61 760 octets, identiques octet par octet à secteur source égal. Les huit restaurations croisées ont livré les 4 133 octets attendus après disparition du fichier de pool. Dans les quatre configurations, le reçu correct a été accepté, un faux reçu refusé et un export tronqué d’un octet a empêché l’émission du nouveau pool. [Empreintes par architecture et géométrie](validation/v0.9.6/exports-croises.json)

Les trois tests Rust d’export ont réussi sur chaque architecture. Ils couvrent les deux types de vue, onze positions de troncature, l’interruption de l’écrivain après un préfixe de page, les octets surnuméraires, les champs et données altérés, les géométries de 255, 257 et 1 025 blocs avec somme recalculée et l’ajout d’une page superflue avec une clôture recalculée. Les rejets de flux ont laissé le reçu indépendant du pool destinataire valide et sa trace d’écritures et de synchronisations vide. Une erreur du premier flush de destination a conservé le seul objet précédemment confirmé après réouverture. Les deux tests CLI de migration ont aussi réussi par architecture, avec contrôle des octets de la source, de la sortie et du fichier restitué.
