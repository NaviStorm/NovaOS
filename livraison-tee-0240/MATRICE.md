# Conformité à GNU tee 9.12

Le jeu Darwin compte 191 invocations par implémentation. Chaque ligne de la [recette différentielle](preuves/macOS.json) conserve les arguments, la locale, le statut exact de chaque processus, stderr, la longueur et le SHA-256 de stdout et de chacun des fichiers. L’oracle et Nova travaillent sur le même corpus pseudo-aléatoire et des répertoires indépendants initialisés à l’identique. Le test compare les flux binaires et les états finaux. Les messages d’erreur sont comparés après normalisation du nom de l’exécutable. Les sorties documentaires sont contrôlées selon ARC-026.

| Forme ou situation | Essai discriminant Darwin, résultat | Quatre profils NovaOS |
|---|---|---|
| Sans opérande, un, plusieurs | Copie exacte sur stdout et les fichiers, jusqu’à 13 noms. Un flux vide tronque l’existant. | Moteur du témoin avec stdout capturé, miroir et destination P9. Création de 0 et 1 030 octets relue hors VM. |
| `-a`, `--append`, `--a`, répétitions | Un fichier prérempli conserve son préfixe. Nom répété avec deux ouvertures en ajout, octets comparés à GNU. | Options admises. L’ouverture P9 d’un nom occupé renvoie une erreur et conserve les octets. |
| `-i`, `--ignore-interrupts` | SIGINT après réception de « pret ». Signal observable sans option, copie complète avec chacune des formes et avec `-aip`. | Options admises. L’ABI ne livre aucun SIGINT POSIX au processus. |
| Défaut | Pipe déjà fermé, SIGPIPE si disposition par défaut. Si SIGPIPE est hérité ignoré, la copie suivante continue sans diagnostic EPIPE. Une erreur de fichier produit le statut 1 et les autres sorties continuent. | Le moteur renvoie CLOSED (11) pour une fermeture native en mode par défaut. Erreurs de fichier distinctes. Fermeture injectée au niveau du moteur. |
| `-p`, `--output-error`, `--output-error=warn-nopipe` | Pipe fermé ignoré, fichier suivant complet, statut 0. Dernier lecteur fermé et entrée muette, fin anticipée. Erreur de fichier signalée, poursuite avec statut 1. | Même décision sur l’erreur injectée. L’attente simultanée du producteur et de la fermeture de la sortie reste à raccorder dans le transport générique. |
| `--output-error=warn` | EPIPE diagnostiqué, fichiers suivants complets, statut 1. | Diagnostic compté et poursuite dans le témoin. |
| `--output-error=exit` | EPIPE sur stdout, statut 1 et fichier suivant déjà créé mais vide. Une ouverture refusée arrête avant l’ouverture suivante. | Arrêt au premier échec simulé, contrôle des octets du miroir. |
| `--output-error=exit-nopipe` | EPIPE silencieux et copie suivante intégrale. Échec de fichier signalé et arrêt immédiat. | Les deux branches sont exercées séparément dans le moteur. |
| Valeur facultative, préfixes | `--output-error exit` crée le fichier nommé `exit`. Chaque préfixe de chaque option longue et des quatre valeurs est comparé. `w`, `ex` et la valeur vide sont ambigus. Les valeurs exactes courtes `warn` et `exit` sont prioritaires. | Statut du véritable ELF pour les vecteurs ASCII dédupliqués. Les modes effectifs complets sont exercés dans le témoin. |
| Options groupées ou intercalées | `-aiip`, option après opérande, groupes invalides et permutations `-p --output-error=exit` / inverse. Le dernier réglage de la politique prévaut. `POSIXLY_CORRECT` arrête l’analyse au premier opérande. | Groupement et priorité dans le parseur commun. L’environnement POSIXLY_CORRECT appartient au port Darwin. |
| `-`, `--`, nom vide, nom non UTF-8 | `-` est un fichier ordinaire. Après `--`, `--append` est un nom. Les erreurs de nom vide ou d’octets interdits par le volume sont comparées. | L’ABI argv accepte de l’UTF-8. Les octets du contenu sont indépendants de cet encodage. |
| Arguments invalides | `-h`, `-v`, option inconnue, argument interdit, mode inconnu ou ambigu. Statut 1 et aucun effet fichier. `--help` interrompt l’analyse immédiatement. | Validation avant la demande de capacité. Statuts vérifiés par attente de la fin de l’ELF. |
| Lecture/écriture partielles | Pipe non bloquant avec lecteur retardé, quota de 1 537 octets, sorties en lecture seule, entrée répertoire, entrée/sortie fermées. Comparaison des préfixes, de l’ordre des erreurs et du statut. | Écriture du témoin par blocs de 128 octets avec WRITE et SYNC. Les fautes E/S et CLOSED du modèle sont injectées séparément. |
| `--help`, `--version`, `man tee` | Aide en français pour fr et fr-CA, anglais pour en-US, de-DE et fr--CA. Version du paquet nova-cli 0.20.0. Le manuel interactif appartient à nsh. | Vrai nsh avec fr-CA, en-US et fr--CA. Aide, version et manuel renvoient 0. SHA du volume et compteurs QMP de modification identiques avant/après. L’ELF exact répond également dans le profil SDK. |

**Lecture des statuts invités.** Les 148 jeux d’arguments distincts de la recette SDK reçoivent uniquement OUTPUT et ERROR. Une commande métier valide atteint donc le refus d’entrée 5, une syntaxe invalide rend 1 et une requête documentaire rend 0. Les résultats métier par mode viennent du témoin avec des flux identifiés. La syntaxe réelle `cat entree | tee sortie` rend 3 dans nsh sur cette base, avant création de sortie. Les fichiers des quatre profils conservent les deux séries indépendamment.

| Architecture / secteur P9 | Arguments et statut de l’ELF | Moteur, documentation nsh et relecture P9 |
|---|---|---|
| AArch64 / 512 | [148 appels](preuves/aarch64-512/elf.json) | [Résultats](preuves/aarch64-512/resultats.json) |
| AArch64 / 4096 | [148 appels](preuves/aarch64-4096/elf.json) | [Résultats](preuves/aarch64-4096/resultats.json) |
| x86_64 / 512 | [148 appels](preuves/x86_64-512/elf.json) | [Résultats](preuves/x86_64-512/resultats.json) |
| x86_64 / 4096 | [148 appels](preuves/x86_64-4096/elf.json) | [Résultats](preuves/x86_64-4096/resultats.json) |
