// Inclus dans le catalogue partagé par le manuel, l'ELF et le Mach-O.
Page {
    name: "dirname",
    provider: "nova-dirname",
    description: Text("Print the directory part of each name.", "Afficher la partie répertoire de chaque nom."),
    syntax: Text::same("dirname [-z | --zero] [--] NAME..."),
    arguments: Text(
        "Remove the final component and adjacent slashes. A bare name yields a dot. -z, --zero end each result with NUL. -- ends option processing.",
        "Retirer la dernière composante et les barres adjacentes. Un nom simple donne un point. -z, --zero terminent chaque résultat par NUL. -- termine les options."
    ),
    examples: Text::same("dirname /usr/bin/outil a/b\ndirname -z -- -nom/fin"),
    rights: Text("Output streams only.", "Flux de sortie uniquement."),
    effects: Text("Purely lexical, including for missing names.", "Calcul lexical, y compris pour les chemins absents."),
    limits: Text("Status 1 on usage or output error. The v1 text console rejects NUL.", "Statut 1 en cas d’erreur de syntaxe ou d’écriture. La console texte v1 refuse NUL."),
    topics: &[],
}
