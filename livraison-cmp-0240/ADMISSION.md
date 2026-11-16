# Proposition à la MOA/MOE — admission de cmp

La commande ordinaire `cmp a.bin b.bin` rend 127 sur les quatre profils P9. La grammaire de `nsh` refuse `cat a.bin | cmp -s b.bin -` avant le lancement de ses deux membres. Les sorties et les empreintes figurent dans les archives de qualification.

## Décision demandée

Attribuer en coordination avec les autres utilitaires un indice ELF, une identité d’installation et un rôle de lecteur à `cmp`. Le témoin SDK conserve l’indice de qualification existant, réservé aux essais sans canaux métier. Les branches `uniq`, `sort` et `sha256sum` utilisent ou réservent déjà les valeurs de pipe `PIPE_OPEN + 4` à `+ 6`. Aucun numéro commun n’est attribué sur cette branche.

Le chemin deux fichiers peut reprendre la sélection `SELECT_FILE` pour les deux opérandes, dans l’ordre, puis une délégation `GROUP`. Le lanceur doit analyser les valeurs numériques, les formes accolées et `--` avec le parseur du composant avant toute résolution de nom. L’enfant reçoit les handles numérotés à partir de 1, dans l’ordre des seuls opérandes fichier. Un refus de préparation entre dans le statut 2 de la commande.

Le lecteur de pipe a également besoin du fichier explicitement nommé. Aujourd’hui `spawn_program` exige des canaux fichier nuls pour un lecteur. Le producteur `cat` utilise déjà sa propre délégation de lecture. Le service de fichiers doit distinguer leurs deux délégations simultanées et leurs révocations. L’arbitrage couvre ce cumul de droits dans deux processus isolés, les deux attentes et la restitution du groupe et du pipe. Les droits des lecteurs `wc` et `head` gardent leur profil actuel.

## Preuve de recette jointe

Le correctif séparé `admission-recette.patch.gz` s’applique dans une extraction jetable avec `recette.sh`. Son catalogue privé accepte uniquement l’ELF livré pour l’architecture choisie, sous l’identité de recette `0x434d505f4c414201` à l’indice 240. Le profil cli-test possède le droit QUALIFY. L’admission exige stdout, stderr et un groupe de fichiers en lecture. Les 202 comparaisons par profil comprennent `cmp a.bin b.bin` et le témoin de 256 Kio. Les journaux corrèlent les octets avant rendu à la sortie effective du PID. Le correctif se limite à l’essai à deux fichiers. Les rôles de pipe relèvent toujours de la décision commune.

## Critères de reprise

Rejouer la matrice sur les quatre profils à travers les ELF de production. Capturer les flux avant rendu, identifier les deux PID et leurs statuts, injecter le retrait de chaque délégation pendant une lecture, puis contrôler le drainage et la collecte. Exercer également stdin fourni au premier opérande et le second omis. Pour l’aide et la version lancées directement en ELF, vérifier les canaux nuls, stdin intact et les compteurs de mutations P9. Mesurer les paires P9 par taille et position dès l’admission du chemin effectif.

[Liste des images et droits](../novaOS/abi/installation.rs) · [Admission des lecteurs](../novaOS/kernel/src/sched/process/ipc/spawn.rs) · [Sélection des fichiers](../novaOS/user/novafs/files/selected.rs) · [Exécution des pipes](../components/shell/executor.rs)
