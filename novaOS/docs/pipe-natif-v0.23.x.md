# Premier pipe natif de nsh · NovaOS v0.23.x

## Révisions du 16 novembre 2026

| Objet | Commit | Vérification |
|---|---|---|
| Planification ARC-027 validée par la MOA | `89d5924f97b2dfc35806c697f439f6503c301118` | Référence de la première tranche de pipes et des travaux jusqu’à v0.30.0 |
| Candidat ARC-026 phase 2 | `09b2d96faeb4eb3c98881be99c245975c358abb6` | Six archives comparées aux originaux et aux manifestes SHA-256. Respectivement 17, 24, 17, 33, 76 et 239 fichiers, soit 406 |
| Fusion publiée sur `origin/main` | `3ba6e3e22c72c2e7c5119a559354471c8b25a0ce` | Premier parent `3bd662503b5fc6df0578e814b1e9d39243009dd4`, second parent candidat ARC-026. Arbre identique au candidat, `ff17b7d1612ec1b02c4094c79f9f40121542c2bb` |
| Code du pipe, branche `codex/pipe-natif-023x` | `7152ba6a652f6f6340bd45f65c2eceaa0a6a6636` | Parent direct du code du pipe = fusion ARC-026 ci-dessus. Les anomalies historiques et intermittentes figurent dans le [registre de phase 2](documentation-v0.23.0-phase2.md#régressions-et-réserves) |

## Exécution et droits

Dans une session `nsh` sur P9, `cat fichier | wc -l` lance le véritable ELF `cat` et le véritable ELF `wc`. Le shell leur délègue les extrémités, attend les deux PID et rend le statut de `wc` dans `$?`. Le diagnostic de `cat` passe par son canal d’erreur. Un échec d’admission du lecteur rend l’erreur du lanceur après retrait de l’autorité et collecte du producteur. La qualification observe les deux statuts individuellement et vérifie le refus d’une seconde collecte. [Exécuteur](../../components/shell/executor.rs) · [Collecte](../kernel/src/sched/process/syscall.rs)

- Le contrôleur détenu par le lanceur a le droit `MANAGE`. L’écrivain reçoit `SEND` et sa sélection de fichiers en lecture. Le lecteur reçoit `RECEIVE` et ses sorties terminal. Chaque extrémité refuse la direction inverse, la délégation et l’administration. Le second lancement exige la même racine de pipe et l’identité exacte du producteur associé au droit d’attente. [Admission](../kernel/src/sched/process/ipc/spawn.rs) · [Extrémités](../kernel/src/sched/process/ipc/pipe.rs)
- La file admet huit messages de 128 octets, soit 1 024 octets. Lorsque la file est pleine, l’unique producteur peut suspendre un envoi de 128 octets en rendez-vous avec le lecteur. Les deux types d’envoi respectent le rang d’admission. Chaque attente a une échéance de 15 000 ticks. Le shell admet un pipeline au premier plan. Les quotas IPC existants plafonnent à quatre les extrémités racines par propriétaire et à 64 les objets endpoint pour le système. [Transport et réveils](../kernel/src/sched/process/ipc/traffic.rs) · [Tables et quotas](../kernel/src/sched/process/ipc/mod.rs)
- `cat` lit par tranches de 704 octets et transmet les octets originaux, y compris NUL, ESC et les octets UTF-8 invalides. Sa sortie console ordinaire conserve l’échappement. `wc -l` compte les octets LF, y compris lorsqu’une ligne traverse plusieurs trames. La fermeture du producteur rend EOF après les données admises. Une fermeture du lecteur réveille l’écrivain suspendu avec `CLOSED`. La révocation retire les deux capacités et réveille les attentes. [Producteur](../../components/coreutils/text.rs) · [Lecteur](../../components/coreutils/wc.rs)
- Le contrat binaire reste en v1 avec 48 sélecteurs. L’extension utilise trois valeurs de programme du sélecteur `SPAWN` pour créer le contrôleur et lancer les deux rôles. Les deux lancements de flux ajoutent la clé de contrôle en fin de descripteur. Les anciens numéros, descripteurs, structures IPC et appels de collecte gardent leur contrat. Le diff du moteur NovaFS, du registre `syscalls.toml` et des en-têtes générés est vide. [Contrat de lancement](../abi/terminal.rs) · [Registre](registre-abi.md)

## Qualification

Quatre configurations sous QEMU direct 11.1.3, chacune avec deux vCPU, 256 Mio et un disque VirtIO brut jetable. Chaque configuration démarre trois sessions sur le même volume P9. La première exécute `wc -l court` sans pipe comme témoin de ressources. La deuxième exerce les commandes de l’utilisateur. La troisième observe les PID, les transferts et les fermetures avec les sondes. Les essais du pipe portent sur les deux architectures en invité. Les contrôles du catalogue documentaire tournent aussi en Darwin ARM64 natif et x86_64 sous Rosetta. [Banc reproductible](../tests/pipe_natif.py)

| Architecture | Secteur logique | Observations finales | Cadres libres après témoin / après pipes | Pic de cadres mobilisés témoin / pipes |
|---|---:|---:|---:|---:|
| AArch64 | 512 | 40 | 61 875 / 61 875 | 1 152 / 1 292 |
| AArch64 | 4 096 | 40 | 61 875 / 61 875 | 1 152 / 1 292 |
| x86_64 | 512 | 40 | 62 417 / 62 417 | 1 154 / 1 294 |
| x86_64 | 4 096 | 40 | 62 417 / 62 417 | 1 154 / 1 294 |

Les pics concernent la session entière. Dans les quatre campagnes, la différence de pic est de 140 cadres de 4 Kio. En fin de collecte, le noyau relève zéro page IPC, zéro page utilisateur et zéro processus. Le compteur DMA final vaut 0 sur ARM et 9 sur x86, exactement comme pour le témoin sans pipe de chaque profil. Ces valeurs n’attribuent aucune page DMA au transport.

| Cas | Résultat observé dans les quatre configurations |
|---|---|
| `A\nB\nC`, fichier vide, binaire de 16 389 octets | 2, 0 et 65 LF. Statuts `cat/wc/nsh` = 0/0/0. Même sortie et mêmes statuts avec GNU coreutils 9.12. Les trois fichiers sont relus et comparés octet par octet hors invité après vérification du volume. |
| Saturation contrôlée | Le banc attend la trace du producteur réellement suspendu avant d’admettre le lecteur. Huit dépôts occupés, puis livraison des 16 389 octets. Empreinte d’ordre FNV-1a au point de livraison `0a870eae5bd6ce01`. Les empreintes d’archive et de relecture utilisent SHA-256. |
| Fermeture anticipée | Le lecteur réel refuse un argument injecté dans le profil de test et quitte avant sa première lecture, alors que le producteur est bloqué. Statuts 11/3/3. |
| Admission du lecteur refusée | La clé invalide échoue avec 5. Aucun PID lecteur créé. Le producteur suspendu sort avec 10 et est collecté. |
| Révocation pendant l’attente | Le producteur suspendu reçoit 10 (`REVOKED`), le lecteur 5 (`DENIED`) lors de sa tentative de lecture, le shell 5. Le pipeline suivant réussit. |
| Source absente | Diagnostic de `cat`, puis zéro LF et statuts 20/0/0. |
| Droits et collecte | PID distincts pour les deux ELF, sondes sur les droits dans leurs tables locales, collecte unique. Après fermeture du contrôleur, zéro nœud appartenant au pipe, zéro attente pendante et file libérée. |
| Langage et documentation | Pipe en mode interactif et via `nsh -c`, barre verticale entre guillemets, court-circuit, refus de `wc -c` comme lecteur de flux. Un script avec `touch interdit; echo a | cat` est refusé avant de créer le fichier. L’aide publique, `false` (1), `false --help` (0), `exit --help` (0) et la continuité de session ont été exercées. |

### Non-régression et limites

- Neuf groupes du registre ABI réussis. Les quatre ELF historiques v0.20.0 ont été relus, comparés à leur SHA-256 de référence et exécutés sans recompilation sur le profil noyau `thread-probe` du candidat. Les deux exemples SDK ont terminé avec 0. Les deux témoins de threads ont restitué les cadres observés. Les cinq tests du catalogue ont réussi sur chaque architecture Darwin.
- Les deux invités ont passé `elf-test`, `lifecycle-test` (64 familles, 128 processus) et les suites `ipc-test` et `ipc-races` (10 000 répétitions et cinq ordres pour la course IPC). La relecture de chaque P9 utilise `inspect`, `verify` et `salvage`. Les campagnes longues du moteur NovaFS n’ont pas été rejouées dans ce mandat.
- `nsh-scripts-test` s’arrête sur les deux architectures en P9/512 au glob récursif depuis la racine avec « capacité révoquée ». Cette assertion échouait déjà sur la base ARC-026. Les assertions suivantes de cette suite n’ont pas été exécutées. Les réserves concernant `vi.py`, `tmp_prive_cli.py`, `ls -latr --color /` et le refus intermittent de `ls src/` sur x86/P9/4096 restent ouvertes. [Anomalies et preuves antérieures](documentation-v0.23.0-phase2.md#régressions-et-réserves)
- Deux attentes de mesure ont été corrigées dans le banc. La première supposait que la file se remplirait spontanément. La seconde supposait que le compteur DMA global reviendrait à zéro sur x86. Le banc synchronise la saturation et compare désormais les ressources à un témoin indépendant du pipe. Les essais exploratoires et leurs traces ont été conservés.
- La syntaxe qualifiée exige les noms de commande littéraux `cat` et `wc` et le lecteur `-l`. Les chaînes à trois processus, le contrôle des jobs, les redirections et `pipefail` restent hors de cette tranche. Le cas de fermeture anticipée couvre un lecteur quittant avant consommation. La campagne porte sur QEMU direct. L’image UTM distribuée reste en v0.17.0.

## Reproduire et vérifier les preuves

Depuis la racine du worktree, avec le `TMPDIR` défini par `AGENTS.md`, fournir un répertoire de sortie neuf et durable pour chaque couple architecture/secteur. Sur l’hôte de référence, GNU cat et wc sont disponibles sous les noms `gcat` et `gwc`. Le banc accepte aussi les noms `cat` et `wc` lorsque leur version annonce GNU coreutils.

```sh
arch=aarch64
secteur=512
sortie="../preuves-pipe-rejeu/$arch-$secteur"
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
make -C novaOS ARCH="$arch" kernel
python3 -B novaOS/tests/pipe_natif.py "$arch" "$sortie" --sector "$secteur"
python3 -B novaOS/tests/pipe_natif.py "$arch" "$sortie" --oracle-gnu
```

Les originaux sont sous `OS-worktrees/preuves-pipe-023x/` et la copie vérifiée sous `OS-worktrees/archives-pipe-023x/`, hors des worktrees Git et du dossier temporaire. Les 172 membres de `pipe-natif.tar.gz` ont été relus et comparés individuellement au manifeste et aux originaux. L’archive conserve ISO, volumes, configurations, sorties série, résultats GNU, ELF historiques et refus. `references.json` rattache les binaires testés au commit de code. Les fiches VM prises pendant le développement indiquent la révision parente et le SHA-256 effectif du noyau.

| Fichier | SHA-256 |
|---|---|
| `pipe-natif.tar.gz` | `bb7d29e19b76275bfb1c13c90dcea2d95601df44e1c8edc258edd1d6d76b86f2` |
| `fichiers.sha256` | `fca404c9b225a6182cdbffa6433fc3898f4a5816bf2b9ec43964a65816a82785` |
