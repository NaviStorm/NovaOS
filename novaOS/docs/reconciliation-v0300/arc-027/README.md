# ARC-027 — Utilitaires système de la stable serveur NovaOS v0.30.0

**Décision de périmètre du 16 novembre 2026.** L’administration quotidienne depuis NovaOS est obligatoire pour la stable. Les choix d’implémentation, options divergentes, budgets et créneaux de cette étude sont soumis à la MOA/MOE. La base de travail est `origin/main` au commit `3bd662503b5fc6df0578e814b1e9d39243009dd4`. La branche ARC-026 de phase 2, `09b2d96faeb4eb3c98881be99c245975c358abb6`, sert de preuve complémentaire distincte. L’image UTM distribuée reste en v0.17.0.

### Inventaire

| Ensemble | Entrées | Traitement |
|---|---:|---|
| Commandes métier présentes dans le catalogue public | 37 | Conservation ; huit pilotes ARC-026 dans la base, 29 pages supplémentaires sur le candidat documentaire. |
| Liste initiale MOA | 70 | 66 entrées supplémentaires ; ps, top, service et journal figurent déjà parmi les 37. La correction MOA remplace « name » par uname, compté une seule fois. |
| Complément serveur | 17 entrées supplémentaires proposées | shutdown, reboot, backup, restore, update, firewall, ssh-keygen, storage, volume, sleep, read, command, basename, dirname, mktemp, fg et bg. Les autres demandes enrichissent des entrées déjà comptées. |
| Besoins supplémentaires soumis à décision | 2 | sync pour demander et vérifier le drainage d’un volume avant retrait ; sshd pour le service entrant exigé dans la recette distante. Un démon distribué a aussi droit à son manuel. |
| **Total étudié** | **122 noms distincts** | 37 existants et 85 entrées de contrat. Les 122 ne sont pas 122 binaires à produire : une partie relève de nsh, d’autres d’un même service. |

L’opérateur `&` appartient à la grammaire. `command -v` est une forme de `command`, et le besoin « logger » est instruit sous `journal emit`. L’identité et le comportement d’`uname` sont tranchés : `uname -s` doit rendre exactement `NovaOS\n`, code 0. Les lignes par commande, les écarts, les références et les estimations se trouvent dans les documents suivants.

- [37 commandes existantes, provenance et adaptations à qualifier](existantes.md)
- [85 contrats proposés, options, effets, statuts et divergences](contrats.md)
- [Comparaison des paramètres observés sur les références](parametres-amont.md)
- [Sources, licences, versions et SHA complets](sources.md)
- [Services d’administration, conditions de sûreté et parcours de panne](administration.md)
- [Dépendances, créneaux proposés, coûts, mémoire et critères par lot](lots.md)
- [Preuves et reproduction de l’étude](preuves.md)

### Architecture et flux

1. Conserver les 37 implémentations existantes. Pour chaque ajout, comparer d’abord la réutilisation d’un algorithme ou d’une bibliothèque à la maintenance d’un nouveau programme. Séparer parsing, calcul pur, ouverture de ressources et publication. Le paquet de documentation ARC-026 et le runtime utilisateur sont partagés ; chaque commande conserve sa version réelle.
2. Un adaptateur **utilisateur** peut proposer lecture, écriture, seek si l’objet le permet, temps, mémoire contrôlée et lancement limité. Chaque handle porte une génération, une direction, un propriétaire, une borne et une révocation vérifiable. Pour les composants en C, étudier des callbacks ou un sous-ensemble de libc à symboles listés au lien. Pour Rust, choisir core/alloc ou des traits d’I/O si possible ; une dépendance à std, nix ou libc bloque la compilation directe sur la cible nue. Aucun /proc, /dev, /etc/passwd, fork, netlink, espace de noms Unix global ou ABI Linux à simuler dans le noyau.
3. **Flux ordinaires proposés.** Stdin apporte les données, stdout le résultat, stderr les diagnostics. Une commande à fichiers utilise les vues de chemins déléguées ; dans les filtres sans fichier, stdin est la source. `-` figure comme flux uniquement pour les utilitaires dont le contrat l’admet. Pour les commandes de contrôle et de service, l’entrée est l’argv et la sortie un état ou un accusé ; une éventuelle confirmation passe par un terminal de confiance, jamais par le flux de données. Une commande interactive sans terminal doit annoncer explicitement si elle fournit un mode batch ou échoue.
4. Un pipe doit préserver les octets hors terminal (y compris NUL, ESC, UTF-8 invalide, absence de LF). Réserver l’échappement visuel à l’affichage terminal. Éprouver lectures courtes, écritures partielles, attente bornée en mémoire, contre-pression, EOF après le dernier écrivain et réveil à la fermeture du lecteur ou à la révocation. Les deux extrémités acquièrent des droits distincts. En pipeline, proposer le statut du dernier élément, conserver le statut de chaque élément pour le diagnostic, et arbitrer séparément pipefail. Pour une intégrée modifiant le shell, définir sa portée dans le pipeline avant d’autoriser, par exemple, `producteur | read x`.
5. Redirections proposées : `<` requiert lecture, `>` création exclusive ou publication autorisée, `>>` append explicitement délégué, `2>` une seconde destination. Valider le plan et les droits avant la première mutation ; la seule présence de `>` ne permet pas de détruire un fichier existant. Pour une consultation publique ou une option invalide, ne pas créer/tronquer une destination ni demander un droit métier. Décider si une redirection de sortie explicite est refusée ou nécessite un geste distinct ; la recette de non-effet doit couvrir `rm --help > témoin`. Les scripts de phase 1 ne disposent pas déjà de cette infrastructure.
6. Les filtres binaires comptent des octets. Les outils textuels distinguent octets, caractères et largeur affichée ; spécifier par option UTF-8 invalide, collation, locale et fin sans LF. Geler d’abord une référence reproductible en locale C pour les données, puis qualifier les locales promises. La langue documentaire est indépendante de la collation et de l’horloge.

### Contrat CLI commun

- `--help`, `--version`, aides des sous-commandes et `man commande` doivent rendre une sortie correcte dans la langue du catalogue. Pour aide et version : code 0, aucune acquisition de droit métier, aucun lancement métier, aucun tmp sur P9, aucun changement d’état. Une commande interne donne sa provenance nsh et sa version. `uname --version` identifie l’utilitaire, `uname -r` le noyau démarré et `uname -v` les métadonnées de construction de ce noyau. Le français exige la langue principale fr et une étiquette conforme à la politique BCP 47/IANA déjà qualifiée ; anglais sinon.
- Analyser les options avant d’agir. Rejeter l’option inconnue avec diagnostic sur stderr et code non nul ; après `--`, les chemins sont littéraux. Protéger les règles spécifiques à `test`, `[`, `echo`, `printf`, `find` et aux arguments d’une commande enfant. Documenter si des options de sous-commande persistent après l’opérande selon la grammaire de référence. Pour plusieurs opérandes, consigner les objets déjà traités et le statut d’un échec partiel ; aucune réussite globale après erreur.
- Le statut **N** dans les matrices propose la convention native : 0 succès, 3 arguments invalides, 5 refus d’autorité, 20 absent, 21 corruption, 22 issue incertaine, 23 quota/espace, 24 occupé/conflit, 25 non pris en charge et 26 E/S certaine, selon les erreurs réellement possibles. Les codes de fichiers proviennent des définitions natives actuelles. Pour les utilitaires aux statuts de référence significatifs (`grep`, `cmp`, `diff`, `ssh`, `curl`, `xargs`, `timeout`, etc.), conserver la sémantique de script de l’oracle et documenter la traduction et le diagnostic détaillé. Ne pas confondre « aucun résultat », refus et échec. Une écriture dont le résultat durable est inconnu ne doit être ni annoncée comme réussie ni rejouée automatiquement. [ABI fichiers](../../../abi/files.rs) · [ABI native](../../../abi/native.rs)

### Conventions de données à geler par famille

| Famille | Valeurs par défaut et sorties de référence à préciser dans chaque page |
|---|---|
| Texte et chemins | Les commandes lexicales basename/dirname ne consultent pas le stockage. cmp, cut -b, tr en locale C, sha256sum, archives et transferts doivent tolérer tout octet de données. grep/sed/awk, cut -c, sort, uniq et les affichages terminal doivent préciser traitement de l’UTF-8 invalide, collation, caractères composés et LF terminale. Aucune normalisation implicite de nom avant l’autorisation P9. |
| État et services | Diagnostic sur stderr, résultat de requête sur stdout, aucune lecture stdin par défaut pour status/list/show. Les vues filtrées n’annoncent pas le nombre d’objets cachés. Pour date, uptime, free et ps distinguer valeur inconnue, refus et valeur zéro. |
| Intentions administratives | Une demande mutatrice ne lit une confirmation interactive que si le protocole la prévoit ; l’acceptation de la demande et sa réalisation durable sont deux événements. Pour les primitives multi-objets, publier les succès partiels identifiables sans réessayer une mutation incertaine. |
| Fichiers distants et secrets | Mode batch et PTY ont des propriétés d’octets distinctes. Préserver la séparation stdout/stderr quand le protocole la fournit. Saisie des facteurs et usage de clés hors des flux enregistrables. Les erreurs doivent être exploitables sans contenir les secrets. |

### Registre des incompatibilités à arbitrer

| Sujet | Impact concret | Décision proposée et condition de levée |
|---|---|---|
| GNU/BSD et uutils | stat -f a deux sens incompatibles ; sed -i et xargs sur entrée vide divergent ; versions et OS modifient les options. | Publier un profil nominatif et versionné. Arbitrer chaque collision ; refuser les formes non qualifiées. La dernière colonne des matrices est le registre par commande. |
| Stockage et publication | cp/mv/vi refusent une destination occupée. Patch, gzip, tee, redirections et transferts distants peuvent demander son remplacement. | Profil conservateur et erreurs explicites ; aucune suppression préalable. Toute nouvelle garantie transactionnelle ou évolution P9 retourne à la MOA/MOE. |
| Liens et contrôle d’accès | ln/readlink/realpath et les triplets Unix supposent des objets et permissions spécifiques. | Exiger preuve des primitives et de leur persistance ; sinon refuser. Fixer la projection exacte par type d’objet et la gestion des handles déjà délégués. |
| Identités et sessions | Jobs, pkill, cron et administration ne peuvent s’autoriser par nom, PID ou identifiant de travail connu. | Lier les opérations à un sujet attesté et une génération ; tester les deux sessions et le recyclage des identifiants. |
| Réseau et SSH | Options dépendantes d’un backend, certificats, temps, transferts et pare-feu distant. | Qualifier un profil de protocoles et suites, la politique de clés et le secours distant ; interdire les replis silencieux. |
| A/B, restauration et fraîcheur | Restaurer un disque signé ancien peut réactiver un droit révoqué. | Exiger une autorité de fraîcheur vérifiable ou refuser de rendre accessible l’état incertain ; qualifier les pannes du profil matériel séparément. |
| Port et licences | Un frontal sous licence permissive peut embarquer libc, regex, codec ou composants sous d’autres termes. | Inventaire transitif et mesures par binaire ; choisir l’adaptateur ou l’oracle avant copie. Aucun portage massif approuvé. |

### Recette de la stable, à exécuter sur les binaires candidats

Pour chaque ligne des matrices, inscrire **un cas par option annoncée**, ses combinaisons courantes, le cas sans argument et les valeurs manquantes. Capturer argv, stdin, stdout/stderr bruts, statuts, compteurs d’opérations et empreintes des objets avant/après. Comparer avec la version GNU/BSD/amont choisie dans une locale, un fuseau et une horloge fixés ; neutraliser seulement les champs variables documentés (par exemple PID attesté ou durée). Pour les outils natifs, utiliser un oracle indépendant sur l’état des services et les données relues. Aucun succès déduit du seul message imprimé.

| Axe | Épreuve requise sur AArch64 et x86_64 | Acceptation |
|---|---|---|
| Documentation, options, arité | Sessions sans droits métier ; fr-CA, fr-Zzzz, fr-zzzzz, locale absente ; aide, version, man, sous-commandes, option invalide et `-- --inconnue`. | Sortie attendue, statut, zéro requête métier et zéro écriture/FLUSH P9 pour consultation, contrôle positif prouvant le fonctionnement de la sonde. |
| Scripts et pipes | `false` → 1, son aide → 0, `exit --help` puis commande suivante ; filtre en chaîne avec sortie binaire, erreur du producteur, lecteur fermé tôt, redirection existante refusée. | Continuité du shell, statuts prédictibles, données exactes, pas de blocage ni de fuite après fermeture. |
| Sécurité | Deux comptes, deux installations homonymes, anciens handles, application arrêtée et vivante ; retirer une capacité pendant une lecture/écriture et après redémarrage. | Aucune élévation, aucune fuite intersession, statut de l’opération en vol fidèle, aucune résurrection de droits. |
| Fiabilité | Espace/quota épuisé, coupures avant et après acquittement, révocation et arrêt contrôlé ; P9/512 et P9/4096, redémarrage et relecture sur copie distincte. | Préserver contenu, politique et erreurs. Arrêt forcé d’une VM et coupure électrique physique donnent deux preuves différentes. |
| Mémoire et performances | 1 Kio, 4 Mio, 100 Mio, très longue ligne, répertoire profond, charge concurrente 1/2/4 vCPU si pertinente ; au moins 20 passages après échauffement plus démarrage à froid. | Conserver valeurs individuelles, p50/p95/max, débit, RSS/pages privées, pic de tas, allocations, IPC, buffers des services, restitution et limites en cas de manque de mémoire. Fixer les seuils par lot sur machine de référence avant acceptation. |
| Distribution | Installer la distribution serveur sur VM vierge, débrancher le média, reconstruire et administrer depuis nsh sans outil Darwin/Linux ; reproduire sauvegarde, restauration protégée, réseau et mise à jour. | Les hôtes GNU/BSD servent d’oracles indépendants. Les deux invités doivent réaliser le parcours avec les seuls utilitaires livrés, puis rendre accessibles preuves et diagnostics. |

**Décisions demandées.** Valider le profil des paramètres par famille, la portée des redirections/jobs, la projection des permissions, les services de confiance pour clés et fraîcheur, les noms de commandes administratives proposés et les lots budgétaires. Après arbitrage, commencer par une tranche réversible de filtres en flux et sa recette différentielle. Les ports de bibliothèques, les nouvelles autorités et les seuils de performance restent à autoriser et qualifier.
