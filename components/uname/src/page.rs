Page {
    name: "uname",
    provider: "nova-uname",
    description: Text("Print system and kernel information.", "Afficher les informations du système et du noyau."),
    syntax: Text::same("uname [-aAsnrvmpio] [--help] [--version] [--]"),
    arguments: Text(
        "-s/--kernel-name (--sysname), -n/--nodename, -r/--kernel-release (--release), -v/--kernel-version, -m/--machine, -p/--processor, -i/--hardware-platform, -o/--operating-system select fields in this order. Default: -s. -a/--all prints one line. -A/--all-labeled prints labeled lines. Both omit unknown processor and platform. -- ends options. No operands.",
        "-s/--kernel-name (--sysname), -n/--nodename, -r/--kernel-release (--release), -v/--kernel-version, -m/--machine, -p/--processor, -i/--hardware-platform, -o/--operating-system sélectionnent les champs dans cet ordre. Par défaut : -s. -a/--all affiche une ligne. -A/--all-labeled affiche une ligne étiquetée par champ. Tous deux omettent le processeur et la plateforme inconnus. -- termine les options. Aucun opérande."
    ),
    examples: Text::same("uname -srmo\nuname --all-labeled"),
    rights: Text("Output streams only.", "Flux de sortie uniquement."),
    effects: Text("Reads no input and creates no files.", "Ne lit aucune entrée et ne crée aucun fichier."),
    limits: Text("Status 0 on success, 1 on usage or I/O error. NovaOS release and banner refer to the paired kernel build.", "Statut 0 en cas de succès, 1 pour une erreur de syntaxe ou d’E/S. La révision et la bannière correspondent au noyau construit avec cet ELF."),
    topics: &[],
}
