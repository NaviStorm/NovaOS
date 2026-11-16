//! Catalogue v1 des pages publiques. Les versions suivent le paquet qui construit le binaire.
use alloc::{format, string::String};
use core::fmt::Write;

#[path = "documentation_locale.rs"]
mod locale;
pub use locale::Language;

#[derive(Clone, Copy)]
pub struct Text(&'static str, &'static str);
impl Text {
    const fn same(value: &'static str) -> Self {
        Self(value, value)
    }
    pub fn get(self, lang: Language) -> &'static str {
        lang.select(self.0, self.1)
    }
}

pub struct Page {
    pub name: &'static str,
    pub provider: &'static str,
    pub description: Text,
    pub syntax: Text,
    pub arguments: Text,
    pub examples: Text,
    pub rights: Text,
    pub effects: Text,
    pub limits: Text,
    pub topics: &'static [(Text, Text)],
}
pub static PAGES: &[Page] = &[
    Page {
        name: "sed",
        provider: "nova-sed",
        description: Text("Edit a stream of records.", "Éditer un flux d’enregistrements."),
        syntax: Text("sed [OPTIONS] SCRIPT [FILE]", "sed [OPTIONS] SCRIPT [FICHIER]"),
        arguments: Text(include_str!("../sed/help.en"), include_str!("../sed/help.fr")),
        examples: Text::same("sed -n '1,3p' fichier.txt"),
        rights: Text("Documentation needs no file or pipe capability.", "La consultation ne demande aucune capacité fichier ou pipe."),
        effects: Text("Help and manual only write to the terminal.", "L’aide et le manuel écrivent uniquement au terminal."),
        limits: Text("ELF admission pending. P9 blocks -i. Script file access and shell execution require separate capabilities.",
                    "Admission de l’ELF en attente. P9 bloque -i. Les fichiers nommés par le script et le shell exigent des capacités distinctes."),
        topics: &[],
    },
    Page {
        name: "nsh",
        provider: "shell",
        description: Text(
            "Interpret NovaOS commands and scripts.",
            "Interpréter des commandes et des scripts NovaOS.",
        ),
        syntax: Text(
            "nsh -c 'command' | nsh [--] script.nsh",
            "nsh -c 'commande' | nsh [--] script.nsh",
        ),
        arguments: Text(
            "UTF-8 command text or a script path. -c interprets the supplied text.",
            "Texte UTF-8 ou chemin de script. -c interprète le texte.",
        ),
        examples: Text("nsh -c 'echo Hello'", "nsh -c 'echo Bonjour'"),
        rights: Text(
            "Script commands use the session's capabilities.",
            "Les commandes du script disposent des droits de la session.",
        ),
        effects: Text(
            "Changes depend on the commands executed.",
            "Les modifications dépendent des commandes exécutées.",
        ),
        limits: Text(
            "Returns the last command's status. Invalid syntax: 3. Native pipelines connect cat to wc -l or head -n 1.",
            "Code de la dernière commande. Syntaxe invalide : 3. Les pipes natifs relient cat à wc -l ou head -n 1.",
        ),
        topics: &[],
    },
    Page {
        name: "service",
        provider: "intégrée à nsh",
        description: Text(
            "Manage services in the active catalogue.",
            "Administrer les services du catalogue actif.",
        ),
        syntax: Text(
            "service <subcommand> [name|id] [capability]",
            "service <sous-commande> [nom|id] [capacité]",
        ),
        arguments: Text(
            "Canonical name or numeric ID. service enable|disable SERVICE; service capabilities restrict|restore SERVICE CAPABILITY. The catalogue defines the available rights.",
            "Nom canonique ou ID numérique. service enable|disable SERVICE ; service capabilities restrict|restore SERVICE CAPACITÉ. Les droits proviennent du catalogue.",
        ),
        examples: Text::same(
            "service list\nservice restart 9\nservice restart storage-info\nservice capabilities 9\nservice capabilities storage-info\nservice capabilities restrict storage-info private-tmp\nservice capabilities restore storage-info private-tmp",
        ),
        rights: Text(
            "Read authority for queries, operation authority for starting or stopping, maintenance authority for configuration.",
            "Lecture pour consulter, exploitation pour démarrer ou arrêter, maintenance pour configurer.",
        ),
        effects: Text(
            "Stopping a critical provider can interrupt dependent services. enable|disable affects the next boot. Capability restrictions and restorations apply to the next instance.",
            "Arrêter un fournisseur critique peut interrompre ses dépendants. enable|disable agit au prochain démarrage. service capabilities restrict|restore applique le changement à la prochaine instance.",
        ),
        limits: Text(
            "Access denied: 5. Unknown service: 20. If the outcome is uncertain, read the stored state before deciding what to do.",
            "Accès refusé : 5. Service inconnu : 20. Issue incertaine : relire avant toute décision.",
        ),
        topics: &[
            (
                Text::same("help"),
                Text("Show general help.", "Afficher l’aide générale."),
            ),
            (
                Text::same("list"),
                Text(
                    "List IDs, names, states and startup policies.",
                    "Lister ID, nom, état et démarrage.",
                ),
            ),
            (
                Text::same("status SERVICE"),
                Text(
                    "Show state and configuration generations.",
                    "Afficher l’état et les générations de configuration.",
                ),
            ),
            (
                Text::same("info SERVICE"),
                Text(
                    "Query the current service.",
                    "Interroger le service courant.",
                ),
            ),
            (
                Text::same("dependencies SERVICE"),
                Text(
                    "List required providers.",
                    "Lister les fournisseurs obligatoires.",
                ),
            ),
            (
                Text::same("policy SERVICE"),
                Text("Read the restart policy.", "Lire la politique de relance."),
            ),
            (
                Text::same("start SERVICE"),
                Text("Start the service now.", "Démarrer maintenant."),
            ),
            (
                Text::same("stop SERVICE"),
                Text("Stop the current instance.", "Arrêter l’instance courante."),
            ),
            (
                Text::same("restart SERVICE"),
                Text(
                    "Restart with the stored capabilities.",
                    "Relancer avec les capacités enregistrées.",
                ),
            ),
            (
                Text::same("enable SERVICE"),
                Text(
                    "Enable automatic startup on the next boot.",
                    "Activer le prochain démarrage automatique.",
                ),
            ),
            (
                Text::same("disable SERVICE"),
                Text(
                    "Disable automatic startup on the next boot.",
                    "Désactiver le prochain démarrage automatique.",
                ),
            ),
            (
                Text::same("capabilities SERVICE"),
                Text(
                    "Show the capability ceiling, stored mask and effective rights.",
                    "Afficher plafond, masque enregistré et droits effectifs.",
                ),
            ),
            (
                Text(
                    "capabilities restrict SERVICE CAPABILITY",
                    "capabilities restrict SERVICE CAPACITÉ",
                ),
                Text(
                    "Remove a right from the next instance.",
                    "Retirer un droit à la prochaine incarnation.",
                ),
            ),
            (
                Text(
                    "capabilities restore SERVICE CAPABILITY",
                    "capabilities restore SERVICE CAPACITÉ",
                ),
                Text(
                    "Explicitly restore a right within the ceiling.",
                    "Restaurer explicitement un droit dans le plafond.",
                ),
            ),
        ],
    },
    Page {
        name: "journal",
        provider: "intégrée à nsh",
        description: Text(
            "Read and manage events by boot.",
            "Consulter et administrer les événements par démarrage.",
        ),
        syntax: Text(
            "journal <subcommand> [arguments]",
            "journal <sous-commande> [arguments]",
        ),
        arguments: Text(
            "Use self or a service as the source. Read filters: --level TRACE|DEBUG|INFO|WARN|ERROR|CRITICAL, --boot HEX32, --limit 1..128. boot accepts --service NAME. tail accepts --follow.",
            "Émetteur self ou service. Filtres de lecture : --level TRACE|DEBUG|INFO|WARN|ERROR|CRITICAL, --boot HEX32, --limit 1..128. boot accepte --service NOM. tail accepte --follow.",
        ),
        examples: Text::same("journal status\njournal show self --limit 4"),
        rights: Text(
            "Ordinary sessions read their own events. Maintenance authority controls retention.",
            "La session ordinaire consulte ses propres événements. La maintenance contrôle la rétention.",
        ),
        effects: Text(
            "emit records an event. rotate, retain and purge change the P9 store.",
            "emit enregistre un événement. rotate, retain et purge modifient le magasin P9.",
        ),
        limits: Text(
            "A cursor can expire. After UNCERTAIN, read the revision before making another decision.",
            "Un curseur peut expirer. Après UNCERTAIN, relire la révision avant une nouvelle décision.",
        ),
        topics: &[
            (
                Text::same("help"),
                Text("Show general help.", "Afficher l’aide générale."),
            ),
            (
                Text::same("list"),
                Text("List event sources.", "Lister les sources."),
            ),
            (
                Text::same("status"),
                Text(
                    "Show the revision and retention settings.",
                    "Consulter la révision et la rétention.",
                ),
            ),
            (
                Text::same("show SOURCE [options]"),
                Text("Display events.", "Afficher les événements."),
            ),
            (
                Text::same("tail SOURCE [options]"),
                Text(
                    "Read recent events and follow new ones with --follow.",
                    "Consulter les derniers événements et suivre avec --follow.",
                ),
            ),
            (
                Text::same("boot [options]"),
                Text(
                    "Read events from a boot.",
                    "Consulter les événements du démarrage.",
                ),
            ),
            (
                Text::same("emit N"),
                Text("Publish a notice (0..255).", "Publier une notice (0..255)."),
            ),
            (
                Text::same("rotate"),
                Text("Rotate segments.", "Faire tourner les segments."),
            ),
            (
                Text::same("retain N"),
                Text("Keep 1 to 8 segments.", "Conserver de 1 à 8 segments."),
            ),
            (
                Text::same("purge"),
                Text("Purge the journal.", "Purger le journal."),
            ),
        ],
    },
    Page {
        name: "ls",
        provider: "nova-cli",
        description: Text(
            "List accessible entries.",
            "Lister les entrées accessibles.",
        ),
        syntax: Text(
            "ls [-aAlhrRSt1dF] [-w COLUMNS] [--color[=always|auto|never]] [--] [path...]",
            "ls [-aAlhrRSt1dF] [-w COLONNES] [--color[=always|auto|never]] [--] [chemin...]",
        ),
        arguments: Text(
            "-a/--all and -A/--almost-all include hidden names; -l shows details; -h/--human-readable formats sizes; -r/--reverse reverses order; -R/--recursive descends; -S/-t sort; -1 shows one entry per line; -d/--directory lists the directory itself; -F/--classify appends suffixes; -w/--width sets width.",
            "-a/--all, -A/--almost-all : cachés ; -l : détails ; -h/--human-readable : tailles ; -r/--reverse : inverser ; -R/--recursive : descendre ; -S/-t : trier ; -1 : une entrée ; -d/--directory : répertoire seul ; -F/--classify : suffixes ; -w/--width : largeur.",
        ),
        examples: Text::same("ls -lh -- /documents"),
        rights: Text(
            "Read or enumerate the selected paths.",
            "Lecture ou énumération des chemins sélectionnés.",
        ),
        effects: Text(
            "Displays entries within the delegated root.",
            "Affiche les entrées. Le parcours respecte la racine déléguée.",
        ),
        limits: Text(
            "Missing path: 20. Access denied: 5.",
            "Chemin absent : 20. Accès refusé : 5.",
        ),
        topics: &[],
    },
    Page {
        name: "rm",
        provider: "nova-cli",
        description: Text(
            "Remove explicitly named entries.",
            "Supprimer les entrées explicitement désignées.",
        ),
        syntax: Text("rm [-fivrR] [--] path...", "rm [-fivrR] [--] chemin..."),
        arguments: Text(
            "-f/--force ignores missing entries; -i/--interactive asks for confirmation; -v/--verbose reports removals; -r, -R and --recursive remove a tree. -- ends option parsing.",
            "-f/--force : ignorer une absence ; -i/--interactive : confirmer ; -v/--verbose : afficher ; -r, -R, --recursive : retirer un arbre. -- termine les options.",
        ),
        examples: Text("rm -- --unknown", "rm -- --inconnue"),
        rights: Text(
            "A delegation to remove the selected name or tree.",
            "Délégation de retrait du nom ou de l’arbre sélectionné.",
        ),
        effects: Text(
            "Removes authorized entries. A batch can stop after an earlier removal.",
            "Supprime les cibles autorisées. Un lot peut s’interrompre après un retrait.",
        ),
        limits: Text(
            "Access denied: 5. Missing path: 20. Names after -- are literal.",
            "Accès refusé : 5. Chemin absent : 20. Un nom après -- reste littéral.",
        ),
        topics: &[],
    },
    Page {
        name: "cd",
        provider: "intégrée à nsh",
        description: Text(
            "Change the session's current directory.",
            "Changer le répertoire courant de la session.",
        ),
        syntax: Text("cd [--] directory", "cd [--] répertoire"),
        arguments: Text(
            "A relative or absolute path. Use -- before a name beginning with a dash.",
            "Un chemin relatif ou absolu. -- protège un nom commençant par un tiret.",
        ),
        examples: Text::same("cd -- /documents"),
        rights: Text(
            "Access to the directory within the session root.",
            "Accès au répertoire dans la racine de session.",
        ),
        effects: Text(
            "Changes the current directory on success.",
            "Modifie le répertoire courant si le changement aboutit.",
        ),
        limits: Text(
            "The directory may be absent or inaccessible. A directory argument is required.",
            "Répertoire absent ou chemin inaccessible. Aucun répertoire implicite.",
        ),
        topics: &[],
    },
    Page {
        name: "vi",
        provider: "nova-cli",
        description: Text(
            "Edit a file in the native terminal.",
            "Éditer un fichier avec le terminal natif.",
        ),
        syntax: Text("vi [--] file", "vi [--] fichier"),
        arguments: Text(
            "One file path. Use -- before a name beginning with a dash.",
            "Un chemin de fichier. -- protège le nom.",
        ),
        examples: Text::same("vi -- notes.txt"),
        rights: Text(
            "Read access to the source. Saving requires an authorized destination.",
            "Lecture de la source. La sauvegarde exige une destination autorisée.",
        ),
        effects: Text(
            "i inserts, Escape leaves insert mode, u undoes and Ctrl+R redoes. :w saves to an unused name after confirmation, :q quits and :q! discards edits.",
            "i insère, Échap quitte l’insertion, u annule, Ctrl+R rétablit. :w enregistre vers un nom libre après confirmation, :q quitte, :q! abandonne.",
        ),
        limits: Text(
            "An occupied destination is refused. The buffer survives a failed save.",
            "Refuse une destination occupée. Une erreur de sauvegarde conserve le tampon.",
        ),
        topics: &[],
    },
    Page {
        name: "man",
        provider: "intégrée à nsh",
        description: Text(
            "Read native manual pages in the terminal.",
            "Consulter le manuel natif dans le terminal.",
        ),
        syntax: Text(
            "man command [subcommand...]",
            "man commande [sous-commande...]",
        ),
        arguments: Text(
            "Page name, optionally followed by the subcommand path.",
            "Nom de page et, pour un verbe, chemin de sous-commande.",
        ),
        examples: Text::same("man man\nman service capabilities restrict"),
        rights: Text(
            "Terminal input and output. No file or service authority is required.",
            "Entrée et sortie du terminal. Aucun accès NovaFS ni autorité de service.",
        ),
        effects: Text(
            "Arrows or j/k move one line, Space/b one page, and g/G to the start/end. Press / to search, Enter to submit, n for the next occurrence and q to return to the shell.",
            "Flèches ou j/k : une ligne. Espace/b : une page. g/G : début/fin. / : chercher, Entrée : valider. n : occurrence suivante. q : retour au shell.",
        ),
        limits: Text(
            "Missing page: 20. Searches are case-sensitive.",
            "Page absente : 20. La recherche distingue les majuscules.",
        ),
        topics: &[],
    },
    Page {
        name: "pwd",
        provider: "nova-cli",
        description: Text(
            "Display the current directory.",
            "Afficher le répertoire courant.",
        ),
        syntax: Text::same("pwd [-LP]"),
        arguments: Text(
            "-L/--logical and -P/--physical both display the delegated working directory.",
            "-L/--logical et -P/--physical affichent le répertoire courant délégué.",
        ),
        examples: Text::same("pwd -P"),
        rights: Text("Current directory view.", "Vue du répertoire courant."),
        effects: Text("Prints its path.", "Affiche son chemin."),
        limits: Text("An extra path returns 3.", "Un chemin en trop donne 3."),
        topics: &[],
    },
    Page {
        name: "cat",
        provider: "nova-cli",
        description: Text(
            "Display the contents of selected files.",
            "Afficher le contenu des fichiers sélectionnés.",
        ),
        syntax: Text("cat [--] file...", "cat [--] fichier..."),
        arguments: Text(
            "Supply one or more paths in display order.",
            "Fournir un ou plusieurs chemins dans l’ordre de lecture.",
        ),
        examples: Text::same("cat -- /documents/exemple.txt ; cat /lignes | wc -l"),
        rights: Text(
            "Read access to each selected file.",
            "Lecture de chaque fichier sélectionné.",
        ),
        effects: Text(
            "Escapes unsafe bytes on the console. Sends the original bytes to wc -l or head -n 1. An interrupted pipe write returns 11 (CLOSED).",
            "Échappe les octets de contrôle sur la console. Transmet les octets originaux à wc -l ou head -n 1. Une écriture interrompue par la fermeture du lecteur retourne 11 (CLOSED).",
        ),
        limits: Text(
            "An inaccessible or absent file stops the command (5 or 20).",
            "Un fichier inaccessible ou absent interrompt la commande (5 ou 20).",
        ),
        topics: &[],
    },
    Page {
        name: "head",
        provider: "nova-cli",
        description: Text(
            "Display the start of each file.",
            "Afficher le début de chaque fichier.",
        ),
        syntax: Text(
            "head [-qv] [-n N | -c N] [--] file... ; cat file | head -n 1",
            "head [-qv] [-n N | -c N] [--] fichier... ; cat fichier | head -n 1",
        ),
        arguments: Text(
            "-n/--lines N counts lines (default 10), -c/--bytes N counts bytes. -q/--quiet/--silent hides headers, -v/--verbose shows them.",
            "-n/--lines N compte les lignes (10 par défaut), -c/--bytes N les octets. -q/--quiet/--silent masque les en-têtes, -v/--verbose les affiche.",
        ),
        examples: Text::same("head -n 2 /lignes ; cat /lignes | head -n 1"),
        rights: Text(
            "Read access to each selected file, or a receive-only pipe endpoint and terminal output.",
            "Lecture de chaque fichier sélectionné ou extrémité de réception du pipe et sortie terminal.",
        ),
        effects: Text(
            "Prints the selected portion to the console.",
            "Affiche la partie sélectionnée sur la console.",
        ),
        limits: Text(
            "An inaccessible or absent file stops the command (5 or 20).",
            "Un fichier inaccessible ou absent interrompt la commande (5 ou 20).",
        ),
        topics: &[],
    },
    Page {
        name: "tail",
        provider: "nova-cli",
        description: Text(
            "Display the end of each file.",
            "Afficher la fin de chaque fichier.",
        ),
        syntax: Text(
            "tail [-qv] [-n N | -c N] [--] file...",
            "tail [-qv] [-n N | -c N] [--] fichier...",
        ),
        arguments: Text(
            "-n/--lines N counts lines (default 10), -c/--bytes N counts bytes. -q/--quiet/--silent hides headers, -v/--verbose shows them.",
            "-n/--lines N compte les lignes (10 par défaut), -c/--bytes N les octets. -q/--quiet/--silent masque les en-têtes, -v/--verbose les affiche.",
        ),
        examples: Text::same("tail -n 2 /lignes"),
        rights: Text(
            "Read access to each selected file.",
            "Lecture de chaque fichier sélectionné.",
        ),
        effects: Text(
            "Prints the selected portion to the console.",
            "Affiche la partie sélectionnée sur la console.",
        ),
        limits: Text(
            "An inaccessible or absent file stops the command (5 or 20).",
            "Un fichier inaccessible ou absent interrompt la commande (5 ou 20).",
        ),
        topics: &[],
    },
    Page {
        name: "wc",
        provider: "nova-cli",
        description: Text(
            "Count lines, words and text units.",
            "Compter les lignes, les mots et les unités de texte.",
        ),
        syntax: Text("wc [-lwcmL] [--] file...\ncat file | wc -l", "wc [-lwcmL] [--] fichier...\ncat fichier | wc -l"),
        arguments: Text(
            "-l/--lines counts newlines, -w/--words words, -c/--bytes bytes, -m/--chars Unicode characters, -L/--max-line-length display columns.",
            "-l/--lines compte les sauts de ligne, -w/--words les mots, -c/--bytes les octets, -m/--chars les caractères Unicode, -L/--max-line-length la largeur affichée.",
        ),
        examples: Text::same("wc -lc /lignes ; cat /lignes | wc -l"),
        rights: Text(
            "Reads the selected files, or receives a read-only pipe endpoint from nsh.",
            "Lit les fichiers sélectionnés ou reçoit de nsh une extrémité de pipe en lecture seule.",
        ),
        effects: Text(
            "Prints counts per file and totals for multiple files.",
            "Affiche les comptes par fichier et le total pour plusieurs fichiers.",
        ),
        limits: Text(
            "An inaccessible or absent file stops the command (5 or 20).",
            "Un fichier inaccessible ou absent interrompt la commande (5 ou 20).",
        ),
        topics: &[],
    },
    Page {
        name: "more",
        provider: "nova-cli",
        description: Text(
            "Page through selected files.",
            "Parcourir les fichiers page par page.",
        ),
        syntax: Text("more [--] file...", "more [--] fichier..."),
        arguments: Text(
            "Space advances a page, Enter one line and q ends paging.",
            "Espace avance d’une page, Entrée d’une ligne et q quitte la pagination.",
        ),
        examples: Text::same("more -- /lignes"),
        rights: Text(
            "Read selected files and receive console input.",
            "Lecture des fichiers sélectionnés et accès au clavier.",
        ),
        effects: Text(
            "Pauses at a full page.",
            "Attend une touche en fin de page.",
        ),
        limits: Text(
            "An inaccessible or absent file stops the command (5 or 20).",
            "Un fichier inaccessible ou absent interrompt la commande (5 ou 20).",
        ),
        topics: &[],
    },
    Page {
        name: "mkdir",
        provider: "nova-cli",
        description: Text("Create a directory.", "Créer un répertoire."),
        syntax: Text("mkdir [--] directory", "mkdir [--] répertoire"),
        arguments: Text(
            "One path under an authorized parent.",
            "Un chemin sous un répertoire parent autorisé.",
        ),
        examples: Text::same("mkdir -- dossier"),
        rights: Text(
            "Traverse the parent and add the named entry.",
            "Traverser le parent et y ajouter l’entrée nommée.",
        ),
        effects: Text(
            "Creates the named directory.",
            "Crée le répertoire désigné.",
        ),
        limits: Text(
            "Existing name: 27. Access denied: 5.",
            "Nom occupé : 27. Accès refusé : 5.",
        ),
        topics: &[],
    },
    Page {
        name: "rmdir",
        provider: "nova-cli",
        description: Text(
            "Remove empty directories.",
            "Retirer des répertoires vides.",
        ),
        syntax: Text(
            "rmdir [-v] [--] directory...",
            "rmdir [-v] [--] répertoire...",
        ),
        arguments: Text(
            "-v/--verbose reports each removal. nsh processes successive names separately.",
            "-v/--verbose affiche les retraits. nsh traite les noms successivement.",
        ),
        examples: Text::same("rmdir -v -- dossier"),
        rights: Text(
            "Traverse the parent and remove the named empty directory.",
            "Traverser le parent et retirer le répertoire vide nommé.",
        ),
        effects: Text(
            "An error stops a batch after earlier removals.",
            "Une erreur interrompt le lot après les retraits précédents.",
        ),
        limits: Text(
            "Nonempty directory: 28. Missing entry: 20.",
            "Répertoire non vide : 28. Entrée absente : 20.",
        ),
        topics: &[],
    },
    Page {
        name: "touch",
        provider: "nova-cli",
        description: Text(
            "Create files or update timestamps.",
            "Créer des fichiers ou actualiser leurs dates.",
        ),
        syntax: Text("touch [-amc] [--] file...", "touch [-amc] [--] fichier..."),
        arguments: Text(
            "-a selects access time, -m modification time, both by default. -c/--no-create leaves absent entries absent.",
            "-a règle la date d’accès, -m celle de modification, les deux par défaut. -c/--no-create ignore les entrées absentes.",
        ),
        examples: Text::same("touch -m -- notes"),
        rights: Text(
            "Timestamp or create authority for the selected names.",
            "Délégation de datation ou de création pour les noms sélectionnés.",
        ),
        effects: Text(
            "Updates metadata or creates an empty file.",
            "Modifie les dates ou crée un fichier vide.",
        ),
        limits: Text(
            "Access denied: 5. Earlier names may already be updated.",
            "Accès refusé : 5. Des noms précédents peuvent déjà avoir été traités.",
        ),
        topics: &[],
    },
    Page {
        name: "cp",
        provider: "nova-cli",
        description: Text(
            "Copy files or trees to an unused name.",
            "Copier des fichiers ou des arbres vers un nom libre.",
        ),
        syntax: Text(
            "cp [-finvRr] [--] source... destination",
            "cp [-finvRr] [--] source... destination",
        ),
        arguments: Text(
            "-f/--force selects noninteractive handling, -i/--interactive asks before replacement, -n/--no-clobber skips existing names, -v/--verbose prints transfers. -r/-R/--recursive copies a tree.",
            "-f/--force choisit le mode sans question, -i/--interactive demande confirmation, -n/--no-clobber ignore les noms occupés, -v/--verbose affiche les transferts. -r/-R/--recursive copie un arbre.",
        ),
        examples: Text::same("cp -n -- notes copie"),
        rights: Text(
            "Read the selected source and create the destination; tree copy needs scoped traversal.",
            "Lire la source sélectionnée et créer la destination. La récursion exige la traversée déléguée.",
        ),
        effects: Text(
            "Creates an independent copy.",
            "Crée une copie indépendante.",
        ),
        limits: Text(
            "Replacement is unavailable, including after confirmation (27). Multiple sources require a destination directory. A batch can stop partway.",
            "Le remplacement reste indisponible, même après confirmation (27). Plusieurs sources exigent un répertoire de destination. Un lot peut s’interrompre en cours de traitement.",
        ),
        topics: &[],
    },
    Page {
        name: "mv",
        provider: "nova-cli",
        description: Text(
            "Move entries to an unused name.",
            "Déplacer des entrées vers un nom libre.",
        ),
        syntax: Text(
            "mv [-finv] [--] source... destination",
            "mv [-finv] [--] source... destination",
        ),
        arguments: Text(
            "-f/--force selects noninteractive handling, -i/--interactive asks before replacement, -n/--no-clobber skips existing names, -v/--verbose prints transfers.",
            "-f/--force choisit le mode sans question, -i/--interactive demande confirmation, -n/--no-clobber ignore les noms occupés, -v/--verbose affiche les transferts.",
        ),
        examples: Text::same("mv -n -- notes copie"),
        rights: Text(
            "Named move source and destination delegations.",
            "Délégations de déplacement sur la source et la destination nommées.",
        ),
        effects: Text(
            "Changes the source and destination names.",
            "Modifie les entrées source et destination.",
        ),
        limits: Text(
            "Replacement is unavailable, including after confirmation (27). Multiple sources require a destination directory. A batch can stop partway.",
            "Le remplacement reste indisponible, même après confirmation (27). Plusieurs sources exigent un répertoire de destination. Un lot peut s’interrompre en cours de traitement.",
        ),
        topics: &[],
    },
    Page {
        name: "help",
        provider: "intégrée à nsh",
        description: Text(
            "List public command names.",
            "Lister les noms de commandes publiques.",
        ),
        syntax: Text::same("help"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("help"),
        rights: Text("Console output.", "Sortie de la console."),
        effects: Text(
            "Prints the shell command list.",
            "Affiche la liste de commandes du shell.",
        ),
        limits: Text("Unexpected operand: 3.", "Opérande inattendu : 3."),
        topics: &[],
    },
    Page {
        name: "history",
        provider: "intégrée à nsh",
        description: Text(
            "Display this session’s command history.",
            "Afficher l’historique de cette session.",
        ),
        syntax: Text::same("history"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("history"),
        rights: Text(
            "Interactive shell history and console output.",
            "Historique du shell interactif et sortie console.",
        ),
        effects: Text(
            "Prints numbered history entries.",
            "Affiche les entrées numérotées.",
        ),
        limits: Text(
            "A script cannot list interactive history (5).",
            "Un script ne peut pas consulter l’historique interactif (5).",
        ),
        topics: &[],
    },
    Page {
        name: "clear",
        provider: "intégrée à nsh",
        description: Text("Clear the terminal screen.", "Effacer l’écran du terminal."),
        syntax: Text::same("clear"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("clear"),
        rights: Text(
            "Terminal screen control.",
            "Contrôle de l’écran du terminal.",
        ),
        effects: Text("Clears the current display.", "Efface l’affichage courant."),
        limits: Text("Unexpected operand: 3.", "Opérande inattendu : 3."),
        topics: &[],
    },
    Page {
        name: "ps",
        provider: "intégrée à nsh",
        description: Text(
            "Display processes visible to this session.",
            "Afficher les processus visibles dans la session.",
        ),
        syntax: Text::same("ps"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("ps"),
        rights: Text(
            "Session observation capability for nsh and its child.",
            "Capacité d’observation de la session nsh et de son enfant.",
        ),
        effects: Text(
            "Samples PID, role, state and private memory once.",
            "Relève une fois le PID, le rôle, l’état et la mémoire privée.",
        ),
        limits: Text(
            "Observation may be denied (5). The scope is the current session.",
            "L’observation peut être refusée (5). La portée est limitée à la session courante.",
        ),
        topics: &[],
    },
    Page {
        name: "top",
        provider: "intégrée à nsh",
        description: Text(
            "Monitor the session’s processes.",
            "Surveiller les processus de la session.",
        ),
        syntax: Text::same("top"),
        arguments: Text(
            "Press q, Escape or Ctrl+C to leave.",
            "Appuyer sur q, Échap ou Ctrl+C pour quitter.",
        ),
        examples: Text::same("top"),
        rights: Text(
            "Session observation and interactive terminal control.",
            "Observation de la session et contrôle du terminal interactif.",
        ),
        effects: Text(
            "Refreshes private memory and process state until you quit.",
            "Actualise la mémoire privée et l’état des processus jusqu’à la sortie.",
        ),
        limits: Text(
            "Observation may be denied (5).",
            "L’observation peut être refusée (5).",
        ),
        topics: &[],
    },
    Page {
        name: "true",
        provider: "intégrée à nsh",
        description: Text("Return a true condition.", "Renvoyer une condition vraie."),
        syntax: Text::same("true"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("true && echo ok"),
        rights: Text("No business capability.", "Aucune capacité métier."),
        effects: Text(
            "Sets the status used by shell conditionals.",
            "Détermine le statut utilisé dans les conditions du shell.",
        ),
        limits: Text(
            "Normal invocation returns 0.",
            "L’invocation ordinaire rend 0.",
        ),
        topics: &[],
    },
    Page {
        name: "false",
        provider: "intégrée à nsh",
        description: Text(
            "Return a false condition.",
            "Renvoyer une condition fausse.",
        ),
        syntax: Text::same("false"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("false || echo ok"),
        rights: Text("No business capability.", "Aucune capacité métier."),
        effects: Text(
            "Sets the status used by shell conditionals.",
            "Détermine le statut utilisé dans les conditions du shell.",
        ),
        limits: Text(
            "Normal invocation returns 1; help and version return 0.",
            "L’invocation ordinaire rend 1, l’aide et la version rendent 0.",
        ),
        topics: &[],
    },
    Page {
        name: "alias",
        provider: "intégrée à nsh",
        description: Text(
            "List or define command aliases.",
            "Lister ou définir les alias de commandes.",
        ),
        syntax: Text("alias [name=value]", "alias [nom=valeur]"),
        arguments: Text(
            "Use one definition; quote a value containing spaces.",
            "Fournir une définition. Protéger une valeur qui contient des espaces.",
        ),
        examples: Text::same("alias l='ls -l'"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Lists aliases or records an expansion in this frame.",
            "Affiche les alias ou enregistre une expansion dans le cadre courant.",
        ),
        limits: Text(
            "Invalid definition: 3. Expansion and recursion are bounded.",
            "Définition invalide : 3. Les expansions et la récursion sont bornées.",
        ),
        topics: &[],
    },
    Page {
        name: "unalias",
        provider: "intégrée à nsh",
        description: Text("Remove a shell alias.", "Retirer un alias du shell."),
        syntax: Text("unalias name", "unalias nom"),
        arguments: Text(
            "Supply one existing alias name.",
            "Fournir le nom d’un alias existant.",
        ),
        examples: Text::same("unalias l"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Removes that definition from the current frame.",
            "Retire la définition du cadre courant.",
        ),
        limits: Text("Unknown alias: 20.", "Alias inconnu : 20."),
        topics: &[],
    },
    Page {
        name: "export",
        provider: "intégrée à nsh",
        description: Text(
            "Export variables to nested shell scripts.",
            "Exporter des variables vers les scripts nsh imbriqués.",
        ),
        syntax: Text("export name[=value]...", "export nom[=valeur]..."),
        arguments: Text(
            "Supply identifiers with optional assignments.",
            "Fournir des identifiants, avec une affectation facultative.",
        ),
        examples: Text::same("export LANG=fr-CA"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Marks variables for inheritance by a child shell frame.",
            "Marque les variables pour la transmission au cadre nsh enfant.",
        ),
        limits: Text(
            "Invalid identifier: 3. Frame budget exceeded: 6.",
            "Identifiant invalide : 3. Budget du cadre dépassé : 6.",
        ),
        topics: &[],
    },
    Page {
        name: "unset",
        provider: "intégrée à nsh",
        description: Text("Remove shell variables.", "Retirer des variables du shell."),
        syntax: Text("unset name...", "unset nom..."),
        arguments: Text(
            "Supply one or more identifiers.",
            "Fournir au moins un identifiant.",
        ),
        examples: Text::same("unset LANG"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Removes values and their export flags from this frame.",
            "Retire les valeurs et leur marque d’export du cadre courant.",
        ),
        limits: Text(
            "Invalid identifier: 3. An absent variable is ignored.",
            "Identifiant invalide : 3. Une variable absente est ignorée.",
        ),
        topics: &[],
    },
    Page {
        name: "echo",
        provider: "intégrée à nsh",
        description: Text("Print the words supplied.", "Afficher les mots fournis."),
        syntax: Text("echo [--] [word...]", "echo [--] [mot...]"),
        arguments: Text(
            "A leading -- makes the following words data, including --help.",
            "Un -- initial rend les mots suivants littéraux pour l’analyse des options, y compris --help.",
        ),
        examples: Text::same("echo -- --help"),
        rights: Text("Terminal output.", "Sortie du terminal."),
        effects: Text(
            "Writes words separated by spaces, then a newline.",
            "Écrit les mots séparés par des espaces, puis un saut de ligne.",
        ),
        limits: Text(
            "Shell expansions still apply. No -n switch.",
            "Les expansions du shell restent actives. L’option -n est absente.",
        ),
        topics: &[],
    },
    Page {
        name: "test",
        provider: "intégrée à nsh",
        description: Text(
            "Evaluate a shell condition.",
            "Évaluer une condition du shell.",
        ),
        syntax: Text::same("test [--] expression"),
        arguments: Text(
            "-n/-z test nonempty/empty strings. -e/-f/-d test existence, file or directory. =, == and != compare strings. -lt/-le/-eq/-ne/-ge/-gt compare signed integers.",
            "-n/-z testent une chaîne non vide/vide. -e/-f/-d testent l’existence, le fichier ou le répertoire. =, == et != comparent les chaînes. -lt/-le/-eq/-ne/-ge/-gt comparent des entiers signés.",
        ),
        examples: Text::same("test -3 -lt 2"),
        rights: Text(
            "No capability for string or integer comparisons. File predicates require lookup within the session root.",
            "Les comparaisons de chaînes ou d’entiers sont locales. Les prédicats de fichier exigent une recherche sous la racine de session.",
        ),
        effects: Text(
            "Returns a status; only file predicates inspect the namespace.",
            "Renvoie un statut. Seuls les prédicats de fichier consultent l’espace de noms.",
        ),
        limits: Text(
            "Conditions return 0 for true, 1 for false; syntax or integer error: 3.",
            "Une condition vraie rend 0, fausse 1. Syntaxe ou entier invalide : 3.",
        ),
        topics: &[],
    },
    Page {
        name: "[",
        provider: "intégrée à nsh",
        description: Text(
            "Evaluate a bracketed condition.",
            "Évaluer une condition entre crochets.",
        ),
        syntax: Text::same("[ expression ]"),
        arguments: Text(
            "-n/-z test nonempty/empty strings. -e/-f/-d test existence, file or directory. =, == and != compare strings. -lt/-le/-eq/-ne/-ge/-gt compare signed integers.",
            "-n/-z testent une chaîne non vide/vide. -e/-f/-d testent l’existence, le fichier ou le répertoire. =, == et != comparent les chaînes. -lt/-le/-eq/-ne/-ge/-gt comparent des entiers signés.",
        ),
        examples: Text::same("[ -f /documents/exemple.txt ]"),
        rights: Text(
            "No capability for string or integer comparisons. File predicates require lookup within the session root.",
            "Les comparaisons de chaînes ou d’entiers sont locales. Les prédicats de fichier exigent une recherche sous la racine de session.",
        ),
        effects: Text(
            "Returns a status; only file predicates inspect the namespace.",
            "Renvoie un statut. Seuls les prédicats de fichier consultent l’espace de noms.",
        ),
        limits: Text(
            "Conditions return 0 for true, 1 for false; syntax or integer error: 3. A final ] closes the expression. [ --help alone opens help.",
            "Une condition vraie rend 0, fausse 1. Syntaxe ou entier invalide : 3. Un ] final ferme l’expression. [ --help seul affiche l’aide.",
        ),
        topics: &[],
    },
    Page {
        name: "break",
        provider: "intégrée à nsh",
        description: Text(
            "Leave the innermost loop.",
            "Sortir de la boucle la plus interne.",
        ),
        syntax: Text::same("break"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("for x in a b; do break; done"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Resumes after the loop.",
            "Reprend à l’instruction suivant la boucle.",
        ),
        limits: Text("Outside a loop: 3.", "Hors d’une boucle : 3."),
        topics: &[],
    },
    Page {
        name: "continue",
        provider: "intégrée à nsh",
        description: Text(
            "Continue the innermost loop.",
            "Passer à l’itération suivante de la boucle interne.",
        ),
        syntax: Text::same("continue"),
        arguments: Text("Takes no operands.", "Ne prend aucun opérande."),
        examples: Text::same("for x in a b; do continue; done"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Skips the remainder of the current iteration.",
            "Saute la suite de l’itération courante.",
        ),
        limits: Text("Outside a loop: 3.", "Hors d’une boucle : 3."),
        topics: &[],
    },
    Page {
        name: "exit",
        provider: "intégrée à nsh",
        description: Text(
            "Exit the shell or script.",
            "Quitter le shell ou le script.",
        ),
        syntax: Text::same("exit [0..255]"),
        arguments: Text(
            "Optional status. A script defaults to its preceding status.",
            "Statut facultatif. Dans un script, la valeur par défaut est le statut précédent.",
        ),
        examples: Text::same("nsh -c 'exit 7'"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Ends the interactive session or the current script invocation.",
            "Termine la session interactive ou l’invocation du script courant.",
        ),
        limits: Text(
            "At the prompt the default is 0. An invalid status returns 3.",
            "À l’invite, la valeur par défaut est 0. Un statut invalide donne 3.",
        ),
        topics: &[],
    },
    Page {
        name: "return",
        provider: "intégrée à nsh",
        description: Text(
            "Return from the current script.",
            "Rendre la main à l’appelant du script.",
        ),
        syntax: Text::same("return [0..255]"),
        arguments: Text(
            "Optional status. A script defaults to its preceding status.",
            "Statut facultatif. Dans un script, la valeur par défaut est le statut précédent.",
        ),
        examples: Text::same("nsh -c 'return 7'"),
        rights: Text(
            "No business capability; uses the current shell frame.",
            "Aucune capacité métier. Utilise le cadre courant du shell.",
        ),
        effects: Text(
            "Stops executing the current script.",
            "Arrête l’exécution du script courant.",
        ),
        limits: Text(
            "Outside a script: 3. Returns the selected status to the caller.",
            "Hors script : 3. Transmet le statut choisi à l’appelant.",
        ),
        topics: &[],
    },
];

pub fn lookup(name: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.name == name)
}

impl Page {
    pub fn version(&self, lang: Language) -> String {
        if self.name == "sed" {
            return "NovaOS sed 0.1.0 (QSE 0.9.1 / TRE)\n".into();
        }
        let component = if self.provider == "nova-cli" {
            "nova-cli"
        } else {
            "nsh"
        };
        let provider = match self.provider {
            "intégrée à nsh" => lang.select("nsh builtin", "intégrée à nsh"),
            other => other,
        };
        format!(
            "{} ({provider}) — {component} {}\n",
            self.name,
            env!("CARGO_PKG_VERSION")
        )
    }

    pub fn render(&self, path: &[&str], manual: bool, lang: Language) -> Option<String> {
        let mut text = String::new();
        if path.is_empty() {
            writeln!(
                text,
                "{} — {}\n{} {}",
                self.name,
                self.description.get(lang),
                lang.select("Usage:", "Syntaxe :"),
                self.syntax.get(lang)
            )
            .ok()?;
            for (syntax, desc) in self.topics {
                writeln!(
                    text,
                    "  {} {} — {}",
                    self.name,
                    syntax.get(lang),
                    desc.get(lang)
                )
                .ok()?;
            }
        } else {
            let (syntax, desc) = self.topics.iter().find(|(syntax, _)| {
                syntax
                    .get(Language::English)
                    .split_ascii_whitespace()
                    .take_while(|word| word.bytes().all(|c| c.is_ascii_lowercase()))
                    .eq(path.iter().copied())
            })?;
            writeln!(
                text,
                "{} {} — {}",
                self.name,
                syntax.get(lang),
                desc.get(lang)
            )
            .ok()?;
        }
        writeln!(
            text,
            "{} {}\n{}\n{}\n{}",
            lang.select("Arguments and options:", "Arguments et options :"),
            self.arguments.get(lang),
            lang.select(
                "--help: help; --version: version.",
                "--help : aide ; --version : version."
            ),
            lang.select("Examples:", "Exemples :"),
            self.examples.get(lang)
        )
        .ok()?;
        if manual {
            writeln!(
                text,
                "{} {}\n{} {}\n{} {}\nVersion: {}",
                lang.select("Capabilities:", "Capacités :"),
                self.rights.get(lang),
                lang.select("Effects and resources:", "Effets et ressources :"),
                self.effects.get(lang),
                if self.name == "sed" { lang.select("Status and limits: syntax 1, missing input 2, I/O 4.", "Codes et limites : syntaxe 1, entrée absente 2, E/S 4.") } else { lang.select(
                    "Status, errors and limits: 0 success, 3 invalid argument.",
                    "Codes, erreurs et limites : 0 succès, 3 argument invalide."
                ) },
                self.limits.get(lang),
                self.version(lang).trim_end()
            )
            .ok()?;
        }
        writeln!(
            text,
            "{} man {}",
            lang.select("Manual:", "Manuel :"),
            self.name
        )
        .ok()?;
        Some(text)
    }
}

/// Argument de présentation privé d'un lancement documentaire, sans valeur de capacité.
pub fn child_language(args: &[&str]) -> Language {
    if matches!(args, ["--help" | "--version", "--", "nsh-doc=fr"]) {
        Language::French
    } else {
        Language::English
    }
}
