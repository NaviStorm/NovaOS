# NovaFS P3 v0.9.3 — espace, quotas et réservations

## Audit préalable du commit 6f15350

| Chemin examiné | État en P2 et risque | Décision P3 |
| --- | --- | --- |
| Allocation du LV | Des intervalles bornés contiennent le libre et les retraits différés par génération. Un bloc encore accessible à travers l’ancienne ancre doit rester protégé. Le compteur `free` inclut aussi la marge d’urgence. | Publier une partition en blocs de 4 Kio et distinguer ce qui peut être promis à une nouvelle transaction ordinaire. |
| Préparation CoW | Une mutation travaille sur une copie de l’allocateur. Les données, les chemins de B+Trees, le nouvel arbre d’espace et le manifeste sont préparés avant F1. Les deux ancres ont déjà leur emplacement. Le coût varie avec l’alignement, la fragmentation et les scissions. | Réserver le plan complet avant la première E/S d’écriture. Rendre observables le coût et l’annulation et préserver la séquence F1, ancre, F2. |
| Disque plein | Le seuil ordinaire est fixé à 128 blocs. Les retraits et la collecte peuvent le franchir. Une pénurie de blocs, de mémoire ou de grains n’a pas la même unité ni la même cause. | Séparer les erreurs, borner la configuration de maintenance et tester la libération au voisinage du seuil. |
| Espace du pool | Le catalogue P0 affecte des grains entiers de 256 blocs. Il admet 16 LV et 256 segments et impose 4 grains par domaine. La fixture réserve les grains des deux générations, mais n’expose aucun ticket d’admission concurrent. L’import et l’analyse de cohérence existent déjà. | Réutiliser le gestionnaire et les segments effectifs. Interdire deux promesses portant sur un même grain, y compris lorsqu’une ancienne racine en garde la propriété. Compter séparément la tête du PD, la queue incomplète et les tickets. |
| Consommation logique | La capacité d’un LV est enregistrée dans le catalogue. Les trous des fichiers ne consomment pas de blocs de données ; la somme de leurs tailles peut donc dépasser le volume physique. | Mesurer les octets logiques des fichiers vivants, indépendamment des blocs effectivement alloués et des quotas physiques de domaine. |

## Décision de compatibilité avant implémentation

Le catalogue P0 sérialise la taille épaisse de chaque LV et la totalité de ses segments. Il ne possède aucun champ pour une promesse fine, une limite administrative indépendante de la géométrie ou une politique de maintenance. Les décodeurs vérifient les octets réservés. Les réinterpréter rendrait certaines images illisibles en P0–P2.

P3 conserve les octets du catalogue et du FS. Pour un LV géré, la taille épaisse persistante sert de garantie physique. Les limites supplémentaires de session doivent être fournies par le détenteur de la politique lors du montage. Une politique indépendante à conserver dans le catalogue nécessiterait un nouveau profil identifié avec migration explicite hors ligne vers une nouvelle image, vérification des deux catalogues et des LV, puis bascule uniquement après validation ; P3 ne migre aucune image. Les blocs partagés entre LV restent interdits par le vérificateur P2. Le partage entre générations d’un même LV ne crée pas une seconde allocation physique.

## Trois unités de comptabilisation

- **PV et pool.** Le laboratoire utilise un seul PD, des grains de 256 blocs et des secteurs de 512 ou 4 096 octets. En blocs de 4 Kio, la capacité est la somme du grain de tête, de la queue non alignée à un grain, des grains attribués dans la tête, des grains différés propres à l’ancien catalogue, des grains promis aux tickets et du libre encore attribuable. Un grain réservé au pool est exclu des attributions LV, même si une partie de ses octets ne contient pas de métadonnées.
- **LV et FS.** Une fois un grain attribué, son espace appartient au LV, y compris les blocs encore libres *dans ce LV*. Le compte physique du pool reste identique quand le FS écrit dans ce grain. Dans le FS, `alloués = vivants + récupérables` et `capacité = alloués + réservés_transaction + réservés_maintenance + disponibles`. Les blocs vivants comprennent les emplacements fixes des deux ancres. Un bloc retiré reste récupérable tant que la fenêtre de générations le protège. Il redevient disponible après rotation et sous réserve de la marge de maintenance.
- **Fichiers.** La consommation logique est la somme en `u128` des tailles des fichiers vivants, trous compris. Les répertoires ne sont pas facturés en octets logiques. La suppression d’un nom libère sa charge logique au commit. La collecte des extents peut intervenir plus tard. Le montage recalcule la somme lors du parcours d’intégrité P2. Les mutations suivantes mettent à jour le compteur dans leur plan privé. `Engine::verify` le rapproche du support.

Une garantie épaisse de 1 Mio réserve 256 blocs dans le pool et donne au FS géré un plafond logique de 1 048 576 octets. Le premier octet écrit à l’offset 8 192 d’un fichier vide consomme 8 193 octets logiques et touche une seule page de données. Il faut aussi réserver les métadonnées de cette transaction. Une égalité entre la garantie physique et le quota logique ne promet donc pas qu’un fichier dense de cette taille pourra être écrit.

## Admission, annulation et publication

Le moteur travaille sur un allocateur privé dérivé de la tête validée. Il résout le chemin, contrôle la charge logique, choisit les blocs et construit les pages de données, les chemins d’arbres, l’index d’espace et le manifeste. Une adresse prise puis rendue pendant cette préparation ne reste pas comptée dans le plan final. Le maximum pour le plan ainsi construit est **exact** avant toute écriture du périphérique. Il comprend jusqu’à 64 blocs CoW, dont jusqu’à 17 pages de données pour une écriture non alignée de 64 Kio. La publication remplacera également une ancre à son adresse fixe, déjà attribuée.

```rust
let plan = fs.prepare_write("/journal", 4095, &donnees)?;
let cout = plan.cost();
assert_eq!(
    plan.accounting().transaction_reserved,
    cout.data_blocks + cout.metadata_blocks,
);
plan.commit()?; // ou plan.cancel() avant toute E/S de publication
```

Un prêt mutable exclusif tient le même écrivain et la même génération du début de la préparation jusqu’à `commit`, `cancel` ou la destruction du plan. Un abandon ne fait aucune écriture et aucun flush. Les compteurs de ce plan montrent la consommation logique **publiée** et les blocs physiques retenus pour le candidat. Les autres mutations du FS empruntent le même chemin d’admission. Un commit sans changement n’écrit aucune page. Un commit modifiant l’état écrit les dépendances, exécute F1, écrit l’ancre, exécute F2, puis installe les nouveaux compteurs.

Dans le pool, plusieurs threads peuvent réserver des grains distincts auprès d’une même instance. Le ticket opaque retient des segments précis dans un registre de 16 entrées au plus. Le gestionnaire reçoit le domaine de son appelant administratif. Son contrôle des identités, des quotas de domaine et des grains libres inclut les autres tickets. Le commit d’un ticket est sérialisé, relit les catalogues avant tout effacement et libère le verrou du registre avant les E/S. Un ticket annulé ou perdu au redémarrage ne laisse aucune dette sur disque. Un ticket créé avant un nouvel import ne peut pas y être présenté. [Gestion du pool et résultats des admissions concurrentes](novafs-p3-pool.md)

L’exclusion physique de l’écrivain reste un contrat du périphérique. Deux montages RW indépendants sur le même fichier ou le même PD ne constituent pas une utilisation supportée. Le registre protège les appels concurrents à une instance. Il ne fournit pas de verrou distribué ou interprocessus. Le libre non attribué du pool est accessible aux demandes de **nouveaux** LV. Un LV existant conserve sa géométrie pendant sa vie. Aucun grain n’est partagé entre deux LV et aucun espace physique n’est promis deux fois.

## Politique et erreurs

La réserve de maintenance vaut 128 blocs par défaut. Le propriétaire peut fixer une cible comprise entre 64 et `min(4096, capacité_en_blocs − 5)`. La marge effectivement disponible est limitée par le nombre de blocs éligibles. Les écritures ordinaires, créations et renommages doivent laisser la cible intacte. Seuls le retrait par `unlink` et la collecte par `collect` ont accès à la marge. À la libération de nouveaux blocs, la réserve se reconstitue avant le libre ordinaire. Si la préparation d’un retrait ne tient pas, son refus préserve le dernier état publié.

La politique `VolumePolicy` peut définir un quota logique de session plus strict que le plafond fourni par le périphérique. `Engine::mount_with_policy` l’applique à l’ouverture, `set_volume_policy` sur une instance vivante. Un LV géré garde en outre le plafond dérivé des grains inscrits dans le catalogue, même si le quota de session est absent ou plus grand. Une image FS autonome ouverte sans catalogue conserve le comportement historique sans quota administratif implicite. Si un ancien volume creux dépasse déjà la limite, il peut être relu, modifié à taille constante ou libéré ; toute croissance supplémentaire échoue.

| Erreur de l’API Rust | Conséquence |
| --- | --- |
| `NoSpace` | Le plan ordinaire ne tient pas dans le LV en préservant la réserve. Aucun commit. |
| `MaintenanceExhausted` | Le retrait ou la collecte manque de blocs de travail même avec la marge. Aucun commit. |
| `PoolFull` | Le nombre de grains non attribués, non différés et non promis est insuffisant. |
| `QuotaExceeded` | Croissance logique du FS ou somme des garanties engagées du domaine au-delà du plafond. |
| `Limit`, `NoMemory`, `Busy` | Limite du profil, manque de mémoire privée ou exclusion. Une admission refusée ne publie rien. |
| `Io` avant l’ancre, `Publication` à l’ancre ou à F2 | Des dépendances peuvent avoir été écrites. Après le début de publication, l’ancienne ou la nouvelle vue complète peut subsister ; une ancre déchirée peut aussi rendre la paire inadmissible. L’écrivain cesse de servir de nouveaux budgets. Remonter et vérifier. Au besoin, examiner le slot survivant en secours RO. |

Les méthodes historiques `generation`, `volumes` et `Engine::space_stats` peuvent encore décrire la dernière tête acquittée en RAM. Les nouvelles API `Engine::accounting` et `Pool::space_stats` refusent de fournir un budget si la santé de l’instance est indéterminée. Le statut d’une ancienne transaction ne prouve pas la fraîcheur d’une image. [Durabilité P1](novafs-p1-durabilite.md) · [Contrôles d’intégrité P2](novafs-p2-integrite.md)

## Utilisation hors ligne

Construire le binaire séparément. Les commandes suivantes prennent un fichier ordinaire immuable sur stdin. Elles reprennent les contrôles P2 de taille, de type de descripteur et de séparation des sorties. Le rapport JSON utilise le schéma `novafs-space-1`. Le nombre logique en octets est une chaîne décimale pour préserver toute la précision du compteur. Les autres champs portent leur unité dans leur nom.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
outil="${CARGO_TARGET_DIR:-novaFS/target}/aarch64-apple-darwin/release/novafs"
"$outil" space 320 512 "$FS_ID" "$LV_ID" < volume.raw
"$outil" space-pool 4352 4096 < pool.raw
```

`space` donne la partition du FS autonome avec sa politique d’ouverture par défaut. `space-pool` contrôle les deux catalogues et fournit, pour chaque LV prêt, son budget physique garanti et la comptabilité du FS monté en lecture seule à travers ses vrais segments. Les LV RAW et en préparation conservent leur garantie et portent `fs: null`. L’outil ne publie pas de bilan complet si un LV prêt est corrompu. Aucun ticket en vol n’est visible sur une image hors ligne. Pour observer ces réservations, interroger l’instance propriétaire.

Dans `novafs script`, la ligne `policy 80 8193` fixe une réserve de 80 blocs et un quota de 8 193 octets. `policy 128 none` retire le plafond de cette session et rétablit la marge par défaut. `space` écrit le bilan JSON sur stderr pour laisser l’image sur stdout. Un script refusé n’exporte pas son image. La politique supplémentaire ne figure pas dans le flux exporté.

## Validation et reproduction

Sur ARM64 natif et x86_64 sous Rosetta, la livraison a passé **78 tests ordinaires et un million d’opérations différentielles par architecture**. Les 18 scénarios de P3 comprennent des refus sur des volumes effectivement saturés, des courses d’admission, des fautes de publication et la préservation des deux générations sous corruption et pénurie combinées. Les anciens cas de pénurie mémoire, les injections P1 et les 14 familles de corruption P2 sont conservés. Chaque campagne P1 sur fichiers et disque virtuel a aussi provoqué 24 arrêts d’écrivain macOS et 10 arrêts de VM QEMU x86_64. Ces VM rejouent les E/S P1 ; elles ne chargent pas NovaOS. [Matrice, oracles et résultats détaillés](novafs-p3-saturation.md) · [Bilan des exécutions](validation/v0.9.3/campagne.json)

Les programmes actuels ont lu les images FS des trois versions de référence. Leurs lecteurs ont également vérifié les images produites par P3. À part, les producteurs P2 et P3 des deux architectures ont généré les mêmes 17 825 792 octets pour le pool de 16 LV. Les deux profils de secteurs ont été employés lors des lectures croisées. [Rapport du pool et empreinte de l’image](novafs-p3-pool.md) · [Mesures mémoire et performances](novafs-p3-mesures.md)

Depuis la racine du dépôt, sur un Mac avec Rust stable, les deux cibles Darwin, Rosetta et les dépendances Cargo disponibles hors ligne. Le banc P1 requiert QEMU et EDK II. Le banc de mémoire P2 utilise des fichiers creux et des copies APFS. Les mesures RSS requièrent l’accès aux compteurs Darwin de `/usr/bin/time -l`.

```sh
set -e
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
    for secteur in 512 4096; do
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reprise
    done
    python3 novaFS/lab/measure-space.py --arch "$arch"
done
unset NOVAFS_TARGET
python3 novaFS/lab/measure-check.py
(
    refs=$(mktemp -d)
    trap 'rm -rf "$refs"' EXIT
    for commit in d0ba7cb 38c8699 6f15350; do
        mkdir "$refs/$commit"
        git archive "$commit" novaFS | tar -x -C "$refs/$commit"
        CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py \
            --reference "$refs/$commit/novaFS/lab/run"
    done
    python3 novaFS/lab/test-pool-interop.py --reference "$refs/6f15350/novaFS/lab/run"
)
```
