# Shell et commandes natives de NovaOS v0.13.0

## Du terminal au volume

Le profil `cli` publie quatre processus. Le shell monte le volume en envoyant une requête au service NovaFS. Celui-ci utilise le relais bloc et le pilote VirtIO. Le noyau contrôle le DMA et les espaces d’adressage. La console série est COM1 sur x86_64 et PL011 sur AArch64. Sans disque, le profil graphique de diagnostic conserve son clavier AZERTY x86_64.

En v0.12.0, le superviseur pouvait charger des ELF multipages et un parent pouvait attendre un enfant autorisé. Il manquait un lancement à l’initiative de l’utilisateur, une entrée de terminal utilisateur et une session NovaFS dont les droits puissent être réduits à l’objet choisi. La v0.13.0 ajoute deux appels aux 17 sélecteurs existants et un message au service de fichiers. Les structures persistantes du moteur NovaFS v0.9.12 gardent le format P8.

Le workspace des composants est en Rust `no_std + alloc`. Sur les deux cibles nues, son graphe Cargo comprend `dlmalloc 0.2.14`, `cfg-if 1.0.5` et `spin 0.9.9`. Son allocateur privé utilise `HEAP_RESIZE`, comme le service NovaFS. La licence MIT du shell, des commandes et de la bibliothèque accompagne les ISO sous `licenses/cli-MIT.txt`.

Le catalogue du lanceur associe des numéros à des images ELF embarquées à la compilation. Le shell résout un nom dans son catalogue explicite. Un nom tel que `./pwd` ne lance rien. Chaque commande dispose de son propre PID, de ses propres pages, d’une pile gardée et d’un tas extensible. Le chargeur conserve les protections W^X. Le manifeste de `components/` sélectionne l’ELF du shell. Un autre interpréteur peut le remplacer à la construction en utilisant les mêmes droits initiaux et le même protocole. Le catalogue de cette version est statique.

### Saisie et exécution

| Module | Responsabilité |
|---|---|
| `shell/input` | Décoder les octets UART en scalaires UTF-8 ou en touches. Entrée accepte CR, LF et CRLF. Les flèches et Ctrl-B/Ctrl-F déplacent le curseur. |
| `shell/line_editor` | Insérer, effacer avec Retour arrière, déplacer le curseur et valider la ligne. L’invite est `nova> `. |
| `shell/lexer` | Traiter les espaces ASCII, les guillemets simples ou doubles et les arguments vides. La barre oblique inverse protège le caractère suivant hors guillemets simples. |
| `shell/parser` | Construire une invocation avec le nom et le vecteur d’arguments. Une ligne vide redonne l’invite. |
| `shell/executor`, `builtins`, `capabilities` | Choisir l’interne ou l’ELF, préparer l’autorité nécessaire, lancer, attendre, restituer les ressources. |
| `libraries/process`, `console` | Encoder les arguments et le résultat d’attente, séparer les flux de sortie et d’erreur, rendre le texte imprimable. |

La ligne accepte 2 048 octets et 32 arguments après le nom. L’éditeur déplace le curseur par scalaire Unicode. Sa fenêtre de 56 octets utilise le retour chariot pour se redessiner. La segmentation des graphèmes et la largeur des colonnes dépendent du terminal. Un UTF-8 invalide, un contrôle interdit, une ligne trop longue, un échappement incomplet ou un guillemet non fermé font rejeter la ligne entière. L’invite suivante reste utilisable. Le shell exécute une seule commande au premier plan. L’historique, les pipes, les redirections, les tâches et le langage de script restent à développer.

### Contrat de lancement et de console

- `SPAWN` (`0x10042`) reçoit le numéro d’image, l’adresse utilisateur d’un paquet et sa longueur. Cinq mots little-endian de 64 bits précèdent les arguments (40 octets). Ils portent les deux handles IPC locaux du parent, le masque stdout/stderr, l’erreur éventuelle de préparation et la longueur encodée. Le corps contient un compteur `u8` puis, pour chaque argument, une longueur `u16` little-endian et ses octets UTF-8. Le maximum du corps est 2 560 octets.
- Le noyau vérifie le droit de lancer, le catalogue, les droits demandés, l’absence d’une attente antérieure et la lisibilité de l’ensemble. Il copie le paquet, crée l’espace privé et dérive les deux handles avant de rendre le fils exécutable hors de la section critique. Si la seconde dérivation échoue, il retire le fils et les ressources déjà admises. La valeur de succès est une clé d’attente propre à ce parent.
- Le fils récupère son endpoint d’envoi via le bootstrap IPC et son endpoint de réception dans le premier mot de sa page de configuration privée. Les valeurs des handles du parent sont effacées. Le runtime décode les arguments dans la page de données de cet ELF. Une erreur de préparation lance la commande avec son diagnostic et sans autorité NovaFS.
- `PROCESS_WAIT` restitue le type de terminaison et le code. Un enfant en faute est collecté. Le shell ferme les deux racines IPC, libère le handle source et affiche l’erreur avant la prochaine invite.
- `CONSOLE` (`0x10043`) sépare entrée, sortie, erreur et gestes d’édition. L’entrée renvoie un octet du terminal. Les écritures sont bornées à 256 octets par appel et contrôlées en UTF-8. Les flux de sortie et d’erreur aboutissent au même UART physique avec des droits distincts. Seul le shell reçoit l’entrée et l’édition. Les commandes d’affichage reçoivent stdout et stderr, les mutations reçoivent stderr. La qualification ajoute un droit de sonde absent de la session interactive normale.

### Droits délégués et commandes

Le service reçoit deux capacités par le mécanisme IPC de transfert existant, puis le message `DELEGATE` (opération de fichiers 21). Il vérifie la session montée, le type, les droits du handle source et l’absence de transaction d’écriture en attente. Il crée une session limitée à ces endpoints. Le fils reçoit SEND vers le service et RECEIVE pour la réponse. Il ne reçoit ni GRANT ni MANAGE. Les handles du protocole ne donnent accès à un objet qu’au sein de cette session. Le service contrôle chaque opération. Le décès du fils et la fermeture des racines après l’attente mettent fin à la délégation.

| Commande | Opération réelle et autorité |
|---|---|
| `pwd` | Lit `GETCWD` par fragments depuis la session NovaFS. Le répertoire courant est un objet du service. |
| `cd chemin` | Interne au shell. Ouvre un répertoire, effectue `CHDIR` puis ferme le handle temporaire. |
| `ls [-l] [chemin]` | Énumère un seul répertoire et consulte les métadonnées de ses enfants directs. Affiche nom et type. L’option détaillée ajoute identifiant et taille fournis par NovaFS. |
| `cat fichier` | Reçoit uniquement la lecture du fichier choisi. Affiche le flux par tranches de 704 octets, avec au plus trois octets UTF-8 reportés. |
| `head [-n N] fichier` | Utilise la même lecture limitée au fichier. Affiche dix lignes par défaut, respecte N et la fin de fichier même sans saut de ligne. |
| `mkdir chemin` | Le service fixe le parent et le nom avant lancement. Une création peut être demandée sur cette seule entrée. Un doublon produit « existe déjà ». |
| `rm chemin` | Une suppression sur le nom fixé dans le parent délégué. Les répertoires sont refusés. |
| `exit` | Interne. Ferme la session de fichiers, attend les services par le superviseur et termine le shell. |

Les mutations réutilisent les transactions CoW et les confirmations F1/F2 du service NovaFS. Le service rejette un autre opcode ou une autre cible, même si le programme connaît un identifiant valable dans la session du shell. La portée d’un fichier ouvert est fixée par son identité. Les noms suivent les règles P8, avec 255 octets par composant, 1 024 octets par requête et 32 niveaux. Les composants `..` sont refusés. Une suite de `cd` peut dépasser 1 024 octets cumulés. Les erreurs d’autorité, de syntaxe, d’accès, de lancement et de terminaison restent distinguées. Les contrôles C0/C1 et les marqueurs bidirectionnels dangereux sont filtrés. `cat` et `head` rendent visibles les contrôles du fichier et les octets UTF-8 invalides.

Le service sérialise les délégations de cette session de premier plan. Les identités d’utilisateur, ACL et règles de surprotection restent des travaux futurs. Le catalogue et la délégation actuelle ne confèrent aucun privilège implicite lié au nom d’un programme ou à une identité utilisateur. [Contrat de l’espace de noms](namespace-v0.12.0.md) · [IPC à capacités](ipc-architecture.md)

## Reproduction sur macOS

Depuis la racine du monorepo, avec les prérequis de construction de NovaOS et les dépendances Cargo du laboratoire disponibles hors ligne.

```sh
make -C novaOS ARCH=x86_64 cli
make -C novaOS ARCH=aarch64 cli SECTOR=4096
```

Chaque commande crée son image de démonstration au premier démarrage, dans `novaOS/build/shell-ARCH-SECTEUR.raw`, puis la réutilise. Pour choisir un nouveau chemin de fichier image et l’ouvrir en lecture/écriture, utiliser par exemple les commandes suivantes. Le secteur au lancement doit correspondre à la géométrie choisie à la création.

```sh
make -C novaOS cli-image IMAGE=/tmp/nova-cli.raw SECTOR=512
make -C novaOS ARCH=x86_64 cli IMAGE=/tmp/nova-cli.raw SECTOR=512
```

Le générateur refuse de remplacer un fichier existant. Le laboratoire initialise `/documents/exemple.txt` avec les sept octets `NovaOS\n`. À la première session, `mkdir documents` signale le doublon et renvoie 27. Le scénario continue avec `cd documents`, `pwd`, `ls`, `cat exemple.txt`, `head -n 1 exemple.txt`, `rm exemple.txt` et `ls`. Pour créer un nouveau répertoire, utiliser `mkdir 'répertoire neuf'`. `exit` ferme la session. Ctrl-A puis X quitte QEMU. Après suppression, le fichier d’exemple reste absent au redémarrage sur cette image.

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test lifecycle-test block-test elf-test ipc-races file-test namespace-test cli-test || exit 1
done
```

Les journaux de la session série sont dans `novaOS/build/cli-ARCH-SECTEUR-*.serial` et les durées dans `novaOS/build/cli-ARCH-SECTEUR.json`. Le banc crée une image temporaire dédiée à chaque combinaison.

## Qualification fonctionnelle

La campagne utilise macOS 27.2 ARM64, Rust 1.99.1 et QEMU 11.1.3 en TCG, avec un vCPU et 256 Mio configurés par VM. Chaque géométrie utilise un volume autonome P8 jetable de 512 blocs NovaFS de 4 Kio. Les deux tailles de secteur testées sont 512 et 4 096 octets.

| Contrôles du scénario CLI | Preuve vérifiée |
|---|---|
| 1–5. Démarrage, services, shell, invite et entrée | L’hôte attend la publication de la chaîne bloc et le premier prompt, envoie une ligne par l’UART et lit la réponse. |
| 6–12. Commandes | Le scénario lit le contenu exact de `exemple.txt`, refuse la création en doublon, crée un répertoire neuf, supprime le fichier et vérifie la deuxième énumération. |
| 13–16. Analyse et chemins | Nom inconnu ou `./pwd`, ligne vide, arguments multiples, guillemets, échappement, chemin absolu et relatif. Tests de suppression d’un caractère accentué, d’insertion entre deux caractères, d’UTF-8 invalide et de dépassement de ligne. |
| 17–18. Autorité | Le service refuse `..`. Un ELF ayant uniquement la vue du répertoire courant ne peut ni ouvrir un fichier ni supprimer une entrée. Le noyau lui refuse l’entrée du terminal et le lancement. La révocation du canal rend le droit inutilisable. |
| 19–23. Processus | Les traces du noyau associent chaque lancement au PID sorti de Ring 3 ou EL0 et collecté. Une faute volontaire dans le fils de qualification est suivie d’un `pwd` réussi. Quatre répétitions à chaud comparent les pages privées, les cadres libres et l’occupation du Buddy. |
| 24–25. Redémarrage et persistance | Une nouvelle VM vérifie le répertoire créé et l’absence du fichier supprimé. Une troisième VM crée `/confirme` et subit un SIGKILL à la prochaine invite. Une quatrième remonte l’image et vérifie `pwd` dans ce répertoire. |

Les tests de lecture font traverser la limite des 704 octets à un caractère UTF-8, contrôlent le dernier mot sans saut de ligne et vérifient que les séquences de contrôle dans un fichier sont rendues inertes. Le répertoire du 32ᵉ niveau est accessible, la création du 33ᵉ est refusée. Les tests utilisent le vrai shell et de vrais ELF dans les quatre VM. Seule la première active la télémétrie et la sonde de qualification.

L’oracle macOS relit les ancrages, exécute le vérificateur de l’image et compare les octets de `/lignes`. Les sessions passent de G40 à G43 après les mutations et la collecte de blocs à la fermeture. Le remontage conserve G43. La création confirmée puis l’arrêt forcé laissent G44, vérifiée dans un nouveau QEMU avant et après une fermeture normale. Cette expérience porte sur le disque virtuel en cache writeback. Une coupure électrique de l’hôte ou du support physique reste hors de sa portée.

Un essai supplémentaire a exécuté la commande de démarrage manuel sur un pseudo-terminal macOS pour chaque architecture. Après 67 secondes sans entrée, `pwd` a rendu `/`, `exit` a fermé la session et Ctrl-A puis X a terminé QEMU. Une deuxième tentative de création au même emplacement a échoué en conservant l’image octet pour octet.

Le cœur NovaFS et son format sont identiques à la base v0.12.0. La campagne utilise les outils Darwin pour produire et relire les images. Les suites autonomes P8 et le million d’opérations différentielles conditionnées à une modification du moteur n’ont pas été rejoués.

### Résultats par architecture

| Campagne exécutée | x86_64 | AArch64 |
|---|---:|---:|
| CLI, secteurs 512 et 4 096, quatre VM par géométrie | 2/2, Ring 3 | 2/2, EL0 |
| Démarrage, affichage, mémoire, ordonnanceur, processus, ABI et IPC historiques | 25/25 | 25/25 |
| Groupes bloc, erreurs d’E/S, FLUSH et révocation DMA | 24/24 | 24/24 |
| Contrôles fichiers v0.11.0, avec coupures transactionnelles | 44/44 | 44/44 |
| Contrôles d’espace de noms v0.12.0 sur les deux géométries | 30/30 | 30/30 |
| Profils supplémentaires ELF multipage et courses IPC | 2/2 | 2/2 |
| Collecte des familles et des orphelins | 128 processus sur 64 familles | 128 processus sur 64 familles |

Le banc d’espace de noms regroupe ses 30 contrôles en 12 bilans par géométrie. Les tests CLI ont démarré 16 VM en tout. Chaque journal instrumenté atteste le niveau de privilège du shell (PID 4) et des enfants par leur sortie ou leur exception matérielle. L’unique changement des cinq empreintes du banc graphique historique correspond au glyphe de version dans la bannière. La comparaison continue de couvrir la totalité des captures de 800 × 600 pixels.

### Méthode de mesure

L’horloge monotone du Mac mesure le démarrage de QEMU jusqu’à l’invite, hors assemblage de l’ISO. L’intervalle « chaîne → invite » commence à la publication des quatre processus et inclut le montage. Pour une commande, le banc chronomètre l’envoi de la ligne jusqu’au retour du prompt. La mesure inclut l’écho, les contrôles de droits, le chargement, les E/S, les traces de qualification et la collecte. Les opérations de métadonnées incluent la confirmation transactionnelle. Chaque valeur par commande correspond à un essai sur l’image indiquée. Le lancement et la terminaison utilisent le compteur de l’invité et sa fréquence mesurée. La terminaison inclut le délai entre l’entrée dans le chemin de sortie et la récupération du résultat par le parent.

Les histogrammes `IPC hist type=aller_retour` couvrent les échanges instrumentés de la session, y compris les échanges du service pendant le montage. Les latences sont sensibles à l’émulation TCG et à la charge du Mac pendant les régressions parallèles.

Pour la mémoire, le shell et les commandes sont comptés en pages privées de 4 Kio, pile comprise. Le relevé du shell est pris à l’invite après échauffement, celui des commandes à la création. Le noyau à l’invite comprend l’image, les tables, les piles superviseur, le tas, les métadonnées Buddy et les pages IPC. Le total résident comprend aussi les processus et le DMA. Le pic est calculé à partir du minimum des cadres libres sur toute la session. Sur AArch64, le framebuffer retient en plus 1 876 Kio de RAM système. Les compteurs décrivent la mémoire physique invitée et n’incluent pas le RSS de l’émulateur sur le Mac.

Pour examiner les durées individuelles et les compteurs de l’invité après la campagne, remplacer l’architecture et la géométrie dans ces commandes.

```sh
python3 -m json.tool novaOS/build/cli-aarch64-4096.json
rg 'CLI (spawn|collecte|repos)|RAM phase=|IPC hist type=aller_retour|BLOC memoire' novaOS/build/cli-aarch64-4096-session.serial
```

### Mesures QEMU

| Invité | Secteur (octets) | Démarrage → invite (s) | Chaîne → invite (s) | ls (ms) | cat (ms) | mkdir (ms) | rm (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| x86_64 | 512 | 12,09 | 10,16 | 8,309 | 23,141 | 82,628 | 107,187 |
| x86_64 | 4 096 | 8,94 | 7,04 | 5,772 | 18,851 | 58,267 | 70,719 |
| AArch64 | 512 | 18,52 | 12,87 | 6,607 | 29,605 | 104,705 | 129,303 |
| AArch64 | 4 096 | 14,63 | 8,96 | 5,897 | 25,200 | 72,656 | 103,028 |

`ls` énumère `/documents` avant suppression. `cat` lit `exemple.txt`, `mkdir` crée le nom neuf et `rm` supprime l’exemple. Chaque ligne du tableau suivant compte 33 enfants lancés et 33 résultats collectés, y compris les essais négatifs.

| Invité | Secteur | Lancement médian (µs) | Sortie → attente satisfaite, médiane (µs) | Échantillons IPC | Aller-retour p50 (µs) | p99 (µs) |
|---|---:|---:|---:|---:|---:|---:|
| x86_64 | 512 | 192,6 | 250,4 | 90 146 | 114,4 | 179,8 |
| x86_64 | 4 096 | 183,9 | 227,8 | 63 432 | 114,6 | 180,1 |
| AArch64 | 512 | 305,0 | 736,0 | 90 146 | 147,5 | 229,4 |
| AArch64 | 4 096 | 356,0 | 786,0 | 63 432 | 147,5 | 262,1 |

La mémoire mesurée est identique pour les deux géométries d’un même invité.

| Invité | Noyau à l’invite (Kio) | Shell (Kio) | ELF de commande à la création (Kio) | Total à l’invite (Kio) | Pic résident (Kio) | Pic avec framebuffer en RAM (Kio) | Allocations de cadres |
|---|---:|---:|---:|---:|---:|---:|---:|
| x86_64 | 3 076 | 356 | 280–284 | 4 012 | 4 476 | 4 476 | 4 158 |
| AArch64 | 2 960 | 356 | 280–284 | 3 880 | 4 344 | 6 220 | 4 154 |

`mkdir` et `rm` occupent 280 Kio à leur lancement. Les quatre commandes d’affichage en occupent 284. Le compteur global saisit aussi les engagements de pages ultérieurs pendant l’exécution. À quatre retours successifs à l’invite, 62 935 cadres sont libres sur x86_64 et 62 369 sur AArch64. Le shell conserve 89 pages. L’occupation Buddy est stable à 42 016 et 43 040 octets respectivement après normalisation de son cache.

Après `exit`, les deux invités ont zéro processus, zéro page utilisateur et zéro page IPC. L’occupation du Buddy revient à 9 248 octets sur x86_64 et à 10 272 sur AArch64, comme avant le lancement. Il reste trois cadres supplémentaires de tables sur chaque architecture et neuf cadres supplémentaires de DMA sur x86_64. Ces ressources du premier démarrage du stockage sont identifiables dans le bilan. Aucune croissance par commande n’a été observée sur les répétitions mesurées. [Comptabilité du stockage en v0.12.0](namespace-v0.12.0.md)
