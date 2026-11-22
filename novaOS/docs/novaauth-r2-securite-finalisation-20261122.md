# NovaOS v0.24.0 — Finalisation de la qualification de sécurité R2

Rapport du 22 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`.
Référence : `669326751aa8fd98a5591d4672c4153bbe295ca8`.
Candidat fonctionnel : `55838a0863c205087a136fd99ab76fe2d467dd6a` ; arbre
`0582fba456a5a7903c08a520b55dfbf473e28c3f`.
Le commit de livraison ajoute uniquement le présent bilan et ses références de preuve.

## Verdict et portée

L’[audit de clôture R2](novaauth-r2-audit-cloture-20261122.md) complète ce bilan :
les 36 cellules restent acquises, mais les opérations administratives manquantes
interdisent encore la clôture fonctionnelle.

Les neuf scénarios S1–S9 disposent de preuves invitées applicables sur les quatre
profils, dans le périmètre décrit ci-dessous. 40 exécutions QEMU nouvelles :
35 réussies, cinq échecs conservés puis corrigés et rejoués ; un échec de préparation
hôte supplémentaire n’a démarré aucune VM. Les résultats hôtes sont distingués.

Cela qualifie ces scénarios de sécurité R2 dans le laboratoire QEMU ; cela ne clôture
ni l’ensemble fonctionnel R2, ni NovaAuth v0.24.0 et ne qualifie pas la distribution
ordinaire. Les chemins de création d’utilisateur et de gestion des appartenances
restant refusés par le service doivent être finalisés dans un mandat distinct.
Aucune suppression définitive de compte ou de données n’est ajoutée.

## Matrice consolidée

Q = qualifié dans le périmètre explicitement décrit ; R = preuve antérieure
réutilisée après comparaison des sources. La matrice JSON donne pour chacune des
36 cellules les chemins absolus, les commits sources/banc/sonde, l’attendu,
l’observé et le SHA-256 du résultat. Elle ne présente pas tous les essais comme
exécutés avec un unique binaire reconstruit au dernier commit.

| Cas | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| S1 Auto-attribution | Q/R | Q/R | Q/R | Q/R |
| S2 Registre persistant | Q/R | Q/R | Q/R | Q/R |
| S3 Références d’Alice | Q | Q | Q | Q |
| S4 Requête directe | Q/R | Q/R | Q/R | Q/R |
| S5 Substitutions | Q + R | Q + R | Q + R | Q + R |
| S6 États des tickets | Q + R | Q | Q | Q |
| S7 Concurrence | Q | Q | Q | Q |
| S8 UNCERTAIN | Q | Q | Q | Q |
| S9 Réactivation | Q/R | Q | Q | Q |

### Attendus et résultats natifs

- **S1** — DENIED(5), Bob non administrateur, aucune mutation de privilèges ; contrôles positifs Alice.
- **S2** — APPEND au port administratif BAD_ARGUMENT(3) ; sans capacité de stockage DENIED(5) ; triplet et données inchangés.
- **S3** — Ticket, handle IPC et session authentiques Alice présentés par Bob : DENIED(5), aucun enfant Bob ; Alice lance son propre ticket (0).
- **S4** — Confirm et Outcome directs de Bob : DENIED(5), sans passage par un contrôle CLI ni mutation.
- **S5** — Identité/cible forgée : DENIED(5) ; contexte et métadonnées invalides : BAD_ARGUMENT(3) ; session Alice substituée : DENIED(5).
- **S6** — Expiré : TIMED_OUT(8) ; révoqué avant expiration : REVOKED(10) ; consommé : REFUSED(12). Deux threads : un succès, un refus, un enfant, une collecte ; reçu retrouvé sans relancement.
- **S7** — Admission Bob concurrente à Confirm Alice : REVOKED(10) avant expiration, aucun enfant, retrait durable. Deux requêtes de retrait dernier administrateur : DENIED(5). Publication de groupe concurrente : succès(0), retrait administrateur DENIED(5).
- **S8** — Après perte du reçu durable : UNCERTAIN(22), CLI20. Outcome explicite du même identifiant : COMPLETE(0), une seule mutation durable et un seul groupe créé.
- **S9** — Ticket vivant retiré : REVOKED(10). Après réactivation : ancien ticket10, ancienne session5 ; nouvelle authentification explicite, SID distinct et lancement Bob0, droits courants préservés.

Les refus de Bob ne passent pas uniquement par le parseur des commandes : les
requêtes arrivent au contrôle serveur. Les références réelles d’Alice sont copiées
comme valeurs opaques par le banc, sans transfert de capacité. Bob ne reçoit ni
console ni flux privé d’Alice ; Alice ne reçoit pas ceux de Bob. Les deux sorties
restent sous collecte exclusive du banc. Les anciens tests vérifient les vues
inchangées ; les nouveaux contrôlent aussi les mutations persistantes.

S2 exerce un refus au port administratif et au contrôle de capacité avant accès au
stockage. Il ne prétend pas exercer une écriture via un handle de volume légitime :
un tel handle n’est précisément pas accordé à Bob. Les trois objets d’identité,
les droits et le TSV synthétique restent inchangés.

S6 utilise les vrais appels invités, dont deux threads présentant le même ticket.
Les anciennes formes SPAWN restent refusées dans les threads secondaires. Le reçu
permet de retrouver le PID après réponse non utilisée ; aucun second enfant ni
seconde collecte ne sont acceptés. Le timeout 8, le refus 12 et REVOKED 10 sont
des résultats distincts et ne sont jamais assimilés entre eux.

S7 couvre trois témoins : deux consommations concurrentes ; présentation du ticket
Bob pendant la confirmation de sa désactivation par Alice ; requêtes contre le
dernier administrateur pendant une publication administrative valide. Deux ELF
établissent leurs requêtes partielles avant de les compléter. Le groupe
`equipe-r2` est publié une seule fois, le retrait d’Alice est refusé et `whoami`
réussit ensuite. Le préflight et Confirm utilisent le même moteur transactionnel,
qui réévalue la mutation sur la tête courante ; le service sérialise la publication.
Le système actuel interdit conservativement la désactivation de **tout**
administrateur. Les essais ne qualifient donc pas une future suppression,
promotion, démission ou procédure de transfert du dernier rôle. Ils établissent les
interleavings observés, sans prétendre couvrir tous les ordonnancements possibles.

S8 supprime une fois le reçu de la mutation de groupe déjà durable auprès de
l’autorité (opération 50). La commande rend le natif 22 / CLI20. Après une nouvelle
authentification explicite d’Alice, la consultation Outcome du même identifiant
rend COMPLETE. Le contrôle SQLite, la signature Ed25519 du reçu et la relecture
P9 démontrent une mutation unique, un groupe unique, aucun changement des comptes
ou facteurs. Le champ générique de reprise des remises 2-sur-2 n’est pas utilisé
comme oracle de cette mutation administrative : ce sont le résultat de la commande
et `persistance-resultats-final.json` qui établissent S8.

S9 conserve les droits courants de Bob, avec une nouvelle époque d’authentification.
La réactivation ne réutilise ni sa session ni son ancien ticket. Le nouveau SID est
distinct dans la même instance nsh. La relecture du triplet confirme seulement les
changements attendus de révision/époque de Bob, sans privilège administratif, sans
changement des autres comptes, groupes, facteurs ou droits. Aucun changement
supplémentaire d’appartenance pendant la désactivation n’est revendiqué.

### Chronologies invitées

Toutes les différences suivantes utilisent les ticks monotones de **la même VM**
à 250 Hz. Elles ne soustraient jamais une heure hôte à un instant invité.
S9 donne la marge à la présentation ; S7 donne la marge au retour natif, après
chevauchement de l’admission Bob et de Confirm Alice.

| Profil | S9 : marge avant expiration | S7 : marge au retour REVOKED |
|---|---:|---:|
| x86_64/512 | 3372 ms | 3332 ms |
| x86_64/4096 | 3268 ms | 3340 ms |
| aarch64/512 | 3900 ms | 3928 ms |
| aarch64/4096 | 4320 ms | 4332 ms |

Les anciens tickets sont encore vivants dans ces témoins ; aucun enfant n’est créé.
Le reçu terminal `RETIREE` est vérifié cryptographiquement et porte les trois
acquittements d’effecteurs (masque 7). Le résultat concurrent de Bob peut précéder
le reçu final : il ne remplace pas ce reçu et n’autorise pas une déclaration anticipée
de fin du drainage. Les contrôles après reçu restent ceux du témoin S9 consolidé.

## Applicabilité des preuves et binaires

- S1/S2/S4 et une partie S5 : sources et banc `f6399d1930c29ba011a60597d06e181281df8afe`.
- S9 : sources invitées `71bba986ac39258de35eb3b91db79f35420ed50d` ; banc historique
  x86_64/512 `19e5a055fd4dd90e4b6c30bfe236f5dd6b896b56`, banc x86_64/4096 `6693267`,
  banc AArch64 `2cd0747`.
- S3/S5 : sources `d8f7abc17232511a7cb7e221e2f1f4c103d780ba`.
- S6/S8 : sources `2615e51f10d34d21ae3fae7c4c9233ea4c24ff40` ; bancs distincts archivés.
- S7 admission/révocation : sources et banc `6b6f21219fd90cfd2288fdd46a65fb6e3e31c4fc`.
- S7 dernier administrateur : noyau/services `7ca56d3ea452f40d9918deb5917d5d697402fb47` ;
  dernière sonde et banc de publication `55838a0863c205087a136fd99ab76fe2d467dd6a`.

`applicabilite-sources.json` compare 499 fichiers sélectionnés après génération de
chaque profil. Les profils réactivation, séparation, tickets et dernier
administrateur sont identiques aux sources invitées sélectionnées du candidat.
Les changements ultérieurs de sonde et de banc n’affectent pas leurs noyaux/services.
Le profil course est inchangé depuis `6b6f212`.

Pour l’ancien profil de refus, les différences sont limitées à l’ajout de Reactivate,
de son état de retrait et de ses tests : trois fichiers fonctionnels et un fichier
de tests. Les gardes de rôle, propriétaire, canal et identité des refus S1/S2/S4/S5
précèdent ces ajouts et restent inchangés. Les branches de la sonde `--r2-securite`
et leurs dépendances n’ont pas été modifiées par les modes additionnels. Ces
preuves utilisent les binaires historiques conservés, pas une identité binaire
supposée après reconstruction.

205 empreintes d’ISO, noyaux, registres et images initiales mentionnées dans les
41 comptes rendus de coordination ont été relues et vérifiées ; aucun écart de
métadonnées de commit/arbre. Les binaires QEMU contenant des secrets de laboratoire
restent dans les répertoires privés durables, hors archive publique et hors Git.
Les sources des noyaux ordinaires, de l’ABI, du moteur P9, des services ordinaires
et de NXEXEC01 sont inchangées depuis `6693267`.

## Échecs conservés et corrections

| Essai | Première cause | Correction et validation |
|---|---|---|
| AArch64/512 désactivation-03 | Le refus natif réussi était séparé du prompt par une sortie concurrente ; attente hôte du mauvais motif. | `2cd0747` utilise le statut corrélé PID/étape ; test hôte reproduisant l’entrelacement ; désactivation-04 réussie. |
| x86_64/512 lectures-s3-01 | La sonde peer.inspect réutilisait une séquence antérieure sur la conversation partagée du registre. | `d8f7abc` synchronise les séquences privées avant/après inspection ; quatre profils réussis. |
| x86_64/512 course-01 | Le lancement de deux ELF administratifs épuisait les 64 endpoints globaux. | Le producteur Bob utilise le témoin ordinaire sans canal administratif ; événement corrélé à Confirm (`6b6f212`). Aucun quota relevé ; quatre profils réussis. |
| x86_64/512 dernier-01 | Bob avait fermé sa session mais conservait ses ressources de processus pendant les deux ELF témoins. | Sortie normale de Bob (`7ca56d3`), sans arrêt privilégié ; quatre profils réussis. |
| x86_64/512 publication-02 | Prepare et Confirm provenaient d’installations distinctes ; refus de liaison attendu par la sécurité. | Même ELF attesté pour les deux étapes (`55838a0`) ; quatre profils réussis. |

`publication-01` a échoué avant QEMU : export hôte incomplet d’un module du banc.
La copie figée a été complétée depuis Git. Ce cas ne compte pas parmi les 40 essais
invités. Les erreurs de compilation/génération et les journaux originaux sont aussi
conservés. Aucun refus n’a été transformé en succès et aucun délai de sécurité,
quota ou droit n’a été augmenté.

## Ressources et contrôles hôtes

Dans les 35 essais invités retenus : maximum quatre racines par acteur ; cinquième
demande refusée LIMIT(6). Pics globaux observés : 64 endpoints, 135 nœuds,
18 espaces, 74 cadres IPC. Ce sont des pics mesurés, pas de nouveaux budgets.
Après drainage final : endpoints, nœuds, espaces et cadres IPC à zéro ; pour chaque
processus collecté, racines, handles, attentes, pages, pile et threads à zéro.

Les tuples signataire `(racines, handles, attentes, pages, pile, cadres IPC globaux)`
restent avant/actif/après à `(4,6,0,68,64,31)` en x86_64 et `(4,6,0,59,64,31)` en
AArch64. Le registre reste avant/après à `(4,7,0,145,64,31)` et `(4,7,0,139,64,31)`.
Les détails par PID et les compteurs de cadres libres sont conservés dans les traces.
Ces mesures n’établissent pas à elles seules une preuve de restitution de toutes
les allocations physiques internes de NovaFS.

Les échanges TLS effectivement horodatés durent 2 648 à 3 536 ms, sous le délai
inchangé de 10 000 ms. Le quota DEBUG peut omettre des lignes ; les échanges non
horodatés ne sont pas ajoutés artificiellement aux statistiques.

Tests hôtes : 62 tests Python de banc, 18 tests Rust du client, huit tests du registre,
neuf contrôles ABI et cohérence de l’index ABI : réussis. Les tests ABI/registre
ont été exécutés avant les derniers ajouts de sonde/banc, qui ne modifient pas leurs
sources. Les 62 tests du banc et les 18 tests client ont été rejoués au candidat final.
Les quatre ELF historiques ne sont pas réexécutés dans ce mandat ; leurs preuves
précédentes sont conservées, avec les sources ordinaires/ABI inchangées. Aucune
qualification hôte n’est présentée comme un essai invité.

## Coordination et durée

Le verrou commun a été acquis avant chaque banc et libéré dans les chemins de
succès comme d’échec. Les heures UTC, commandes QEMU, artefacts initiaux, durées et
références se trouvent dans `campagne-consolidee.json` et les 41 fichiers
`*-coordination.json`. Attente cumulée : 0,015298374 s, exclue du budget.
Exécution effective des bancs : 979,482348376 s (16 min 19,482 s).
Aucun conflit de planification ni campagne parallèle incompatible observé.
Le temps hors banc et la borne de mesure finale figurent dans `temps-final.json`.
Les temps hôtes servent à la comptabilité du mandat, pas à décider la validité
monotone des tickets invités.

## Réserves et gouvernance

- Réserve instrumentée AArch64 de 301 cadres maintenue, sans relèvement du seuil.
- Reproductibilité binaire entre environnements toujours réservée ; identité des
  sources, compatibilité et identité SHA-256 des artefacts sont trois affirmations distinctes.
- QEMU de laboratoire : ni autorité ni gardien de clés de production qualifié.
- Pas de nouvelle recette matérielle/UTM, ni de répétition générale du rollback VFS.
- Pas de nouvelle capacité, primitive, règle d’authentification, durée de bail ou
  modification P9. Les observations ne constituent pas une preuve exhaustive des courses.
- Aucune restriction Daybreak concernant l’exécution des recettes. Le refus initial d’archivage et sa résolution par inventaire explicite sont décrits ci-dessous.
- Aucune fusion, aucun tag, aucune activation ordinaire ; utilitaires toujours en pause.

## Commits et preuves

Les 310 fichiers de la campagne de référence ont été revérifiés. Manifeste antérieur :
`638396286251ae1c210cecc0f0fa1f8c21b7b85e83d46a03aa438d4207e0f349`.
Archive antérieure : `a61acbbde07257befc4199154587648bd4aa04642e4ce1d421df04d16f4afa15`.
Le nouveau manifeste conserve les succès et les échecs. Une recherche des secrets
connus (octets bruts, hexadécimal, base64 et marqueurs PEM) précède le scellement ;
elle ne prétend pas détecter universellement toute transformation de secret.

```text
2cd07474763b6715a554c1a8788dc160984c51a7 NovaOS v0.24.0 — R2 : synchronisation S9 sur statut atomique
73029bfb19a7e570e5740750eeb91ee3d828a423 NovaOS v0.24.0 — R2 : témoin de références Alice sans délégation
d8f7abc17232511a7cb7e221e2f1f4c103d780ba NovaOS v0.24.0 — R2 : séquence commune de la sonde de séparation
d3b151f118656579de41fd318aab3632802f9878 NovaOS v0.24.0 — R2 : recettes des tickets expirés et consommés
2615e51f10d34d21ae3fae7c4c9233ea4c24ff40 NovaOS v0.24.0 — R2 : réutiliser le drainage du profil EXEC_TICKET
ade87e09bc58922d8e74bdb3cc1703631707b13c NovaOS v0.24.0 — R2 : vérifier le témoin concurrent EXEC_TICKET existant
019071eaf9c60eb884c4703eae58b04c62409f6e NovaOS v0.24.0 — R2 : isoler les parcours de réconciliation
066e559cc7894c7461f191c3400807b750056653 NovaOS v0.24.0 — R2 : deux requêtes concurrentes contre le dernier administrateur
9851ee33b1ed2ecb78aecbc6b0b343bb90878878 NovaOS v0.24.0 — R2 : course entre admission et révocation
63c3c70401d11a6a702dda9713ea506d418928e7 NovaOS v0.24.0 — R2 : borner le drainage du témoin Bob
7ca56d3ea452f40d9918deb5917d5d697402fb47 NovaOS v0.24.0 — R2 : restituer Bob avant le témoin du dernier administrateur
6b6f21219fd90cfd2288fdd46a65fb6e3e31c4fc NovaOS v0.24.0 — R2 : corréler la course avec un ELF sans canal administratif
e5df59408eee041314cdfe3f5b52af2918298035 NovaOS v0.24.0 — R2 : tester la corrélation du rendez-vous concurrent
6c93b419c209aec72709ce404a80af580275ceee NovaOS v0.24.0 — R2 : publication concurrente au refus du dernier administrateur
55838a0863c205087a136fd99ab76fe2d467dd6a NovaOS v0.24.0 — R2 : conserver l’installation de préparation dans la confirmation concurrente
```

Archive durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/finalisation-securite-6693267`.

La matrice publiée est dans `validation/novaauth-r2-securite-finalisation-20261122.json`.

Borne de comptabilité avant scellement/publication : 2026-11-22T20:23:41.496470+00:00 UTC. Travail actif : 4116.481 s ; diagnostic/développement/vérifications hors banc : 3136.999 s. Les temps de scellement et de publication sont ajoutés dans le reçu `finalisation-securite-6693267-publication.json`, voisin de l’archive.

Contrôle de publication : la première demande d’archivage récursif a été refusée par la revue automatique, faute d’exclusion explicite des noyaux/ISO privés. Aucun fichier n’avait été archivé. La méthode retenue emploie une liste de fichiers autorisés par type et nom, une exclusion explicite des formats ELF/ISO/clés, puis une vérification de chaque membre ; aucun artefact privé n’est ajouté.

### Scellement vérifié

- Fichiers de preuve : 1897 ; archive : 1898 fichiers avec le manifeste.
- Manifeste SHA-256 : `d697c9ff155932a6acbc692ee8ec17e8279db83a5f5242bacab086f684924883`.
- Archive SHA-256 : `5b6f9dc0140ce3bd7d862ea6329893ce1b2e84d05c2b2e79723e4586a75be5a5`.
- Archive : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/finalisation-securite-6693267.tar.gz` (17017009 octets).
- Chaque fichier a été relu, puis chaque membre de l’archive décompressé et comparé au manifeste.
