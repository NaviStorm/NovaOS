# Matrice fonctionnelle de uname

GNU 9.12 est l’oracle du parseur. Chaque ligne ci-dessous a un cas isolé, un cas précédé de `-mr`, un cas suivi de `--`, un opérande et une tentative d’affectation. Les 255 sélections non vides des huit champs sont aussi exercées en ordre inverse. Les arguments peuvent arriver dans tout ordre. La sortie suit toujours `snrvmpio`. Une répétition ne duplique aucun champ. Le code de succès est 0.

| Option | Alias longs | Champ ou opération | macOS ARM64 | ELF NovaOS apparié |
|---|---|---|---|---|
| défaut, `-s` | `--kernel-name`, `--sysname` | Nom du noyau | Darwin | NovaOS |
| `-n` | `--nodename` | Nom du nœud | `uname(3).nodename` | unknown |
| `-r` | `--kernel-release`, `--release` | Révision du noyau | 27.2.0 | 0.20.0 |
| `-v` | `--kernel-version` | Version de construction du noyau | chaîne complète de `uname(3).version` | NovaOS v0.20.0 |
| `-m` | `--machine` | Architecture | arm64 | aarch64 ou x86_64 |
| `-p` | `--processor` | Type de processeur | arm | unknown |
| `-i` | `--hardware-platform` | Plateforme matérielle | unknown | unknown |
| `-o` | `--operating-system` | Système | Darwin | NovaOS |
| `-a` | `--all` | Une ligne, champs séparés par un espace, LF final | 7 champs | 6 champs |
| `-A` | `--all-labeled` | Un champ étiqueté par ligne, LF final | 7 lignes | 6 lignes |
| aide | `--help` | Aide ARC-026 | Mach-O | ELF et nsh |
| version | `--version` | Version du programme | uname (nova-uname) 0.24.0 | Idem |
| manuel | `man uname` | Lecteur ARC-026 | — | nsh, français et anglais |

Les sélections explicites `-p` et `-i` produisent `unknown` si l’information manque. Le mode `-a` ou `-A` omet ces seuls champs inconnus, même si la ligne de commande les sélectionne aussi explicitement. En invité, le nom de nœud inconnu reste affiché. `-Aa` donne la ligne simple et `-aA` les lignes étiquetées. Les alias longs acceptent leurs préfixes non ambigus.

| Entrée ou condition | Résultat contrôlé |
|---|---|
| `--` seul | Comme `-s` |
| `-- --help`, `-`, chaîne vide, deuxième opérande | Premier opérande superflu cité, stderr, statut 1 |
| `-x`, `-zz`, option longue absente | Diagnostic d’option, stderr, statut 1 |
| `--kernel`, `--h`, `--a` | Ambiguïté et liste ordonnée des possibilités, statut 1 |
| `--machine=x`, `--all=x` | Aucun argument autorisé sur une sélection, statut 1 |
| `--help -x`, `--version --help` | Première consultation rencontrée, statut 0 |
| `-x --help` | Première erreur, statut 1 |
| `x --version` | GNU permute les arguments. Avec POSIXLY_CORRECT sur Darwin, opérande superflu |
| Cinq variables Darwin UNAME_* | Remplacement par octets, y compris valeur vide, espaces ou saut de ligne |
| Consultation documentaire | Français BCP 47/IANA dont la langue principale est fr, anglais sinon. Sur Darwin, priorité LC_ALL, LC_MESSAGES, LANG |
| Entrée standard ouverte sans données | Sortie immédiate, les deux producteurs Darwin passent aussi dans wc -l |
| stdout fermé ou pipe sans lecteur | Darwin, même diagnostic et statut ou même signal que GNU |
| stdout refusé ou deux ports absents | ELF, statut 1. Diagnostic seulement si stderr est autorisé |
| argv invalide en UTF-8 | Darwin accepte les octets et les cite. En invité, le runtime v1 rejette l’encodage avant le parseur |
| Terminal et pipe ordinaires | Sur cette base, résolution métier 127, pipeline non admis 3 |

Les deux couples de flux et le statut de chaque appel Darwin sont encodés en hexadécimal dans [options.json](preuves/options.json). Les champs réels du Mac ont en plus un oracle indépendant fondé sur `os.uname()` et `/usr/bin/uname -p`. Le journal conserve les longueurs et les empreintes de ces valeurs. Les substitutions synthétiques rendent les comparaisons d’octets reproductibles. [Oracle système](preuves/systeme.json)

En invité, chaque ligne de résultat porte argv, l’ID du vecteur, le PID, les ports, le statut, les deux flux en base64 et le résultat de la comparaison. L’oracle remplace les champs Darwin de GNU par l’identité du noyau attendu. La recette vérifie en série sa bannière réelle et relit le noyau de l’ISO. Les traces séparent les trames stdout et stderr avant présentation. [Résultats par architecture et secteur](preuves/qemu)

Le différentiel normalise le préfixe exécutable `guname:` de Darwin dans les diagnostics. L’aide, le manuel et la version proviennent du catalogue NovaOS. La langue des diagnostics dépend du contrat du composant. Les valeurs système, la lecture des seules variables UNAME_* de Darwin et les contraintes UTF-8 de l’ABI figurent explicitement dans les oracles.
