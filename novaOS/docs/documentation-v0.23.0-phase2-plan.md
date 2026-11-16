# Extension du catalogue aux 29 commandes restantes

Proposition soumise à la MOA/MOE. Le catalogue public compte 37 noms, dont huit pilotes. Les 29 lignes ci-dessous correspondent au répartiteur du candidat. Le script installé `/verification.nsh` garde son suivi séparé. Aucune des 29 pages n’est encore livrée. Les ELF ont un synopsis court et une version générique du paquet. L’état historique des options des intégrées est détaillé dans l’[inventaire](reconciliation-v0300/inventaire-commandes.md).

## Recette commune à chaque ligne

Pour chaque nom, lancer **trois essais distincts** dans les deux invités. Pour `--help`, comparer syntaxe, options, exemples et renvoi au manuel. Pour `--version`, vérifier la provenance et la version de construction (`nsh` pour les intégrées, `nova-cli` pour les ELF). Pour `man NOM`, vérifier page, droits, effets, erreurs et retour à l’invite. Exiger le code 0 pour les trois essais, même privé des droits métier de la ligne. Vérifier les textes anglais et français et appliquer le sélecteur commun, notamment pour `fr-zzzz`, `fr-zzzzz` et `fr-CA`. Conserver la sortie, le statut, l’état avant/après et le témoin positif prouvant que la sonde observe l’effet.

Intercepter la documentation avant globbing, délégation, création de temporaire ou opération métier. Mesurer les compteurs d’écriture/FLUSH et le SHA-256 de P9 dans la fenêtre des invocations. Pour les commandes à chemin, tester un objet réel nommé `--inconnue` et un voisin témoin. Avant `--`, exiger le refus d’option sans effet. Après `--`, vérifier l’opération autorisée sur le seul opérande littéral et relire les objets. Pour les commandes sans chemin, distinguer les données et les opérateurs du langage des options, et conserver l’arité après le séparateur. Les règles précises des cas ambigus ci-dessous sont des propositions à valider. Aucune de ces commandes n’a de sous-commande métier.

## Matrice des commandes

La dernière colonne complète les trois essais obligatoires de chaque ligne. Le numéro du lot renvoie au découpage proposé ci-dessous. Les droits décrivent l’exécution métier.

| Nom | Implantation | Syntaxe ou options métier actuellement admises | Effets et capacités nécessaires | Séparateur, témoin de non-effet et risque à qualifier |
|---|---|---|---|---|
| `pwd` | ELF | `-L -P --logical --physical`, aucun chemin | Affiche le répertoire délégué par CWD_VIEW | Lot 1. Après `--`, refuser un chemin en trop. Aide sans demande de CWD_VIEW. |
| `cat` | ELF | Un ou plusieurs fichiers | Lit les objets SELECT_FILE vers la console | Lot 1. Lire le nom à tiret après `--`. Aucune lecture du fichier homonyme pendant l’aide. |
| `head` | ELF | `-n N -c N -q -v`, `--lines --bytes --quiet --silent --verbose` | Lit le début des fichiers sélectionnés | Lot 1. Vérifier le compte numérique avant le séparateur et le fichier littéral après. |
| `tail` | ELF | Mêmes options de comptage et d’en-tête que head | Lit la fin des fichiers sélectionnés | Lot 1. Tester fichier court et nom à tiret, refuser `-f` en position d’option. |
| `wc` | ELF | `-l -w -c -m -L`, `--lines --words --bytes --chars --max-line-length` | Lit et compte les fichiers sélectionnés | Lot 1. Vérifier les unités avec UTF-8, comparer le témoin après `--`. |
| `more` | ELF | Un ou plusieurs fichiers | SELECT_FILE, affichage paginé, entrée console aux pauses | Lot 1. Aide sans pause ni lecture interactive. Positif avec page longue et nom littéral après `--`. |
| `mkdir` | ELF | Un chemin | Création nommée CREATE_ENTRY avec traversée et ajout au parent | Lot 2. `-- --help` crée le répertoire seulement lors du contrôle métier autorisé. |
| `rmdir` | ELF | `-v --verbose`, exactement un répertoire | Retrait nommé REMOVE_EMPTY_DIR, traversée et retrait au parent | Lot 2. Tester répertoires vide et occupé après `--`, relire le voisin. Le synopsis historique au pluriel excède l’arité. |
| `touch` | ELF | `-a -m -c --no-create`, un ou plusieurs noms | SELECT_TOUCH, création ou mise à jour des dates sélectionnées | Lot 2. Comparer existence et horodatages du nom à tiret et du voisin. |
| `cp` | ELF | `-f -i -n -v -R -r`, `--force --interactive --no-clobber --verbose --recursive`, deux opérandes | Lecture source et création nommée, ou SELECT_COPY_TREE et SELECT_COPY_DEST | Lot 2. Placer les deux noms après `--`. Tester destination occupée, source inchangée, aucun dialogue ni création pour l’aide. |
| `mv` | ELF | `-f -i -n -v`, `--force --interactive --no-clobber --verbose`, deux opérandes | SELECT_FROM et SELECT_TO, déplacement vers un nom libre | Lot 2. Après `--`, relire les deux noms. Le mode forcé ne permet pas de remplacer la destination. |
| `help` | Interne | Sans argument | Affiche la liste embarquée, sortie console | Lot 3. Distinguer liste ordinaire et aide de help, rejeter l’opérande superflu après `--`. |
| `history` | Interne | Sans argument, session interactive | Lit l’historique de la session | Lot 3. Aide accessible en script sans exposer les entrées. Contrôler le journal de saisie normal séparément. |
| `clear` | Interne | Sans argument | Effacement via la console | Lot 3. Aucune séquence d’effacement pour l’aide, arité zéro après `--`. |
| `ps` | Interne | Sans argument | Échantillonne la session et son enfant via PROCESS_SAMPLE | Lot 3. Zéro appel d’observation avec le droit retiré, même en présence d’un opérande incorrect. |
| `top` | Interne | Sans argument | Observation, écran interactif et lecture temporisée | Lot 3. Aucune prise d’écran, attente clavier ou mesure de processus pour l’aide. |
| `true` | Interne | Sans argument | Produit le statut 0 sans droit métier | Lot 3. Vérifier le texte d’aide en plus du statut et l’arité après `--`. |
| `false` | Interne | Sans argument | Produit le statut 1 sans droit métier | Lot 3. Distinguer statut documentaire 0 et métier 1, y compris en court-circuit. |
| `alias` | Interne | Sans argument ou une définition `nom=valeur` | Consulte ou modifie la table de la session | Lot 4. Aucune définition créée, séparateur sans effet sur la validation du nom. |
| `unalias` | Interne | Un nom | Supprime un alias de la session | Lot 4. Vérifier que l’alias témoin fonctionne après l’aide et après option refusée. |
| `export` | Interne | Un ou plusieurs noms ou affectations | Modifie les variables transmissibles aux scripts nsh | Lot 4. Aucune variable créée ou exportée, vérifier les arguments avant les mutations multiples et préserver LANG. |
| `unset` | Interne | Un ou plusieurs identifiants | Retire les variables du cadre courant | Lot 4. Vérifier les variables témoins, y compris lors d’une option invalide après un nom valide. |
| `echo` | Interne | Texte libre, aucun drapeau métier | Imprime les mots sur la console | Lot 4. Définir la position d’option et un chemin littéral pour imprimer `--help` comme texte avec `--`. Vérifier sortie et absence de globbing documentaire. |
| `test` | Interne | Expression vide, chaîne, `-n -z -e -f -d`, `= == != -lt -le -eq -ne -ge -gt` | Statut booléen, lookup autorisé pour les trois prédicats de fichier | Lot 4. Distinguer option, expression et nombre négatif. Prévoir `test -- -e --help` sur un fichier témoin, sans lookup pour l’aide. |
| `[` | Interne | Grammaire de test avec `]` final | Mêmes droits et prédicats que test | Lot 4. Qualifier `[ --help`, la page `man [` et la forme expression `[ --help ]`. Préserver fermeture, opérandes et droit de lookup. |
| `break` | Interne | Sans argument dans une boucle | Quitte la boucle courante | Lot 4. L’aide doit rendre la main à la même itération, même hors contexte de boucle. Argument après `--` refusé. |
| `continue` | Interne | Sans argument dans une boucle | Passe à l’itération suivante | Lot 4. Vérifier que l’instruction suivant l’aide s’exécute, y compris en boucle imbriquée. |
| `exit` | Interne | Statut facultatif de 0 à 255 | Quitte le contexte d’exécution | Lot 4. Après l’aide, exécuter une sentinelle dans la même session. Conserver le statut numérique comme opérande. |
| `return` | Interne | Statut facultatif de 0 à 255 dans un script | Rend le contrôle à l’appelant | Lot 4. L’aide fonctionne aussi à l’invite, sans retour anticipé. Tester la suite du script et le statut de son appelant. |

Sources contrôlées pour la matrice : [noms publics et observation](../../components/shell/builtins.rs), [options des ELF](../../components/libraries/options.rs), [arité](../../components/libraries/command.rs), [délégations](../../components/shell/capabilities.rs), [commandes du langage](../../components/shell/script.rs), [alias](../../components/shell/aliases.rs).

## Découpage proposé

1. **Lecture et pagination, six ELF, risque modéré.** Réutiliser le chemin public avant création d’instance. Qualifier la distinction entre aide et lecture de fichier par un témoin positif. Le banc historique de listage de racine reste une réserve.
2. **Mutations de fichiers, cinq ELF, risque élevé.** Dépend du premier lot et des oracles P9. Utiliser des images jetables relues hors invité. Préserver les délégations nommées et le refus de remplacement.
3. **Observation et terminal, sept intégrées, risque faible à modéré.** Réutiliser le catalogue dans nsh. Sonder explicitement PROCESS_SAMPLE et les opérations d’écran. Vérifier l’accès depuis un script pour l’aide de history.
4. **Langage et état de session, onze intégrées, risque élevé.** Définir les ambiguïtés de grammaire avant de modifier le répartiteur. Rejouer les scripts, alias, boucles et courts-circuits sur les deux ISA. Conserver la syntaxe métier acceptée.

La couverture des 29 commandes exige une preuve individuelle avant v0.30.0. La publication de cette proposition laisse le développement de phase 2 soumis à arbitrage.
