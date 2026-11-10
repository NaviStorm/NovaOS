# Contrôle du catalogue de volumes P0 en lecture seule

`novafs check-pool BLOCS SECTEUR < pd.raw` examine une image du périphérique physique de laboratoire. La taille annoncée doit correspondre exactement au fichier ordinaire. Les champs `slots`, `paire` et `volumes` du JSON qualifient successivement le catalogue de chaque ancre, le lien entre les deux catalogues, puis les contenus FS du catalogue retenu. Pour chaque LV prêt, `contenu.diagnostic` conserve les résultats indépendants des deux ancres FS et de leur filiation. Les adresses de ce diagnostic sont relatives au LV. Un résultat `paire.etat=verifiee` sur le catalogue peut coexister avec une erreur dans un LV prêt et un échec global. Le code 0 exige le succès de tous les contrôles applicables. Le code 1 signale une incohérence, 2 une erreur d’usage et 3 une analyse incomplète.

## Invariants et périmètre

- Les deux ancres occupent les blocs physiques 0 et 1. Le lecteur vérifie séparément la signature, les identités de laboratoire de pool et de PD, le slot, la génération, la bande désignée, les réserves nulles et le SHA-256 de chaque ancre. Chaque catalogue occupe 16 blocs de 4 Kio dans l’une des trois bandes du grain 0. Son SHA-256, son en-tête, son unique membre, sa taille et son remplissage sont vérifiés avant de valider les entrées.
- Le catalogue contient au plus 16 LV, 256 segments et quatre grains épais par domaine. Le contrôle refuse une identité nulle, des identités LV/FS dupliquées, un propriétaire nul, une somme débordante, un trou logique, un chevauchement de grains physiques ou une attribution recouvrant le grain 0 ou la queue du support. Un grain représente 256 blocs NovaFS dans les deux géométries de secteur.
- Le formateur P0 produit une genèse vide dans la bande 0, sans prédécesseur. Deux copies de cette genèse peuvent être identiques. Les générations suivantes sont consécutives, sur des bandes distinctes, et liées par le digest de leur prédécesseur. Un LV conservé garde son propriétaire, ses identités, sa taille et sa correspondance logique–physique. Il peut passer de préparation à prêt. Une nouvelle entrée FS prête exige une préparation antérieure. Toute réattribution prématurée d’un grain encore référencé est rejetée.
- En cas de paire inutilisable, le rapport conserve le résultat local de chaque slot et ne dérive aucune vue de LV. Pour une paire vérifiée, les LV prêts sont parcourus **un à un**, avec les identités FS et LV attendues et les mêmes contrôles des deux générations que `novafs check`. Chaque lecture de bloc logique passe par la table de segments bornée. Le parent du lecteur est un emprunt partagé. Aucune écriture ni synchronisation n’est transmise au disque.
- Une entrée RAW est marquée `donnees_sans_checksum_fs`. Un LV en préparation est marqué `non_publie`. Leur géométrie et leurs attributions sont contrôlées. Une image de LV seule permet de vérifier le FS et ses allocations internes ; les attributions physiques de son catalogue nécessitent séparément l’image du PD. Le contenu actuel d’un LV prêt peut avoir progressé indépendamment de la génération du catalogue.

Le format du banc contient des identités fixes, des checksums non secrets et une géométrie de segments. La capacité exacte du support et la taille de secteur ne sont pas enregistrées dans un champ authentifié. Le diagnostic compare les segments à la géométrie fournie ; une modification cohérente située au-delà des dernières attributions, un retour à une ancienne paire cohérente, la modification des octets RAW, ou une image entièrement refaite avec les mêmes identités peuvent lui échapper. Le décodeur et les règles de filiation sont partagés avec la fixture P01/P02. L’administration du pool reste dans le banc de tests. [Lecteur de catalogue](../src/pool/catalog.rs) · [Parcours des volumes](../src/pool.rs) · [Contrat des volumes](novafs-volumes.md)

## Essais exécutés

Résultats observés sur ARM64 natif et x86_64 sous Rosetta. Les fichiers contrôlés avant et après ont été comparés octet par octet.

| Injection | Nombre par architecture | Oracle |
| --- | ---: | --- |
| Bande et ancre recalculées après altération d’un ID, d’une plage ou du nombre de membres | 7, secteur 512 | Slot désigné en erreur, second slot localement valide, paire indisponible et zéro volume exposé. |
| Genèse portant un prédécesseur, avec deux ancres et des sommes recalculées | 2, une par géométrie | Les deux générations locales sont rejetées avec le diagnostic de genèse du catalogue. |
| Catalogues localement valides avec migration d’un grain, changement de propriétaire, réattribution anticipée, publication prête sans préparation ou mauvais prédécesseur | 10, cinq par géométrie | Deux slots locaux valides, paire rejetée. Aucun événement d’écriture ou de flush pendant l’analyse. |
| CLI sur fichier sain, bit changé dans les données du LV prêt, somme de l’ancre 0 endommagée, puis descripteur ouvert uniquement en écriture pour obtenir une vraie erreur de lecture EBADF | 8, quatre par géométrie | Codes respectifs 0, 1, 1 et 3. Le FS endommagé laisse le catalogue vérifiable. L’ancre endommagée laisse un seul slot de catalogue vérifiable. Le descripteur illisible ne valide aucun slot. |

Les scénarios P01/P02 et les 3 549 interruptions P02 ont également abouti sur les deux architectures. Le fichier de mesure maximal en nombre de volumes contient 4 352 blocs de 4 Kio, avec 16 LV prêts contenant chacun un fichier. Sa création s’effectue hors du processus mesuré. [Essais du CLI et fixture de mesure](../tests/volumes.rs) · [Injections dans les catalogues](../tests/support/pool.rs)

```sh
for cible in aarch64 x86_64; do
    NOVAFS_TARGET="$cible-apple-darwin" novaFS/lab/run test --offline --test volumes -- --nocapture || exit 1
done
repertoire=$(mktemp -d)
NOVAFS_POOL_FIXTURE="$repertoire/pool.raw" novaFS/lab/run test --offline --test volumes p2_pool_image_de_mesure -- --ignored --exact
novaFS/lab/run run --offline -- check-pool 4352 4096 < "$repertoire/pool.raw"
```
