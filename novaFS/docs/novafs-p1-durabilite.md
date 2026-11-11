# NovaFS P1 v0.9.1 — contrat de durabilité

## Audit préalable du commit d0ba7cb

| Mécanisme P0 inspecté | Garantie et risque | Décision P1 |
| --- | --- | --- |
| Préparation CoW privée, au plus 64 blocs, deux générations protégées contre le recyclage | Un échec de préparation ne publie rien. Les écritures de dépendances atteignent ensuite le périphérique dans un ordre qui ne prouve aucune persistance. | Conserver le plan CoW et interrompre la publication au premier échec. Tester les données anciennes avec un oracle applicatif indépendant. |
| Dépendances, F1, ancre alternée, F2, réponse | F1 doit stabiliser toutes les dépendances avant la soumission de l’ancre. Seul le retour réussi de F2 autorise la confirmation au client. Une complétion perdue peut laisser une transaction *entière* récupérable sans confirmation observée. | Garder les deux barrières. Injecter aussi des opérations partiellement accomplies qui rendent EIO, indépendamment de la panne du service. |
| Deux ancres de 4 Kio, manifestes chaînés par digest, tailles et générations vérifiées | Aucune hypothèse d’atomicité d’un bloc ou d’un secteur. Une déchirure qui change le contenu protégé invalide la somme. Si les octets finaux forment exactement l’ancienne ou la nouvelle ancre, ils représentent une ancre complète. | Conserver le format P0 et le refus de montage normal sur paire invalide. Vérifier les deux géométries ainsi que les réserves et octets initialisés. |
| Montage RW et vérification | Le montage inspecte les deux générations et les références puis stabilise la tête avant de rendre l’écrivain ; les données sont contrôlées à la lecture, ou intégralement par `verify`. Le sauvetage est explicite et en lecture seule. | Comparer génération, contenu et reçus après plusieurs remontages, y compris après une réponse indéterminée. |
| Fichier temporaire ordinaire sur macOS | `write_all_at` peut avoir modifié une partie du fichier avant une erreur. La réussite de l’écriture ne confirme que son acceptation. `File::sync_all` utilise `F_FULLFSYNC` sur les deux cibles Darwin de la toolchain contrôlée. Les erreurs remontent. | Garder la primitive. Mesurer son coût et tester sur images réelles. Le retour du système ne permet pas d’observer un cache matériel qui ment. |
| Simulateur P0 | Le cache volatil et le support stable sont séparés. Les coupures déterministes existent avant et après les E/S, avec perte, inversion ou déchirure. Le flush réussi transfère tout le cache dans le modèle. Il manque les erreurs de lecture autonomes, les écritures courtes et les flushs partiellement effectués. | Étendre le backend de laboratoire, garder les événements et les contre-exemples P0, comparer avec une coupure du processus de machine virtuelle. |

## Hypothèses de la garantie

Un seul écrivain possède le volume. Une écriture qui échoue peut avoir modifié une partie de sa **plage adressée**. Toute écriture acceptée depuis la dernière barrière peut être perdue, persistée ou réordonnée. Une barrière réussie doit rendre toutes les écritures précédentes persistantes dans le domaine annoncé, y compris les caches intermédiaires. Elle doit également empêcher une écriture antérieure retardée d’écraser une version ultérieure du même emplacement. Une erreur de barrière ne dit pas quelle partie a persisté. Une seconde instance ne réutilise le support qu’après cessation des anciennes E/S.

Avant la soumission de l’ancre, la dernière génération confirmée reste la tête. Pendant la publication, une perte de réponse permet l’ancienne ou la nouvelle transaction complète, ou un refus de montage si l’ancre est déchirée. Le client ne reçoit aucun succès dans cette fenêtre. Après l’effet de F2, la nouvelle génération est exigée si le support respecte la barrière. Une copie ancienne mais cohérente ne révèle pas d’elle-même son recul. Un reçu de génération et de manifeste conservé indépendamment permet de le détecter, sans restaurer les données.

Les sommes SHA-256 détectent des altérations accidentelles sous les hypothèses usuelles du hachage. Elles ne donnent aucune authentification contre un acteur capable de réécrire l’image entière. Perte du support, écrasement d’une plage voisine, cache qui accuse réception à tort et défaillance commune du support et du témoin sortent de la garantie.

Les mesures antérieures et leur provenance restent dans les [résultats P0](novafs-prototype-macos.md). Le [format expérimental](novafs-disk-format.md) et le [protocole de récupération](novafs-recovery.md) définissent les mêmes octets et la même filiation pour les deux versions.

## Résultats sur les quatre domaines de panne

Les essais ont tourné sur le même Mac Apple Silicon, sous macOS 27.2, avec Rust stable 1.99.1 et QEMU 11.1.3. Les deux exécutables du moteur sont des Mach-O distincts, ARM64 natif et x86_64 exécuté sous Rosetta.

| Domaine éprouvé | Essai exécuté | Ce qu’établit la relecture |
| --- | --- | --- |
| Support simulé | Par architecture, 126 coupures de formatage, 162 coupures de mutation, 2 après acquittement, 126 erreurs d’écriture, 18 flushs en échec, 8 erreurs de lecture et 96 cycles d’arrêt/reprise. Secteurs de 512 et 4 096 octets. | L’oracle retrouve la génération et les octets imposés par le sous-ensemble persisté. Les ancres mixtes non valides font échouer le montage normal. Les essais P0 de barrières mensongères et de retour à une image ancienne restent actifs. |
| Fichier ordinaire sur macOS | 12 arrêts du processus écrivain par géométrie et par architecture, soit 48 `SIGKILL`. Deux remontages RO indépendants par arrêt. | Tous les octets et la génération sont vérifiés. Le cache macOS survit au processus. Dès le retour de l’écriture complète de l’ancre, la nouvelle tête est visible sur ce même hôte, même avant F2. |
| Disque raw virtio-blk sous QEMU | 5 arrêts de VM par géométrie et par architecture du moteur hôte, soit 20 `SIGKILL`. Une sonde UEFI x86_64 rejoue les écritures **et les flushs** produits par le moteur. | Les compteurs QMP du disque correspondent au nombre exact d’écritures, d’octets et de synchronisations demandés. Après l’arrêt brutal, le moteur hôte retrouve deux fois la vue complète attendue sur le fichier raw. |
| Stockage physique hors tension | Aucun essai de perte d’alimentation du Mac, du contrôleur ou du média. | La persistance électrique demeure non qualifiée. Les compteurs QMP et `F_FULLFSYNC` ne permettent pas de mesurer l’énergie disponible ni le contenu des caches après perte d’alimentation. |

Les injections détaillées couvrent également les racines perdues, les blocs remis à zéro et le réordonnancement des écritures non synchronisées. Les instantanés, le journal et l’oracle appartiennent au banc. Le moteur ne conserve aucun de ces témoins sur le volume. [Matrice des défaillances simulées](novafs-p1-coupures.md) · [Documentation Apple de la requête F_FULLFSYNC](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html)

### Frontières sur fichier et dans la VM

La génération 5 contient deux fichiers. Une écriture de 4 250 octets à l’offset 4 059 modifie trois blocs logiques du fichier de 11 000 octets. Le vérificateur reconstitue le fichier entier hors du moteur, y compris ses deux plages inchangées, et contrôle le fichier témoin, le répertoire, la génération et les checksums des deux générations. L’image de départ est copiée à neuf pour chaque arrêt. Elle contient 320 blocs de 4 Kio. Chaque relecture utilise un descripteur RO. [Fixture Rust](../lab/p1-image.rs) · [Orchestration des arrêts](../lab/test-persistence.py)

| Étape de la coupure | Écritures / flushs vus par QMP | Résultat observé dans les quatre couples architecture hôte / secteur |
| --- | --- | --- |
| Avant la première écriture | 0 / 0 | Génération 5. |
| Après toutes les dépendances, avant F1 | 7 / 0 | Génération 5. |
| Après F1, avant l’ancre | 7 / 1 | Génération 5. |
| Après l’écriture de l’ancre, avant F2 | 8 / 1 | Génération 6 complète. La VM pouvait encore perdre cette ancre. Le client n’avait reçu aucune confirmation. |
| Après F2 | 8 / 2 | Génération 6 complète, exigée. |

Sur le fichier hôte, le banc tue aussi l’écrivain après chaque dépendance et après retour du succès à l’application. Dans la VM, la sonde utilise les services Block I/O du firmware sur le disque virtuel. Elle s’immobilise à la frontière choisie. Le parent interroge QMP puis tue la VM. Cette campagne couvre des **frontières de complétion**. Les déchirures à l’intérieur d’une E/S et les caches qui perdent un sous-ensemble déterminé sont exercés dans le simulateur. L’invité utilise une petite sonde de laboratoire, indépendante de NovaOS. [Relevé des 68 arrêts](validation/v0.9.1/coupures-supports.tsv) · [Sonde UEFI](../lab/p1-probe.c) · [Contrôle QEMU](../lab/p1_qemu.py) · [UEFI Block I/O](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html) · [Compteurs QMP](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html#command-QMP.query-blockstats)

## Compatibilité et ressources

- **48 tests courts et 15 scénarios P0 réussis par architecture**, dont N02/N03/N05, les 3 549 coupures du banc de volumes P02 et les deux géométries. La campagne différentielle distincte a exercé un million d’opérations, 54 643 pénuries et 1 000 remontages avec audit sur chaque architecture, en 24,785 s et 39,874 s respectivement.
- F01 a effectué 24 échanges entre architectures et tailles de secteur. Huit ont utilisé P1 aux deux extrémités, huit P0 en écriture et P1 en lecture, huit P1 en écriture et P0 en lecture. P0 a été recompilé depuis `d0ba7cb`. Chaque image canonique fait 1 572 864 octets et conserve le SHA-256 P0 `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. Les encodeurs, les règles de filiation, les limites du volume et les deux barrières de publication sont ceux de P0.
- Le majorant mesuré, tas Rust vivant et piles de travail réservées, atteint 3 809 952 octets sur ARM64 et 3 809 616 sur x86_64. Les 256 cycles de récupération retrouvent leur niveau initial de tas à chaque destruction. La RSS du processus traduit atteint 14 798 848 octets. Les mesures distinguent ces périmètres et donnent les débits, les latences et les coûts de synchronisation comparés à P0. [Rapport mémoire et performances](novafs-p1-mesures.md)
- Les commandes `make ARCH=x86_64 test` et `make ARCH=aarch64 test` ont chacune terminé avec 24 scénarios QEMU réussis sur le noyau v0.8.0. Au premier passage x86_64, le lecteur du banc ABI a interprété la dernière ligne série `RAM phase=repos_ui` pendant son émission et a levé `KeyError: 'resident'`. La copie du journal s’arrêtait au milieu de `heap_va`. Une relance intégrale a réussi avec le même noyau et le même banc. Ce risque de lecture partielle demeure dans le banc NovaOS.

## Reproduire depuis la racine du dépôt

Le banc fichier utilise les deux cibles Darwin, Rust stable et Rosetta. Le banc VM demande aussi Python 3, Clang avec cible COFF x86_64, le `rust-lld` livré avec Rust, `qemu-system-x86_64` et les firmwares EDK II. Le lanceur découvre les firmwares distribués avec QEMU. Les variables `OVMF_CODE` et `OVMF_VARS` acceptent une installation distincte.

```sh
set -e
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline -- --nocapture
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
    python3 novaFS/lab/test-persistence.py --support tous
done
unset NOVAFS_TARGET

# Extraire uniquement le paquet P0 pour l’essai de lecture croisée.
(
    p0=$(mktemp -d)
    trap 'rm -rf "$p0"' EXIT
    git archive d0ba7cb novaFS | tar -x -C "$p0"
    CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py --p0 "$p0/novaFS/lab/run"
)

make -C novaOS ARCH=x86_64 test
make -C novaOS ARCH=aarch64 test
```

L’option `--support fichier` lance uniquement les morts du processus hôte, `--support qemu` uniquement les coupures de VM. La commande F01 sans `--p0` teste les deux architectures de la version courante. Les campagnes de débit et de restitution mémoire disposent de leurs [commandes de reproduction](novafs-p1-mesures.md#reproduction).
