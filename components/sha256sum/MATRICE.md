# Options et limites soumises à la MOA/MOE

| Usage | Darwin ARM64 | NovaOS AArch64 et x86_64 |
|---|---|---|
| Calcul par fichier, mode texte et binaire | Octets et marqueurs comparés à GNU, y compris UTF-8 invalide, NUL et CRLF | Lecture P9 sur les handles sélectionnés, empreinte binaire identique. `-b/--binary` et `-t/--text` marquent la sortie. |
| Sans opérande et `-` | stdin seul, répété, ou entouré de fichiers | Le pipe admis est `cat FICHIER \| sha256sum`. Le lecteur reçoit uniquement ses sorties terminal et l’extrémité de réception. La lecture clavier de données jusqu’à EOF et les pipelines arbitraires restent hors du profil. |
| `--tag` | Émission et relecture BSD, bidirectionnelles avec GNU | Émission BSD, analyseur de manifeste commun. |
| `-z/--zero` | Vérifié jusqu’au NUL final, noms bruts | Le terminal de l’ABI v1 accepte du texte. Une sortie NUL y échoue avec le statut natif 25. |
| `-c/--check` | Un ou plusieurs manifestes sur fichier ou stdin, relatifs au répertoire courant | Les variables explicites `SHA256SUM_READ_1` à `SHA256SUM_READ_32` nomment les fichiers admis. Le shell délègue chaque handle existant au lancement. Un nom de manifeste à lui seul n’accorde aucun droit. |
| `--ignore-missing` | Un absent et un vérifié, tous absents, fichier illisible | Un absent explicitement sélectionné peut être ignoré. Un voisin non sélectionné reste refusé avec le code 5 et entraîne le statut de vérification 1. |
| `--quiet`, `--status` | Succès, différences, erreurs d’E/S, vérification du statut, priorité de la dernière option de rapport | Même logique utilisateur ; les erreurs d’accès contiennent la cause native. |
| `--strict`, `-w/--warn` | Lignes mal formées seules, avant ou après une concordance, compteurs et statuts | Même analyseur et mêmes règles de statut. |
| Aide, version et manuel | Aide anglais/français ; `man -M ./man sha256sum` dans la livraison | `sha256sum --help` et `--version` dans un ELF sans fichier métier ; `man sha256sum` dans le lecteur natif ; sélection ARC-026 par `LANG`. |
| Formes incompatibles | Vérification avec mode texte/binaire explicite, `--tag` ou `--zero`, modes de rapport sans `-c`, messages comparés à GNU en locale C | Le shell contrôle les options avant de préparer les handles. Un refus de syntaxe par `nsh` donne le statut 1 et son diagnostic sur le terminal. |

Le test rapide compare 208 triplets (statut, stdout, stderr), normalise le nom d’invocation GNU dans les diagnostics et vérifie séparément les documents localisés. Le volume hôte a refusé la création d’un nom hors UTF-8 (errno 92). P9 possède des fichiers et répertoires, ses chemins sont en UTF-8 et son format ne représente aucun lien symbolique. Les refus de chemins absolus et de remontée ont été exercés avec des noms extérieurs à la liste autorisée.

Les métadonnées privées et les arguments partagent les 2 560 octets et 32 arguments du lanceur. La sélection P9 admet au maximum 32 objets, manifestes compris. L’ELF compare les noms à l’inventaire autorisé par leur orthographe exacte. Une ligne de manifeste de plus de 16 384 octets est traitée comme mal formée pour borner la mémoire. Ce plafond peut différer de GNU pour des lignes artificiellement prolongées par des blancs.

La preuve du pipe concerne les octets effectivement reçus jusqu’à EOF. Le statut du pipeline `nsh` est celui du lecteur. Une erreur propre au producteur figure dans son statut, et l’ABI de flux existante ne transmet pas au lecteur une attestation de réussite du producteur. Pour vérifier un fichier entier identifié, utiliser la lecture directe du fichier et `-c`. Une erreur après lecture partielle de son handle empêche toute émission du digest du préfixe.

Le pipe vers `sha256sum` accepte uniquement le lecteur sans argument. La vérification d’un manifeste transmis par stdin et le mélange fichier/stdin sont qualifiés sur macOS. La vérification sur P9 s’effectue depuis un manifeste nommé. Les textes documentaires appartiennent à NovaOS. La compatibilité GNU complète et l’élargissement du profil de flux restent soumis à l’arbitrage MOA/MOE.
