Page {
    name: "readlink",
    provider: "nova-readlink",
    description: Text("Print a link target or a canonical path.", "Afficher la cible d’un lien ou un chemin canonique."),
    syntax: Text::same("readlink [-femnqsvz] [--] FILE..."),
    arguments: Text(
        "-f --canonicalize allows a missing last component. -e --canonicalize-existing requires every component. -m --canonicalize-missing allows missing components. Last mode wins. -n --no-newline omits the single delimiter. -z --zero uses NUL. -q --quiet and -s --silent suppress diagnostics; -v --verbose enables them.",
        "-f --canonicalize accepte une dernière composante absente. -e --canonicalize-existing exige leur existence. -m --canonicalize-missing accepte les absences. Le dernier mode prévaut. -n --no-newline omet le séparateur pour un seul nom. -z --zero utilise NUL. -q --quiet et -s --silent taisent les erreurs. -v --verbose les affiche."
    ),
    examples: Text::same("readlink -- lien\nreadlink -ez -- a b"),
    rights: Text("Documentation uses output only. Path lookup requires directory search.", "La documentation utilise uniquement la sortie. La résolution exige la traversée des répertoires."),
    effects: Text("Reads link targets as bytes. No writes.", "Lit les cibles en octets sans écriture."),
    limits: Text("0 success, 1 error. NovaFS P9 has no link type or delegated link lookup.", "0 succès, 1 erreur. NovaFS P9 ne fournit aucun type lien ni lecture de cible déléguée."),
    topics: &[],
}
