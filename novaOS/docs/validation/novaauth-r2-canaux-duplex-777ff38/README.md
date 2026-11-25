# NovaAuth R2 — E2, canaux privés de l’enfant H

Référence `777ff386446d70f38c17752275a6fbdd73a98377`. Reconstruire l’export E3 selon le dossier voisin `novaauth-r2-exec-g-duplex-20261125`, puis appliquer dans cet export la décompression de `novaOS/user/novaauth/service/duplex_fichiers_qemu.patch.gz` par `git apply`. Les empreintes avant/après de chaque source sont dans `sources.json`. Ce delta est spécifique au banc de qualification. Les deux ISO utilisent le profil vérifié `novaauth-sessions`.

## Parcours exécuté

Dans chaque essai, le ticket H de Bob est inscrit en position 12. Le retrait de G est réservé en 13, publié en 14 et confirmé en 18 avec trois acquittements distincts (1, 2 et 4). La porte EXEC du ticket H est ensuite admise en 19 et l’instance de l’enfant, PID 23, confirmée en 20. Les quatre opérations VFS de cet enfant ont chacune une admission et un terminal distincts entre 21 et 28. Les positions sont propres à chaque base de qualification. Le collecteur privé vérifie le propriétaire, les deux flux, leur fermeture et les octets de stdout. Les quatre essais rendent 14 octets identiques et aucun octet de stderr, avec statut natif 0.

| Profil | Fixture | Marge du ticket à sa présentation (ticks invités) | Pic observé de cadres IPC | Minimum observé de cadres libres |
|---|---|---:|---:|---:|
| x86_64/P9-512 | Compacte, identité de l’ELF de confirmation distincte | 809 | 74 | 59 025 |
| x86_64/P9-4096 | Complète | 805 | 74 | 59 025 |
| AArch64/P9-512 | Compacte usermod | 234 | 74 | 58 512 |
| AArch64/P9-4096 | Complète | 848 | 74 | 58 512 |

Chaque fichier `ancien-h-*.json` contient les identifiants des opérations du banc, les preuves d’issue VFS, les mesures par phase et les empreintes des sources brutes conservées localement. Les mesures invitées portent sur les phases instrumentées. Les deux racines de sorties et les deux racines fichier s’installent successivement. Maximum observé de quatre racines par acteur, de 64 endpoints globaux et de 74 cadres IPC. L’enfant et son shell rendent toutes leurs ressources. Le banc observe en fin de parcours zéro endpoint, nœud, espace et cadre IPC. Aucun des maxima observés ne borne un instant non instrumenté.

## Contrôle des tailles et corrections

Les déclarations compilées pour chaque cible donnent 32 octets pour le pool de deux voies exclusives et leurs phases, 32 pour chaque fichier ouvert et 272 pour l’échange en cours. Le sous-total statique est de 4 090 octets sur 4 096. Les mesures ont précédé le démarrage fonctionnel. Le journal des deux tests unitaires du pool couvre la saturation et les acquittements tardifs, y compris sur une réservation partielle.

L’essai initial saturait les 64 endpoints pendant l’attachement de la quatrième racine. Le banc attend désormais la collecte et la fermeture effective du ticket d’Alice. La boucle privée NovaFS tenait pour inactive une itération qui venait de servir un port privé. Elle conserve l’attente bloquante pour une itération inactive et tient compte du travail réellement effectué. Les résultats dépendants de l’ancienne boucle sont archivés séparément. Le précédent oracle série ne pouvait pas observer les octets de Bob. Le contrôle utilise les trames du collecteur privé indépendant, avec la même chaîne témoin attendue et le même statut natif.

## Réserves

La consultation de l’admission H historique par son identifiant original, dans les deux journaux conservés, donne encore `Pending` sans enfant ni terminal. Cette opération conserve l’issue `UNCERTAIN` et la quarantaine. Les admissions des quatre nouveaux essais ont leurs propres identifiants.

La recette E2 a réussi dans les fixtures ci-dessus. La matrice des courses E7/E8/E10 nécessite des preuves dédiées. La saturation du pool en concurrence entre deux véritables enfants et l’isolation dans ces entrelacements restent à démontrer. Les réserves historiques couvrent les 301 cadres instrumentés AArch64, la reproductibilité entre environnements et la fixture complète AArch64/P9-512. Les mesures des essais Direct précédents restent propres à leurs fixtures.
