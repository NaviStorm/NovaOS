//! Catalogue v1 des pages publiques. Les versions suivent le paquet qui construit le binaire.
use alloc::{format, string::String};
use core::fmt::Write;

pub struct Page {
    pub name: &'static str,
    pub provider: &'static str,
    pub description: &'static str,
    pub syntax: &'static str,
    pub arguments: &'static str,
    pub examples: &'static str,
    pub rights: &'static str,
    pub effects: &'static str,
    pub limits: &'static str,
    pub topics: &'static [(&'static str, &'static str)],
}
pub static PAGES: &[Page] = &[
    Page {
        name: "nsh",
        provider: "shell",
        description: "Interpréter des commandes et des scripts NovaOS.",
        syntax: "nsh -c 'commande' | nsh [--] script.nsh",
        arguments: "Texte UTF-8 ou chemin de script. -c interprète le texte.",
        examples: "nsh -c 'echo Bonjour'",
        rights: "Les commandes du script disposent des droits de la session.",
        effects: "Les modifications dépendent des commandes exécutées.",
        limits: "Code de la dernière commande. Syntaxe invalide : 3. Pas de pipelines.",
        topics: &[],
    },
    Page {
        name: "service",
        provider: "intégrée à nsh",
        description: "Administrer les services du catalogue actif.",
        syntax: "service <sous-commande> [nom|id] [capacité]",
        arguments: "Nom canonique ou ID numérique. Les droits proviennent du catalogue.",
        examples: "service list\nservice restart 9\nservice restart storage-info\nservice capabilities 9\nservice capabilities storage-info\nservice capabilities restrict storage-info private-tmp\nservice capabilities restore storage-info private-tmp",
        rights: "Lecture pour consulter, exploitation pour démarrer ou arrêter, maintenance pour configurer.",
        effects: "Arrêter un fournisseur critique peut interrompre ses dépendants. enable|disable agit au prochain démarrage. service capabilities restrict|restore applique le changement à la prochaine instance.",
        limits: "Accès refusé : 5. Service inconnu : 20. Issue incertaine : relire avant toute décision.",
        topics: &[
            ("help", "Afficher l’aide générale."),
            ("list", "Lister ID, nom, état et démarrage."),
            (
                "status SERVICE",
                "Afficher l’état et les générations de configuration.",
            ),
            ("info SERVICE", "Interroger le service courant."),
            (
                "dependencies SERVICE",
                "Lister les fournisseurs obligatoires.",
            ),
            ("policy SERVICE", "Lire la politique de relance."),
            ("start SERVICE", "Démarrer maintenant."),
            ("stop SERVICE", "Arrêter l’instance courante."),
            (
                "restart SERVICE",
                "Relancer avec les capacités enregistrées.",
            ),
            (
                "enable SERVICE",
                "Activer le prochain démarrage automatique.",
            ),
            (
                "disable SERVICE",
                "Désactiver le prochain démarrage automatique.",
            ),
            (
                "capabilities SERVICE",
                "Afficher plafond, masque enregistré et droits effectifs.",
            ),
            (
                "capabilities restrict SERVICE CAPACITÉ",
                "Retirer un droit à la prochaine incarnation.",
            ),
            (
                "capabilities restore SERVICE CAPACITÉ",
                "Restaurer explicitement un droit dans le plafond.",
            ),
        ],
    },
    Page {
        name: "journal",
        provider: "intégrée à nsh",
        description: "Consulter et administrer les événements par démarrage.",
        syntax: "journal <sous-commande> [arguments]",
        arguments: "Émetteur self ou service. Filtres de lecture : --level TRACE|DEBUG|INFO|WARN|ERROR|CRITICAL, --boot HEX32, --limit 1..128. boot accepte --service NOM. tail accepte --follow.",
        examples: "journal status\njournal show self --limit 4",
        rights: "La session ordinaire consulte ses propres événements. La maintenance contrôle la rétention.",
        effects: "emit enregistre un événement. rotate, retain et purge modifient le magasin P9.",
        limits: "Un curseur peut expirer. Après UNCERTAIN, relire la révision avant une nouvelle décision.",
        topics: &[
            ("help", "Afficher l’aide générale."),
            ("list", "Lister les sources."),
            ("status", "Consulter la révision et la rétention."),
            ("show SOURCE [options]", "Afficher les événements."),
            (
                "tail SOURCE [options]",
                "Consulter les derniers événements et suivre avec --follow.",
            ),
            ("boot [options]", "Consulter les événements du démarrage."),
            ("emit N", "Publier une notice (0..255)."),
            ("rotate", "Faire tourner les segments."),
            ("retain N", "Conserver de 1 à 8 segments."),
            ("purge", "Purger le journal."),
        ],
    },
    Page {
        name: "ls",
        provider: "nova-cli",
        description: "Lister les entrées accessibles.",
        syntax: "ls [-aAlhrRSt1dF] [-w COLONNES] [--color[=always|auto|never]] [--] [chemin...]",
        arguments: "-a/--all, -A/--almost-all : cachés ; -l : détails ; -h/--human-readable : tailles ; -r/--reverse : inverser ; -R/--recursive : descendre ; -S/-t : trier ; -1 : une entrée ; -d/--directory : répertoire seul ; -F/--classify : suffixes ; -w/--width : largeur.",
        examples: "ls -lh -- /documents",
        rights: "Lecture ou énumération des chemins sélectionnés.",
        effects: "Affiche les entrées. Le parcours respecte la racine déléguée.",
        limits: "Chemin absent : 20. Accès refusé : 5.",
        topics: &[],
    },
    Page {
        name: "rm",
        provider: "nova-cli",
        description: "Supprimer les entrées explicitement désignées.",
        syntax: "rm [-fivrR] [--] chemin...",
        arguments: "-f/--force : ignorer une absence ; -i/--interactive : confirmer ; -v/--verbose : afficher ; -r, -R, --recursive : retirer un arbre. -- termine les options.",
        examples: "rm -- --inconnue",
        rights: "Délégation de retrait du nom ou de l’arbre sélectionné.",
        effects: "Supprime les cibles autorisées. Un lot peut s’interrompre après un retrait.",
        limits: "Accès refusé : 5. Chemin absent : 20. Un nom après -- reste littéral.",
        topics: &[],
    },
    Page {
        name: "cd",
        provider: "intégrée à nsh",
        description: "Changer le répertoire courant de la session.",
        syntax: "cd [--] répertoire",
        arguments: "Un chemin relatif ou absolu. -- protège un nom commençant par un tiret.",
        examples: "cd -- /documents",
        rights: "Accès au répertoire dans la racine de session.",
        effects: "Modifie le répertoire courant si le changement aboutit.",
        limits: "Répertoire absent ou chemin inaccessible. Aucun répertoire implicite.",
        topics: &[],
    },
    Page {
        name: "vi",
        provider: "nova-cli",
        description: "Éditer un fichier avec le terminal natif.",
        syntax: "vi [--] fichier",
        arguments: "Un chemin de fichier. -- protège le nom.",
        examples: "vi -- notes.txt",
        rights: "Lecture de la source. La sauvegarde exige une destination autorisée.",
        effects: "i insère, Échap quitte l’insertion, u annule, Ctrl+R rétablit. :w enregistre vers un nom libre après confirmation, :q quitte, :q! abandonne.",
        limits: "Refuse une destination occupée. Une erreur de sauvegarde conserve le tampon.",
        topics: &[],
    },
    Page {
        name: "man",
        provider: "intégrée à nsh",
        description: "Consulter le manuel natif dans le terminal.",
        syntax: "man commande [sous-commande...]",
        arguments: "Nom de page et, pour un verbe, chemin de sous-commande.",
        examples: "man man\nman service capabilities restrict",
        rights: "Entrée et sortie du terminal. Aucun accès NovaFS ni autorité de service.",
        effects: "Flèches ou j/k : une ligne. Espace/b : une page. g/G : début/fin. / : chercher, Entrée : valider. n : occurrence suivante. q : retour au shell.",
        limits: "Page absente : 20. La recherche distingue les majuscules.",
        topics: &[],
    },
];

pub fn lookup(name: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.name == name)
}

impl Page {
    pub fn version(&self) -> String {
        format!(
            "{} ({}) — NovaOS {}\n",
            self.name,
            self.provider,
            env!("CARGO_PKG_VERSION")
        )
    }

    pub fn render(&self, path: &[&str], manual: bool) -> Option<String> {
        let mut text = String::new();
        if path.is_empty() {
            writeln!(
                text,
                "{} — {}\nSyntaxe : {}",
                self.name, self.description, self.syntax
            )
            .ok()?;
            for (syntax, summary) in self.topics {
                writeln!(text, "  {} {} : {}", self.name, syntax, summary).ok()?;
            }
        } else {
            let (syntax, summary) = self.topics.iter().find(|(s, _)| {
                s.split_whitespace()
                    .take_while(|w| w.bytes().all(|b| b.is_ascii_lowercase()))
                    .eq(path.iter().copied())
            })?;
            writeln!(text, "{} {} — {}", self.name, syntax, summary).ok()?;
        }
        writeln!(
            text,
            "Arguments et options : {}\n--help : aide ; --version : version.\nExemples :\n{}",
            self.arguments, self.examples
        )
        .ok()?;
        if manual {
            writeln!(text, "Capacités : {}\nEffets et ressources : {}\nCodes, erreurs et limites : 0 succès, 3 argument invalide. {}\nVersion : {}", self.rights, self.effects, self.limits, self.version().trim_end()).ok()?;
        }
        writeln!(text, "Manuel : man {}", self.name).ok()?;
        Some(text)
    }
}
