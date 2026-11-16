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
            "Returns the last command's status. Invalid syntax: 3. Pipelines are unsupported.",
            "Code de la dernière commande. Syntaxe invalide : 3. Pas de pipelines.",
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
        description: Text("Display the current directory.", "Afficher le répertoire courant."),
        syntax: Text::same("pwd [-LP]"),
        arguments: Text("-L/--logical and -P/--physical both display the delegated working directory.", "-L/--logical et -P/--physical affichent le répertoire courant délégué."),
        examples: Text::same("pwd -P"),
        rights: Text("Current directory view.", "Vue du répertoire courant."),
        effects: Text("Prints its path.", "Affiche son chemin."),
        limits: Text("An extra path returns 3.", "Un chemin en trop donne 3."),
        topics: &[],
    },
    Page {
        name: "cat",
        provider: "nova-cli",
        description: Text("Display the contents of selected files.", "Afficher le contenu des fichiers sélectionnés."),
        syntax: Text("cat [--] file...", "cat [--] fichier..."),
        arguments: Text("Supply one or more paths in display order.", "Fournir un ou plusieurs chemins dans l’ordre de lecture."),
        examples: Text::same("cat -- /documents/exemple.txt"),
        rights: Text("Read access to each selected file.", "Lecture de chaque fichier sélectionné."),
        effects: Text("Writes printable text to the console and escapes unsafe bytes.", "Affiche le texte sur la console et échappe les octets de contrôle."),
        limits: Text("An inaccessible or absent file stops the command (5 or 20).", "Un fichier inaccessible ou absent interrompt la commande (5 ou 20)."),
        topics: &[],
    },
    Page {
        name: "head",
        provider: "nova-cli",
        description: Text("Display the start of each file.", "Afficher le début de chaque fichier."),
        syntax: Text("head [-qv] [-n N | -c N] [--] file...", "head [-qv] [-n N | -c N] [--] fichier..."),
        arguments: Text("-n/--lines N counts lines (default 10), -c/--bytes N counts bytes. -q/--quiet/--silent hides headers, -v/--verbose shows them.", "-n/--lines N compte les lignes (10 par défaut), -c/--bytes N les octets. -q/--quiet/--silent masque les en-têtes, -v/--verbose les affiche."),
        examples: Text::same("head -n 2 /lignes"),
        rights: Text("Read access to each selected file.", "Lecture de chaque fichier sélectionné."),
        effects: Text("Prints the selected portion to the console.", "Affiche la partie sélectionnée sur la console."),
        limits: Text("An inaccessible or absent file stops the command (5 or 20).", "Un fichier inaccessible ou absent interrompt la commande (5 ou 20)."),
        topics: &[],
    },
    Page {
        name: "tail",
        provider: "nova-cli",
        description: Text("Display the end of each file.", "Afficher la fin de chaque fichier."),
        syntax: Text("tail [-qv] [-n N | -c N] [--] file...", "tail [-qv] [-n N | -c N] [--] fichier..."),
        arguments: Text("-n/--lines N counts lines (default 10), -c/--bytes N counts bytes. -q/--quiet/--silent hides headers, -v/--verbose shows them.", "-n/--lines N compte les lignes (10 par défaut), -c/--bytes N les octets. -q/--quiet/--silent masque les en-têtes, -v/--verbose les affiche."),
        examples: Text::same("tail -n 2 /lignes"),
        rights: Text("Read access to each selected file.", "Lecture de chaque fichier sélectionné."),
        effects: Text("Prints the selected portion to the console.", "Affiche la partie sélectionnée sur la console."),
        limits: Text("An inaccessible or absent file stops the command (5 or 20).", "Un fichier inaccessible ou absent interrompt la commande (5 ou 20)."),
        topics: &[],
    },
    Page {
        name: "wc",
        provider: "nova-cli",
        description: Text("Count lines, words and text units.", "Compter les lignes, les mots et les unités de texte."),
        syntax: Text("wc [-lwcmL] [--] file...", "wc [-lwcmL] [--] fichier..."),
        arguments: Text("-l/--lines counts newlines, -w/--words words, -c/--bytes bytes, -m/--chars Unicode characters, -L/--max-line-length display columns.", "-l/--lines compte les sauts de ligne, -w/--words les mots, -c/--bytes les octets, -m/--chars les caractères Unicode, -L/--max-line-length la largeur affichée."),
        examples: Text::same("wc -lc /lignes"),
        rights: Text("Read access to each selected file.", "Lecture de chaque fichier sélectionné."),
        effects: Text("Prints counts per file and totals for multiple files.", "Affiche les comptes par fichier et le total pour plusieurs fichiers."),
        limits: Text("An inaccessible or absent file stops the command (5 or 20).", "Un fichier inaccessible ou absent interrompt la commande (5 ou 20)."),
        topics: &[],
    },
    Page {
        name: "more",
        provider: "nova-cli",
        description: Text("Page through selected files.", "Parcourir les fichiers page par page."),
        syntax: Text("more [--] file...", "more [--] fichier..."),
        arguments: Text("Space advances a page, Enter one line and q ends paging.", "Espace avance d’une page, Entrée d’une ligne et q quitte la pagination."),
        examples: Text::same("more -- /lignes"),
        rights: Text("Read selected files and receive console input.", "Lecture des fichiers sélectionnés et accès au clavier."),
        effects: Text("Pauses at a full page.", "Attend une touche en fin de page."),
        limits: Text("An inaccessible or absent file stops the command (5 or 20).", "Un fichier inaccessible ou absent interrompt la commande (5 ou 20)."),
        topics: &[],
    },
];

pub fn lookup(name: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.name == name)
}

impl Page {
    pub fn version(&self, lang: Language) -> String {
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
                lang.select(
                    "Status, errors and limits: 0 success, 3 invalid argument.",
                    "Codes, erreurs et limites : 0 succès, 3 argument invalide."
                ),
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
