# Matrice de conformité de seq

Le Mach-O passe 1 395 contrôles. Les 446 invocations différentielles sont rejouées en `C`, `en_US.UTF-8` et `fr_FR.UTF-8`. Chaque comparaison exige le même statut et les mêmes octets sur les deux flux. Pour stderr, le comparateur retire uniquement le préfixe initial `seq: ` ou `gseq: `. Les fichiers bruts conservent ces préfixes. S’ajoutent 44 contrôles documentaires ARC-026, dix fermetures de lecteur et trois fermetures de stdout.

| Domaine | Cas C représentatifs | Comportement observé |
|---|---|---|
| Un, deux, trois opérandes | 2–16, 30 | Départ et pas implicites de 1, dernière valeur incluse si atteinte, aucune sortie pour une plage vide. |
| Entrées numériques | 9–27, 433–444 | Signe de zéro, refus de NaN et d’un pas nul, infini, sous-flux, débordement, décimal et hexadécimal, exposants, décimales à la borne. |
| Fin des options et erreurs | 28–45, 53–55 | `--`, arrêt au premier opérande, valeur manquante, option groupée invalide, argument superflu. |
| `-f`, `--format`, `--f` à `--forma` | 42–45, 56–61, 118, 120, 160–437, 445–446 | Formes accolées, séparées ou avec égal. Une conversion flottante, zéro ou plusieurs `%%`, un préfixe et un suffixe arbitraires. La dernière valeur du format l’emporte. |
| Drapeaux et conversions | 160–415 | `a A e E f F g G`, longueur `L`, drapeaux `- + # 0 espace '`, largeur, précision absente, nulle ou explicite. Préfixe hexadécimal et signe avant le remplissage. |
| Formats rejetés | 42–44, 82–84, 416–431 | Format vide ou sans conversion, directive terminale, conversions multiples, `%n`, `%d`, `*`, position, modificateur invalide, format avec `-w`. La largeur hors plage ne déclenche une erreur que si une valeur doit être imprimée. |
| `-s`, `--separator` et tous ses préfixes uniques | 62–70, 85–120 | Séparateur vide, multioctet, contrôle, octet hors UTF-8, argument ressemblant à une option. Séparateur entre valeurs et LF en fin de séquence non vide. |
| `-w`, `--equal-width` et tous ses préfixes uniques | 71–81, 114, 121–159 | Zéros en tête, signe, point implicite, précision du pas, exposant, combinaison avec le séparateur. |
| Grandes sorties | 432, 438–442 et profil complet | Précision de 70 000, franchissement d’une puissance de dix jusqu’à 8 192 chiffres sur Mac, suite de 65 000 000 valeurs. |
| `--help`, `--version` et leurs préfixes | ARC-026 | Sortie immédiate, version réelle, français admis et repli anglais. Les valeurs `-s --version` et `-f --help` restent des arguments. |
| Erreurs d’écriture | Contrôles de flux | SIGPIPE par défaut, EPIPE et statut 1 si le signal est ignoré, EBADF sur stdout fermé avec données, succès pour une sortie vide. |

Les [résultats par argument et cible](preuves/matrice.tsv.gz) associent chaque cas à son statut. Les [captures macOS](preuves/options.jsonl.gz) contiennent argv, stdout et stderr en Base64. Les fichiers `sdk-*.json.gz` donnent le PID natif, le statut, stdout et stderr réellement observés. Les journaux série et les ISO correspondantes se trouvent dans le même dossier.

Le cas 89, `seq -s :: 7 9`, donne les huit octets `37 3a 3a 38 3a 3a 39 0a`. Le cas 85, avec séparateur vide, donne `37 38 39 0a`. Le cas 6, dont les bornes décroissent avec un pas implicite positif, réussit avec zéro octet.

Le véritable lecteur macOS a également rendu le manuel dans quatre locales, avec le français de France et du Canada et le repli anglais. Le chemin retenu et les deux flux figurent dans la [recette man Darwin](preuves/man-darwin.json).

## Résultats par cible

| Cible | Comparaisons directes GNU C sur ELF | Contrats natifs exercés sur ELF | Vrai nsh |
|---|---:|---:|---:|
| AArch64 P9/512 | 434/434 | 16/16 | 19/19 |
| AArch64 P9/4096 | 434/434 | 16/16 | 19/19 |
| x86_64 P9/512 | 434/434 | 16/16 | 19/19 |
| x86_64 P9/4096 | 434/434 | 16/16 | 19/19 |

Par profil, six cas rencontrent le refus du terminal pour des caractères de contrôle, cinq le refus d’argv pour des octets hors UTF-8, quatre vérifient l’aide et la version françaises et anglaises, un retire stdout et vérifie stderr et le statut 1. L’opérande de 8 192 chiffres dépasse le bloc argv de 2 560 octets de la CLI. Le moteur ELF est exercé jusqu’à 200 chiffres dans cette matrice. Le contrat de locale numérique de l’ELF est C. La locale de documentation de `nsh` est indépendante.

Le banc SDK charge chaque ELF livré dans un processus distinct. Les mots de canaux métier valent zéro. Le contrôleur fournit uniquement les ports de sortie prévus pour le cas et la page d’arguments native. L’observateur copie en trace les octets acceptés par la console avec le port et le PID. L’image P9 est jointe en lecture seule dans ce scénario. Le banc ordinaire monte P9 dans une autre VM, exerce le vrai `nsh` et compare les compteurs QMP d’octets écrits, d’opérations d’écriture et de FLUSH ainsi que le SHA du volume avant et après les quinze invocations documentaires.

Les quatre invocations restantes dans chaque vrai shell vérifient `seq 3`, `seq -s , 3`, `/system/bin/seq 3` (127) et `seq 3 | head -n 1` (3). Le raccord public au producteur externe reste à qualifier. À sa livraison, rejouer ces commandes, le séparateur TAB dans un flux binaire et la fermeture du lecteur, en observant les deux PID, leurs statuts, les octets reçus et la collecte des deux extrémités.

## Temps de l’ELF sur la console série QEMU

Les durées sont les médianes de trois exécutions après échauffement. Chaque sortie de 1 000 valeurs contient 3 893 octets avant rendu. Celle de 10 000 valeurs en contient 48 894. Les campagnes des deux architectures se chevauchent, avec d’autres essais sur le même Mac. Les deux géométries de chaque architecture ont été mesurées successivement.

| Cible | 1 000 valeurs | 10 000 valeurs |
|---|---:|---:|
| aarch64 P9/512 | 0,021353 s | 0,151128 s |
| aarch64 P9/4096 | 0,018196 s | 0,136450 s |
| x86_64 P9/512 | 0,016456 s | 0,135825 s |
| x86_64 P9/4096 | 0,013597 s | 0,108189 s |

## Suite GNU amont

Les 78 cas de `tests/seq/seq.pl` et quatre scripts shell passent sur le Mach-O. `seq-io-errors.sh` demande le périphérique `/dev/full`, absent de ce Mac. `seq-long-double.sh` exige glibc et un `long double` plus large que `double`. Tous deux rendent 77. Les cas EPIPE, EBADF et de sortie refusée ont des contrôles distincts. [Statuts amont](preuves/amont.tsv) · [Journal Perl](preuves/seq-pl.txt)

## Limites de ressources

Les suites ne qualifient pas l’épuisement de la mémoire du processus par une seule conversion de plusieurs gigaoctets. Au-delà de la précision représentable par un entier signé de 32 bits, Nova signale un échec d’allocation pour une plage non vide. Selon le format et la mémoire disponible, la libc Darwin peut consacrer davantage de temps ou de mémoire à cette conversion avant d’échouer.
