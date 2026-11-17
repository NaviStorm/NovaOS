# Compatibilité de date avec GNU Coreutils 9.12

La [matrice par invocation](matrice.tsv) donne les arguments exacts, le contexte, le statut et les flux observés sur Darwin, puis le verdict et l’oracle de chaque profil natif. Les [observations Darwin](preuves/options.jsonl) et les fichiers `sdk-*.json.gz` conservent les octets en base64, le PID invité, les deux ports et le statut. Une cellule « non exercé » désigne uniquement le scénario de cette ligne. Les délégations de fichier et les pipes disposent de leur propre recette `lot`. La documentation et les appels ordinaires disposent de la recette `nsh`.

| Interface | Valeurs, défaut et interactions | Identifiants de la matrice et contrôle |
|---|---|---|
| Sans option, `+FORMAT` | Heure courante. Le format vide et les directives numériques, textuelles, de semaine ISO, de zone, d’époque et de fraction s’appliquent au même instant. | 1–3, 91–137 et série de modificateurs à partir de 138. Heure réelle dans l’intervalle mesuré. |
| `-d`, `--date` | Une chaîne, attachée ou séparée. Époque signée, nanosecondes, années bissextiles, dates absolues et relatives, abréviations et transitions de fuseau. Dernière occurrence retenue. | 18–19, 27, 29, 40, 46–49, 81–84, 91–130. Erreur 1 pour argument absent ou invalide. |
| `-f`, `--file` | Un chemin ou `-` pour stdin. Une sortie par ligne valide. Un lot mixte continue après l’erreur et rend 1. Un argument de format est accepté. | 20–21, 31, 33–34, 41, 50, 81, 85–87. Fichier absent, accès interdit, répertoire, erreur de ligne, dernière ligne sans saut. Recette de flux P9 séparée. |
| `-r`, `--reference` | mtime de la cible. Date de modification connue et précision de la plateforme. | 22–23, 32, 42, 82, 85, 88–89. Recette P9 avec et sans mtime. Un objet non délégué est refusé. |
| `-s`, `--set` | Chaîne ou date numérique `MMDDhhmm[[CC]YY][.ss]`. Le refus écrit le diagnostic, affiche l’instant demandé et rend 1. | 24–26, 30, 43, 51, 84, 87, 89–90 et série numérique. Sur Darwin, succès et refus du double privé. Dans NovaOS, refus du réglage. |
| `-u`, `--utc`, `--uct`, `--universal` | Fuseau UTC pour l’interprétation et l’affichage, y compris en combinaison et en groupe court. | 5–8, 27, 51, séries d’alias à époque positive et négative. |
| `-I`, `--iso-8601` | Sans valeur, date seule. Valeur courte accolée ou longue avec `=`. Valeurs `date`, `hours`, `minutes`, `seconds`, `ns` et préfixes uniques. Fraction avec virgule. | 9, 52–71. Valeur vide des options longues, valeur erronée et dernières options de format. Les lignes `UTC-…` utilisent le fuseau effectif de l’invité. |
| `-R`, `--rfc-email`, `--rfc-822`, `--rfc-2822` | Format courriel avec noms anglais et décalage numérique. Priorité à la dernière option de format prédéfini. | 10–13, 45, 52–53 et séries d’alias. |
| `--rfc-3339` | Précision requise `date`, `seconds`, `ns` ou préfixe unique. Fraction avec point. | 16, 72–80 et séries de préfixes. Valeurs inconnues ou vides, erreur 1. |
| `--resolution` | Résolution du clock, format par défaut `%s.%N`. Accepte un format explicite. | 14–15, 17, 83, 86, 88, 90. RTC v1 à une seconde. |
| `--debug` | Analyse et étapes de conversion sur stderr. stdout contient la date. Une entrée invalide donne le statut 1. | 4, 17, 47–50. Script GNU `date-debug.sh` sur Darwin. |
| `--help`, `--version`, `man date` | ARC-026. Langue principale fr sur étiquette BCP 47 validée, anglais sinon. Les préfixes uniques sont reconnus. La version annonce le fournisseur Nova. | Cas ARC-026, tests du catalogue et 3 langues de la recette nsh. Même SHA-256 et mêmes compteurs écriture/flush P9 avant et après consultation. |
| `--`, diagnostics et combinaisons | Fin effective des options. Deux formats explicites, plusieurs sources de lecture ou lecture associée à un réglage rendent 1. Formats prédéfinis répétés et options abrégées suivent GNU. | 2, 27–29, 35–53, 81–90 et séries de préfixes. Un échec opérationnel préserve stdout déjà produit. |

## Limites vérifiables

- Le terminal v1 filtre certains octets de contrôle. Les résultats du catalogue identifient le refus de sortie et son statut. Le flux natif de pipe dispose d’un transport distinct.
- L’ABI v1 fournit la consultation RTC UTC par secondes entières. `--resolution` et la précision automatique `%-N` en dépendent. Le réglage réussi est qualifié exclusivement sur un double de l’horloge Darwin.
- Le lancement de recette reçoit UTC et la locale de formatage C. Les chaînes qui nomment leur fuseau peuvent utiliser la base embarquée. La transmission de TZ et LC_TIME d’une session vers le programme installé reste à qualifier. Les textes d’aide suivent séparément ARC-026.
- Le formatage borne la largeur d’un champ à 1 048 576 caractères. L’entrée native utilise les bornes d’arguments et d’objets de son profil. Les diagnostics des octets non UTF-8 dans les lignes natives passent par un décodage avec remplacement.
- Les messages d’erreur fonctionnels sont qualifiés en locale C. Les cas localisés de Darwin comparent les formats de date. Les diagnostics traduits de toutes les locales GNU et les formats d’ère de toutes les locales restent hors de la couverture mesurée.
- Le chemin ordinaire de `nsh` requiert encore le raccord de la session à NovaAuth, la résolution de l’ELF installé et les délégations de fichiers et flux associées. Le profil de catalogue et le profil de lot exécutent les ELF livrés dans des copies instrumentées.
