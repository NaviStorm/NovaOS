# Tickets de lancement et retrait du tmp privé

Essais du 17 novembre 2026 sur la branche `codex/lancement-generique-elf`. Base du noyau `0bd8d679ea3b1fade9789557f506e9b66be099cd`. QEMU 11.1.3, deux vCPU pour la course et la contention, volume NovaFS P9 jetable. Les fichiers `tickets-validation/<architecture>/ticket-<secteur>/` désignent les passages de référence dans l’archive.

## Deux threads, une admission et une collecte

Le sélecteur reste `0x10042`. La seule exception au refus de SPAWN dans un thread secondaire est la combinaison exacte `(0x0000000200000000, ticket_local, 0)`. Dans chaque invité, les deux threads présentent le même ticket après une barrière. Chacun a d’abord tenté le catalogue et les quatre variantes historiques de pipe, ainsi que la variante ticket avec un troisième argument non nul. Les six demandes ont rendu `DENIED` (5) dans chacun des threads. Le gagnant a ensuite tenté lui-même `PROCESS_WAIT` et obtenu `DENIED`.

| Invité | Secteur P9 | Parent | TID 10 | TID 11 | Enfant retrouvé | Collectes réussies |
|---|---:|---:|---|---|---:|---:|
| AArch64 | 512 | 6 | 12 REFUSED | 0 OK | 7 | 1 |
| AArch64 | 4 096 | 6 | 12 REFUSED | 0 OK | 7 | 1 |
| x86_64 | 512 | 6 | 12 REFUSED | 0 OK | 7 | 1 |
| x86_64 | 4 096 | 6 | 12 REFUSED | 0 OK | 7 | 1 |

Le gagnant termine sans communiquer au thread principal la valeur WAIT reçue. Le thread principal envoie deux requêtes `NXQUERY1` sur le reçu IPC local. Les réponses portent le même PID et le même WAIT. La première collecte donne une sortie normale 0, la seconde renvoie `DENIED`. Une troisième interrogation retrouve le PID 7 et une valeur WAIT nulle. Une nouvelle consommation du ticket publié donne `REFUSED`. Le banc couvre également ticket révoqué (10), expiré (8), émetteur étranger et dernier octet de session divergent.

La décision de la sonde est locale au scénario d’amorçage. Le ticket lie un propriétaire, une instance, une session de 32 octets et une installation à une révision d’objet et au SHA-256 de la copie ELF scellée. L’essai emploie une identité de laboratoire. L’authentification de la session du shell et l’attribution de ses flux par le service réel restent ouvertes. Le test de perte porte sur l’abandon de la réponse par le gagnant, dans le même démarrage. Il ne couvre pas une restauration après coupure.

## Contention réelle et retour des blocs

Le client public prépare une écriture réelle sur `/documents/exemple.txt` et en retient la réservation CoW. Les compteurs du volume passent de 357 à 352 blocs disponibles. Le client contrôle la réservation puis l’abandonne par DISCARD. Le contenu public relu après remontage doit correspondre à l’état initial.

Deux autres instances possèdent chacune un `commun` privé. Le fichier de l’instance retirée contient 32 768 octets vérifiés par motif, celui de l’instance survivante contient `B`. Dans la même section critique que la terminaison, le noyau constate la transition du droit actif au droit inaccessible. Avant drainage, le survivant présente l’ancien droit et le handle par TMP_CALL et reçoit `DENIED`. Sa propre opération reçoit `BUSY` tant que la transaction publique occupe P9. Après drainage, l’ancien handle présenté dans sa vue reçoit `REVOKED` et son propre handle relit `B`.

| Architecture | Secteur | Disponibles avant WRITE | Réservation active | Après drainage, survivant encore vivant | Anciens blocs de données récupérés à la fin |
|---|---:|---:|---:|---:|---:|
| AArch64 | 512 | 357 | 352 | 360 | 8/8 |
| AArch64 | 4 096 | 357 | 352 | 360 | 8/8 |
| x86_64 | 512 | 357 | 352 | 358 | 8/8 |
| x86_64 | 4 096 | 357 | 352 | 360 | 8/8 |

Un oracle monte les images en lecture seule, lance le contrôle d’intégrité P9 et suit les huit adresses physiques du fichier dans les générations successives. `blocs-physiques.txt` distingue les blocs disponibles pour allocation, ceux encore protégés par la génération précédente et ceux réaffectés au VFS. Le décompte global inclut les métadonnées CoW. Les 4 à 16 ms précédemment relevées mesurent le retrait et la collecte du **processus**. Le banc ne fixe aucun délai maximal pour la récupération physique.

Le service réserve désormais la place des retraits différés avant de créer la racine. Auparavant, un échec d’allocation après retrait de l’entrée active pouvait perdre la référence à purger. Le drainage garde la référence jusqu’à la réussite de la purge. Une erreur ambiguë conserve la racine en quarantaine et interdit la répétition automatique.

## Régressions et limites

Les quatre parcours historiques du tmp, les deux pipes dans les quatre configurations avec leurs oracles, les scripts `nsh`, le retrait borné du producteur, les courses IPC et le cycle de vie des processus ont réussi sur les noyaux corrigés. Les quatre ELF v0.20.0 archivés ont été vérifiés par SHA-256 puis exécutés sans recompilation. Les deux variantes de raccord NovaAuth s’appliquent avec vérification de leurs préimages. La campagne n’a pas utilisé UTM ni de machine physique.

Les 48 numéros ABI demeurent identiques. Le premier passage statique a donné 7/9. La fiche du sélecteur SPAWN intègre la nouvelle variante dans les champs historiques que deux contrôles comparent littéralement. Les assertions restent intactes. Le regroupement des deux formes dans la documentation du registre reste à corriger avant qualification intégrée. Le format P9 et le moteur portable sont identiques à la base.

Les premiers essais sont conservés. Une sonde placée dans le démarrage historique avait déplacé ses identifiants attendus. Une vérification initiale estimait à tort les blocs récupérés à partir du seul compteur global. Un passage intermédiaire a relu des données Limine après restitution et provoqué une panique dans la sonde. Le correctif utilise un témoin créé lors de la sélection du scénario. Les quatre premiers appels à l’oracle GNU ont précédé par erreur la création des résultats invités. La campagne de référence a exécuté le banc invité avant l’oracle dans chaque configuration.

Le commit `abe4368ed9fb3b0904ceb6cab271e60f56f8e5b4` contient les clients installables. Les six binaires ont été compilés pour leurs ISA respectives. Sur Darwin, le moteur CUT rend exactement `41 09 43 0a 44 09 46 0a`, stderr vide et statut 0, pour le TSV fourni par fichier ou stdin. Deux tests du décodage passent. Les quatre invocations depuis nsh, la vérification que seul le PID de cat ouvre le TSV dans le pipe, et l’installation à chaud du second ELF pendant le même démarrage restent à exécuter.

## Archives

Le [manifeste SHA-256](validation/lancement-generique-tickets-tmp.sha256) couvre 680 fichiers. Son empreinte est `d592a6bc648f3201d2cf3ed07e218d401808475b83712ace9f460a082a5e66ec`. La copie externe `OS-worktrees/archives-lancement-generique-elf/tickets-drainage-20261117.tar.gz` mesure 264 485 911 octets et porte l’empreinte `4341be0c740ef052e45bebb7455a7b063a1d81586140e316dda08e7ec605388c`. Les 680 membres décompressés ont été comparés individuellement aux originaux, y compris les séries de diagnostic.
