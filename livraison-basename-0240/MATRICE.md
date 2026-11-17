# Conformité au 17 novembre 2026

Les deux formats d’appel utilisent le même parseur sur les trois cibles. La colonne ELF rapporte les quatre cellules AArch64/512, AArch64/4096, x86_64/512 et x86_64/4096. Les identifiants renvoient aux 121 enregistrements par cellule. Les sorties et les canaux sont consultables octet par octet. [Matrice des cibles](preuves/matrice-cibles.json) · [Essais hôte et pseudo-terminal](preuves/options.json)

| Forme ou cas | Entrée discriminante | Résultat Darwin, référence GNU | Résultat ELF dans les quatre cellules | Cas invité |
|---|---|---|---|---|
| `NAME [SUFFIX]` | `d/fichier.rs/// .rs` | `fichier` + LF | identique sur le port texte | 1 |
| Suffixe positionnel commençant par un tiret | `chemin/t-a -a` | `t` | identique | 2 |
| Suffixe vide, égal, absent, trop long ou avec slash | `rep/abc.h///` et huit suffixes | retire uniquement une fin propre non vide | identique | 3–10 |
| `-a`, `--multiple` et répétition | `-a d/abc f`, puis liste de noms vides, points, racines, noms avec espace et LF | un nom par opérande, guillemets sur terminal | comparaison à GNU sur terminal UTF-8 | 29–36, 99–100, 103–104 |
| `-s TEXT`, `-sTEXT`, `--suffix TEXT`, `--suffix=TEXT` | plusieurs noms dont `.rs` et `x.rs.rs` | multiple implicite, retire une seule occurrence | identique | 20–24, 37–42 |
| Suffixe répété, groupe `-as` | `-s.h -s.rs`, `-as.rs` | la dernière valeur gagne | identique | 25, 27 |
| `-z`, `--zero`, `-zz` | nom contenant LF et suffixe positionnel | nom brut + NUL | moteur identique, console refuse avant sortie, code 25 | 16–19, 43–46 |
| `-za`, `-zas.rs`, `--z -s.rs -a` | liste, racine, suffixes, plusieurs résultats | un NUL par résultat | moteur identique, console 25 | 26, 28, 101, 105 |
| Toutes les abréviations sans ambiguïté | tous les préfixes non vides de multiple, suffix, zero, help et version | même action que la forme entière | même action, limite NUL ci-dessus | 29–57 |
| `--`, `-`, fin au premier opérande | `-- -z`, `-- --version ion`, `-a file -z` | tiret littéral, suffixe ou opérande selon la position | identique | 65, 70–72 |
| Chaîne vide et slashs | `""`, `/`, `// /`, `//// //` | vide ou racine `/` | identique sur terminal, nom vide affiché `''` | 66–69, 100 |
| Noms inaccessibles et inexistants | `-a absent/path/// no-access/secret` | `path` puis `secret`, répertoire macOS de mode 000 | mêmes noms, aucun canal de fichiers | 89, 102 |
| Octets non UTF-8 | argument contenant FF ou suffixe FE | octets préservés, retrait lexical | moteur conforme, argv de l’ELF refuse avec 3 | 11–15, 88 |
| Erreurs de grammaire | manque de nom ou de suffixe, opérande en trop, `-h`, `-v`, `-azQ`, `--=`, valeur interdite | stderr, stdout vide, statut 1 | erreur native 3, canal ERROR pour les arguments UTF-8 | 58–64, 73–74, 77–85 |
| Priorité d’aide et de version | `--help --unknown`, `--unknown --help`, `--suffix --help foo--help` | première action décisive | même ordre, aide et version propres à Nova | 86, 90–96 |
| Bornes de transport | 32 arguments, puis trame de 2 560 octets | résultat complet | résultat complet | 97–98 |
| Échec de sortie | descripteur en lecture seule, pipe fermé avec SIGPIPE normal ou ignoré | EBADF et EPIPE avec SIGPIPE ignoré donnent 1, signal SIGPIPE si disposition normale | retrait de OUTPUT exercé séparément, statut 5 | 108 |
| Environnement du terminal | 10 styles, abréviations valides, ambiguës et invalides, tty ou redirection, avec et sans `-z` | stdout, avertissement et statut comparés en C et UTF-8 ; cas complémentaires ISO-8859-1 et EUC-JP | style terminal par défaut, aucun transport public de cette variable vers l’enfant | 100 et recette hôte |
| `--help`, `--version`, `man basename` | enfant documentaire français, shell fr-CA, fr-Zzzz, fr-zzzzz et en-GB | documentation propre au composant 0.24.0, repli anglais | ELF et manuel du shell validés, consultation publique sans mutation P9 | 47–57, 106–107 et sessions nsh |

GNU permet une racine `//` distincte sur certaines plateformes. Le binaire Homebrew testé ramène `//` à `/`. Les cibles NovaOS de cette livraison adoptent ce résultat. Le suffixe positionnel compte au maximum une fois. Un nom identique au suffixe reste entier. Le moteur conserve `.` et `..` tels qu’ils figurent à la fin du chemin.

## Mesures invitées

QEMU 11.1.3 TCG, un vCPU, 256 Mio, cinq passages mesurés après un échauffement. La préparation couvre l’admission de l’ELF et l’initialisation des droits du terminal. Le total va de l’admission à la collecte et inclut l’exécution et les écritures série de l’enfant. Chaque ligne donne des médianes en millisecondes. Les fréquences et les compteurs individuels se trouvent dans les fichiers `resultats.json` de l’archive. Les deux architectures et d’autres campagnes partageaient le Mac.

| QEMU et secteur P9 | 1 nom, préparation | 1 nom, total | 30 noms, préparation | 30 noms, total |
|---|---:|---:|---:|---:|
| AArch64 · 512 | 1,658 | 5,528 | 1,196 | 5,895 |
| AArch64 · 4096 | 1,184 | 5,840 | 1,188 | 5,770 |
| x86_64 · 512 | 1,298 | 5,758 | 1,879 | 7,011 |
| x86_64 · 4096 | 0,937 | 2,872 | 0,814 | 4,355 |

## Raccord public à reprendre

Le contrôleur de qualification SDK reçoit l’ELF de livraison par `NOVA_THREAD_PROBE_ELF`. Le banc adapte la fonction de test `run_example` dans une copie temporaire du noyau, garde les fonctions de lancement existantes, vérifie l’absence de capacités IPC de l’enfant et lui accorde uniquement le port attendu. Chaque sortie est comparée à celle de l’oracle après restitution des LF de l’UART. Les lignes de collecte du noyau sont identifiées dans la série brute. Le profil SDK a un disque P9 attaché en lecture seule. Une deuxième ISO utilise le noyau ordinaire, monte P9 et exécute le vrai `nsh`. Les deux campagnes ont des empreintes de noyau distinctes. [Contrôleur](lab/controleur.rs) · [Banc invité](lab/invite.py)

Sur la base `425e4e7`, le catalogue d’ELF de production est statique. L’entrée documentaire permet les consultations publiques. `basename -a /x/a /y/b` et `basename x --help` rendent 127. Ce second cas garantit aussi que le shell laisse l’option placée après un opérande au programme. Le candidat parallèle `codex/lancement-generique-elf` documentait encore la dépendance au raccord public. Cette livraison garde l’ABI v1 et le format P9.

Dès que le lancement externe est intégré sur une base autorisée, reprendre dans le terminal et dans `nsh -c` les deux syntaxes, tous les groupes d’options, les erreurs, les locales et les bornes d’argv. Vérifier le SHA de l’ELF admis dans chaque ISA, la collecte, les deux canaux et l’absence de droit de fichiers. Pour `-z`, comparer les octets sur la destination binaire publique lorsqu’elle sera disponible. Rejouer la consultation sans effet et comparer les empreintes P9 avant et après. La distribution UTM et le matériel physique sont hors de cette qualification.
