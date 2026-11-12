# nsh et commandes natives de NovaOS v0.15.1

## Exécutable et navigation

Le shell interactif s’appelle `nsh`. Le noyau lance `nsh.elf` et le terminal série affiche `nsh> `. La construction conserve une copie identique nommée `shell.elf` pour les outils existants. Les numéros d’image des anciennes commandes sont stables. Les images v0.15.0 conservent leur interpréteur embarqué. Le prompt de diagnostic graphique x86_64 reste `nova>`. Le chemin proposé pour un futur interpréteur de scripts est `/bin/nsh`. Cette version lance les ELF de son catalogue et ne traite ni scripts ni lignes `#!`. [Construction des ELF](../kernel/build.rs) · [Lancement du shell](../kernel/src/sched/process/storage.rs) · [Catalogue](../../components/libraries/command.rs)

La session porte le répertoire courant et la racine de son volume. Pour remonter avec `..`, le shell obtient le chemin courant par fragments d’une même génération, vérifie son identité, ouvre le parent avec le droit de traversée et vérifie que l’entrée désigne encore le même enfant. Arrivé à la racine, un `..` supplémentaire échoue. Un composant absent ou un fichier suivi de `/..`, `/.` ou d’une barre finale échoue également. Les doubles séparateurs sont invalides. Une commande enfant reçoit les objets sélectionnés par le shell. [Résolution des chemins](../../components/shell/paths.rs) · [Protocole des espaces de noms](namespace-v0.12.0.md)

## Expansion des noms

| Mot de la ligne de commande | Correspondance |
| --- | --- |
| `*`, `?` | Zéro ou plusieurs scalaires Unicode, ou exactement un, à l’intérieur d’un composant. |
| `[abc]`, `[0-9]`, `[a-z]`, `[A-Z]`, `[!abc]` | Ensemble, intervalle inclusif ou complément. Ordre des scalaires, sensible à la casse. Un intervalle inversé est invalide. Un crochet ouvrant sans fermeture est littéral. |
| `**` | Composant entier formé de deux étoiles actives. Il accepte zéro répertoire intermédiaire et peut poursuivre la descente. Une séquence d’étoiles dans `ab**cd` reste locale au nom. |
| `**/*`, `**/*.txt` | Entrées visibles récursivement, ou noms en `.txt` dès le répertoire courant. Un même chemin apparaît une seule fois par motif. |
| `src/**/test?.rs` | Inclut `src/test1.rs` et les noms correspondants dans ses descendants. `src/*/test?.rs` impose exactement un intermédiaire. |
| `'*.rs'`, `"*.rs"`, `\*.rs` | L’étoile protégée reste littérale. Les autres segments du même argument peuvent comporter des opérateurs actifs. |
| `.*`, `.cache/**/*.txt` | Une entrée cachée exige un point littéral en tête de son composant. La descente implicite de `**` ignore les répertoires cachés. |

Les résultats sont triés par octets UTF-8 pour chaque opérande, puis insérés dans l’ordre de la ligne. Un nom relatif commençant par un tiret reçoit le préfixe `./`. Un motif sans correspondance est transmis littéralement. Une barre finale sélectionne les répertoires. Un préfixe explicite suivi de `/**` peut désigner ce répertoire lui-même. À la racine implicite d’un motif, `**` ne génère pas d’entrée `.`. Les chemins ne sont pas normalisés en Unicode. Un `..` après un composant de motif est refusé.

L’expansion se termine avant tout lancement. Elle ouvre les répertoires avec traversée et énumération, vérifie les identités sur la branche en cours et descend uniquement dans des objets de type répertoire. P9 expose fichiers et répertoires. La recherche est bornée à 32 niveaux, 128 énumérations et 1 024 entrées examinées par ligne. Une limite atteinte ou une erreur de parcours annule l’expansion. La ligne contient au plus 2 048 octets et la trame de lancement 32 arguments dans 2 560 octets, longueurs comprises. Un composant accepte 255 octets et un chemin transmis au service 1 024. [Lexer et protections](../../components/shell/lexer.rs) · [Expansion et limites](../../components/shell/glob.rs) · [ABI terminal](../abi/terminal.rs)

## Opérations et autorité

- `rmdir nom` reçoit la délégation 7, limitée au retrait du répertoire nommé et identifié s’il est vide. Une entrée ordinaire, un répertoire occupé ou non vide, le répertoire courant et ses ancêtres sont refusés.
- `rm -r nom` reçoit la délégation 8, limitée au nom sélectionné et à ses descendants. Le service épingle l’identité, vérifie les handles dérivés et n’accepte qu’un nom simple par descente. Il autorise l’inspection et le retrait dans cet arbre. L’ouverture du contenu, la création, le renommage, la remontée et la résolution ambiante restent refusés. L’ancienne délégation 5 conserve son seul retrait de fichier nommé.
- `rm -ri` demande une réponse avant de descendre, puis avant de retirer chaque objet. Une réponse négative conserve l’objet ainsi que ses ancêtres nécessaires. La marche en profondeur vérifie l’identité avant chaque retrait, dans la limite de 32 niveaux et 1 024 descendants par opérande. Chaque suppression acquittée est une transaction distincte. Une coupure permet de retrouver le préfixe déjà exécuté. Une erreur arrête la commande et indique combien de retraits ont réussi.
- `cp` et `mv` reçoivent une sélection source/destination par enfant. Avec plusieurs sources, le dernier opérande doit d’abord désigner un répertoire. `nsh` attend et révoque chaque délégation avant la suivante. Le premier échec arrête le lot. Les opérations précédentes conservent leurs effets. `mv` renomme aussi les répertoires. `cp -R` prépare sa copie et demande la publication vers un nom libre.
- `--help` et `--version` lancent un seul processus sans capacité de fichiers, quel que soit le nombre d’opérandes. Le premier essai du découpage des lots perdait ces options en reconstruisant les arguments. Le correctif conserve la demande d’information entière. Les tests vérifient les deux options pour `rm`, `rmdir`, `cp` et `mv`, puis relisent les sources et les répertoires concernés.
- Sur collision, `-n` conserve les deux objets et réussit, `-i` demande confirmation et une réponse affirmative aboutit à `EXISTS` (27). `-f` supprime la demande et aboutit au même refus. Les options de politique se traitent de gauche à droite. Un opérande identique à sa destination demeure invalide. Aucune délégation ne permet d’effacer une destination pour rendre un transfert possible.

Les canaux de premier plan sont révocables. Le service contrôle le numéro d’opération, l’identité et les droits, y compris lorsque le processus client s’interrompt. Ces deux délégations sont les seules extensions du protocole fichiers v2. Le format NovaFS v0.9.13 P9 et les mécanismes CoW, snapshots, chiffrement et récupération appartiennent au moteur existant. Le protocole PC1 et le remplacement transactionnel restent expérimentaux. [Délégations du service](../user/novafs/files/delegated.rs) · [Sélection par le shell](../../components/shell/capabilities.rs) · [Parcours des retraits](../../components/coreutils/removal.rs) · [Audit de publication](../../novaFS/docs/novafs-publication-conditionnelle.md)

## Options GNU et différences de comportement

Le relevé utilise le manuel GNU Coreutils 9.12 et l’aide de ses neuf exécutables sur Darwin. Les commandes NovaOS reconnaissent `--help`, `--version` et le séparateur `--`. Les options courtes se groupent. Les noms longs doivent être complets. Une option inconnue ou une valeur invalide renvoie 3 et désigne l’argument fautif. L’entrée standard POSIX et le système de permissions Unix ne font pas partie de ces contrats. [Manuel GNU Coreutils](https://www.gnu.org/software/coreutils/manual/coreutils.html) · [Analyse partagée des options](../../components/libraries/options.rs)

| Commande | Options disponibles | Comportement à connaître |
| --- | --- | --- |
| `ls` | `-a/--all -A/--almost-all -d/--directory -F/--classify -h/--human-readable -l -1 -r/--reverse -R/--recursive -S -t -w/--width`, `--color[=always\|auto\|never]` | `-a` inclut `.` et les cachés, `-A` les cachés. Aucun parent synthétique. Le dernier tri `-t/-S` et le dernier format `-1/-l` prévalent. Les dates inconnues passent après les dates connues. `-F` marque les répertoires. |
| `cp` | `-r/-R/--recursive -f/--force -i/--interactive -n/--no-clobber -v/--verbose` | Copie ordinaire ou récursive vers un nom libre, plusieurs sources vers un répertoire. `-n` laisse le nom occupé intact. |
| `mv` | `-f/--force -i/--interactive -n/--no-clobber -v/--verbose` | Renommage dans le volume, fichiers et répertoires. Même règle de collision. |
| `rm` | `-r/-R/--recursive -f/--force -i/--interactive -v/--verbose` | `-f` tolère les noms absents et la liste vide. `-f/-i` sélectionnent la dernière politique donnée. `-i` exige une réponse par objet. |
| `rmdir` | `-v/--verbose` | Retrait des seuls répertoires vides demandés. Un refus sur un opérande arrête le lot. |
| `cat` | Options communes | Concaténation en ordre d’arguments. Le terminal protège les caractères de contrôle. La copie binaire utilise `cp`. |
| `head`, `tail` | `-n/--lines -c/--bytes -q/--quiet/--silent -v/--verbose` | Dix lignes par défaut. Compteurs décimaux non signés, zéro admis. La dernière unité et la dernière option d’en-tête prévalent. Aucun suivi en continu. |
| `wc` | `-l/--lines -w/--words -c/--bytes -m/--chars -L/--max-line-length` | Compte les octets, scalaires, mots, lignes ou colonnes selon la sélection. Plusieurs fichiers donnent un total. |

### Options GNU refusées

Les listes ci-dessous donnent les formes courtes et longues restantes du relevé. Les valeurs des options GNU disponibles seulement en partie sont précisées après la table. Les options qui demandent propriétaire, ACL, contexte de sécurité Unix, liens, informations de périphérique ou entrée standard n’accordent aucun droit supplémentaire.

| Commande | Formes courtes refusées | Formes longues refusées |
| --- | --- | --- |
| `ls` | `-b -B -c -C -D -f -g -G -H -i -I -k -L -m -n -N -o -p -q -Q -s -T -u -U -v -x -X -Z` | `--author --escape --block-size --ignore-backups --dired --file-type --format --full-time --group-directories-first --no-group --si --dereference-command-line --dereference-command-line-symlink-to-dir --hide --hyperlink --indicator-style --inode --ignore --kibibytes --dereference --numeric-uid-gid --literal --hide-control-chars --show-control-chars --quote-name --quoting-style --size --sort --time --time-style --tabsize --context --zero` |
| `cp` | `-a -b -d -H -L -P -l -p -s -S -t -T -u -x -Z` | `--archive --attributes-only --backup --copy-contents --debug --dereference --no-dereference --keep-directory-symlink --link --preserve --no-preserve --parents --reflink --remove-destination --sparse --strip-trailing-slashes --symbolic-link --suffix --target-directory --no-target-directory --update --one-file-system --context` |
| `mv` | `-b -S -t -T -u -Z` | `--backup --debug --exchange --no-copy --strip-trailing-slashes --suffix --target-directory --no-target-directory --update --context` |
| `rm` | `-I -d` | `--one-file-system --no-preserve-root --preserve-root --dir` |
| `rmdir` | `-p` | `--ignore-fail-on-non-empty --parents` |
| `cat` | `-A -b -e -E -n -s -t -T -u -v` | `--show-all --number-nonblank --show-ends --number --squeeze-blank --show-tabs --show-nonprinting` |
| `head` | `-z` | `--zero-terminated` |
| `tail` | `-f -F -s -z` | `--debug --follow --max-unchanged-stats --pid --retry --sleep-interval --zero-terminated` |
| `wc` | Aucune du relevé | `--debug --files0-from --total` |

`ls --classify` accepte la forme sans valeur. Les valeurs GNU de `--classify=WHEN` et de `rm --interactive=WHEN`, les suffixes multiplicatifs de `head/tail` et leurs positions signées sont refusés. Les options GNU qui changent le rendu sont détaillées dans la table des formes disponibles. Les commandes `pwd`, `touch`, `mkdir` et `more` conservent leur contrat antérieur. [Manuel GNU de cp](https://www.gnu.org/software/coreutils/manual/html_node/cp-invocation.html) · [Commandes historiques](cli-v0.14.0.md)

## Affichage et mesure

Sur P9, `ls -l` aligne les colonnes de type, taille, mtime UTC et nom. La largeur du champ de taille est celle de la valeur affichée la plus longue du groupe. Une date sans fraction se termine par `Z`. Une fraction significative garde ses chiffres jusqu’à la nanoseconde. Le tri utilise toujours les secondes et les nanosecondes stockées. Une date absente affiche `inconnue`. Aucun propriétaire, groupe ou mode Unix n’est synthétisé.

La série ne fournit pas de notification de redimensionnement. `-w N` ou `--width=N` fixe la largeur effective pour le nom, `0` désactive le retour imposé. À défaut, `NOVA_MORE_COLUMNS` à la construction vaut 80. Un nom qui déborde passe sous les métadonnées, puis s’affiche entier, avec calcul des largeurs Unicode et des échappements. Sur un terminal plus étroit que la partie métadonnées, le terminal replie également cette partie. L’ancien profil sans dates conserve son rendu historique. [Rendu de ls](../../components/coreutils/ls.rs)


## Résultats de qualification

Qualification du 12 novembre 2026, à partir de `c4bd4c18c64884a98fca7e4bfa16b6dacad7155b`. Les invités NovaOS ont tourné sous QEMU 11.1.3 pour le développement et sous le moteur 10.0.12 d’UTM 5.0.8 pour les profils d’installation et DMA. Les exécutables Darwin de NovaFS ont tourné sur macOS 27.2, en ARM64 natif et en x86_64 sous Rosetta. [Résultats, compteurs et empreintes](validation/cli-v0.15.1/nsh.json)

| Suite réussie | AArch64 | x86_64 |
| --- | ---: | ---: |
| Démarrages historiques | 25 | 25 |
| Groupes bloc | 24 | 24 |
| Contrôles ELF, cycle de vie et courses IPC | 3 | 3 |
| Essais CLI historiques | 4 | 4 |
| Groupes fichiers et espaces de noms, deux géométries réunies | 68 | 68 |
| Commandes du banc `nsh`, secteurs 512 puis 4 096 octets | 113 + 113 | 113 + 113 |
| NovaFS standards / instrumentés | 196 / 197 | 196 / 197 |
| Injections sur les ancres / opérations différentielles NovaFS | 2 304 / 1 000 000 | 2 304 / 1 000 000 |
| DMA/GICv3 / DMA séquentielle sur le moteur UTM | 11 / 23 | Profil ARM |

Les quatre essais de `nsh` comparent les correspondances, leur ordre, les classes, les guillemets, les échappements et les fichiers cachés. Des processus isolés vérifient le confinement et la révocation des deux nouvelles délégations. Dans chaque géométrie, le banc interrompt `rm -riv coupure` après l’acquittement du retrait d’`alpha`, au dialogue suivant. Un lecteur hôte contrôle P9 avant le remontage invité. `beta` et le témoin extérieur retrouvent leur contenu, et `alpha` reste absent. Les oracles GNU exercent les transferts vers un nom libre et les retraits. Les essais d’interopérabilité relisent P9 dans les deux sens entre architectures ainsi que les fixtures CLI P8 aux deux tailles de secteur. [Banc interactif](../tests/nsh.py) · [Interopérabilité P9](../../novaFS/lab/p9-interop.py)

| Mémoire physique invitée en Kio, pages de 4 Kio | AArch64 | x86_64 |
| --- | ---: | ---: |
| Pages privées de `nsh`, pile incluse | 400 | 408 |
| Pages privées au lancement de `rm` / `rmdir` | 296 / 284 | 296 / 284 |
| Pic résident échantillonné pendant la session CLI P9 | 4 844 | 5 020 |
| Pic au remontage, mesuré à chaque allocation | 4 964 | 5 140 |
| Allocations de cadres au remontage (nombre) | 1 465 | 1 473 |

Les mesures sont identiques sur secteurs de 512 et 4 096 octets pour une même architecture. Par rapport au banc CLI v0.14.0, les pages privées du shell augmentent de 20 Kio sur AArch64 et de 24 Kio sur x86_64. Le pic échantillonné augmente respectivement de 124 et 156 Kio. Après fermeture, le banc retrouve trois cadres de tables sur AArch64 et douze cadres sur x86_64, avec le même nombre d’octets du Buddy qu’à l’ouverture. [Méthode et mesures de référence](cli-v0.14.0.md#mesures-de-qualification)

Sur le moteur d’UTM, l’installateur a installé GPT, l’ESP et P9, démarré depuis la cible seule puis relu les données aux deux géométries. Pour chacune, treize refus ont conservé le SHA-256 de la cible, avec zéro écriture et zéro FLUSH sur ce disque. [Banc GPT](../tests/installation_gpt.py) · [Empreintes par refus](validation/cli-v0.15.1/nsh.json)

Présenté au noyau installé aux deux géométries, un volume expérimental PC1 a été refusé avec `Geometry` pour le type GPT privé et `Device` pour une fausse étiquette P9. Dans les quatre cas, le service a refusé le montage avant l’ouverture de `nsh`. Les images sources et les disques présentés ont conservé leurs empreintes, avec zéro écriture et zéro FLUSH sur la cible. [Banc de compatibilité des lecteurs](../tests/publication_compatibilite.py)

Dans **l’application UTM**, l’archive distribuée a installé le système sur une cible QCOW2 de 64 Mio, secteurs de 512 octets. Le support a conservé son SHA-256. Après son retrait, un premier démarrage a créé `recettes/preuve.txt`. Après arrêt complet de la VM, un second démarrage a relu `NovaOS` et retrouvé ce chemin avec `**/*.txt`. Les traces firmware comptent 805 droits et 805 retraits à l’installation (SID 48), puis 436 et 436 à chaque démarrage autonome (SID 56), avec zéro droit actif, BME et bypass nuls à la transition. Le terminal graphique natif d’UTM a également affiché `nsh>`, donné `NovaOS 0.15.1` pour `ls --version`, lu `/systeme` et terminé sur `SYSTEME bilan Ok(())`. La géométrie 4 096 a été exercée sur le moteur QEMU d’UTM. Les coupures de VM décrites ici laissent le Mac alimenté. [Banc de l’application UTM](../tests/nsh_utm.py)

Rejouer depuis la racine sur le Mac équipé des deux cibles Rust et de GNU Coreutils. La dernière commande attend l’archive extraite dans `NOVA_DIST` et un chemin de sortie neuf pour les VM jetables.

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test file-test namespace-test || exit 1
    python3 -B novaFS/lab/p51-validation-suites.py "$arch" || exit 1
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib fs::remplacement:: -- --nocapture || exit 1
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --lib qualification_ancres -- --nocapture || exit 1
done
python3 -B novaOS/tests/nsh_utm.py "$NOVA_DIST" "$NOVA_TRAVAIL/qualification-utm"
```

## Image et installation dans UTM

L’archive de livraison contient l’installateur QCOW2 en lecture seule, le firmware privé expérimental, le modèle de variables EFI et les licences. Son SHA-256 est `f22af0a9cdd8d45c313a85da1e736ced2b524d5056b211d2f15ef8cfa498b674`. La procédure utilise UTM 5.0.8 sur Mac Apple Silicon, QEMU 10.0.12 intégré et `qemu-img` sur l’hôte. [Archive UTM AArch64](../../distribution/images/novaos-0.15.1-utm-aarch64.tar.gz)

Depuis la racine, créer une VM et sa cible vierge de 64 Mio dans un répertoire neuf.

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-0.15.1.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.15.1-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.15.1-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir le paquet `.utm` dans UTM et démarrer. Attendre la fin de la vérification du disque vierge. Contrôler la série `NOVA-CIBLE-000000001`, le SID 56 et les 67 108 864 octets.
2. Saisir `AUTORISER NOVA-CIBLE-000000001 67108864` uniquement pour cette cible. Attendre `INSTALLATION bilan Ok(())`, puis arrêter la VM. Tout disque refusé doit être conservé tel quel.
3. Retirer le **raccourci** de la bibliothèque comme indiqué dans la procédure GPT, conserver les fichiers sur le Mac et exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`. Le script vérifie le disque installé puis retire le support de la configuration.
4. Rouvrir le même paquet, démarrer et attendre `nsh>`. Exécuter `mkdir recettes`, `cp /systeme recettes/preuve.txt`, `cat recettes/preuve.txt` puis `exit`. Après `SYSTEME bilan Ok(())`, arrêter et redémarrer. `cat recettes/preuve.txt` relit `NovaOS` sur le même disque. [Procédure et protections GPT](installer-v0.15.0.md#installation-dans-utm-508)

Pour reconstruire l’installateur depuis les sources, extraire et vérifier l’archive ci-dessus dans `NOVA_DIST`. Depuis `novaOS/`, construire les deux ELF avec Rust stable et leur verrou Cargo, puis créer le support dans un nouveau répertoire. Le profil de développement ISO et le profil UTM ont des configurations d’interruptions différentes. [Construction du paquet](../scripts/paquet_installation.py) · [Préparation des secteurs EFI](../scripts/esp_installation.py)

```sh
mkdir -p build
NOVA_BUILD="$(mktemp -d "$PWD/build/nsh-install.XXXXXXXX")"
NOVA_TARGET=aarch64-unknown-none-softfloat
mkdir "$NOVA_BUILD/fv"
cp "$NOVA_DIST/prototype.fd" "$NOVA_BUILD/fv/QEMU_EFI.fd"
cp "$NOVA_DIST/modele-vars.fd" "$NOVA_BUILD/fv/QEMU_VARS.fd"
export RUSTC="$(rustup which --toolchain stable rustc)"
rustup run stable cargo build --offline --locked --release --target "$NOVA_TARGET" \
    --features systeme-gpt --target-dir "$NOVA_BUILD/systeme"
python3 scripts/esp_installation.py "$NOVA_BUILD/systeme/$NOVA_TARGET/release/novaos" "$NOVA_BUILD/esp"
NOVA_ESP="$NOVA_BUILD/esp" rustup run stable cargo build --offline --locked --release --target "$NOVA_TARGET" \
    --features installation-gpt --target-dir "$NOVA_BUILD/installation"
python3 scripts/paquet_installation.py construire "$NOVA_BUILD/fv" \
    "$NOVA_BUILD/installation/$NOVA_TARGET/release/novaos" "$NOVA_BUILD/distribution"
```

## Historique des audits du remplacement

Les sections suivantes sont les rapports antérieurs à la consolidation de `nsh`. Les mentions de version distribuée et les demandes d’arbitrage y décrivent la situation au commit cité. La règle de refus d’un remplacement reste applicable dans v0.15.1.

Le 12 novembre 2026, la qualification a atteint la condition d’arrêt du §6 du mandat. Le remplacement d’un nom occupé requiert une opération que le moteur P9 et le service de fichiers n’exposent pas. La version distribuée lors de cet audit était NovaOS v0.15.0 avec NovaFS v0.9.13 P9. Les observations ci-dessous portent sur le commit `0f63772dd4ac7b40a21b61e9c36bfd6658282c06`, vérifié sur `main` propre avant l’essai.

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
