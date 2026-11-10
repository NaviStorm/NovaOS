# NovaFS P4 — coordinateur de validation par pool

## Propriété et admission

Le détenteur administratif remet un `Pool` ouvert à `PoolCoordinator::mount`. Le coordinateur refuse les tickets de provisionnement encore vivants, relit les deux catalogues, compare la tête au reçu du pool et conserve la seule propriété du PD. Chaque LV prêt est monté une fois avec ses identités, sa géométrie, son quota épais et la vérification P2. Les vues du coordinateur restent privées. Pour changer la géométrie, il faut rendre le PD et réimporter le pool. Le contrat P3 d’un écrivain exclusif sur un même périphérique continue de s’appliquer aux autres processus de l’hôte. [Implémentation du coordinateur](../src/pool/coordinator.rs) · [Catalogue et exports P3](novafs-p3-pool.md)

La file contient au plus **16 requêtes**. L’insertion sous mutex attribue un rang unique croissant ; une file pleine renvoie `Busy` sans retenir de données de l’appelant. Une requête ordinaire concerne un seul LV prêt. Les appels d’admission n’accordent aucun droit client et ne réalisent aucune authentification. Un futur service devra vérifier l’autorisation d’accès au LV avant de déposer sa requête.

Le transfert à `mount` ou `mount_with_cache_budget` se fait par valeur, y compris en cas de refus. Un appelant qui doit garder son périphérique pour réessayer ou diagnostiquer une image peut remettre au pool un **emprunt** de ce périphérique. Avec `let pool = Pool::import(&mut disk)?;`, un échec de `PoolCoordinator::mount(pool)` détruit le gestionnaire et rend la main sur `disk`. Un ticket de provisionnement précédent doit être annulé ou détruit ; aucun ticket d’une ancienne instance ne se valide sur la suivante.

| Opération | Condition pour avancer | Travail réalisé |
| --- | --- | --- |
| Lecture ou audit explicite | Aucune écriture plus ancienne en file. Plusieurs lecteurs consécutifs peuvent commencer ensemble. | Vue stable du moteur sous verrou de lecture. `verify` relit les blocs selon les contrôles P2. |
| Préparation d’écriture | Requête parmi les deux premières écritures dans la file, sans écriture antérieure sur le même LV. | Vrai plan CoW P3, comprenant adresses, données et métadonnées. Les LV distincts peuvent préparer en parallèle. Le plan coûte au plus 64 pages CoW et reste attaché au ticket. |
| Validation | Ticket préparé en tête de la file. | Retrait de la concurrence d’E/S, vérification du reçu du plan, puis publication exclusive des dépendances, F1, ancre, F2. Le reçu retourné est celui de la tête acquittée. |
| Annulation | Ticket encore détenu par l’appelant. | `cancel` ou Drop libère le plan privé puis son rang, sans écriture ni flush. Un nouvel arrivant ne peut pas passer devant un ticket plus ancien pour publier. |

`Busy` pendant la préparation ou la validation conserve le rang et les ressources du ticket pour une nouvelle tentative. La file n’exécute pas elle-même les requêtes. Un client qui garde le premier ticket sans terminer ni annuler peut retarder ses successeurs. Les bornes de deux plans et seize requêtes limitent la mémoire conservée dans ce cas. P4 ne déclare aucun délai maximal d’attente et ne dispose pas d’un mécanisme de révocation d’un client extérieur. Un appel hôte peut attendre l’achèvement d’un verrou court ou d’une phase d’E/S déjà engagée.

La préparation et les lectures prennent une phase partagée, puis le verrou du moteur du LV. Le backend physique accepte les lectures partagées et sérialise l’accès mutable au PD. La publication réclame la phase exclusive pour toute la transaction et empêche l’admission de nouveaux travaux d’E/S durant ce passage. Le mutex de file est relâché avant d’attendre un verrou de moteur ou d’effectuer des E/S. Les verrous et les références Rust délimitent ces durées de vie, sans opération `unsafe` propre au coordinateur.

## Budgets et erreurs

Le budget dynamique de cache par défaut vaut 1 Mio pour le **pool entier**. `mount_with_cache_budget` accepte de zéro à 2 Mio et répartit les octets entre les LV prêts. Les octets restants de la division sont inutilisés. Les en-têtes fixes des moteurs et les plans privés sont comptés séparément. `cache_stats(lv)` rend les comptes du cache du LV. `reclaim_caches()` restitue les pages et les index sous pression ; un plan privé déjà admis conserve ses adresses. Appeler `set_volume_policy` demande le prêt exclusif du coordinateur et une file vide, puis applique les mêmes plafonds P3.

Un refus de quota, de capacité ou de mémoire à la préparation termine la demande concernée et libère son rang. Un défaut d’intégrité ou d’E/S connu gèle le service sur ce PD. Une erreur à l’ancre ou à F2 conserve le caractère indéterminé de la publication. La requête fautive rend son erreur originale. Les demandes en attente ne peuvent plus valider ni obtenir de données du cache. Leurs tickets restent annulables. Une tentative de validation sur un pool gelé restitue également le plan. Une fois les tickets rendus, `into_device` permet de récupérer le PD, puis `Pool::import` et un nouveau montage rétablissent une vue vérifiée. Aucune transaction incertaine n’est rejouée d’office.

Après le gel global, les caches des autres LV peuvent encore occuper de la mémoire. Leur contenu est inaccessible aux demandes. Le propriétaire peut libérer ces allocations immédiatement par `reclaim_caches()` ou en détruisant le coordinateur.

```rust
let coordinateur = novafs::pool::PoolCoordinator::mount(pool)?;
let mut a = coordinateur.request_write(lv_a)?;
let mut b = coordinateur.request_write(lv_b)?;
b.prepare("/journal", 0, b"B")?;
a.prepare("/journal", 0, b"A")?;
a.commit()?;
b.commit()?;
assert_eq!(coordinateur.request_read(lv_a)?.read("/journal", 0, 1)?, b"A");
```

## Vérifications exécutées

Les sept tests fonctionnels et l’exécutable de restitution ont réussi sur ARM64 natif et x86_64 sous Rosetta. Les fautes et charges décrites ici s’exécutent dans le prototype macOS sans IPC ou threads NovaOS. [Scénarios fonctionnels](../tests/coordinator.rs) · [Mesure de restitution](../tests/coordinator_memory.rs)

- Sur fichiers images ordinaires, pour les secteurs 512 et 4 096, deux appels de lecture sur le même LV ont dû atteindre ensemble le backend. Deux préparations CoW sur des LV distincts ont satisfait le même rendez-vous temporisé. Ces observations établissent le chevauchement des appels hôtes au backend ; elles ne mesurent pas le parallélisme du matériel de stockage.
- Huit clients hôtes ont traité huit lots FIFO sur deux LV par géométrie, soit 128 transactions par architecture. Les threads ont été lancés dans l’ordre inverse des admissions. Chaque reçu devait avoir exactement la génération attendue par LV. Les deux barrières étaient comptées à chaque commit. Les deux derniers octets témoins ont été relus après réimport.
- Les essais ont saturé 16 places et annulé des requêtes préparées, vérifié le maintien du rang après `Busy`, le refus du troisième plan et la poursuite après dépassement de quota. Sur 0, 1 et 16 LV, la somme du cache chargé est restée inférieure au plafond. Une restitution en cours de préparation a rendu toutes les allocations dynamiques de cache et autorisé la publication ultérieure du plan.
- Une corruption explicite d’un bloc auparavant lu a été détectée par un audit qui contourne le cache. Le deuxième LV possédait un plan préparé ; sa publication et les futures lectures ont été refusées. Le journal a enregistré zéro écriture et zéro flush après détection.
- Une erreur de F1 et deux issues de F2 ont été injectées par géométrie. Après perte du cache volatile et réimport, la tête retrouvée correspondait à l’effet stable réel de la barrière ; l’autre LV conservait son contenu confirmé.
- Le périphérique a aussi accepté une dépendance puis renvoyé `NoSpace` au lieu de l’acquitter. La première requête reçoit cette cause. Le coordinateur consulte la santé du moteur pour distinguer cette erreur de sortie d’un refus lors de l’admission. La publication sur le second LV a été refusée sans écriture ni flush. Après perte du cache et réimport, les deux contenus confirmés ont été retrouvés.
- À chaque architecture et géométrie, deux séries de 64 cycles préparent des écritures de 64 Kio sur deux LV, révoquent les caches, annulent les tickets ou injectent une F2 indéterminée, puis réimportent et vérifient les deux volumes. Après **chacun des 256 cycles par architecture**, le tas Rust suivi est revenu à sa ligne de base. Sur l’exécution ciblée, elle valait 176 352 octets ARM64 et 159 232 sous Rosetta, simulateur et harnais compris. Le pic le plus élevé était respectivement de 455 216 et 427 408 octets.

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline \
        --test coordinator --test coordinator_memory -- --nocapture || exit 1
done
```
