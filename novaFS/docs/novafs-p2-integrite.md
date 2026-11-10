# NovaFS P2 v0.9.2 — intégrité et récupération

## Audit préalable de la base 38c8699

| Structure ou opération | Protection existante et risque constaté | Décision P2 |
| --- | --- | --- |
| Deux ancres, manifestes, cinq familles de B+Trees | SHA-256 couvre chaque bloc de métadonnées avec domaine, identité FS/LV, position et naissance. Les références des parents lient l’empreinte des enfants. Le contrôle de la paire s’arrête au premier échec, même si l’autre slot demeure intégralement lisible. | Examiner séparément les deux slots, puis leur relation. Distinguer état local lisible, tête admissible, échec de lecture et vérification inachevée. |
| Objets, noms, extents, collecte et espace libre | Des bornes de profondeur, des contrôles inverses et des bitmaps vérifient la géométrie et la propriété. La lecture des seuls en-têtes ne contrôle ni le contenu des fichiers ni toute la parenté. | Relire les données des deux générations sans cache. Vérifier la progression des compteurs et les invariants de représentation imposés par l’écrivain. |
| Générations et reprise | Un montage normal demande deux ancres admissibles et une filiation vérifiée. Le secours P1 sélectionne explicitement un seul slot et ne prouve pas sa fraîcheur. | Ne sélectionner automatiquement une tête que sur preuve locale de la paire. Exposer les candidats vérifiés au lieu de rétrograder silencieusement. Conserver F1 et F2. |
| Image de LV et catalogue du pool | Une image de LV ne contient pas le catalogue du PD. La fixture de pool P0 connaît les identités, segments, quotas et types ; son import effectue un flush. | Proposer un contrôle séparé du profil de pool existant, sans utiliser l’import écrivain. Distinguer catalogue, contenu des LV prêts et données RAW sans somme FS. |
| CLI historique et limites | La commande verify importe les flux dans une copie temporaire et limite leur taille à 128 Mio. Le moteur admet des LV de 8 Gio. Un fichier modifié en parallèle ne constitue pas une vue stable. | Pour check, lire directement un descripteur de fichier ordinaire de taille exacte. Exiger une image hors ligne. Vérifier aussi les alias de sortie, le comportement en cas de troncature et les coûts mémoire. |

Le format P0 contient les informations nécessaires à ces décisions. Les sommes identifient les altérations des structures protégées. Elles n’authentifient pas le producteur. La restauration intégrale d’une ancienne paire cohérente nécessite un reçu conservé hors du support pour être détectée. Les octets libres, les données RAW et l’espace hors LV n’ont pas de checksum de données NovaFS.

## Contrôler une image

Depuis la racine du dépôt, les commandes suivantes utilisent le binaire Darwin du prototype. La taille annoncée est exprimée en blocs de 4 096 octets. Le second argument désigne des secteurs logiques de 512 ou 4 096 octets. Les IDs attendus viennent du formatage ou d’un inventaire indépendant de l’image. L’ancienne commande `inspect` peut afficher les identités inscrites, sans en établir la provenance.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
outil="${CARGO_TARGET_DIR:-novaFS/target}/aarch64-apple-darwin/release/novafs"
# Exemple pour un LV de 512 blocs. Conserver les deux IDs du formatage.
"$outil" check 512 4096 "$FS_ID" "$LV_ID" < image.raw

# Exemple pour le catalogue de laboratoire à 16 LV, sur son propre fichier PD.
"$outil" check-pool 4352 4096 < pool.raw
```

L’outil travaille directement sur le descripteur standard d’entrée d’un fichier ordinaire, par lectures positionnelles. Il n’importe pas l’image dans le tas et n’en écrit aucune copie. Il refuse les tubes pour ces deux commandes. La géométrie est bornée avant le calcul de longueur et la longueur réelle doit correspondre exactement, y compris pour un fichier creux. L’analyse exige une image hors ligne et immuable. Un changement détecté de longueur, `mtime` ou `ctime` pendant l’analyse invalide le rapport. Ce contrôle ne remplace pas l’exclusion des autres écrivains.

Le descripteur source peut être ouvert en lecture seule. Le binaire `novafs` refuse une sortie standard **ou d’erreur** pointant sur le même inode, y compris via un lien physique. Ce refus est silencieux pour préserver l’image. L’exemple sépare la compilation de l’exécution car Cargo émet son propre journal avant le démarrage de `novafs`. Enregistrer le JSON dans un fichier distinct. Une redirection shell avec `>` peut tronquer son fichier cible avant même le démarrage du vérificateur.

### Interpréter le rapport

La sortie normale est un seul document JSON de schéma `novafs-check-1`. Les adresses de `check` sont des blocs relatifs au LV. Chaque slot expose l’enveloppe décodée si elle est lisible, le verdict `vue_verifiee` et la première anomalie bloquante de cette vue, avec étape, raison et numéro de bloc lorsque celui-ci est certain. Le second slot est examiné indépendamment. Les erreurs communes à une génération ou à une relation peuvent ne pas avoir de bloc unique. Un digest de manifeste affiché ne suffit pas à valider ses dépendances.

| Code | Champ `statut` | Interprétation |
| ---: | --- | --- |
| 0 | `coherent` | Toutes les vérifications applicables au domaine demandé sont terminées et ont réussi. Dans un pool, le contenu RAW et celui d’un LV en préparation sont identifiés comme tels. |
| 1 | `incoherent` | Longueur incorrecte, données protégées endommagées, identification incompatible ou invariant du format violé. Consulter chaque slot et la paire. |
| 2 | `usage` si émis | Arguments, géométrie demandée ou transport invalides. Lorsque les descripteurs standard ne peuvent pas être inspectés ou que l’un des canaux de sortie désigne l’image, le précontrôle s’arrête sans écrire de JSON. |
| 3 | `incomplet` si émis | Après le précontrôle, E/S, mémoire ou limite d’analyse, modification concurrente détectée ou échec d’envoi du rapport. Ce code prévaut lorsqu’une autre partie de l’image présente déjà une corruption. |

Un échec de sortie peut laisser un JSON incomplet. Le consommateur vérifie le code de sortie, le schéma et le champ global avant d’exploiter les détails. La commande `check-pool` sépare les verdicts des catalogues et le diagnostic interne de chacun des LV prêts. Les identités, relations et limites du profil mono-disque sont précisées dans le [rapport de contrôle des volumes](novafs-p2-pool.md).

## Invariants et choix de récupération

1. Chaque slot doit contenir une ancre à la position, à la version et aux identités attendues, puis un manifeste de même géométrie et de naissance compatible. Chaque lecture vérifie les octets réservés prescrits et les références avant de les suivre.
2. Les cinq arbres se parcourent sous des bornes de clé et de niveau. Un bitmap repère un bloc d’arbre cyclique ou possédé deux fois avant de redescendre dans le nœud. Les recherches d’ancêtres de répertoire sont bornées et l’index des noms doit correspondre aux enregistrements des objets dans les deux sens.
3. Les extents d’un fichier doivent être ordonnés, disjoints et dans la taille déclarée. Leur naissance ne peut dépasser la révision du fichier. Le calcul du SHA-256 lie le contenu, le bloc, sa naissance et les identités. L’audit relit aussi les données antérieures encore protégées, les zéros de fin de fichier et les extents en attente de collecte.
4. Le bitmap d’occupation et les intervalles d’espace libre couvrent tout le LV, à l’exclusion de ses deux ancres. Un même bloc ne peut appartenir à deux objets d’une vue. Entre deux générations, l’âge exact d’un bloc déclaré nouvellement libéré doit correspondre à une possession réelle du prédécesseur. Un bloc partagé conserve sa référence et son propriétaire. Le prochain numéro d’objet doit être supérieur aux objets présents et ne peut reculer. Des numéros absents sont légitimes après suppression.
5. Une paire normale doit être une genèse redondante valide ou présenter deux générations adjacentes liées par le digest du manifeste précédent. Le contrôle indépendant des slots précède le contrôle de leur relation. Deux slots individuellement lisibles mais divergents ne déterminent aucune tête normale.

| Verdict | Action de récupération |
| --- | --- |
| Deux vues vérifiées, paire vérifiée | La tête de cette image figure dans `paire.slot` et `paire.generation`. Comparer le reçu à un témoin externe si la fraîcheur est requise. |
| Un seul slot vérifié | Le montage normal est refusé. Un opérateur peut extraire explicitement un fichier du slot survivant avec `salvage` dans les limites du CLI P0 (image et fichier extrait plafonnés chacun à 128 Mio). Sur une plus grande image vérifiable par `check`, le diagnostic identifie le slot et l’API de secours RO reste disponible. Sa fraîcheur reste indéterminée. |
| Deux vues valides dont le lien est rejeté | Conserver les deux diagnostics et comparer les identités et reçus à une source indépendante. Ne classer aucune branche par son seul numéro de génération. |
| Aucun slot intégralement vérifié | Aucun état complet n’est déclaré récupérable par ce rapport. Conserver l’original. Le diagnostic ne reconstruit aucun octet. |
| EIO sur un slot, autre slot entièrement vérifié | Le rapport global reste incomplet (code 3). Le slot vérifié conserve son diagnostic local et peut servir au secours explicite. Une nouvelle lecture sur un support stabilisé peut lever une erreur transitoire. |

En P1, une paire aux métadonnées valides pouvait monter alors qu’un bloc de données commun aux deux générations était déjà corrompu. P2 relit aussi les contenus des deux générations **avant** tout montage normal RO ou RW. Cette décision ajoute des lectures proportionnelles aux données atteignables. Une nouvelle corruption après le montage reste contrôlée à la lecture des données sur le support. Le secours explicite P0 peut extraire un fichier lisible sans attester les autres fichiers du slot. Écrire l’extraction dans un fichier temporaire distinct et vérifier le succès de la commande avant de l’utiliser. Aucune commande P2 n’effectue de réparation.

La détection s’arrête aux blocs adressés, dans le modèle d’image immuable. Une permutation entièrement réencodée avec ses nouveaux digests, une autre branche légitime ou le rejeu d’une image valide peuvent satisfaire les invariants. L’analyse ne qualifie ni caches matériels ni persistance électrique. [Résultats des 14 familles d’injections](novafs-p2-injections.md) · [Contrat de durabilité P1](novafs-p1-durabilite.md)

## Validation sur les deux architectures

Les campagnes utilisent Rust stable 1.99.1 sur macOS 27.2, en ARM64 natif et en x86_64 sous Rosetta. Les quinze scénarios P0, les cas P1 et la matrice P2 font partie des **63 tests ordinaires réussis par architecture**. Les campagnes longues et les fixtures ignorées par défaut ont leur propre invocation ci-dessous.

| Campagne | Résultat |
| --- | --- |
| P2, intégrité | 14 familles de corruption, dont les références circulaires, les chevauchements, les métadonnées de volume, les EIO et une corruption après perte du cache. Sur chaque architecture, 168 invocations réelles de `check` dans la matrice LV, avec comparaison intégrale des images. Les injections du catalogue physique s’y ajoutent. |
| P2, pénurie réelle | Par architecture, l’allocateur du test a rendu un pointeur nul à huit demandes réelles et ciblées du moteur, sur deux tailles de secteur. Le diagnostic signale `NoMemory` et la nouvelle tentative retrouve la paire sans modifier l’image. Les tests de crédit des bitmaps couvrent aussi l’insuffisance de ressources avant le parcours. |
| P0, différentiel | 1 000 000 opérations sur chaque architecture. Le modèle a observé 345 183 commits, 54 643 pénuries injectées et 1 000 remontages avec audit par campagne. |
| P1, durabilité | 48 arrêts de processus sur fichiers et 20 arrêts de VM QEMU x86_64, en secteurs 512 et 4 096. Les journaux viennent des deux exécutables hôtes. Deux remontages RO par arrêt. Après F2, les compteurs du disque virtuel confirment les huit écritures et les deux flushs attendus sur la transaction de référence. |
| P0/P1/P2, format | F01 a été exécuté contre `d0ba7cb` puis `38c8699`. Chaque campagne a effectué huit échanges ARM64/x86_64 P2→P2 et seize échanges interversions dans les deux sens. Toutes les images canoniques ont rendu le SHA-256 publié en P0, `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. |
| P2, mémoire | 24 contrôles CLI sur des LV creux de 8 Gio sains et corrompus et un pool de 16 LV prêts. Les versions standard et instrumentées ont produit les mêmes rapports. Les trois images ont été comparées intégralement à leur copie initiale. Les 768 contrôles de restitution ont retrouvé la ligne de base. |

La réserve de pile principale du binaire Darwin est fixée à 2 Mio. Le plus grand pic mesuré du tas et de la pile du vérificateur atteint **2 649 136 octets**. La RSS maximale observée pour la CLI standard atteint **3 162 112 octets en ARM64 et 5 251 072 octets sous Rosetta**, dans la limite de 7 Mio sur ces charges. Le banc complet du moteur a également terminé sur les deux architectures, y compris la pression sur les blocs, leur réutilisation, la contention et la vérification d’une image creuse de 8 Gio. [Bilan des exécutions](validation/v0.9.2/campagne.json) · [Rapport mémoire](novafs-p2-memoire.md) · [Relevé brut](validation/v0.9.2/memoire.json)

### Reproduire la campagne

Depuis la racine du dépôt, avec les deux cibles Darwin installées, Rosetta et QEMU avec les firmwares EDK II pour les essais P1. Les dépendances Cargo doivent être disponibles localement pour utiliser `--offline`. La génération des images mémoire exige un répertoire temporaire APFS.

```sh
set -e
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline -- --nocapture
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
    for secteur in 512 4096; do
        novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" reprise
    done
    case "$arch" in aarch64) secteur=4096 ;; x86_64) secteur=512 ;; esac
    novaFS/lab/run run --offline --bin novafs-bench -- "$secteur" complet
done
unset NOVAFS_TARGET
python3 novaFS/lab/measure-check.py
(
    refs=$(mktemp -d)
    trap 'rm -rf "$refs"' EXIT
    for commit in d0ba7cb 38c8699; do
        mkdir "$refs/$commit"
        git archive "$commit" novaFS | tar -x -C "$refs/$commit"
        CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py \
            --reference "$refs/$commit/novaFS/lab/run"
    done
)
```

Les coupures de VM ci-dessus concernent uniquement le disque virtuel et la sonde P1. Les sources, les configurations de compilation et les composants partagés de NovaOS sont restés hors du changement. Ses suites de démarrage ne font pas partie de cette campagne P2.
