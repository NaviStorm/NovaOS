# P3 — budgets physiques du pool et garanties des volumes

## Unités, attributions et quotas

Le prototype administre un périphérique bloc, un catalogue mono-PD et au plus seize volumes logiques. Le grain mesure 256 blocs de 4 Kio, soit 1 Mio. Les identités du pool et du périphérique, les deux ancres et les trois bandes de catalogue restent celles du profil P0. La bibliothèque hôte fournit maintenant les opérations d’administration qui servaient à la fixture P01/P02. [Gestionnaire](../src/pool/manager.rs) · [Décodage du catalogue](../src/pool/catalog.rs)

`Pool::space_stats()` compte les **adresses physiques du périphérique virtuel**. La somme suivante est une partition de `total_blocks`. Les unités sont des blocs de 4 Kio.

| Champ | Règle |
| --- | --- |
| `metadata_blocks` | Le premier grain entier appartient au gestionnaire. Deux ancres et trois bandes de 16 blocs y sont placées. Les autres adresses de ce grain restent exclues des LV. |
| `tail_blocks` | La fin du PD dont la taille est inférieure à un grain ne peut être attribuée. |
| `assigned_blocks` | Ensemble des grains de la tête courante, RAW, en préparation ou prêts. Chaque grain a exactement un propriétaire. |
| `deferred_blocks` | Grains présents uniquement dans le catalogue précédent. Leur attribution à un nouveau propriétaire attend la rotation des racines. |
| `reserved_blocks` | Grains libres du point de vue des deux catalogues et attribués à des tickets actifs de ce montage. |
| `available_blocks` | Grains entiers libres dans les deux catalogues après déduction des tickets actifs. |

`shared_blocks` vaut zéro. Les recouvrements entre deux LV sont rejetés par le contrôle P2. Un grain visible dans les deux catalogues pour le même LV et à la même adresse logique figure **une fois** dans `assigned_blocks`. Une somme des consommations physiques du moteur NovaFS et du pool compterait deux fois certains blocs. La forme creuse éventuelle du fichier macOS et l’espace APFS réellement alloué appartiennent à un autre domaine de mesure.

Chaque LV a une capacité épaisse persistante et une table de traduction logique → physique totale. Cette capacité est sa garantie minimale, y compris quand le pool n’a plus de grain libre. La création est refusée si toute la garantie ne tient pas. Un domaine ne peut imputer plus de quatre grains pour l’ensemble de ses LV et tickets. Un autre domaine dispose de son propre quota mais concourt pour les derniers grains physiquement disponibles. Le partage s’exerce lors du provisionnement de nouveaux LV. La taille d’un LV déjà publié reste fixe.

Sur une vue FS de pool, le plafond logique de base en octets vaut `guaranteed_blocks × 4096`. Le moteur compare cette limite à la somme des tailles des fichiers vivants, trous compris. L’administrateur peut réduire le plafond de session lors du montage. Une image P0 dont l’usage excédait déjà le plafond peut être lue et son dépassement résorbé. Un export RAW impute toute sa garantie épaisse du côté du pool ; le gestionnaire ne peut déduire du contenu brut un nombre d’octets de fichiers. Le plafond de session inférieur ne figure pas dans le catalogue P0 et doit être fourni de nouveau. [Politique du moteur](novafs-p3-espace.md)

## Réserver puis publier

L’administrateur ouvre une instance avec `Pool::import`. L’import valide séparément les deux catalogues et leur relation et exécute le flush de stabilisation de P1. L’exclusivité des écritures sur le périphérique pendant la vie de cette instance est une condition externe.

1. `reserve_volume(owner, id, fs, grains)` valide les identités, le crédit du domaine, le nombre de places dans le catalogue ainsi que les candidats physiques. Le premier grain et les grains encore protégés par l’ancienne tête sont exclus. Les identités de LV et de FS déjà réservées ou encore protégées ne peuvent être réattribuées.
2. Un mutex protège uniquement la sélection et l’inscription des adresses dans un registre de seize tickets au maximum. Le registre ne dépend pas de la capacité du PD. Un ticket est opaque, transférable entre threads, consommable une seule fois et attaché à l’instance d’origine. Aucun accès au disque n’est nécessaire pendant l’admission.
3. La destruction d’un ticket rend son slot et ses grains immédiatement disponibles. Une coupure perd tous les tickets non publiés ; un ancien ticket présenté à une nouvelle instance est rejeté. Un détenteur qui conserve ou oublie volontairement son ticket peut immobiliser le crédit jusqu’à sa restitution ou à la fin de l’instance. Il n’existe pas de bail temporisé.
4. `commit_volume(ticket)` prend le droit exclusif de publication. Il prépare le catalogue complet en mémoire, valide ses invariants et relit les deux racines du support avant tout effacement pour confronter la tête au reçu courant. Les acquisitions de tickets sont interdites par l’emprunt exclusif du pool pendant ce transfert. Aucun mutex de registre n’est détenu pendant les E/S.
5. La publication remet à zéro les grains puis écrit la bande de travail, attend F1, écrit l’ancre et attend F2. Les échecs de préparation rendent le ticket. Une erreur d’E/S antérieure à la publication suspend l’instance. Un échec d’écriture de l’ancre ou de F2 est indéterminé. Le nouvel import valide la paire effectivement relue. Les compteurs ne sont pas rendus comme actuels par une instance suspendue.
6. La création d’un LV FS donne un état « préparation ». `mark_ready` vérifie d’abord l’instance NovaFS à ses identités via l’export. Il s’agit de publications distinctes avec leurs barrières propres. Après suppression d’un LV, `advance` peut effectuer une rotation sans modifier la table courante. Les grains devenus disponibles seront intégralement réinitialisés lors de leur prochaine attribution.

Le gestionnaire reste mono-écrivain pour les E/S. Les admissions peuvent s’exécuter dans des threads concurrents et leurs tickets coexister. Chaque transaction du moteur dans un LV utilise ensuite sa propre réserve de maintenance et sa propre carte de blocs sans acquérir une nouvelle garantie de capacité auprès du pool. L’accès RAW exige le détenteur fourni par l’administrateur ; la fonction d’export FS refuse le type RAW.

Le formatage initial synchronise la première bande, écrit les deux ancres et attend F2. Une erreur sur une des ancres ou sur F2 est une publication indéterminée. Après F2 acquittée, le formateur relit les deux catalogues et compare la tête au reçu avant de livrer l’instance. Une erreur de cette relecture empêche l’ouverture de la session. La stabilisation d’un import ultérieur est une opération distincte.

## Vérifier et reproduire

`inspect_space` vérifie les deux catalogues sans flush et retourne la partition physique qu’ils décrivent. `inspect_usage` vérifie en outre chaque LV prêt en lecture seule et rend sa consommation logique et ses compteurs physiques internes. Une anomalie ou une E/S incomplète empêche le succès du rapport agrégé. Le détail des défauts reste disponible par `novafs check-pool`. Le support doit rester inchangé pendant la lecture hors ligne. La valeur durable des réservations en vol est zéro.

Les tests hôte emploient les secteurs de 512 et 4 096 octets. Le PD saturé de cinq grains et treize blocs possède quatre grains attribuables. Huit threads attendent la même barrière puis demandent un grain chacun : quatre admissions réussissent et quatre rencontrent `PoolFull`. Le test remplace un ticket annulé, valide les quatre gagnants et écrit puis relit chaque bloc de chaque LV avec un motif distinct. Un autre cas présente un LV prêt de deux grains et un volume RAW voisin ; un dépassement du quota de fichiers creux laisse le reçu, ses octets et la trace de publication intactes.

Les essais couvrent la perte d’un ticket à la réouverture et quatre EIO ciblées par géométrie. Il s’agit d’une écriture partielle de remise à zéro, d’un F1 partiellement effectué et d’un F2 qui perd ou conserve l’ancre. L’oracle vérifie la capacité restituée et le marqueur d’un autre volume dans les deux issues. Le cas d’ancre endommagée pendant la réservation du dernier grain exige un échec avant la moindre écriture ou le moindre flush. Le rapport hors ligne est aussi exercé avec un LV prêt de 12 288 octets logiques, un RAW, un LV en préparation et une corruption de données. Les six scénarios de volume P0/P2 sont répartis entre deux tests de bibliothèque et quatre tests d’intégration ; les 3 549 coupures P02 sont conservées. [Essais de saturation](../tests/pool_space.rs) · [Coupures historiques](../tests/volumes.rs) · [Catalogues forgés](../tests/support/pool.rs)

Le test de genèse relève exactement deux flushs en cas de succès. Pour chaque géométrie il fait échouer successivement chacune des écritures d’ancre après un préfixe de 53 octets, puis provoque une erreur de F2 après persistance complète des deux ancres. L’import refuse les images déchirées et retrouve une génération 1 vide avec ses deux grains disponibles après la perte de la seule réponse de F2.

```sh
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline --lib pool::manager -- --nocapture || exit 1
    novaFS/lab/run test --offline --test volumes --test pool_space -- --nocapture || exit 1
done
```

## Compatibilité de l’image complète du pool

Sur macOS, chacun des exécutables ARM64 et x86_64 des versions P2 (`6f15350`) et P3 a généré une image de 17 825 792 octets. La comparaison intégrale des quatre fichiers a réussi. Le SHA-256 commun est `066acd4a384b817af1327d725ebe365b9afff2788f30e8a59b78579d82a80c6f`. La fixture écrit en géométrie 4 096 ; chaque image est ensuite lue en secteurs de 512 et de 4 096 octets. Pour chaque lecture, le vérificateur tourne dans l’architecture opposée à celle du producteur.

Les seize lectures par `check-pool` (P2 et P3) ont retrouvé la génération 33 et vérifié le contenu des seize LV prêts. Les huit lectures par `space-pool` P3 ont retrouvé le grain administratif de 256 blocs, les 4 096 blocs attribués, zéro bloc disponible, différé, partagé ou réservé par ticket. Chacun des seize LV garantit 256 blocs et contient 4 096 octets logiques sous un plafond de 1 048 576 octets. L’empreinte de chaque image après vérification était identique à celle calculée avant les lectures. [Banc d’échange des catalogues et des LV](../lab/test-pool-interop.py)

```sh
(
    set -e
    reference=$(mktemp -d)
    trap 'rm -rf "$reference"' EXIT
    git archive 6f15350 novaFS | tar -x -C "$reference"
    python3 novaFS/lab/test-pool-interop.py --reference "$reference/novaFS/lab/run"
)
```
