# Matrice GNU sed 4.10 · ARC-027

**Identique** couvre stdout et stderr octet par octet, le statut et l’état final des fichiers. **Diagnostic** indique que seul stderr diffère. **Moteur C** désigne une exécution du témoin ELF avec l’oracle en locale C et un flux privé livré par morceaux de trois octets. Les alias apparaissent tels qu’exécutés. La chronologie de la vidange avec `-u` reste non mesurée.

Les cas **bloqués** exigent l’admission, une délégation ou le remplacement P9. **Non exercé** signifie que le cas précis manque de preuve invitée. **nsh** désigne le traitement documentaire du shell. L’aide et la version ont le contenu propre à NovaOS.

[Détail C](results/C.json) · [Détail UTF-8](results/en_US.UTF-8.json) · [Entrées et état initial](differential.py) · [Témoins embarqués](../src/guest_vectors.in)

| Cas | Arguments | Darwin C | Darwin UTF-8 | AArch64 | x86_64 |
|---|---|---|---|---|---|
| no-match | `-n s/introuvable/X/` | Identique | Identique | moteur C | moteur C |
| address-number | `-n 2p` | Identique | Identique | moteur C | moteur C |
| last | `-n '$p'` | Identique | Identique | moteur C | moteur C |
| address-regex | `-n '/^b/p'` | Identique | Identique | moteur C | moteur C |
| range | `-n 1,2p` | Identique | Identique | moteur C | moteur C |
| negated | `-n '/a/!p'` | Identique | Identique | moteur C | moteur C |
| zero-range | `-n 0,/b/p` | Identique | Identique | moteur C | moteur C |
| step | `-n '1~2p'` | Identique | Identique | moteur C | moteur C |
| relative | `-n 1,+1p` | Identique | Identique | moteur C | moteur C |
| BRE-captures | `-n 's/\([a-z]\)\([0-9]\)/\2\1/p'` | Identique | Identique | moteur C | moteur C |
| BRE-backref | `-n 's/\(a\)\1/X/p'` | Identique | Identique | moteur C | moteur C |
| ampersand | `-n 's@a@[&]@p'` | Identique | Identique | moteur C | moteur C |
| second-global | `-n s/a/Z/2gp` | Identique | Identique | moteur C | moteur C |
| global | `-n s/a/Z/gp` | Identique | Identique | moteur C | moteur C |
| print-first | `-n 'N;P'` | Identique | Identique | moteur C | moteur C |
| delete | `-n '2d;p'` | Identique | Identique | moteur C | moteur C |
| restart | `-n 'N;P;D'` | Identique | Identique | moteur C | moteur C |
| next | `-n 'n;p'` | Identique | Identique | moteur C | moteur C |
| join | `-n 'N;s/\n/ + /;p'` | Identique | Identique | moteur C | moteur C |
| hold | `-n 'h;n;G;p'` | Identique | Identique | moteur C | moteur C |
| hold-append | `-n 'H;$ {g;p;}'` | Identique | Identique | moteur C | moteur C |
| exchange | `-n 'h;n;x;p'` | Identique | Identique | moteur C | moteur C |
| translate | `-n 'y/ab/AB/;p'` | Identique | Identique | moteur C | moteur C |
| label | `-n ':again ↵ s/aa/a/;t again ↵ p'` | Identique | Identique | moteur C | moteur C |
| branch | `-n '1b end ↵ p ↵ :end'` | Identique | Identique | moteur C | moteur C |
| T | `-n 's/a/z/;T end ↵ p ↵ :end'` | Identique | Identique | moteur C | moteur C |
| append | `-n 'a more'` | Identique | Identique | moteur C | moteur C |
| insert | `-n 'i first'` | Identique | Identique | moteur C | moteur C |
| change | `-n '1c changed'` | Identique | Identique | moteur C | moteur C |
| number | `-n =` | Identique | Identique | moteur C | moteur C |
| list | `-n l` | Identique | Identique | moteur C | moteur C |
| read | `-n 'r aux'` | Identique | Identique | bloqué | bloqué |
| read-line | `-n 'R aux'` | Identique | Identique | bloqué | bloqué |
| write | `-n 'w dest'` | Identique | Identique | bloqué | bloqué |
| write-first | `-n 'N;W dest'` | Identique | Identique | bloqué | bloqué |
| subst-write | `-n 's/a/z/w dest'` | Identique | Identique | bloqué | bloqué |
| exec | `-n 'e printf FIXED'` | Identique | Identique | bloqué | bloqué |
| subst-exec | `-n 's/.*/printf FIXED/ep'` | Identique | Identique | bloqué | bloqué |
| clear | `-n 'z;p'` | Identique | Identique | moteur C | moteur C |
| filename | `-n F` | Identique | Identique | non exercé | non exercé |
| version-instruction | `-n 'v 4.10'` | Identique | Identique | moteur C | moteur C |
| comment | `-n '#n ↵ p'` | Identique | Identique | moteur C | moteur C |
| invalid | `-n '?'` | Diagnostic | Diagnostic | rejet moteur | rejet moteur |
| invalid-regex | `-n 's/[/Z/'` | Diagnostic | Diagnostic | non exercé | non exercé |
| q | `-n q42` | Identique | Identique | moteur C | moteur C |
| Q | `-n Q37` | Identique | Identique | moteur C | moteur C |
| caseflag | `-n s/A/z/Ip` | Identique | Identique | moteur C | moteur C |
| multiline | `-n 'N;s/^b/B/Mp'` | Identique | Identique | moteur C | moteur C |
| replacement-case | `-n 's/a/\U&/p'` | Identique | Identique | moteur C | moteur C |
| posix-longest | `-n 's/\(a\\|aa\)/[&]/p'` | Identique | Identique | moteur C | moteur C |
| witness-ERE | `-nE 's/^([a-z])([0-9])$/\2\1/p'` | Identique | Identique | moteur C | moteur C |
| witness-N | `-n 'N;s/\n/ + /;p'` | Identique | Identique | moteur C | moteur C |
| witness-no-match | `s/introuvable/X/` | Identique | Identique | moteur C | moteur C |
| witness-q | `q42` | Identique | Identique | moteur C | moteur C |
| witness-files | `-n -e 1p -e '$p' -- one` | Identique | Identique | bloqué | bloqué |
| mixed-scripts | `--silent --file=prog --expression=1p one two` | Identique | Identique | bloqué | bloqué |
| inter-file | `-n 2,3= one two` | Identique | Identique | bloqué | bloqué |
| separate | `-sn '$=' one two` | Identique | Identique | bloqué | bloqué |
| invalid-utf8 | `s/a/z/` | Identique | Identique | moteur C | moteur C |
| invalid-regex-dot | `s/./X/g` | Identique | Écart stdout | moteur C | moteur C |
| null | `--null-data s/a/z/` | Identique | Identique | moteur C | moteur C |
| null-multi | `-zn 'N;P'` | Identique | Identique | moteur C | moteur C |
| unterminated | `s/a/z/` | Identique | Identique | moteur C | moteur C |
| empty | `p` | Identique | Identique | moteur C | moteur C |
| long | `s/a/Z/` | Identique | Identique | non exercé | non exercé |
| missing | `p absent one` | Diagnostic | Diagnostic | bloqué | bloqué |
| unknown | `--oops` | Diagnostic | Diagnostic | non exercé | non exercé |
| posix | `--posix Q` | Diagnostic | Diagnostic | non exercé | non exercé |
| unbuffered | `--unbuffered 1q` | Identique | Identique | non exercé | non exercé |
| long-ere | `--regexp-extended 's/(a)/X/'` | Identique | Identique | moteur C | moteur C |
| r-alias | `-r 's/(a)/X/'` | Identique | Identique | moteur C | moteur C |
| binary | `-b p` | Identique | Identique | moteur C | moteur C |
| line-length | `--line-length=4 -n l` | Identique | Identique | moteur C | moteur C |
| debug | `--debug 1q` | Écart stdout, stderr, code | Écart stdout, stderr, code | non exercé | non exercé |
| sandbox | `--sandbox 'w dest ↵ e printf FIXED'` | Diagnostic | Diagnostic | rejet apparenté¹ | rejet apparenté¹ |
| sandbox-late | `--sandbox 'w new ↵ r aux'` | Diagnostic | Diagnostic | non exercé | non exercé |
| sandbox-subst | `--sandbox 's/a/b/w dest'` | Diagnostic | Diagnostic | rejet apparenté¹ | rejet apparenté¹ |
| sandbox-exec | `--sandbox s/a/b/e` | Diagnostic | Diagnostic | rejet apparenté¹ | rejet apparenté¹ |
| inplace | `-i s/a/Z/ one` | Identique | Identique | bloqué | bloqué |
| backup | `-i.bak s/a/Z/ one` | Identique | Identique | bloqué | bloqué |
| star-backup | `'--in-place=*.saved' s/a/Z/ one` | Identique | Identique | bloqué | bloqué |
| symlink | `-i s/a/Z/ link` | Identique | Identique | bloqué | bloqué |
| follow | `-i --follow-symlinks s/a/Z/ link` | Identique | Identique | bloqué | bloqué |
| help | `--help` | ARC-026 | ARC-026 | nsh, statut 0 | nsh, statut 0 |
| version | `--version` | ARC-026 | ARC-026 | nsh, statut 0 | nsh, statut 0 |
| help-first | `-i --help` | ARC-026 | ARC-026 | nsh, statut 0 | nsh, statut 0 |
| long-separate | `--quiet --separate '$=' one two` | Identique | Identique | bloqué | bloqué |
| short-file | `-nf prog one` | Identique | Identique | bloqué | bloqué |
| short-width | `-nl4 l` | Identique | Identique | moteur C | moteur C |
| short-unbuffered | `-u 1q` | Identique | Identique | moteur C | moteur C |
| long-binary | `--binary p` | Identique | Identique | moteur C | moteur C |
| stdin-operand | `-n '$p' -` | Identique | Identique | non exercé | non exercé |
| script-on-stdin | `-nf - one` | Identique | Identique | bloqué | bloqué |
| tilde-range | `-n '2,~2p'` | Identique | Identique | moteur C | moteur C |
| address-I | `-n /B/Ip` | Identique | Identique | moteur C | moteur C |
| address-M | `-n 'N;/^b/Mp'` | Identique | Identique | moteur C | moteur C |
| address-previous | `-n '/a/{s//Z/;p;}'` | Identique | Identique | moteur C | moteur C |
| v-high | `'v 99.0'` | Diagnostic | Diagnostic | non exercé | non exercé |
| substitution-print-exec | `-n 's/.*/printf FIXED/pe'` | Identique | Identique | bloqué | bloqué |
| exec-pattern | `e` | Identique | Identique | bloqué | bloqué |
| comment-ordinary | `'# comment ↵ p'` | Identique | Identique | moteur C | moteur C |
| no-input-inplace | `-i p` | Diagnostic | Diagnostic | bloqué | bloqué |
| inplace-quiet | `-ni 2p one` | Identique | Identique | bloqué | bloqué |
| inplace-suffix-dest | `-idest s/a/Z/ one` | Identique | Identique | bloqué | bloqué |
| script-missing | `-f absent` | Diagnostic | Diagnostic | bloqué | bloqué |
| write-no-match | `'s/XXX/Y/w new'` | Identique | Identique | bloqué | bloqué |
| read-absent | `'r absent'` | Identique | Identique | bloqué | bloqué |
| write-refused | `'w absentdir/dest'` | Diagnostic | Diagnostic | bloqué | bloqué |
| sandbox-exec-first | `--sandbox 'e printf FIXED ↵ w dest'` | Diagnostic | Diagnostic | non exercé | non exercé |
| unicode-dot | `s/./X/g` | Identique | Écart stdout | moteur C | moteur C |
| line-no-final-p | `p` | Identique | Identique | moteur C | moteur C |

¹ Les quatre rejets embarqués sont listés dans les fixtures. Ils vérifient l’erreur de compilation avant d’ouvrir un flux. Le statut de processus du témoin vaut 0 si tous les résultats internes correspondent.

## Effets, interactions et limites

| Fonction | Darwin ARM64 | NovaOS AArch64 | NovaOS x86_64 |
|---|---|---|---|
| `man sed`, aide avant `-i`, LANG | Pages mdoc FR et EN rendues, aide et version testées | nsh, quatre consultations et trois tags | Idem |
| `-i` et droits | Sans suffixe, .bak, *, liens suivis/non suivis, mode 0444, ACL et xattrs, EIO, ENOSPC, SIGKILL | Refus 4, P9 intact | Refus 4, P9 intact |
| `r R w W s///w` | Flux nommés de l’utilisateur, comparés au disque fictif GNU | Bloqués, aucune délégation nommée | Idem |
| `e s///e` et ordre `ep/pe` | /bin/sh, témoins fixes | Pas de capacité de lancement | Idem |
| `--posix` | Sous-ensemble strict du moteur, rejet de Q vérifié, conformité POSIX complète non qualifiée | Non qualifié | Non qualifié |
| `--debug` | Refus 1, GNU produit une trace et termine à 0 | Non pris en charge | Non pris en charge |
| Abréviations des longues options, POSIXLY_CORRECT, collations, classes Unicode | Hors profil qualifié | Hors profil | Hors profil |
| Stdin terminal, stderr séparé pendant une édition ELF, fichier sélectionné, q42 à travers nsh | Canaux et statuts couverts par le différentiel Darwin | Admission requise | Admission requise |
| Lecteur jusqu’à EOF, lecteur quittant tôt, producteur vers wc -l | Hors banc NovaOS | 3 refus avant lancement, statut 3 | 3 refus avant lancement, statut 3 |
| Révocation en vol, réveil du producteur avec CLOSED=11, petite écriture acquittée | Hors profil Darwin | Bloqués avant création du pipe | Idem |

## Écarts reproductibles

Les scripts générés couvrent des regex valides et invalides avec `--sandbox`, graine 270240. Les réductions conservent la catégorie d’écart, les statuts et le diagnostic hors position. Chaque réduction repasse le contrôle des fichiers. Les instantanés complets figurent dans le rapport de la locale. [Cas réduits en C](results/minimal-C.json) · [Cas réduits en UTF-8](results/minimal-en_US.UTF-8.json)

| Cas | GNU 4.10 | NovaOS sed sur Darwin | Effet |
|---|---|---|---|
| fuzz-005, ERE `$^`, entrée hex 00 | Aucune impression | Imprime 00 | Une sélection en trop. |
| fuzz-015/063, ERE `^*` | Syntaxe invalide, statut 1 | Acceptée, statut 0 | Une erreur de script peut passer inaperçue. |
| unicode-dot, entrée é漢 | Deux remplacements en UTF-8 | Cinq remplacements | Compte de caractères différent. |
| invalid-regex-dot, entrée ff c3 a9 0a | ff 58 0a en UTF-8 | 58 58 58 0a | Une substitution peut modifier un octet UTF-8 invalide. |
| fuzz-076, sélection par point, entrée ff | Aucune impression en UTF-8 | Imprime ff | Sélection différente de données invalides. |
| 76 écarts de stderr seul par locale | Chemin de gsed, position et libellé GNU | Préfixe sed, erreur QSE ou Darwin | Interprétation textuelle de stderr affectée. |
| --help, -i --help, --version | Texte et version GNU | Texte NovaOS, statut 0 | Extraction de version différente. |
| --debug | Trace du cycle et des lignes, statut 0 | Diagnostic, statut 1 | Traceur indisponible. |
| -i.bak sur un fichier 0444 dans un dossier inscriptible | Remplace le fichier, statut 0 | Refus 4, original conservé | Droit d’écriture sur l’original exigé. |
