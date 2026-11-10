# P3 v0.9.3 — saturation, annulation et reprise

Sur macOS, les six tests du pool et les six tests d’intégration du LV ont réussi séparément en ARM64 natif et en x86_64 sous Rosetta. Chaque test exerce les secteurs de **512 et 4 096 octets**, avec des blocs NovaFS de 4 Kio. Les durées du harnais excluent la compilation. [Tests du LV](../tests/transaction_space.rs) · [Tests du pool](../tests/pool_space.rs)

| Série | Résultat ARM64 | Résultat x86_64 |
| --- | ---: | ---: |
| Pool, grains, garanties et requêtes simultanées | 6 réussis en 0,02 s | 6 réussis en 0,03 s |
| LV, réservations exactes, saturation et récupération | 6 réussis en 4,29 s | 6 réussis en 6,94 s |
| Maintenance épuisée, test ciblé de la bibliothèque | 1 réussi en 0,23 s | 1 réussi en 0,37 s |

## Oracle et observations

L’oracle du LV lit les deux ancres et, pour chaque racine, les cinq arbres et les extents selon les offsets du format P0. Il énumère lui-même les adresses et interdit un second propriétaire dans une même vue. La réunion des deux ensembles donne l’allocation physique à protéger, ancres comprises. Leur différence donne les blocs conservés exclusivement par la génération précédente. Les enregistrements des objets vivants donnent la somme des longueurs logiques, trous compris. Le calcul **n’utilise ni les compteurs du moteur ni les valeurs de l’arbre d’espace**. Les ensembles explicites appartiennent au test sur de petits LV.

Pour la saturation, l’application conserve un marqueur distinct et remplit chaque page d’un octet prédit à partir de sa position. Le test relit toutes les pages, la taille, le marqueur et les checksums. Le contrôleur enregistre les événements du périphérique. « Zéro E/S de publication » signifie zéro écriture et zéro flush. L’admission peut lire les métadonnées nécessaires à son plan.

| Cas | Charge réellement exécutée et critère vérifié |
| --- | --- |
| 01. Disque rempli | Un PD de cinq grains et treize blocs contient un grain de métadonnées et quatre grains attribuables. Après attribution des quatre, le libre attribuable vaut zéro et une nouvelle demande d’un grain renvoie `PoolFull`. Les treize blocs de queue sont comptés à part. |
| 02. Pool presque plein | Sur un PD de trois grains, un LV RAW occupe le premier grain de données. La réservation du dernier grain fait passer le libre à zéro avant publication. L’oracle du catalogue sur disque conserve l’ancienne attribution jusqu’au commit. |
| 03. Quota atteint | Le domaine du premier LV réserve deux grains. Trois grains supplémentaires dépassent son quota de quatre alors qu’un autre domaine peut encore réserver deux grains. Dans un FS avec quota de 8 Kio, deux fichiers de 4 Kio consomment exactement le budget. Leur agrandissement d’un octet échoue sans publication. |
| 04. Réservations concurrentes | Huit threads alignés par barrière demandent chacun un grain. Quatre tickets réussissent, quatre reçoivent `PoolFull`. Un ticket est abandonné et remplacé, puis les quatre sont validés dans l’ordre inverse du parcours des gagnants. |
| 05. Plusieurs LV | Deux LV de deux grains épuisent le même pool. Le fichier témoin du volume FS A survit à l’abandon, à la suppression, à la rotation et à la réaffectation du budget de B. Une vue RAW refuse un propriétaire étranger. |
| 06. Surallocation logique | Une demande de `u64::MAX` grains échoue. Le LV FS épais borne les tailles logiques cumulées à sa capacité. L’image autonome conserve la possibilité P0 de trous plus grands que son support. Après montage avec un quota plus bas, les anciens octets restent lisibles et modifiables à longueur constante. Toute croissance est refusée. La suppression rend le budget. |
| 07. Annulation | Les deux chemins `cancel()` et destruction du plan abandonnent une écriture de 1, 4 097 ou 65 536 octets. Dans chacun de ces six essais par géométrie, le reçu, les compteurs et chaque octet du support restent identiques. N06 exerce aussi la pénurie de mémoire privée pendant la construction et avant publication. |
| 08. Coupure pendant réservation | La perte du cache à plan préparé, sans écriture émise, laisse l’ancien reçu et zéro bloc de transaction sur un montage indépendant. Un ticket de création issu d’une ancienne instance du pool est refusé par la nouvelle instance après coupure. |
| 09. Coupure pendant écriture | Sur un LV à saturation ordinaire, un `unlink` de maintenance présente 24 frontières. Chacune est interrompue avec trois politiques de persistance (zéro bloc volatil, le plus récent, tous en ordre inverse), soit **72 coupures par géométrie et 144 par architecture**. Dès l’acceptation de l’ancre, l’oracle autorise exactement la vue imposée par les octets persistés. Le témoin et les données de la génération précédente restent lisibles. Le pool éprouve aussi une écriture partielle suivie d’EIO pendant la mise à zéro. |
| 10. Échec de synchronisation | Sur le LV saturé, F1 puis F2 échouent après persistance de zéro, du dernier ou de tous les blocs volatils. Les six essais par géométrie distinguent un abandon avant l’ancre d’une publication indéterminée. Pour F2, la nouvelle vue complète est exigée si l’ancre a persisté. Les compteurs de l’instance gelée sont refusés. À la genèse du pool, le banc observe exactement F1 et F2, deux scénarios d’ancres initiales partiellement écrites puis refusées au remontage, et un F2 en erreur après stabilisation des deux ancres, qui donne une genèse intacte à la relecture. |
| 11. Libération près de la saturation | Sur 224 blocs, 76 pages d’utilisateur ont été admises en plus du marqueur. Porter la réserve à 133 blocs laisse zéro bloc ordinaire. L’écriture ordinaire échoue. `unlink` est admis et la charge logique du fichier disparaît immédiatement. Chacun des 76 lots de collecte respecte la partition physique. Une nouvelle écriture de 8 Kio réussit. |
| 12. Remontage après saturation | Après le premier refus d’ajout, la tête enregistrée, le contenu intégral des deux fichiers et l’allocation sont retrouvés au montage RW. Après collecte et réutilisation, le montage RO retrouve le témoin et le nouveau contenu. |
| 13. Cohérence des compteurs | Comparaison avec la carte indépendante des propriétaires pour les deux racines au remontage, après annulation, pendant la collecte et après chacune des coupures. Le pool distingue les grains durablement décrits, différés et réservés seulement en RAM. Sur un PD réunissant trois LV, l’inspection hors ligne qualifie 12 288 octets logiques dans le FS prêt. Elle ne publie aucun total de fichiers pour le RAW et le FS en préparation. Une corruption de données dans le FS prêt provoque le refus du diagnostic logique sans invalider les totaux physiques du catalogue. |
| 14. Absence de double allocation | Pendant un commit, chaque bloc CoW émis avant F1 est nouveau par rapport aux deux racines et distinct des autres blocs de cette émission. Les quatre LV dont les tickets ont finalement été validés reçoivent chacun un motif propre sur leurs 256 blocs. Tous les motifs sont vérifiés après remontage. |
| 15. Absence de fuite | Après abandon, les blocs de transaction ou grains réservés reviennent au libre attendu. Après une panne, les adresses CoW écrites mais inaccessibles ne restent pas comptabilisées comme occupées. Les blocs encore protégés par une ancienne racine restent différés jusqu’à la rotation. |
| 16. Préservation des données confirmées | Chaque refus d’ajout au LV plein conserve l’image stable entière. Pendant les EIO et les coupures, le marqueur acquitté survit. Lorsque la suppression est publiée, le secours explicite sur l’ancienne ancre relit l’ancien fichier page par page. |
| 17. Compatibilité | F01 a transféré huit images entre les deux exécutables Darwin et les deux géométries. Les quatre images produites de 1 572 864 octets sont identiques et conservent le SHA-256 P0 `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. |
| 18. Régression | Les suites P0/P1/P2 couvrent notamment les 3 549 coupures P02 du catalogue, les refus d’images corrompues et les erreurs d’E/S. Dans la campagne différentielle d’un million d’opérations par architecture, le modèle a observé 345 183 commits, 54 643 pénuries injectées et 1 000 remontages avec audit. |

### Coût variable et limites de la maintenance

Une écriture de 65 536 octets à l’offset 1 réserve 17 blocs de données et 4 blocs de métadonnées dans la fixture simple. À contenu constant de **1 octet**, 80 écritures séparées par des trous entraînent trois coûts distincts de métadonnées (4, 5 et 6 blocs). Les 160 commits de cet essai par architecture ont émis exactement le nombre de blocs réservé, puis l’écriture de l’ancre fixe. La relecture confirme les octets et les trous.

L’écrivain public protège la marge de maintenance. Un test unitaire fabrique en laboratoire une image plus contrainte en désactivant le plancher **uniquement pendant sa construction privée**. Le montage normal et le contrôle intégral acceptent l’image. Avec six blocs effectivement disponibles, le `unlink` réel ne tient plus et renvoie `MaintenanceExhausted`, sans écriture ni flush. La comparaison intégrale du support et le reçu confirment l’absence de publication. [Test d’épuisement](../src/engine/admission.rs)

### Corruption combinée à la pénurie

Sur le LV déjà saturé, une altération du digest de l’ancre courante rend la paire indisponible. Le montage normal RW est refusé. La vue survivante reste vérifiée et, en secours RO, restitue le marqueur et les 75 premières pages du fichier. Le support demeure identique à l’image déjà altérée. Le banc du pool altère aussi l’ancre de catalogue alors que son dernier grain est réservé. Le commit du ticket et l’import ultérieur échouent avant tout effacement, écriture ou flush. Ces deux contrôles ont réussi sur les deux architectures et les deux géométries.

## Reproduction depuis la racine

```sh
set -e
for arch in aarch64 x86_64; do
    export NOVAFS_TARGET="$arch-apple-darwin"
    novaFS/lab/run test --offline --test transaction_space --test pool_space -- --nocapture
    novaFS/lab/run test --offline --lib p3_refus_de_maintenance_sur_image_saturee -- --nocapture
    novaFS/lab/run test --offline
    novaFS/lab/run test --offline --test properties -- --ignored --nocapture
done
unset NOVAFS_TARGET
CARGO_NET_OFFLINE=true python3 novaFS/lab/test-interop.py
```

Les pannes de cette matrice sont injectées dans le modèle de stockage. Seule la concurrence d’admission du pool fait intervenir plusieurs threads hôtes. Les écritures d’un même FS gardent un propriétaire exclusif et la publication du catalogue est sérialisée. Le résultat ne qualifie aucune perte d’alimentation physique. [Comptabilisation et réservations P3](novafs-p3-espace.md) · [Contrat P1](novafs-p1-durabilite.md)
