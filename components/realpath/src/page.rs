Page{
 name:"realpath",provider:"nova-realpath",
 description:Text("Print resolved path names.","Afficher les chemins résolus."),
 syntax:Text::same("realpath [OPTION]... [--] FILE..."),
 arguments:Text("Default -EP. -E/--canonicalize permits a missing last name, -e/--canonicalize-existing requires existence, -m/--canonicalize-missing permits missing components. -P/--physical follows links immediately, -L/--logical after .., -s/--strip/--no-symlinks preserves them. --relative-to=DIR selects the origin, --relative-base=DIR restricts relative output to descendants. -q/--quiet suppresses operand errors. -z/--zero terminates each result with NUL.",
 "Par défaut -EP. -E/--canonicalize accepte un dernier nom absent, -e/--canonicalize-existing exige son existence, -m/--canonicalize-missing accepte les composantes absentes. -P/--physical suit les liens immédiatement, -L/--logical après .., -s/--strip/--no-symlinks les conserve. --relative-to=DIR fixe l’origine, --relative-base=DIR limite la sortie relative aux descendants. -q/--quiet masque les erreurs des opérandes. -z/--zero termine chaque résultat par NUL."),
 examples:Text::same("realpath -ms /a/../b\nrealpath --relative-to=. -- fichier"),
 rights:Text("Documentation uses only the output stream. Resolving names requires a session namespace.","La documentation utilise uniquement la sortie. La résolution des noms dépend de l’espace de noms de session."),
 effects:Text("Reads metadata on demand. No filesystem writes.","Lecture des métadonnées à la demande. Aucune écriture sur le volume."),
 limits:Text("0 on success, 1 on error. The v1 text console cannot emit NUL.","0 en cas de succès, 1 en cas d’erreur. La console texte v1 refuse NUL."),topics:&[]}
