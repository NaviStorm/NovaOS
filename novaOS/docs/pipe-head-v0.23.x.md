# NovaOS v0.23.x · Première ligne d’un pipe natif

Le 16 novembre 2026, le scénario `cat fichier | head -n 1` a été qualifié sur la branche `codex/pipe-head-023x`, commit de code `57ce4ae04817c3122cfcafc465af5fe5d6c2fdc6`. Le contrat accepté est son parent direct `402d3e1c46d2d5e2770ea4dd23828778b38993b9`. [Contrat de flux ARC-027](contrat-flux-arc027-v0.23.x.md)

## Exécution

`nsh` lance les ELF `cat` et `head`. Le premier lit les fichiers sélectionnés et écrit les octets bruts sur son extrémité SEND. Le second détient RECEIVE, la sortie et la sortie d’erreur. Le lanceur vérifie l’absence de canaux fichier chez le lecteur. Le shell garde le contrôleur et les deux droits d’attente. Il collecte les deux processus et rend le statut de `head` dans `$?`. Le banc vérifie que le noyau refuse une seconde collecte de chacun. [Lanceur](../kernel/src/sched/process/ipc/spawn.rs) · [Exécuteur](../../components/shell/executor.rs)

Le transport admet huit trames de 128 octets, puis au plus une offre suspendue du producteur. Une lecture pleine de `cat` couvre cinq trames entières. En cas d’erreur, la bibliothèque distingue le préfixe admis et le code de l’envoi refusé. Le producteur termine sans répéter ce préfixe. Le lecteur commun à `wc` et `head` distingue données, EOF et erreur. `head` ferme son extrémité dès qu’il repère le premier LF et affiche jusqu’à cet octet inclus. Le rendu progressif conserve au plus trois octets d’un scalaire UTF-8 incomplet entre deux trames. À EOF, une ligne incomplète garde sa fin d’origine. [Bibliothèque de flux](../../components/libraries/process.rs) · [ELF head](../../components/coreutils/head.rs) · [Décodage du texte](../../components/coreutils/text.rs)

## Résultats dans les quatre invités

Chaque profil QEMU 11.1.3 utilise 2 vCPU, 256 Mio et un disque VirtIO brut jetable. Les sessions ordinaires et instrumentées utilisent le même noyau. Chaque profil a exécuté 41 commandes du nouveau banc et les 40 observations du banc historique `cat | wc -l`. Les 11 fichiers témoins du nouveau banc ont été relus hors invité avec `inspect`, `verify` et `salvage`, puis comparés octet par octet aux originaux. Les lancements ordinaires continuent à créer et retirer les temporaires privés des processus. [Banc du lecteur](../tests/pipe_head.py) · [Banc du compteur](../tests/pipe_natif.py)

| Invité | Secteur P9 | Cat suspendu, trois passages | Petit fichier acquitté | Cadres libres après drainage | Pages DMA au repos | Pic de cadres, session ligne longue / témoin |
|---|---:|---|---|---:|---:|---:|
| AArch64 | 512 | 11 / 0 | 0 / 0 | 61 876 | 0 | 1 273 / 1 053 |
| AArch64 | 4 096 | 11 / 0 | 0 / 0 | 61 876 | 0 | 1 289 / 1 053 |
| x86_64 | 512 | 11 / 0 | 0 / 0 | 62 419 | 9 | 1 292 / 767 |
| x86_64 | 4 096 | 11 / 0 | 0 / 0 | 62 419 | 9 | 1 292 / 1 056 |

Les couples donnent les codes de `cat` puis de `head`. Les pics portent sur toute la session de la VM en cadres de 4 Kio. Le témoin exécute `pwd` sans pipe. Après collecte, les compteurs IPC, pages utilisateur et processus sont à zéro. Les cadres libres et les pages DMA reviennent exactement aux valeurs du témoin du même profil. Le noyau mesure également zéro nœud, zéro attente et zéro file active pour l’objet retiré.

### Fermeture, révocation et erreurs

| Essai | Observation sur les quatre profils | Cat / head / shell |
|---|---|---|
| 64 Kio, envoi suspendu | Huit dépôts totalisant 1 024 octets. Première réception de 128 octets. Fermeture du lecteur avec sept dépôts restants. Le noyau termine l’envoi suspendu avec CLOSED et réveille le PID de `cat`. Le lecteur affiche uniquement les neuf octets de `premiere\n` | 11 / 0 / 0 |
| Même fichier, ordonnancement libre | Une seule ligne et une seule trame reçue. Le moment précis de la fermeture par rapport au prochain SEND peut varier | 11 / 0 / 0 |
| Fichier de dix octets `A\nSECONDE\n` | Fermeture du producteur avant admission du lecteur. Le lecteur affiche seulement `A\n`, puis ferme | 0 / 0 / 0 |
| Lecteur déjà bloqué sur file vide | Envoi du petit fichier ou fin du fichier vide. Le premier événement livre les données, le second livre EOF | 0 / 0 / 0 |
| Retrait entre admission et première lecture | Envoi suspendu retiré, première lecture sur une clé retirée refusée, aucun octet remis | 10 / 5 / 5 |
| Retrait pendant RECEIVE | Le lecteur endormi reçoit REVOKED. Le premier envoi du producteur sur sa clé retirée reçoit DENIED | 5 / 10 / 10 |
| Source absente | Diagnostic du producteur, EOF chez le lecteur | 20 / 0 / 0 |
| Refus du second lancement | Contrôleur invalide refusé, retrait du producteur bloqué, une seule collecte | 10 / aucun enfant / 5 |
| Refus de sortie terminal | Le banc retire OUTPUT au lecteur. Son écriture console rend DENIED | 11 / 5 / 5 |

À titre d’exemple, dans la première répétition de chaque profil les PID de `cat` et `head` étaient 13 et 16. Les traces enregistrent l’admission du lecteur sans canal fichier, avec le masque terminal 6, ainsi que la collecte des deux PID. Le banc synchronise les ordres imposés sur des événements observés. Pour endormir le lecteur il retient `cat` avant sa première requête fichier, puis envoie un octet de contrôle via le canal de réponse déjà délégué. Le shell n’achemine aucune donnée du fichier. L’injection de révocation avant la première lecture exige le contrôleur du pipe et le profil de qualification.

### Fidélité des octets et compatibilité

- L’oracle GNU `head` 9.12 calcule la première ligne de chaque témoin. Le banc compare exactement le rendu série attendu après conversion CRLF et retrait des diagnostics identifiés. Il couvre LF initial, LF aux offsets 127 et 128, ligne de 5 000 octets suivie de LF, fichier vide, absence de LF, scalaire UTF-8 scindé, séquence tronquée, NUL, ESC et octet invalide. Le marqueur `SECONDE` se trouve dans la première trame du fichier de 64 Kio. Il n’atteint pas l’affichage.
- Les sessions éprouvent aussi `head -n 1 fichier`, `nsh -c`, les consultations ARC-026 en français et en anglais et la continuation de session. Le scénario historique `cat | wc -l` garde son oracle GNU et ses contrôles de FIFO et de restitution.
- Les invités ont réussi les suites IPC, courses IPC (10 000 répétitions, cinq ordres), chargement ELF et cycle de vie. Le sous-banc IPC exerce aussi la clé étrangère, le sens inverse, la délégation interdite, la fermeture avant un nouvel envoi, l’échéance, l’annulation et le quota de contrôleurs. [Banc IPC](../kernel/src/sched/process/ipc/probes/pipe.rs)
- Les neuf groupes du registre ABI ont réussi. Les 48 sélecteurs, leurs définitions générées et les structures natives correspondent à la base acceptée. L’admission de `head` utilise le sélecteur SPAWN existant. Les quatre ELF v0.20.0 archivés ont été contrôlés par SHA-256 et exécutés sans recompilation sur les noyaux du profil `thread-probe`. Les deux exemples SDK ont terminé avec 0 et les deux témoins de threads ont restitué leurs cadres. Le catalogue documentaire a passé ses cinq tests sur Darwin ARM64 natif et x86_64 sous Rosetta.
- La suite `nsh-scripts-test` s’arrête sur les deux architectures en P9/512 au glob récursif `**/*.txt` depuis la racine avec « capacité révoquée ». Le même échec est documenté sur le parent. La suite n’atteint pas ses assertions ultérieures. Les campagnes complètes du moteur NovaFS et UTM n’ont pas été exécutées pour ce changement de flux. [Réserves du premier pipe](pipe-natif-v0.23.x.md#non-régression-et-limites)

## Reproduction et preuves

Depuis le worktree dédié, appliquer les variables `NOVAOS_TMPDIR` et `TMPDIR` du dépôt. Le banc requiert GNU `head` et un nouveau répertoire durable par exécution. [Politique des preuves](../../AGENTS.md#worktrees-additionnels-et-preuves)

```sh
arch=aarch64
secteur=512
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
make -C novaOS ARCH="$arch" kernel
python3 -B novaOS/tests/pipe_head.py "$arch" "$secteur" "../rejeu-pipe-head/$arch-$secteur"
```

L’archive publiée contient les traces UART, les fiches QEMU, les résultats et oracles, les ISO, les volumes jetables, les noyaux ordinaires et les quatre ELF historiques. Le manifeste interne couvre chaque membre de preuve. Les originaux sont sous `OS-worktrees/preuves-pipe-head-023x/` et l’archive indépendante de toute la campagne sous `OS-worktrees/archives-pipe-head-023x/`. [Preuves publiées](validation/pipe-head-v0.23.x.tar.xz) · [SHA-256 de l’archive](validation/pipe-head-v0.23.x.sha256)

| Archive relue | Fichiers de preuve | SHA-256 de l’archive |
|---|---:|---|
| `pipe-head-v0.23.x.tar.xz`, publiée | 170 | `fd5fe14a4c17bd00fd47562a099a499e7dabfc95446b3513a8c1f369e46ea07a` |
| `campagne-complete.tar.gz`, copie locale indépendante | 299 | `581686c36540d3bfffb1c9603a107b03f7278b1ca98537cd88e47f05696ccc7f` |

Les essais de mise au point font partie de l’archive locale complète. Le premier banc incluait dans la sortie de `head` les traces du retrait du `tmp` écrites sur la même UART. Un passage suivant a détecté une lecture de 704 octets demandée pour une fenêtre de 640 octets. La longueur est désormais calculée sur le tampon réel, et le test compare toute la sortie au contenu attendu, y compris pour le fichier vide. Les quatre profils publiés utilisent le commit corrigé.
