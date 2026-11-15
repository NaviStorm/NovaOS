# Journalisation persistante — audit de la phase 2 de v0.22.0

Le candidat de phase 2 `8b0131918c28bfdac536489b03c4dfab9c667a0f` a été intégré par `7e705d37989bf451b486d1fa6c97e00e4da6e751`. Le candidat de phase 3 `dec941d25302ffcd4619aa563ecabb3894257e74` conserve le magasin du journal, son automate, les contrôles de lecture et d'émission et le producteur du gestionnaire. La phase 3 a modifié les raccordements IPC, la réaction à l'absence de `JOURNAL_STORE`, les codes d'événement admis et leurs libellés dans `nsh`. Les essais P2 et les régressions sur le binaire P3 ont donc des portées distinctes. [Contrat](journal-protocole-v0.22.0.md) · [Qualification P2](journalisation-v0.22.0.md) · [Résultats P3](validation/capacites-services-v0.22.0.json)

## Matrice d'acceptation

Les références J désignent les fichiers et les SHA-256 du tableau des preuves ci-dessous. « QEMU » couvre les invités AArch64 et x86_64. « Hôte » couvre Darwin ARM64 natif et x86_64 sous Rosetta. Chaque ligne relève de la phase 2. Les chemins de code cités dans le tableau sont relatifs à `novaOS/user/`, sauf celui de `nsh` situé dans `components/shell/`.

| ID | Exigence et code | Test et résultat | Architectures et preuve | Statut et limite |
| --- | --- | --- | --- | --- |
| P2-01 | Service 16 supervisé, `journal_service.rs` | Le démon utilisateur publie `ONLINE` sur P9 avant l'annonce au gestionnaire. Le refus du premier append conduit à `FAILED`. `journalisation.py`, `journal_saturation.py` | QEMU, J1, J5, J6 | VALIDÉ |
| P2-02 | Événements structurés, `services/journal_wire.rs` | Les 12 mots portent la séquence, le BootID, le sujet, l'émetteur, la gravité, le code, la qualité d'horloge et deux données numériques. Les trames invalides sont rejetées. `journal_service.rs` (tests) | Hôte et QEMU, J1, J2 | VALIDÉ |
| P2-03 | Identité et autorité des émetteurs et lecteurs, `services/journal_access.rs` | Contrôle des 64 masques d'autorisation, du BootID et de la génération. Usurpation refusée, attente révoquée, session ordinaire limitée à ses propres événements. `journal_service.rs`, `journalisation.py` | Hôte et QEMU, J1, J2 | VALIDÉ |
| P2-04 | Persistance de l'acquit, `novafs/files/journal_store.rs`, `services/journal_backend.rs` | Publication CoW sous F1/F2. Après arrêt du processus et de QEMU, le témoin acquitté est toujours présent dans l'historique retenu. `journal_pannes.py`, `journal_stockage.rs` | QEMU 1/2/4 vCPU et hôte 512/4096, J3, J4 | VALIDÉ AVEC LIMITES — rétention, refus de montage P9 et absence d'essai de coupure électrique physique |
| P2-05 | Rotation et lecture concurrente, `novafs/files/journal.rs`, `services/journal.rs` | 129 émissions, éviction du premier segment, contrôle des 113 événements restants. Rotation manuelle, lecture inverse et suivi pendant la rotation. `journal_stockage.rs`, `journal_service.rs`, `journalisation.py` | Hôte et QEMU, J1, J2, J3 | VALIDÉ |
| P2-06 | Quota, réserve, purge, `novafs/files/journal_store.rs` | Huit segments de seize événements, maximum utile de 12 288 octets. Avec quinze blocs ordinaires disponibles, la mutation est refusée. Les fichiers publics et la réserve de maintenance survivent à la saturation testée en VM. `journal_stockage.rs`, `journal_saturation.py` | Hôte 512/4096, QEMU deux profils, J3, J5 | VALIDÉ |
| P2-07 | Récupération d'un état entier, `novafs/files/journal_store.rs` | Par architecture et géométrie, 138 arrêts, 24 erreurs de FLUSH et 48 écritures partielles. Les lectures admises retrouvent l'ancien ou le nouvel état complet. `journal_stockage.rs` | Hôte, J3 | VALIDÉ AVEC LIMITES — deux préfixes d'ancre de 96 octets imposent le refus du montage normal pour chaque combinaison |
| P2-08 | Consultation, classement par BootID et diagnostic de 9/10, `journal.rs` de nsh, `services/journal_publisher.rs` | `journal` retrouve successivement la désactivation de 9, l'attente de 10, puis les deux READY après réactivation. Les 36 BootID des sessions de maintenance sont distincts. `journalisation.py` | QEMU 1/2/4 vCPU, secteurs 512/4096, J1, J6 | VALIDÉ |
| P2-09 | Absence de double publication, `services/journal_publisher.rs`, `novafs/files/journal_store.rs` | Avant append, zéro exemplaire. Après commit avec réponse perdue, un seul exemplaire et `UNCERTAIN` côté client. Un EID retenu identique restitue sa séquence, un contenu contradictoire est refusé. Aucune répétition automatique. `journal_stockage.rs`, `journal_pannes.py`, `journal_transport.rs` | QEMU et hôte, J3, J4, J6 | VALIDÉ AVEC LIMITES — l'éviction ou la purge borne la détection d'un EID |
| P2-10 | Indépendance du gestionnaire, `services/journal_publisher.rs`, `service_manager.rs` | Arrêt ou panne de 16, refus d'écriture, rotation en erreur ou réserve atteinte. Lecture P9, supervision, administration et requête native `NVFS` continuent. Le gestionnaire décompte pertes et issues inconnues dans `service status 16`. `journal_pannes.py`, `journal_saturation.py`, `services_capacites_journal.py` | QEMU et hôte, J4, J5, J6 | VALIDÉ AVEC LIMITES — file volatile de 32 trames avant acquit, pertes possibles avant drainage ou lorsque P9 est indisponible |
| P2-11 | Compatibilité du journal intégré avec les quatre déclarations historiques et `JOURNAL_STORE`, `journal_service.rs` | Retrait du droit, refus du nouveau 16, disponibilité de 9/10. Après restitution explicite et redémarrage, la notice est présente une fois. `services_capacites_journal.py` | QEMU P3 AArch64 2 vCPU/4096 et x86_64 4 vCPU/512, J6 | VALIDÉ |

## Provenance et portée des essais

La campagne QEMU 11.1.3 TCG de phase 2 utilise 256 Mio de mémoire et des images VirtIO brutes P9. Les douze combinaisons des deux architectures, des trois nombres de vCPU et des deux secteurs comportent chacune une session ordinaire et trois sessions de maintenance. Deux parcours de saturation/rétention portent le bilan des disques et des ISO à 50 démarrages. Deux rejeux supplémentaires vérifient la pagination. Les 24 cas de panne du démon avec redémarrage et relecture indépendante emploient un binaire instrumenté sur six profils. Les injections au niveau des écritures utilisent le moteur P9 réel sur un périphérique simulé. [Banc invité](../tests/journalisation.py) · [Pannes du démon](../tests/journal_pannes.py) · [Stockage](../tests/journal_stockage.rs)

Le candidat de phase 3 dispose en outre de quatre démarrages de régression du journal par architecture, inclus dans les 96 démarrages qualifiés de phase 3. Deux profils ciblent le retrait puis la restitution de `JOURNAL_STORE` en présence des quatre déclarations historiques. Les traces séparent les profils ordinaires des profils de maintenance. Une perte physique d'alimentation, une restauration intégrale du support et l'exécution dans l'application UTM demeurent hors du périmètre de ces essais. [Résultats individuels du journal](validation/journalisation-v0.22.0.json) · [Résultats intégrés](validation/capacites-services-v0.22.0.json)

## Mesures

| Profil historique | AArch64 | x86_64 |
| --- | ---: | ---: |
| QEMU, p50 / p95 / p99 = maximum d'une émission acquittée (ms) | 51,61 / 106,49 / 108,57 | 43,98 / 48,97 / 75,72 |
| QEMU, débit de 33 émissions séquentielles depuis nsh (événements/s) | 16,01 | 22,17 |
| QEMU, rotation acquittée, une observation (ms) | 92,12 | 74,96 |
| Hôte, moteur P9 sur SimDevice, émissions/s (secteurs 512 / 4096) | 5 120 / 5 134 | 3 608 / 3 831 |

La première série inclut la commande, l'entrée et la sortie série pour un vCPU et des secteurs de 512 octets, avec d'autres invités en activité sur le Mac. La série sur hôte mesure 129 append par géométrie. Les latences de rotation sur hôte varient de 255,8 à 364,8 µs pour les quatre combinaisons. [Mesures du magasin P9](journal-stockage-v0.22.0.md) · [Mesures depuis nsh](validation/journalisation-v0.22.0.json)

Dans les 12 traces J1 `cli/livraison-{arch}-{vCPU}-{secteur}/3-reactivation.serial`, chaque BootID de maintenance a un événement `BOOT` et un `ONLINE` à horloge monotone. En soustrayant les deux champs `ms` pour chacun des 18 BootID par architecture, on obtient **100–208 ms** sur AArch64 et **380–528 ms** sur x86_64. L'intervalle commence à l'entrée en session de stockage et s'achève à la construction de l'événement du démon, avant l'append et l'annonce de disponibilité. Le temps total jusqu'à l'invite et la durée de récupération P9 après interruption ne disposent pas d'un chronométrage dédié dans ces essais. [Construction de ONLINE](../user/journal_service.rs) · [Jalons précoces](journalisation-v0.22.0.md)

Le démon de phase 2 occupe **276 Kio de pages privées mappées**, cinq pages hors pile et 64 pages de pile. Le cache des 32 événements, soit 3 072 octets, appartient à cet espace. Le maximum de pile réellement utilisé demeure inconnu. La file du gestionnaire contient 4 096 octets de trames. Sur 38 arrêts de maintenance mesurés, les compteurs des pages privées, piles, objets IPC et processus reviennent à leur valeur initiale. Les pics du système complet vont de 930 à 1 141 cadres sur AArch64 et de 935 à 1 148 sur x86_64. [Comptabilité QEMU](validation/journalisation-v0.22.0.json)

## Références des preuves

Les archives durables résident dans le dossier `OS-worktrees`, à côté du dépôt principal. L'archive `archives-journalisation-0220-phase2/preuves-journalisation-0220-phase2.tar.gz` occupe 854 031 328 octets. Son SHA-256 est `60baf4334d844a99d0b00c8a4868dcd93305042cde6663a6d310d5f6c857101b`. Le manifeste externe `archives-journalisation-0220-phase2/SHA256SUMS` a pour SHA-256 `18aa09cb354f8b8e608cf15512b3d0e2d323076da44b73b64d8d499f585d626f`. Lors de cet audit, les 1 724 membres réguliers ont été décompressés puis comparés individuellement au manifeste et aux fichiers originaux relus dans `preuves-journalisation-0220-phase2`. Une archive sur ce même Mac partage le risque de perte de l'hôte. J6 désigne l'archive `archives-capacites-services-0220-phase3/preuves-capacites-services-0220-phase3.tar.gz`, SHA-256 `2452e903452c90b1237d190e7090e4ae378f9c405c6a55ab88d575498889d2f9`.

| Réf. | Fichier dans son archive | SHA-256 |
| --- | --- | --- |
| J1 | `cli/bilan-livraison.json`, index des 50 démarrages | `561b3850f194075887ce54cf99ec42452668b2a6794bb134c99cac7918b59441` |
| J1 | `cli/livraison-aarch64-1-512/3-reactivation.serial`, exemple des jalons par BootID | `29ad96174d5b380416aecd2860bc20d16c67af61c17120bb096580a1dee3bf45` |
| J2 | `service/protocole-aarch64.log` | `e5a238fab48e74b42a0613243afd5f208294d48a55d4fb0641773c8eac0a34d0` |
| J2 | `service/protocole-x86_64.log` | `4a8bb5785ffba72b620e97648ec1c278b28a8fa217f4148d3069a0d74b951483` |
| J3 | `stockage/aarch64-complet.log` | `9474d95dfe7f4fe6a96e44e8925a9d1b517e3091188bdcdc3b81031fa152be71` |
| J3 | `stockage/x86_64-complet.log` | `6231da3d8220b7795fd29c5a0e06712a90852faa36c27c4a5b83ded15884616d` |
| J4 | `livraison-pannes-aarch64-2-4096/resultats.json` | `41a442bdbecc95e84a274a6e6da451a0c32aa4781a32d4236936f93f037d4cfe` |
| J4 | `livraison-pannes-x86_64-4-4096/resultats.json` | `d00b1b4ad8677076dc40fe19c78d47c8017812fd883fb60a7dcd40feac5630e3` |
| J5 | `stockage/volume-plein/aarch64-512-02/resultats.json` | `423c83465e5d25544a1edc7d00b8efb7d8b51bc4531c9f2acba4f90bd13e5de1` |
| J5 | `stockage/volume-plein/x86_64-4096-02/resultats.json` | `a36f26dd24ec7ec0f4de5bc11f6af70635d41982122819da68bbaee53b39b4e8` |
| J6 | `journal-regression-aarch64/resultats.json` | `f88220e58f5939b173af4b7c0565503f3afb703b5f9a21b3abd7cdf7045be20c` |
| J6 | `journal-regression-x86_64/resultats.json` | `121a25da4df04f06fadfd4af774cf330e3132b99a5a76f8fe1812df0e01a110a` |
| J6 | `journal-historique-final-aarch64-2-4096/resultats.json` | `367e6e9097a73579d566dabaa41c85f312afd23e43b34938232822b2e62201f9` |
| J6 | `journal-historique-final-x86_64-4-512/resultats.json` | `69b270a1101b0dff56dcbd8e921308b872e361e274b49353e4ed6f23d175bde6` |
