# NovaShell v0.15.1 — audit du remplacement de fichiers

Le 12 novembre 2026, la qualification a atteint la condition d’arrêt du §6 du mandat. Le remplacement d’un nom occupé requiert une opération que le moteur P9 et le service de fichiers n’exposent pas. La version distribuée reste NovaOS v0.15.0 avec NovaFS v0.9.13 P9. Les observations ci-dessous portent sur le commit `0f63772dd4ac7b40a21b61e9c36bfd6658282c06`, vérifié sur `main` propre avant l’essai.

## Décision après l’audit des ancres déchirées

Le blocage du montage normal après une déchirure n’est pas accepté pour v0.15.1. À partir de `03e267b9cf710d7d53612ba89064a9af156be054`, le banc montre pourquoi le seul disque P9 installé ne permet pas un repli automatique sûr : une coupure avant confirmation et une corruption après confirmation produisent la même image de 2 Mio. Dans le second cas, choisir la destination ancienne annulerait un remplacement déjà acquitté. Les 96 premiers octets contiennent le nouveau checksum ; la référence au manifeste à l’offset 136 est toujours ancienne.

Sur ARM64 natif et sur x86_64 sous Rosetta, **par architecture**, 2 304 injections couvrent les deux slots, les deux opérations et les secteurs de 512 et 4 096 octets : 1 288 anciens états et 656 nouveaux états vérifiés au montage, 360 refus en RO et RW. Avec un reçu frais imposé par le banc, le parent engagé par la tête survivante a pu être retrouvé et la paire revalidée sur une copie en lecture seule. Le service installé ne dispose pas d’une telle autorité. Le pool chiffré conserve séparément ses attestations, son snapshot et son clone ; ces preuves ne sont pas disponibles sur le volume autonome. [Rapport, limites et reproduction](../../novaFS/docs/novafs-p9-ancres-dechirees.md) · [Résultats par architecture](../../novaFS/docs/validation/v0.9.13/ancres-dechirees.json)

Les suites NovaFS ont réussi 196 tests standards, 197 instrumentés et 1 000 000 d’opérations différentielles par architecture. Les régressions NovaOS ont passé les 25 démarrages, 24 groupes bloc, 3 contrôles ELF/processus/IPC et 4 essais CLI par architecture, ainsi que les 11 scénarios DMA/GICv3 et 23 séquentiels sur AArch64.

**Arbitrage demandé :** qualifier en priorité la publication indivisible de l’ancre sur le stockage cible ; à défaut, étudier un témoin externe authentifié résistant au recul. Aucun des deux n’est qualifié pour UTM par ces essais. NovaOS reste en v0.15.0, NovaFS en v0.9.13 P9 ; `cp`, `mv` et leurs droits IPC conservent leur comportement distribué.

## Qualification de la publication P9

L’arbitrage suivant a autorisé une primitive transactionnelle à partir de `289c98d18545435076a1f1329d8647d69d05c9b7`, vérifié sur `main` propre. Le banc a exercé le moteur P9 avant l’intégration IPC. **La publication conserve la destination sous les coupures à secteurs entiers. Une écriture partielle à l’intérieur d’un secteur empêche le remontage normal.** Le scénario défaillant est reproductible sur ARM64 natif et sur x86_64 sous Rosetta. [Banc Rust](../../novaFS/src/fs/remplacement.rs) · [Contrat de récupération P9](../../novaFS/docs/novafs-recovery.md#qualification-du-remplacement-dun-fichier-sur-p9-12-novembre-2026)

| Opération testée | Avant publication | Après publication durable |
| --- | --- | --- |
| Déplacement entre répertoires | L’original et l’ancienne destination occupent deux noms distincts. | Le nom destination désigne l’ID de l’original, le nom source est libre. |
| Publication d’une copie préparée et synchronisée | L’original, la candidate et l’ancienne destination ont des ID distincts. | Le nom destination désigne l’ID de la candidate. Le nom et le contenu de l’original subsistent. |

Les deux cas passent de l’ancien au nouvel état en une génération. Le banc vérifie aussi le refus d’une destination dont l’identité diffère de celle sélectionnée, sans E/S de mutation. Le `ctime` du fichier entrant prend l’heure de publication, ses `btime`, `mtime` et `atime` sont conservés. La collecte de l’objet évincé, le total logique et la vérification intégrale réussissent sur la publication complète. Pour `cp`, la candidate est déjà nommée dans la fixture avant le début des injections. L’invisibilité d’une copie partielle et la récupération d’une candidate abandonnée pendant sa préparation restent à démontrer avant de livrer la commande.

| Coupure ou défaut injecté | Résultat sur les deux architectures, pour les deux opérations |
| --- | --- |
| À chacun des 24 événements, perte de toutes les écritures volatiles ou persistance de toutes en ordre inverse | La destination présente exactement l’ancienne ou la nouvelle identité et le contenu correspondant. Après l’effet de F2, le nouveau reçu est obligatoire, même si sa réponse est perdue. |
| Avant l’effet de F2, chaque sous-ensemble de secteurs entiers de l’ancre | 256 masques en 512 octets, 2 en 4 096 octets. Les slots A et B ont été testés indépendamment. Aucune destination absente. |
| Seconde coupure pendant le flush de récupération RW | Après nouvelle ouverture, même contenu et même identité, que la génération sélectionnée soit l’ancienne ou la nouvelle. |
| Avant l’effet de F2, seuls les 96 premiers octets de l’ancre sont stabilisés | Montage normal refusé en RO et en RW. L’autre slot permet l’extraction vérifiée de l’ancienne destination en lecture seule. Les vérifications ne changent aucun octet du disque. |

Le dernier cas représente une déchirure à l’intérieur du secteur. Il a été injecté sur un périphérique simulé. Le pilote VirtIO du profil UTM négocie notamment la géométrie et le flush. Cette négociation ne fournit pas d’attestation d’écriture indivisible jusqu’au support de l’hôte. La cause de l’arrêt est l’indisponibilité du volume après déchirure, et aucune suppression de la destination n’a été observée. [Négociation du pilote](../kernel/src/hardware/block/mod.rs) · [Règle historique en cas d’ancre illisible](../../novaFS/docs/novafs-recovery.md#sélection-au-redémarrage)

La primitive reste privée au banc `cfg(test)`. Les trames IPC, les droits de montage et les délégations des clients v0.15.0 conservent leur définition. L’exposition envisagée requiert une délégation supplémentaire, limitée aux objets sélectionnés, indépendante de `DIR_ALL`. Le service devrait revérifier les identités et droits source et destination à la publication, gérer les poignées actives et refuser un objet hors délégation même avec `-f`. Ces contrôles devront être qualifiés avant toute activation. Les comportements disponibles de `cp` et `mv` restent ceux décrits ci-dessous. La préparation de la copie par le service, les snapshots et clones de la **nouvelle** opération, la surprotection et son utilisation dans une VM installée ne sont pas qualifiés.

Le contrat CLI autorisé prévoit le remplacement par défaut pour les fichiers ordinaires avec cette délégation, une confirmation préalable avec `-i`, la conservation de la destination avec `-n` et la suppression de la confirmation ordinaire avec `-f`. La réponse à l’invite, le chemin absolu et les droits historiques de la racine ne conféreraient aucun droit de remplacement.

Pour reproduire la matrice du remplacement depuis la racine du dépôt, exécuter les deux commandes sur le Mac de qualification.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run test --offline --lib fs::remplacement:: -- --nocapture
NOVAFS_TARGET=x86_64-apple-darwin novaFS/lab/run test --offline --lib fs::remplacement:: -- --nocapture
```

### Régressions rejouées après l’arbitrage

| Contrôle | ARM64 | x86_64 sous Rosetta pour NovaFS, invité QEMU pour NovaOS |
| --- | ---: | ---: |
| Démarrages NovaOS / groupes bloc | 25 / 24 | 25 / 24 |
| ELF, cycle des processus, courses IPC / essais CLI | 3 / 4 | 3 / 4 |
| NovaFS standard / instrumenté, témoin de remplacement inclus | 193 / 194 | 193 / 194 |
| Différentiel NovaFS | 1 000 000 opérations | 1 000 000 opérations |
| DMA/GICv3 / DMA séquentiel sur le moteur UTM | 11 / 23 | Spécifique à AArch64 |

Par architecture, les suites NovaFS comprennent les 192 et 193 tests historiques, dont ceux des snapshots, clones, chiffrement, compression et migrations, plus le témoin. Le différentiel enregistre 345 183 publications, 54 643 pénuries injectées et 1 000 remontages audités. La matrice finale du remplacement a aussi été relancée séparément. Un premier essai DMA/GICv3 avait utilisé l’ELF archivé v0.14.0 et échoué au contrôle de bannière. Les 11 résultats retenus utilisent l’ELF v0.15.0 recompilé et l’oracle historique inchangé. [Effectifs, scénarios et empreintes](validation/cli-v0.15.1/publication-p9.json)

Le banc UTM QEMU 10.0.12 a réinstallé l’image distribuée v0.15.0 sur deux disques neufs (secteurs 512 et 4 096), redémarré deux fois sur chacun et contrôlé la persistance. Les 13 cas de refus par géométrie ont gardé la même empreinte SHA-256 et zéro écriture. Dans l’**interface graphique UTM 5.0.8**, une copie indépendante du disque installé de 512 octets a démarré deux fois sans support. Les huit combinaisons `cp`/`mv` avec comportement par défaut, `-i`, `-n` et `-f` ont confirmé les comportements de v0.15.0. Après redémarrage, `/ancien` faisait toujours zéro octet, `/persistant` et `/systeme` contenaient toujours `NovaOS` suivi de LF. La relecture hors ligne a vérifié les deux contenus depuis chacune des ancres, générations 9 et 10. Ces essais graphiques portent sur les commandes diffusées. [Résultats UTM](validation/cli-v0.15.1/publication-p9.json)

**Suite donnée à cet arbitrage :** le repli vers un simple diagnostic RO a été refusé comme critère de livraison. La [qualification ciblée](#décision-après-laudit-des-ancres-déchirées) ci-dessus conserve l’exigence de remontage sûr.

## Pourquoi `cp` et `mv` refusent une destination existante

1. Dans les deux commandes, si la source et la destination sont deux fichiers distincts et que la destination existe, le comportement par défaut retourne `EXISTS`. L’option `-n` termine avec succès en laissant les deux fichiers en place. Avec `-i`, un refus laisse également les deux en place, mais même une réponse affirmative aboutit à `EXISTS`. Le parseur refuse `-f`. [Traitement de la collision](../../components/coreutils/transfer.rs)
2. Pour une copie vers un nom libre, le service crée un fichier temporaire, puis `cp` en écrit et synchronise le contenu par tranches de 8 Kio. La publication finale utilise `PUBLISH_COPY`. Le déplacement utilise `MOVE_SELECTED`. Les deux appellent `Engine::rename_at`. Changer seulement le traitement de la confirmation laisserait donc un second refus dans le service. Écrire directement les tranches dans le fichier existant exposerait un contenu partiellement remplacé. [Copie](../../components/coreutils/copy.rs) · [Sélection et publication](../user/novafs/files/selected.rs)
3. Dans `move_entry`, deux clés de nom distinctes dont la destination existe donnent `Error::Exists` avant la première mutation. `unlink_at` retire l’ancienne entrée dans une transaction séparée. Une coupure entre suppression et renommage conserve cette suppression au remontage. La transaction du moteur est privée à la bibliothèque. Le service ne peut pas y regrouper ses deux appels. [Opérations de noms](../../novaFS/src/fs.rs) · [Publication P9](../../novaFS/src/engine.rs)
4. La délégation de destination de copie exige `TRAVERSE | ADD | FILE_WRITE`. Celle de déplacement exige `TRAVERSE | MOVE | ADD`. Le parent et le nom sont sélectionnés, mais ces droits n’autorisent pas le retrait de l’objet qui occupait la destination au moment de la sélection. Donner cette signification aux opérations existantes étendrait les droits d’anciens clients. Une réponse au terminal ne confère aucun droit supplémentaire. [Délégations du shell](../../components/shell/capabilities.rs) · [Contrôle par le service](../user/novafs/files/selected.rs)

### Témoin de coupure reproductible

Chaque essai formate un volume P9 simulé de 512 blocs de 4 Kio et crée deux fichiers au contenu distinct. Après le refus de renommage, le témoin compare chaque octet du support, les identités des objets et leur contenu. Il contrôle aussi la génération, les événements d’écriture et de synchronisation et l’intégrité après un montage indépendant en lecture seule. Ensuite, uniquement sur ce volume jetable, il supprime la destination et coupe le support avant le renommage. [Source du témoin](validation/cli-v0.15.1/remplacement.rs)

| Hôte macOS | Secteurs logiques | Refus de renommage | Coupure après la suppression |
| --- | --- | --- | --- |
| AArch64 natif | 512 et 4 096 octets | Génération 5, zéro écriture, zéro synchronisation, image identique | Génération 6 cohérente, destination absente, source intacte |
| x86_64 sous Rosetta | 512 et 4 096 octets | Même résultat | Même résultat |

Depuis la racine du dépôt, avec les deux cibles Rust Darwin installées et Rosetta disponible, les exécutables du témoin se construisent ainsi.

```sh
(
    set -eu
    essais=$(mktemp -d)
    trap 'rm -f "$essais/aarch64-apple-darwin" "$essais/x86_64-apple-darwin"; rmdir "$essais"' EXIT
    for cible in aarch64-apple-darwin x86_64-apple-darwin; do
        NOVAFS_TARGET="$cible" novaFS/lab/run build --lib --offline
        lib="novaFS/target/$cible/release"
        rustup run stable rustc --edition=2024 --target "$cible" \
            -C opt-level=2 -C panic=abort --extern "novafs=$lib/libnovafs.rlib" \
            -L "dependency=$lib/deps" novaOS/docs/validation/cli-v0.15.1/remplacement.rs \
            -o "$essais/$cible"
        "$essais/$cible"
    done
)
```

## Essai sur le système installé dans UTM graphique

L’archive distribuée a donné le SHA-256 `1f7cc3404418a80e4f5c11e4af162e9d93754e7d8c829bf0d7b1c4be6af833cb`. Une VM de test neuve a installé le système sur son disque VirtIO de 64 Mio à secteurs de 512 octets, après confirmation dans la console UTM. L’installateur a rendu `INSTALLATION bilan Ok(())`. Le profil système a ensuite démarré avec ce seul disque dans UTM 5.0.8, QEMU 10.0.12, AArch64, TPM CRB et GICv3 sans ITS. Au second démarrage autonome, le DXE a affiché 427 droits, 427 retraits, zéro droit actif, BME et bypass à zéro. [Image et procédure UTM v0.15.0](installer-v0.15.0.md)

| Commandes saisies dans NovaShell v0.15.0 | Observation |
| --- | --- |
| `mkdir thierry`, `ls`, `ls -l`, `ls -lh`, `ls -latr --color` | Répertoire créé et listé. Types, tailles, dates P9 et couleur du répertoire visibles. |
| `ls -latr --color thierry/` | Argument invalide (3). Le composant vide après la barre finale est rejeté. |
| `cd thierry`, `touch fichier`, `cd ..`, `pwd` | Fichier créé. Remontée refusée. Le répertoire courant reste `/thierry`. |
| `rm thierry/fichier`, puis `rmdir thierry` | Depuis `/thierry`, le premier chemin est introuvable (20). La seconde commande est inconnue. |
| `cd /`, `cat thierry/fichier`, `rm thierry/fichier` | Lecture puis suppression du fichier vide depuis la racine. Ce chemin relatif fonctionne lorsque le répertoire de départ est le bon. |
| `cd ./thierry`, `mkdir niveau`, `cd ./niveau`, `pwd` | Le shell affiche `/thierry/niveau`. `cd /` avec un espace terminal ramène à la racine. |
| `cd ../autre` depuis `/thierry`, avec `/autre` existant | Accès refusé. `pwd` confirme `/thierry`. |
| `ls thierry autre` | Affiche les deux répertoires et l’entrée `niveau` de `thierry`. |
| `rm -r thierry`, `rm -rf thierry` | Argument invalide (3). |
| `wc *`, `more *.txt`, `cp *.txt thierry/` | Introuvable (20). `preuve.txt` existait lors des deux derniers essais. |
| `cp source ancien`, `mv source ancien` | Destination existante (27). `source` contenait sept octets et `ancien` était vide. |
| `cp -i source ancien`, `mv -i source ancien` | La réponse `o` à l’invite de chaque commande aboutit au même refus (27). |
| `cp -n source ancien`, `mv -n source ancien` | Retour à l’invite sans erreur affichée. Source et destination conservées. |
| `cp -f source ancien`, `mv -f source ancien` | Argument invalide (3). |

### Persistance après arrêt forcé

La VM a été arrêtée depuis UTM pendant que `mv -i source ancien` attendait encore sa réponse. Au démarrage à froid, `cat source` et `cat preuve.txt` ont affiché `NovaOS`. `cat ancien` a rendu la main sans contenu. `ls -l` a confirmé les tailles de 7, 7 et 0 octets. Après les dernières commandes, `exit` a donné `SYSTEME bilan Ok(())`. Cette coupure couvre l’attente de confirmation. La coupure entre deux transactions est celle du témoin P9 ci-dessus.

VM arrêtée, `qemu-img check` a validé le QCOW2. Sur sa copie brute, le vérificateur GPT a relu les deux tables, leurs CRC, l’ESP et les ancres P9. La partition P9 occupe les LBA 124928 à 129023 inclus. L’outil NovaFS hôte a vérifié le volume à la génération 23. L’extraction en lecture seule de chaque fichier depuis chacune des deux ancres (générations 22 et 23) a donné les mêmes résultats. [Vérificateur GPT](../scripts/verifier_gpt.py) · [Outils NovaFS de vérification et d’extraction](../../novaFS/src/bin/novafs.rs)

| Fichier | Contenu exact | SHA-256 |
| --- | --- | --- |
| `/source`, `/preuve.txt` | `NovaOS` puis LF | `26168da719b7251c9001fc856ffbdb4a148f7ac6a8bcc925116215242ca29f59` |
| `/ancien` | Vide | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

## Autres limites de la CLI

- **Navigation.** Le service rejette tout composant `..` avant résolution. Le moteur connaît le parent d’un objet, mais son identifiant ne prouve aucun droit de parcours. La navigation demandée doit vérifier une capacité déjà détenue sur le parent réel, notamment après un déplacement du répertoire courant. Un chemin absolu doit partir de la racine autorisée de la session. Une barre terminale doit exiger une destination de type répertoire. [Résolution du service](../user/novafs/files/namespace.rs) · [Résolution dans P9](../../novaFS/src/fs.rs)
- **Suppression.** L’opération native `RMDIR` vérifie le type et refuse un répertoire non vide. La délégation du `rm` actuel autorise uniquement `UNLINK_AT` sur un nom sélectionné. La CLI demandée doit contrôler chaque entrée avant de descendre, refuser la racine, les ambiguïtés et les cycles, et rendre compte des suppressions déjà publiées après une erreur. P9 expose ici les fichiers et répertoires. Les liens symboliques et la surprotection persistante par fichier ne sont pas implémentés sur ce profil. [Délégations natives](../user/novafs/files/delegated.rs) · [Opérations de répertoires](../user/novafs/files/namespace.rs)
- **Expansion des motifs.** Le lexer enlève les guillemets et échappements sans garder l’information nécessaire pour distinguer un joker actif d’un joker littéral. Les motifs `*`, `?`, `[abc]`, `[a-z]` et `[!abc]` restent à implémenter, avec tri déterministe, traitement des noms cachés et conservation des motifs sans correspondance. L’énumération nécessite la capacité du répertoire concerné. Le contrat de lancement limite la liste à 32 arguments et 2 560 octets, et le service prépare au plus 32 cibles. Une expansion limitée seulement par les ressources demande aussi un moyen de transmettre les opérandes et leurs délégations au-delà de cette trame. [Lexer](../../components/shell/lexer.rs) · [ABI terminal](../abi/terminal.rs) · [Lancement](../../components/libraries/process.rs)
- **Arguments multiples.** Le catalogue accepte déjà plusieurs opérandes pour `ls`, `cat`, `wc` et `more`. Il exige exactement deux opérandes pour `cp` et `mv` et un chemin pour `rm`. Le traitement de plusieurs sources vers un même répertoire reste à qualifier. L’essai de `ls` a vérifié l’affichage et les options combinées. Les contrôles comparatifs détaillés de tri et les mesures antérieures figurent dans la qualification CLI v0.14.0. [Catalogue](../../components/libraries/command.rs) · [Qualification CLI](cli-v0.14.0.md)

## Mesures et régressions de l’audit initial

La VM disposait de 256 Mio. Au démarrage autonome après coupure, le noyau a annoncé 56 913 pages libres. Une exécution chronométrée du témoin hôte, toutes tailles de secteur comprises, a duré 14,88 ms en AArch64 (RSS maximal 6 963 200 octets) et 17,98 ms sous Rosetta (12 640 256 octets). Ces mesures utilisent l’horloge monotone hôte et `wait4` sur le processus du témoin, lancement compris. Il n’existe pas de mesure de latence ou de consommation d’une nouvelle CLI à ce stade.

Le premier audit a exécuté le témoin sur deux architectures, l’installation graphique sur disque neuf, deux démarrages autonomes AArch64 et la relecture hors ligne. Les suites complètes ont été relancées après l’arbitrage du remplacement. [Campagne de référence](installer-v0.15.0.md) · [Rejeu après arbitrage](#régressions-rejouées-après-larbitrage)

## Arbitrage initial accordé au commit 289c98d

Autoriser la conception et la qualification d’une opération **supplémentaire** de remplacement transactionnel de fichier ordinaire dans le moteur P9 et d’une délégation explicite dans le service NovaFS. Les anciens appels conserveraient leur refus de collision. La proposition vise le format P9 existant. Toute nécessité d’en modifier l’encodage exige un nouvel arbitrage.

Le contrat à éprouver doit identifier la source et l’ancienne destination, revérifier ces identités et les droits au moment de publier, et couvrir les poignées encore ouvertes ou révoquées. Pour `cp`, le fichier préparé doit être complet et synchronisé. Pour `mv`, le retrait du nom source doit faire partie de la même publication que le remplacement. La qualification doit couper avant et après chacune des barrières F1/F2, traiter l’issue indéterminée d’une synchronisation par remontage et vérifier dans chaque état les noms, les contenus et l’intégrité. Le sort des temporaires et de l’ancien objet doit également être vérifié. L’autre arbitrage possible est de reporter l’écrasement et d’autoriser une livraison CLI de périmètre réduit.
