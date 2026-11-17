# Matrice de readlink 0.24.0

La [matrice machine](preuves/options.json) conserve argv, stdout et stderr en hexadécimal, les deux statuts, l’environnement, le type de sortie et le résultat de chaque comparaison. Le banc macOS neutralise le préfixe de diagnostic propre au binaire Homebrew. Les pages de documentation ont leur oracle ARC-026 bilingue.

| Option ou alias | Valeur, combinaison et résultat attendu | macOS ARM64 | ELF de recette sur les quatre profils P9 |
|---|---|---|---|
| Aucune | Cible brute, LF, échec silencieux sur fichier ordinaire. Sans opérande, diagnostic et statut 1. | Conforme | Lecture indisponible, statut 1 |
| -f, --canonicalize | Suit les liens physiquement. Accepte la dernière composante absente. Une barre après un fichier ordinaire échoue. | Conforme | Parcours indisponible, 1 |
| -e, --canonicalize-existing | Exige toutes les composantes. Dernier choix parmi f/e/m prioritaire. | Conforme | Parcours indisponible, 1 |
| -m, --canonicalize-missing | Accepte les composantes indisponibles. Suit les liens avant les remontées. | Conforme | Parcours indisponible, 1 |
| -n, --no-newline | Un seul opérande supprime LF ou NUL. Plusieurs noms déclenchent un avertissement même en mode silencieux et conservent les séparateurs. | Conforme | Avertissement vérifié sur plusieurs noms. Aucun résultat P9 à délimiter |
| -q, --quiet | Tait les erreurs de chemins. POSIXLY_CORRECT force le diagnostic GNU même avec -q. | Conforme | Refus silencieux, 1. Pas d’environnement POSIX général dans l’ELF |
| -s, --silent | Même effet que -q. Dernier choix parmi q/s/v prioritaire hors POSIX. | Conforme | Refus silencieux, 1 |
| -v, --verbose | Signale chaque échec sur stderr et traite les opérandes suivants. | Conforme | Diagnostic de lecture indisponible pour chaque nom, 1 |
| -z, --zero | NUL après chaque résultat, sauf -n à un seul nom. Désactive la citation TTY. | Conforme, dont pipe | Sortie de données binaire non qualifiée |
| --help, --version | Consultation immédiate, 0. Le premier choix documentaire prévaut. Version Nova 0.24.0. | ARC-026 | 0, sorties anglaise et française contrôlées |
| --, tiret isolé | Fin des options, tiret isolé opérande. GNU permute hors POSIX. En POSIX le premier opérande termine les options. | Grammaire contrôlée | -vs -- -z refuse le nom -z |
| Formes longues | Aucun argument. Nom exact ou préfixe unique. --ver ambigu. Valeur avec =, option inconnue ou cluster invalide donnent 1. | Préfixes, valeurs et erreurs contrôlés | Cas invalides contrôlés. Aucune équivalence des erreurs FS |
| man readlink | Page du catalogue de nsh. Validation de la langue principale et des sous-étiquettes par le composant commun IANA. | Rendu du catalogue testé | Page consultée dans le vrai terminal |
| Descripteurs | Entrée ignorée. Sortie redirigée en octets. PTY et QUOTING_STYLE, sortie fermée, EBADF, SIGPIPE par défaut ou ignoré. | Cas comparés | Pipe vers wc refusé dans nsh, statut 3 |

Le banc Darwin croise les 64 paires de lettres courtes, les alias sur un ou plusieurs opérandes, les liens pendants, cycles, cibles relatives, cibles binaires, refus de traversée, barres finales et remontées à la racine. Les huit scripts GNU ajoutent les répertoires de travail supprimés et la fausse boucle par passage répété sur le même lien. [Statuts amont](preuves/amont/statuts.txt)

Le moteur en mémoire utilise une racine synthétique et des erreurs explicites. L’ELF livré refuse les demandes métier faute de contrat de parcours délégué. Depuis nsh, l’appel ordinaire retourne 127 sur la base qualifiée.
