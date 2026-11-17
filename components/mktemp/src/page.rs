Page {
    name: "mktemp", provider: "nova-mktemp",
    description: Text("Create a temporary file or directory.", "Créer un fichier ou répertoire temporaire."),
    syntax: Text::same("mktemp [-dquVt] [-p DIR] [--tmpdir[=DIR]] [--suffix=SUFFIX] [--] [TEMPLATE]"),
    arguments: Text("Replace the last run of at least three Xs. Default: tmp.XXXXXXXXXX under TMPDIR. -d, --directory create a directory. -u, --dry-run print an unreserved name. -q, --quiet suppress creation errors. -p selects a directory; -t gives TMPDIR priority and accepts one component. --suffix appends text after the Xs. -V prints the version.",
    "Remplace la dernière série d’au moins trois X. Par défaut, tmp.XXXXXXXXXX sous TMPDIR. -d, --directory crée un dossier. -u, --dry-run affiche un nom sans le réserver. -q, --quiet masque les erreurs de création. -p choisit le répertoire. -t donne priorité à TMPDIR et exige un seul composant. --suffix ajoute du texte après les X. -V affiche la version."),
    examples: Text::same("mktemp -d\nmktemp --suffix=.txt note.XXXX"),
    rights: Text("Creation requires the caller’s private temporary directory on NovaOS. Documentation only needs output.", "La création sur NovaOS requiert le temporaire privé de l’appelant. La documentation utilise uniquement la sortie."),
    effects: Text("Print one name followed by LF. Darwin permissions are 0600 or 0700, subject to umask.", "Affiche un nom suivi de LF. Sur Darwin, permissions 0600 ou 0700 réduites par umask."),
    limits: Text("0 success, 1 failure. Caller-owned creation requires launcher support.", "0 succès, 1 échec. La création pour l’appelant dépend du lanceur."),
    topics: &[],
}
