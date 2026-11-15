# Contrat du démon de journalisation v0.22.0, phase 2

## Démarrage et identité

Le gestionnaire supervise un ELF utilisateur distinct, service 16. Le noyau transmet au lancement deux canaux de supervision, une paire vers NovaFS, une paire vers le terminal, le masque d’autorisation du terminal et les identités complètes des deux interlocuteurs. Le démon vérifie son canal initial, son numéro de tentative, son identité native et la cohérence des BootID. Il lit le journal sur P9 et publie son événement `ONLINE` avant d’annoncer `READY`. L’acquittement de cette écriture exerce la chaîne sur le volume effectivement monté. Un échec initial laisse le gestionnaire poursuivre sa boucle. [Entrée du démon](../user/journal_service.rs) · [Raccordement des canaux](../kernel/src/sched/process/ipc/service.rs)

L’identité comporte deux mots de BootID, un numéro local et une génération. Le gestionnaire construit ses événements de cycle de vie avec sa propre identité d’émetteur et l’instance observée comme sujet. Pour une notification native réellement reçue sur le canal d’une instance, le gestionnaire vérifie d’abord l’incarnation, puis relaie un événement de producteur de catégorie `SERVICE`, code `NOTICE`. La trame du terminal ne peut revendiquer aucune de ces identités. L’émission directe du terminal accepte un entier de 0 à 255 et l’associe à l’identité complète du terminal fixée au lancement. Une commande de lecture ne peut pas émettre. [Producteur du gestionnaire](../user/services/journal_publisher.rs) · [Validation des deux canaux](../user/services/journal_access.rs)

## Enregistrement et confirmation

L’enregistrement v1 contient 96 octets, sous forme de douze mots little-endian. L’EID complet d’un producteur regroupe le BootID, le numéro et la génération de son instance et son ordinal. La séquence système est attribuée au moment de la publication. Une seule tâche du démon sérialise émissions, lectures et décisions de rotation. [Schéma partagé](../user/services/journal_wire.rs) · [Décisions du démon](../user/services/journal.rs)

| Mot | Contenu |
| --- | --- |
| 0 | Ordinal local de l’EID, strictement positif |
| 1 | Séquence durable, à partir de 1, continue parmi les enregistrements retenus |
| 2–3 | BootID partagé par l’émetteur et le sujet |
| 4–5 | Numéro local et génération du sujet. Les deux valent zéro si l’instance n’a pas encore été créée |
| 6–7 | Numéro local et génération de l’émetteur authentifié |
| 8 | Bits 0–7 version, 8–15 gravité, 16–19 catégorie, 20–23 qualité du temps, 24–31 origine, 32–47 code et 48–63 numéro stable du service |
| 9 | Millisecondes depuis l’origine monotone du démarrage ou zéro lorsque l’heure est inconnue |
| 10–11 | Données structurées propres au code |

Les six niveaux sont `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR` et `CRITICAL`. Le BootID sépare les horloges de démarrages différents. Un événement enregistré tôt peut être transféré après `ONLINE` tout en conservant son heure d’observation. Les notices du terminal portent une horloge inconnue. Les entrées/descripteurs de fichiers, secrets de capacité et contenus de fichiers n’entrent pas dans ces trames.

Le démon répond à une émission après l’acquittement de l’écriture P9. Si une requête a atteint le destinataire mais que sa réponse se perd, l’issue peut être `UNCERTAIN`. Le gestionnaire détache de sa file un envoi admis. Le client `nsh` remet la mutation une fois. Un lecteur autorisé peut examiner les événements conservés. Les EID retenus permettent de faire reconnaître un doublon identique, tandis qu’un contenu contradictoire produit un conflit. L’éviction ou la purge borne la durée de cette détection. Aucun identifiant actif ne restitue des octets évincés. [Contrat de stockage](journal-stockage-v0.22.0.md) · [Gestion des commandes](journal-nsh-v0.22.0.md)

## Droits et trames du terminal

Chaque trame contient 16 mots de 64 bits little-endian. Les mots 0–2 portent la version, l’opération et le statut, nul dans la requête. Une réponse reprend le tag IPC de la requête. Les autres mots réservés sont contrôlés à zéro. Une session ordinaire possède `OWN`. Le profil explicite d’administration du journal possède les six droits.

| Droit | Valeur | Portée |
| --- | ---: | --- |
| `OWN` | 1 | Lire ou suivre les événements émis par l’incarnation propre, en filtrant sur `SELF` |
| `SYSTEM_READ` | 2 | Lire les sources et les séquences du journal, filtrer et suivre |
| `RETENTION_RIGHT` | 4 | Modifier le nombre de segments conservés |
| `ROTATION_RIGHT` | 8 | Déclencher une rotation |
| `PURGE_RIGHT` | 16 | Purger l’historique |
| `EMIT_RIGHT` | 32 | Émettre une notice propre |

| Opération | Requête après le mot 2 | Réponse |
| --- | --- | --- |
| `STATUS` (2) | Zéros | Mots 3–7 première séquence, suivante exclusive, segments, rétention, révision. Mots 8–11 quota et occupation en octets utiles, BootID du démarrage courant. Un administrateur limité à un droit de mutation reçoit uniquement la révision au mot 7 et la marque de restriction au mot 13 |
| `NEXT` (3), `WATCH` (4) | Mot 3 curseur inclusif, 4 service (0 pour tous, `u64::MAX` pour soi), 5 gravité minimale, 6–7 BootID ou zéros, 8 sens (0 croissant, 1 décroissant, sens croissant exigé pour le suivi) | Mot 3 curseur suivant. En cas de succès les mots 4–15 contiennent l’événement. Un curseur initial nul commence à l’extrémité demandée. `u64::MAX` marque une fin en lecture inverse |
| `ROTATE` (5), `RETAIN` (6), `PURGE` (7) | Mot 3 révision attendue, mot 4 nombre de segments pour `RETAIN` | Mot 3 nouvelle révision, `BUSY` en cas de concurrence sur la révision |
| `SELF_NOTICE` (8) | Mot 3 ordinal du producteur, mot 4 donnée numérique | Mot 3 ordinal local confirmé |

Le port privé du gestionnaire admet `EMIT` (1), mot 3 nul et événement aux mots 4–15. Un acquittement positif porte la séquence durable. Le démon vérifie la version, l’EID, les bornes du schéma, le BootID et l’autorité du producteur. Sur le port du terminal, cette même trame reçoit `DENIED`. Un `PID` ou une identité dans la charge utile n’accorde aucun droit.

Le filtre propre compare les quatre mots de l’émetteur. Il restitue un curseur et un numéro d’ordre locaux. Il garde privé le curseur système utilisé pour faire avancer la lecture. En lecture inverse, rendre le plus ancien événement encore conservé termine naturellement la page avec `END`, y compris après une purge. Une lecture globale dont le curseur a été évincé reçoit `REVOKED` avec la première séquence encore disponible. Une vue propre vide ne révèle pas ce compteur. Le masque de consultation concerne aussi les métadonnées de `STATUS`.

## Concurrence et pannes

Une lecture suit au plus 128 événements conservés. Une erreur d’append réveille le lecteur au dernier curseur vérifié pour permettre de relire un événement qui aurait été durablement écrit sans acquittement. Dès que le démon constate une vue P9 illisible, il invalide ses enregistrements en cache et revalide le stockage avant toute réponse de lecture. `WATCH` admet une attente par canal. Un événement correspondant réveille le consommateur après publication. Une mutation administrative réussie termine l’attente et le prochain curseur est revalidé. Le serveur conserve le tag et le handle de réponse associés à l’incarnation. Il abandonne une réponse adressée à une capacité révoquée. Le lancement suivant reçoit des canaux distincts.

Au repos, la boucle dort sur la réception de supervision pendant deux ticks au plus entre examens du terminal. Les attentes de suivi expirent après 62 ticks d’attente, environ 248 ms à 250 Hz, ou après 64 tours sous trafic. Ces bornes portent sur le service des files et excluent le temps des opérations P9 déjà en cours, de l’ordonnancement et du transport. Réarmer le même suivi propre reprend le curseur système interne. Aucune lecture disque répétée n’est nécessaire quand aucun nouvel événement n’est arrivé.

Dans un ELF de laboratoire compilé avec `--cfg journal_pannes`, seules les notices déjà autorisées par le canal d’administration peuvent stimuler les points d’arrêt du banc. Les valeurs 241, 242 et 243 interrompent le processus avant admission, après admission avant écriture et après publication durable avant la réponse. Les valeurs 244 et 245 prennent effet après publication de la notice et remise de son acquittement au canal IPC. Elles font échouer respectivement la prochaine rotation ou les mutations suivantes avec `IO`. Les images ordinaires traitent ces cinq valeurs comme des données de notice. Il s’agit d’arrêts du processus invité et d’erreurs injectées. Les interruptions du périphérique simulé, les écritures partielles et les barrières P9 ont leurs propres vérifications. [Sondes de qualification](../user/services/journal_probe.rs) · [Banc de pannes invité](../tests/journal_pannes.py) · [Banc du stockage](../tests/journal_stockage.rs)

## Vérifications du protocole et mémoire

Le banc Darwin exécute les décisions du démon et le vrai magasin P9 sur `SimDevice`. Il remplace les extrémités IPC pour choisir un ordre de livraison ou supprimer exactement un acquittement. Les mêmes tests ont réussi dans deux binaires distincts, 8/8 sur ARM64 natif et 8/8 sur x86_64 sous Rosetta. Les essais concernent les 64 combinaisons des six droits, le rejet des formes et identités invalides, la séparation entre BootID et générations, la confidentialité des curseurs, le réveil après acquit, le retrait du lecteur, la rotation pendant le suivi, la fin de pagination après purge, le refus des réponses en cache après corruption détectée et l’absence de réexécution automatique après perte de réponse. Une attente sur 48 événements appartenant à un autre producteur a effectué 48 lectures à sa première installation puis zéro lecture supplémentaire pendant trois réarmements. Les vues propres et les émissions à acquit perdu ont été exercées sur des secteurs de 512 et 4096 octets. [Banc du protocole](../tests/journal_service.rs)

Le cache borné du démon contient 32 événements, soit 3 072 octets. Les buffers de protocole ont 128 octets. Une seule attente et une seule transaction P9 peuvent être en cours dans le démon. Le code n’alloue pas de tas. Les deux ELF autonomes de qualification, compilés avec `opt-level=s` et sans les sondes, occupent respectivement 16 808 octets (AArch64) et 16 944 octets (x86_64) sur disque. L’inspection de leurs en-têtes ELF donne quatre pages de segments `LOAD` et 64 pages de pile par architecture. Le chargeur ajoute une page de données de lancement, soit 69 pages privées (276 Kio) selon sa comptabilité, hors tables de pages et mémoire détenue par les autres services. Ce chiffre est calculé à partir des ELF et des mappages du chargeur. Le maximum de pile effectivement utilisée pendant une opération reste non mesuré. [Chargement des pages](../kernel/src/memory/user.rs)

Pour reproduire sur le worktree, définir `TMPDIR` conformément à `AGENTS.md`, sélectionner successivement `aarch64` et `x86_64`, puis choisir un répertoire de preuves durable.

```sh
preuves="../preuves-journalisation-0220-phase2/service"
mkdir -p "$preuves"
arch=aarch64
target="$arch-apple-darwin"
NOVAFS_TARGET="$target" novaFS/lab/run build --offline --lib
lib="novaFS/target/$target/release"
rustup run stable rustc --edition=2024 --target "$target" --test -C opt-level=2 \
  -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" \
  novaOS/tests/journal_service.rs -o "$preuves/protocole-$arch"
"$preuves/protocole-$arch" --nocapture
```
