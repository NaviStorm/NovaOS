# Matrice fonctionnelle de realpath 0.24.0

Les numéros ci-dessous indexent les commandes enregistrées dans [options.json](preuves/options.json). Chaque ligne JSON conserve les arguments, le statut et les deux flux en hexadécimal pour Nova et GNU. Les consultations documentaires ont pour oracle ARC-026. Les résultats invités sont détaillés par architecture, géométrie et parcours dans [qemu.json](preuves/qemu.json).

| Option ou opérande | Valeur, priorité et effet | Vérification macOS ARM64 | Sonde moteur sous QEMU | ELF ordinaire depuis nsh |
|---|---|---|---|---|
| Chemins sans option | -EP, dernier nom absent admis, erreur sur parent absent | 0, 1, 2 | politiques E et P explicites | 127 |
| -E, --canonicalize | Dernière option du groupe E/e/m gagnante | 1–2 et groupe precedence | 2 formes × 7 choix de liens | lancement métier indisponible |
| -e, --canonicalize-existing | Toutes les composantes existent, barre finale sur fichier refusée | 3–4, références relatives vers un fichier | 2 formes × 7 | idem |
| -m, --canonicalize-missing | Tolère chemin absent, inaccessible ou non répertoire, refuse le nom vide | 5–6, cycle à trois noms, octets non UTF-8 | 2 formes × 7 | idem |
| -L, --logical | Résout les remontées avant les liens, avec les vérifications d’existence du mode | 7–8, precedence | 6 modes × 2 formes | idem |
| -P, --physical | Suit les liens dans l’ordre du chemin | 9–10, precedence | 6 × 2 | idem |
| -s, --strip, --no-symlinks | Conserve les noms de liens, dernière option L/P/s gagnante | 11–13, precedence, -ms | 6 × 3 | idem |
| -q, --quiet | Masque les diagnostics d’opérande, conserve le statut 1. Erreur de base toujours visible | 14–15, syntaxe et relatif | -q et --quiet avec noms refusés et scénario de base invalide | idem |
| -z, --zero | NUL par résultat réussi, y compris en lot et sur PTY | 16–17, abrégé --ze, precedence | NUL comparé en mémoire sur les familles existence et liens, formes courte et longue | console v1 textuelle, écriture refusée dans le profil sans port |
| --relative-base=DIR ou --relative-base DIR | Sous la base ou à égalité, résultat relatif. Comparaison par composantes entières | 18 et groupe relatif | base égale au cwd ou à tree, valeur vide refusée | lancement métier indisponible |
| --relative-to=DIR ou --relative-to DIR | Origine du calcul, dernière occurrence gagnante. En -e, DIR doit être un répertoire | 19 et groupes syntaxe, relatif | origine égale au cwd ou à tree, erreur pour une référence fichier avec -e | idem |
| Deux références relatives | Si l’origine est hors base, résultat absolu. Sans origine, la base sert d’origine | 90 croisements dans le groupe relatif, cas GNU amont de préfixe partiel | origine et base égales, imbriquées ou incompatibles | idem |
| -- | Termine les options, permet -e comme nom. Un tiret seul est un nom | groupe syntaxe | fin des options et argv sans opérande | lancement métier indisponible |
| --help, --version | Consultation immédiate, y compris avant une option inconnue. 0 et texte du composant | groupes syntaxe et catalogue | ELF sans port, tentative stdout | succès ARC-026 dans le vrai nsh, fr-CA et fr--CA |
| man realpath | Lecteur paginé, langue principale fr si étiquette admise, anglais sinon | catalogue en ARM64 et sous Rosetta | hors moteur | vrai nsh, 0 dans les deux langues |
| Erreur d’usage | Option inconnue, abrégé ambigu, valeur interdite ou manquante, zéro opérande. Statut 1 | groupe syntaxe, stdout/stderr distincts | erreurs du parseur en mémoire, ELF sans port pour -h et argv vide | option invalide reconnue avant lancement |
| Racine, droits, liens | /, //, .., chemin vide, fichier/.., lien absolu, lien pendant, cycle, défaut de recherche | oracle GNU, chmod 000 sur le parent de child. Cas amont du répertoire traversable sans droit de lecture | service synthétique et oracles GNU sur fixtures | types P9 fichier et répertoire, résolution P9 par chemin non qualifiée |
| Flux | stdin ignoré, LF/NUL, styles de citation de terminal, fermeture du lecteur | groupes terminal, environnement et [pipe.json](preuves/pipe.json) | flux en mémoire, huit invocations ELF sans port | essai en pipeline refusé, statut 3 |

Le mode complet croise les options d’existence et de résolution, les priorités contradictoires, les bases existantes, absentes et vides. Les groupes de diagnostic comprennent une réussite après erreur dans le même lot. La suite GNU vérifie aussi qu’une base `prefix` ne contient pas `prefixed`. Les refus du noyau sans port prouvent le choix du flux et le statut après refus. Les octets attendus du moteur proviennent de GNU exécuté sur macOS. La qualification de chaque option sur un ELF installé avec des flux bruts et un espace de noms délégué reste ouverte.
