# CLI native et horodatages NovaFS P9

La v0.14.0 utilise les dates d’objet de NovaFS v0.9.13. L’audit v0.13.0, conservé au commit `1eb62f9718a82cf8848386842ed88e7eb1f5e442`, avait établi qu’un renommage pouvait changer la génération d’un fichier sans modifier son contenu. Trier sur la génération donnait alors une réponse erronée. La résolution retenue ajoute une source UTC native et transmet la date persistante au programme `ls`. [Contrat P9 et migration explicite](../../novaFS/docs/novafs-p9-timestamps.md)

## Du disque au terminal

Le shell Rust `no_std + alloc` démarre le programme du catalogue dans un ELF distinct. L’enfant communique par IPC avec le service NovaFS en Ring 3 ou EL0. Le service monte le volume de contenu avec `Engine::mount_current` et utilise les opérations d’objet du moteur portable. Le pilote VirtIO et le service bloc fournissent les lectures, écritures et barrières. Le noyau fournit l’isolation, le lancement, l’horloge et les gestes du terminal. [Lancement](../../components/libraries/process.rs) · [Montage](../user/novafs/files.rs)

Le volume historique reste lisible. Le service distingue le profil de contenu historique et P9 dans `INFO` (octet 81, valeurs 0 et 9). Le champ de longueur des réponses `STAT` et `LOOKUP_AT` vaut 25 sur l’ancien profil et 73 sur P9. Les 25 premiers octets gardent leur interprétation. Les 48 octets suivants sont l’encodage P9 de `btime`, `mtime`, `ctime`, `atime` dans cet ordre. Chaque champ contient les secondes UTC signées sur 64 bits et les nanosecondes sur 32 bits, little-endian. La paire canonique (0, 0xffffffff) signale une date inconnue. Les pools avec clés, snapshots ou clones continuent à relever de l’outil NovaFS autonome. Le montage natif ne migre aucune image. [Réponse de métadonnées](../user/novafs/files/namespace.rs) · [Codec NovaFS](../../novaFS/src/time.rs)

La création date les quatre champs. Modifier le contenu actualise `mtime` et `ctime`. Renommer conserve l’identité, `btime` et `mtime` du fichier et actualise `ctime`. La modification des entrées d’un répertoire actualise ses dates selon le contrat du moteur. Les lectures applicatives emploient `read_access_id` et les énumérations `list_access_id`. Le moteur applique la politique du volume, y compris `noatime`, `relatime` et `strictatime`. Une lecture pouvant publier un accès et rencontrant une réservation d’écriture en attente renvoie `BUSY`. La lecture seule conserve les dates. Une défaillance d’E/S, une corruption ou une publication d’issue incertaine interrompt les opérations du montage. [Sémantique native](../../novaFS/src/fs.rs)

## Horloge civile

`CLOCK_UTC` ajoute le sélecteur `0x10044` aux 19 appels historiques. Les trois arguments valent zéro. La réponse fournit un statut et un nombre de secondes UTC depuis 1970. Un argument invalide rend `BAD_ARGUMENT` et une RTC inutilisable rend `REFUSED`. Les délais et l’ordonnancement conservent leurs compteurs monotones. Le service exige une lecture UTC valide pour ouvrir un P9 inscriptible et dater ses opérations. Le lancement QEMU initialise la RTC avec `-rtc base=utc,clock=host`. L’exactitude civile dépend de l’horloge de l’hôte.

Sur Q35, le noyau lit deux calendriers CMOS concordants hors de la fenêtre de mise à jour. Il vérifie la pile CMOS, le siècle, le mode BCD/binaire, les heures 12/24 et les bornes du calendrier. Des essais rapprochés pouvaient tous tomber dans une seule fenêtre de mise à jour. La lecture cède maintenant le CPU entre deux essais et borne l’attente avec le PIT. Sur ARM virt, le noyau vérifie les identifiants du PL031 avant de lire son compteur UTC non signé de 32 bits, limité à février 2106. Les deux sources ont une résolution d’une seconde. `ls` conserve les neuf décimales des dates plus précises produites sur macOS. [RTC Q35](../kernel/src/arch/x86_64/rtc.rs) · [RTC virt](../kernel/src/arch/aarch64/rtc.rs) · [Modèle CMOS de QEMU](https://github.com/qemu/qemu/blob/master/hw/rtc/mc146818rtc.c) · [Modèle PL031 de QEMU](https://github.com/qemu/qemu/blob/master/hw/rtc/pl031.c)

## Délégation

Le shell détermine les opérandes avec la même grammaire que l’exécutable, résout chaque sélection avec les droits de sa session, puis fournit un canal privé à l’enfant. Un fichier existant reste désigné par son identité. Une destination désigne un parent et un nom précis. La copie récursive délègue la lecture de l’arbre source et la création dans le nouvel arbre destination. Une simple énumération limite les recherches à ses entrées immédiates. Les deux canaux sont fermés après la collecte de l’enfant, y compris après une faute. Le catalogue ne recherche aucun exécutable dans le répertoire courant. Le terminal d’entrée n’est transmis qu’au pager et aux commandes interactives qui le demandent. [Sélection des droits](../../components/shell/capabilities.rs) · [Vérification par le service](../user/novafs/files/selected.rs)

Les limites persistantes restent de 32 niveaux, 255 octets par composant et 8 192 octets cumulés. Une requête de chemin accepte jusqu’à 1 024 octets UTF-8. Les parcours récursifs procèdent par composants. Le service refuse `..`. Le modèle de propriétaire, les groupes, les ACL, les signatures d’application et la surprotection relèvent des phases ultérieures.

## Options et comportements

Les douze programmes externes acceptent `--help`, `--version` et `--`. Le parseur accepte les options courtes groupées, les options longues documentées, et les valeurs séparées ou accolées, par exemple `-n 20`, `-n20` et `--lines=20`. Une option inconnue produit un diagnostic et un statut non nul. Les erreurs gardent les codes natifs, notamment 3 pour un argument invalide, 5 pour un refus, 20 pour une entrée absente et 27 pour une destination existante. [Analyseur et liste des options longues](../../components/libraries/options.rs)

| Programme | Options disponibles | Comportement et différences avec GNU |
|---|---|---|
| `ls` | `-a -A -l -h -R -r -t -S -1 -d -F`, `--color[=always\|auto\|never]` | Sur P9, le format long affiche type, taille, mtime UTC et nom. `-t` trie des secondes et fractions réelles, les égalités par nom UTF-8. Les dates inconnues suivent les dates connues, même avec `-r`. Sur le profil historique, le format d’affichage précédent reste disponible et le tri chronologique renvoie une erreur. `-a` inclut les entrées cachées et le répertoire courant. Le parent reste inaccessible. |
| `cp` | `-i -n -v -r -R` | Lit et écrit par tranches de 8 Kio, fragmentées sur l’IPC. Une candidate dans le répertoire destination reçoit les écritures et les synchronisations. Le renommage final publie son nom en une transaction après validation. Copier sur soi-même ou dans son descendant échoue. |
| `mv` | `-i -n -v` | Un renommage NovaFS conserve l’identité. Une destination répertoire reçoit le nom de la source. Le déplacement entre volumes reste indisponible. |
| `touch` | `-a -m -c` | Crée un fichier vide ou met à jour les champs choisis. La sélection par défaut couvre accès et modification. `-c` laisse une entrée absente. Un objet existant conserve tous ses octets. Les références et dates explicites (`-r -t -d`) restent indisponibles. |
| `more` | Espace, Entrée, `q` | Lecture progressive. Par défaut 24 lignes de 80 colonnes. `NOVA_MORE_LINES` et `NOVA_MORE_COLUMNS` configurent la géométrie lors de la construction. Le terminal série ne fournit pas les dimensions de sa fenêtre. |
| `wc` | `-l -w -c -m -L` | Compte les LF, mots, octets, scalaires UTF-8 et la largeur maximale affichée. Les tabulations avancent au prochain multiple de huit. Les mots sont séparés par les espaces Unicode ou U+2060. Les octets invalides comptent dans le total d’octets. Le calcul de largeur utilise `unicode-width` sans locale hôte. |
| `head`, `tail` | `-n -c -q -v`, `--lines`, `--bytes` | Dix lignes par défaut. Comptes décimaux non négatifs. Les deux programmes acceptent plusieurs fichiers. `tail` recherche la borne en remontant depuis la fin par blocs de 704 octets et affiche vers l’avant. Une ligne finale sans LF est conservée. `-f`, `-F`, les suffixes multiplicateurs et les comptes signés restent indisponibles. |
| `cat`, `mkdir`, `rm`, `pwd` | Options communes, `pwd -L/-P` | `cat` accepte plusieurs fichiers. `mkdir` crée une entrée et `rm` supprime un fichier. `pwd` interroge le service. Les options de numérotation de `cat`, `mkdir -p/-m` et `rm -r/-f` restent indisponibles. |

`cp` et `mv` refusent par défaut une destination existante. `-n` la conserve. `-i` permet de renoncer avec succès. Après une réponse affirmative, le programme signale le refus de remplacement car le moteur publie seulement vers un nom libre. Les options `-f`, `-u`, `cp -a` et `cp -p` restent indisponibles. La fermeture du canal tente de retirer la candidate d’une copie abandonnée après vérification de son identité. Une coupure brutale précédant sa publication peut laisser une entrée `.nova-copie-…`. Les écritures déjà synchronisées de cette candidate occupent de l’espace. La publication finale conserve les deux barrières CoW de NovaFS.

La couleur s’applique aux noms des répertoires. Le terminal émet une palette fixe et remet le style à zéro après le nom. Le mode `auto` vise la sortie interactive série disponible dans cette version. Les noms et le contenu affiché passent par le filtre des caractères de contrôle. Un octet UTF-8 invalide apparaît sous forme échappée. La copie de fichiers préserve leurs octets indépendamment de ce rendu.

## Édition et alias

L’éditeur déplace le curseur par scalaire UTF-8, gère insertion, Retour arrière, Suppr, gauche/droite, début/fin et un défilement horizontal pour les longues lignes. Il redessine son contenu avec les gestes de la console native. La ligne accepte 2 048 octets et 32 arguments. Haut et Bas parcourent un historique privé et restaurent la ligne en cours et son curseur. Les lignes vides et les doublons consécutifs en sont exclus. La capacité par défaut est de 64 lignes. La variable de construction `NOVA_HISTORY_ENTRIES=0` désactive le stockage. L’historique et les alias expirent avec le shell.

```sh
alias l='ls -latr --color'
l /documents
unalias l
```

L’alias stocke des mots issus du lexer. L’expansion conserve les arguments supplémentaires, détecte les cycles et passe ensuite par le catalogue et la délégation habituels. [Éditeur](../../components/shell/line_editor.rs) · [Alias](../../components/shell/aliases.rs)

## Reproduction

Sur le Mac Apple Silicon de qualification, GNU coreutils fournit `gls`, `ghead`, `gtail` et `gwc`. Chaque banc crée ses images dans un répertoire temporaire. La démo interactive conserve son image d’une session à l’autre. Pour essayer P9 quand un ancien volume de démo existe déjà, créer explicitement une image sous un nouveau nom.

```sh
make -C novaOS ARCH=x86_64 cli-image IMAGE="$PWD/demo-p9.raw" SECTOR=512
make -C novaOS ARCH=x86_64 cli IMAGE="$PWD/demo-p9.raw" SECTOR=512
make -C novaOS ARCH=aarch64 cli
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test lifecycle-test elf-test ipc-races ipc-perf
    make -C novaOS ARCH="$arch" block-test file-test namespace-test cli-test cli-p9-test
    python3 novaFS/lab/p51-validation-suites.py "$arch"
done
python3 novaFS/lab/p9-interop.py
python3 novaFS/lab/p8-validation.py cli --sortie "${TMPDIR:-/tmp}/nova-p8-cli.json"
```

Pour les deux constructions portables, utiliser le répertoire `novaFS/` afin que Cargo lise les réglages cryptographiques du projet.

```sh
cd novaFS
for target in x86_64-unknown-none aarch64-unknown-none-softfloat; do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --offline --lib --all-features --target "$target"
done
```

## Qualification

Hôte macOS 27.2 (26B123), Rust stable 1.99.1, GNU coreutils 9.12, QEMU 11.1.3 en TCG mono-CPU, invités de 256 Mio. Les deux profils RTC ont été comparés à l’heure civile du Mac. Le lecteur Darwin de l’oracle utilise l’architecture opposée à celle de l’invité. Avant chaque essai, le banc calcule l’empreinte de l’image historique, la migre dans une deuxième image et contrôle l’empreinte de la source. Un montage de diagnostic P9 en lecture seule vérifie l’identifiant du volume et l’intégralité de l’image avant et après le démarrage. [Banc QEMU](../tests/cli_p9.py) · [Oracle indépendant de la CLI](../tests/cli_temps_oracle.rs)

Les sorties attendues pour le comptage Unicode et les plages de lignes proviennent aussi de GNU coreutils sur Darwin. Pour le tri, deux fichiers ont pour mtime le 1ᵉʳ janvier 2001 à 00:00:00 UTC, avec les fractions .200000123 et .800000001. Les essais confrontent les ordres de `gls` et de NovaOS, les couleurs, les anciennes dates inconnues et la stabilité de l’identité après deux renommages. Après un arrêt forcé de QEMU, l’oracle vérifie les structures, les dates et les 67 509 octets de la copie. Une nouvelle VM reprend les lectures et les mutations. La simulation et la relecture du disque hôte exercent le contrat de persistance de QEMU. La perte d’alimentation physique de l’hôte reste hors de cette campagne.

Pour les cinq captures graphiques historiques, les empreintes v0.14.0 ont été calculées à partir des **captures v0.13.0 dont l’empreinte a été vérifiée**. Seuls les pixels du glyphe 3 dans la bannière ont été remplacés par le glyphe 4 de la police existante. La comparaison à l’écran réel continue de porter sur tous les pixels.

| NovaFS autonome v0.9.13 | macOS ARM64 natif | macOS x86_64 sous Rosetta |
|---|---:|---:|
| Tests standards / instrumentés réussis | 192 / 193 | 192 / 193 |
| Tests ignorés par défaut dans chaque passage | 4 | 4 |
| Opérations différentielles / durée en secondes | 1 000 000 / 25,263 | 1 000 000 / 41,184 |
| Publications / pénuries injectées / remontages audités | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |
| Coupures / erreurs d’E/S / déchirures P5 par géométrie | 342 / 76 / 6 | 342 / 76 / 6 |

Ces exécutions couvrent la récupération, la corruption, les snapshots et clones, le chiffrement, les migrations et le scrub. Le banc P9 croisé a vérifié les quatre combinaisons architecture/secteur, 59 017 octets et deux restaurations par combinaison. Le banc historique P8 a également relu les images avec l’autre architecture. Les compilations verrouillées du cœur complet `no_std + alloc` ont abouti sur les deux cibles nues. Les licences de NovaOS et des composants restent MIT, celle de NovaFS reste MPL-2.0. Les ISO incluent aussi les avis relatifs au calcul de largeur Unicode.

### Résultats par invité

| Contrôle | x86_64 | AArch64 |
|---|---:|---:|
| Scénarios historiques, dont IPC, mémoire et préemption | 25/25 | 25/25 |
| Groupes bloc (DMA, E/S et FLUSH) | 24/24 | 24/24 |
| Contrôles fichiers v0.11 | 44/44 | 44/44 |
| Contrôles d’espace de noms v0.12 | 30/30 | 30/30 |
| Familles/orphelins, ELF multipage et courses IPC | 3/3 | 3/3 |
| CLI v0.13, géométries 512 et 4 096 | 2/2 | 2/2 |
| CLI P9, géométries 512 et 4 096 | 2/2 | 2/2 |

Le banc CLI P9 a exercé les douze commandes externes et l’ELF de qualification. Il rapproche les PID lancés des sorties Ring 3 ou EL0 et des collectes observées. La sonde de métadonnées vérifie également le refus d’ouverture inscriptible sur un répertoire délégué en énumération. La session suivante vérifie l’expiration de l’alias et de l’historique. Les quatre passages contrôlent les dates du fichier copié, la conservation des données du fichier touché, les erreurs de syntaxe, les collisions de destination, les refus de traversée et la reprise du shell après une faute enfant.

### Mesures de qualification

Un essai par case, horloge monotone hôte de l’envoi de la commande jusqu’à l’invite suivante. Ces latences comprennent le lancement, la délégation, les accès disque, la sortie série et la collecte. Le remontage est mesuré dans la nouvelle VM, entre les deux marqueurs entourant MOUNT. STAT est la moyenne de 16 appels consécutifs depuis un enfant déjà lancé. Les cycles invités sont convertis avec le compteur calibré. Les percentiles IPC portent sur les allers-retours instrumentés de la session de remontage. Les deux architectures ont parfois partagé l’hôte pendant la qualification.

| Invité / secteur | Montage (ms) | STAT (ms) | ls -latr (ms) | mv (ms) | touch (ms) | IPC p50 (µs) | IPC p99 (µs) |
|---|---:|---:|---:|---:|---:|---:|---:|
| x86_64 / 512 | 3320,1 | 0,300 | 195,9 | 121,1 | 83,2 | 114,7 | 180,2 |
| x86_64 / 4096 | 2428,9 | 0,316 | 130,7 | 86,7 | 56,0 | 115,1 | 180,9 |
| aarch64 / 512 | 3877,6 | 0,252 | 217,7 | 147,0 | 86,8 | 131,1 | 196,6 |
| aarch64 / 4096 | 2650,6 | 0,254 | 166,4 | 97,6 | 62,1 | 131,1 | 196,6 |

Les débits utiles ci-dessous sont exprimés en Kio/s. La copie publie 67 509 octets. Le comptage parcourt les mêmes octets. Les plages de vingt lignes de head et tail représentent chacune 80 octets rendus. Pour ces courtes commandes, le lancement et l’affichage pèsent dans le résultat.

| Invité / secteur | cp | wc | head | tail |
|---|---:|---:|---:|---:|
| x86_64 / 512 | 8,48 | 10,90 | 3,24 | 2,70 |
| x86_64 / 4096 | 11,82 | 14,55 | 4,03 | 3,27 |
| aarch64 / 512 | 7,42 | 9,45 | 2,81 | 2,15 |
| aarch64 / 4096 | 10,35 | 13,10 | 3,59 | 2,62 |

Mémoire physique invitée en Kio. Le noyau inclut son image, les tables, les piles superviseur, le tas, le Buddy et l’IPC. Les pages privées des processus incluent la pile. Le pic échantillonné couvre la première session arrêtée brutalement. Le pic du remontage est calculé sur chaque allocation de cadre, y compris pendant la seconde copie, jusqu’à la collecte.

| Invité / secteur | Noyau à l’invite | Shell | Service NovaFS | Enfant au lancement | Total à l’invite | Pic échantillonné | Pic du remontage | Allocations de cadres au remontage |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| x86_64 / 512 | 3348 | 384 | 640 | 280–300 | 4460 | 4864 | 4988 | 1461 |
| x86_64 / 4096 | 3348 | 384 | 640 | 280–300 | 4460 | 4864 | 4988 | 1461 |
| aarch64 / 512 | 3224 | 380 | 624 | 280–300 | 4316 | 4720 | 4844 | 1456 |
| aarch64 / 4096 | 3224 | 380 | 624 | 280–300 | 4316 | 4720 | 4844 | 1456 |

Le framebuffer ARM retient 1 876 Kio supplémentaires en RAM. Le RSS de QEMU sur macOS est extérieur à ces compteurs. Les quatre dernières commandes identiques ont retrouvé les mêmes compteurs de pages et du Buddy. Après fermeture, les pages des processus et de l’IPC reviennent à zéro. Le premier démarrage du stockage conserve trois cadres de tables sur les deux architectures et neuf cadres DMA supplémentaires sur x86_64. Aucun accroissement par commande n’a été observé dans cette séquence. Les JSON et les journaux série sont produits sous `novaOS/build/` par les commandes de reproduction.
