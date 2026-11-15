# Consulter le journal des services avec nsh

En session P9 ordinaire, `journal show self` et `journal tail self` consultent les événements émis par l’instance courante de `nsh`. Une instance neuve peut obtenir une liste vide. Le service vérifie l’identité fournie par le canal privé et la compare à l’identité complète de l’émetteur de chaque événement. Un identifiant de service passé sur la ligne de commande sélectionne un filtre dans le périmètre de lecture autorisé.

## Ouverture d’une session de maintenance

Le profil explicite `journal-admin` du banc QEMU accorde à cette session les droits de lecture système, d’émission, de rotation, de rétention et de purge, ainsi que l’administration des services 9 et 10. La commande suivante crée une image de démonstration au premier lancement. Les lancements suivants utilisent la même image. L’outil hôte NovaFS et le noyau correspondant doivent être construits au préalable. [Procédure du banc CLI](../scripts/session_cli.py)

```sh
NOVA_ARCH=aarch64 python3 -B novaOS/scripts/session_cli.py demo ./demo-journal.raw --admin-journal
```

La commande `service status 16` interroge le gestionnaire et expose ses compteurs d’événements perdus, d’issues incertaines et sa dernière erreur de publication, y compris si le journal est arrêté.

Dans `nsh`, les premières commandes d’administration peuvent être les suivantes.

```sh
journal status
journal list
journal boot --limit 64
journal show storage-info
journal show cache --level WARN
journal tail 9 --limit 3 --follow
```

| Commande | Résultat |
| --- | --- |
| `journal status` | BootID courant, intervalle des séquences conservées, occupation et quota en octets d’événements, nombre de segments, rétention et révision du journal. Un canal de mutation sans lecture système reçoit seulement la révision. |
| `journal list` | Nombre d’événements conservés par identifiant de service, par ordre d’identifiant. La lecture prend pour borne la prochaine séquence observée au début de la commande. |
| `journal boot` | Événements du démarrage courant, producteurs et gestionnaire compris. `--boot` permet de sélectionner un démarrage conservé. |
| `journal show <service\|self>` | Événements dans l’ordre chronologique, 32 au maximum par défaut. Le service peut être nommé ou numéroté. |
| `journal tail <service\|self>` | Les 10 derniers événements correspondants, affichés dans l’ordre chronologique. `--follow` prolonge la consultation. |
| `journal emit <0..255>` | Publie une notice numérique rattachée à l’instance émettrice. Le succès restitue son identifiant d’événement local. |
| `journal rotate` | Demande la rotation du segment courant. |
| `journal retain <1..8>` | Enregistre le nombre maximal de segments retenus. Une réduction peut retirer immédiatement des événements anciens. |
| `journal purge` | Demande la purge de l’historique. Elle exige une capacité distincte. |

Les options de lecture `--level`, `--boot` et `--limit` se combinent. `--level` fixe une gravité minimale parmi `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR` et `CRITICAL`. `--boot` reçoit les 32 caractères hexadécimaux du BootID. `--limit` accepte de 1 à 128 résultats. `journal boot --service 9` combine les filtres de démarrage et de service. Les limites portent sur le nombre d’événements correspondant aux filtres. L’aide est accessible avec `journal --help`.

Une ligne contient la séquence, l’identifiant d’événement de l’émetteur, le BootID, le service concerné, l’instance et la génération du sujet puis celles de l’émetteur. Le même BootID complète ces deux identités. Le champ `source` distingue une décision attestée par le gestionnaire, une notification d’un producteur et un événement interne du journal. `état` détaille les transitions. `horloge=monotone` exprime un temps en millisecondes relatif au démarrage. `horloge=inconnue` indique l’absence de temps qualifié. Chaque champ affiché provient d’un entier ou d’un libellé du protocole. Le filtre `self` expose uniquement la séquence propre à l’instance.

`--follow` réarme une attente IPC après réponse ou expiration de son délai. Les touches `q`, Échap et Ctrl+C mettent fin au suivi. En cas de curseur système expiré par rotation, la commande signale la perte de continuité. Une nouvelle consultation part de l’historique réellement conservé.

## Décisions administratives et reprise

Pour diagnostiquer la désactivation de `storage-info`, relever le BootID avec `journal status`, exécuter `service disable 9`, puis redémarrer le même volume. Le nouveau `journal status` donne le deuxième BootID. `journal show cache` permet d’identifier l’attente de la dépendance 9. Après `service enable 9` et un nouveau redémarrage, rechercher les transitions `READY` du troisième BootID et relire chaque historique avec `journal boot --boot <BootID>`. La rétention limite la durée de ce diagnostic.

Une action administrative sur les segments porte la révision que la commande vient de lire. Un refus pour révision périmée exige de relire l’état. Quand la réponse à une mutation se perd après remise au service, `nsh` renvoie `UNCERTAIN`. L’opérateur peut consulter `journal status`, `journal show self` ou l’historique pertinent avant de décider d’une suite. La commande n’envoie chaque mutation qu’une fois. Une lecture en fin d’historique se termine avec le code 0. Un accès refusé renvoie 5. Les scripts accèdent au code natif via `$?`.

## Rejouer le diagnostic

Depuis la racine du worktree qualifié, préparer `TMPDIR` conformément à `AGENTS.md`, construire l’outil hôte NovaFS et le noyau pour l’architecture choisie. Passer ensuite un **nouveau** répertoire de preuves durable au banc.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
make -C novaOS kernel ARCH=aarch64
python3 -B novaOS/tests/journalisation.py aarch64 2 4096 "../preuves-journal/exemple-aarch64-2-4096" --rotation
```

Le banc démarre les services du catalogue réel, puis accède au journal par les commandes natives. Il conserve la sortie série, le média de démarrage, la configuration QEMU, une copie compressée de l’image et un bilan. La relecture de l’image utilise l’outil NovaFS hôte. Le scénario de rétention et de purge utilise une copie indépendante du disque de diagnostic. L’argument d’architecture accepte aussi `x86_64`, avec les tailles de secteur 512 et 4096 et 1, 2 ou 4 vCPU. Les temps hôtes comprennent l’entrée série, le traitement des commandes et leur sortie. [Banc de qualification](../tests/journalisation.py)
