# Compatibilité de stat avec GNU 9.12

Les deux ELF de recette interrogent le service NovaFS sur un volume P9 avec des handles sélectionnés par le contrôleur. Les résultats indiqués pour P9 sont communs aux quatre cellules. Au point d’entrée ordinaire de cette révision, seules les consultations documentaires aboutissent. [Darwin, arguments et octets des deux processus](preuves/macos-options.json) · [UART et statuts P9/512 AArch64](preuves/QEMU-aarch64/512/mesures.json) · [P9/4096 AArch64](preuves/QEMU-aarch64/4096/mesures.json) · [P9/512 x86_64](preuves/QEMU-x86_64/512/mesures.json) · [P9/4096 x86_64](preuves/QEMU-x86_64/4096/mesures.json)

| Option ou opérande | macOS ARM64 | ELF de recette NovaOS |
|---|---|---|
| Sans option | Informations du fichier ou dossier, présentation spéciale pour les périphériques, lien affiché avec sa cible | Type, identifiant, taille et dates sélectionnés. Champs indisponibles marqués `?`. Les colonnes utilisent des espaces sur la console |
| `-L`, `--dereference` | Suit la cible. Le lien cassé échoue. Sans option, le lien lui-même est décrit | Option admise. P9 fournit des fichiers et répertoires, sans objet lien symbolique |
| `-f`, `--file-system` | Informations du volume. Modes normal, condensé, format et directive personnalisés | Statut 25 pour les formats de volume. Le handle sélectionné ne fournit pas de statistiques de volume |
| `-t`, `--terse` | Une ligne par objet ou volume | Une ligne par objet. Un format explicite a priorité |
| `-c FORMAT`, `-cFORMAT`, `--format[=]FORMAT` | Format littéral, LF par opérande. La dernière option de format l’emporte | Même moteur. LF est diffusé en CRLF par le terminal |
| `--printf[=]FORMAT` | Échappements, NUL, espaces et absence de LF final testés en octets | Texte UTF-8 accepté. Un NUL ou un caractère de contrôle interdit échoue avec 25 |
| `--cached` avec `always`, `never`, `default` ou préfixe unique | Les trois choix utilisent les appels Darwin disponibles. La campagne porte sur un volume local | Options admises. Le protocole de lecture ne transporte pas de demande de fraîcheur. Aucun effet de synchronisation distante qualifié |
| `--help`, `--version`, `man stat` | Aide et version Nova. Étiquettes fr-CA, en-GB et fr-zzzzz contrôlées | `nsh` rend la page ou la version avant toute résolution d’objet. En production, contrôle SHA du P9 et compteurs QMP d’écriture et de FLUSH avant/après |
| `--`, plusieurs opérandes, regroupements et préfixes | Fin des options, permutation, demandes répétées, nom commençant par un tiret, nom avec espace, option après opérande et erreurs mixtes dans le JSON | Un handle numéroté par position. Les arguments sont UTF-8. La présélection de `f0 absent f1` a fait échouer le groupe entier avant la première ligne |
| `-` | `fstat(0)` en mode fichier. Erreur 1 avec `-f` | Statut 25 pour la requête de métadonnées du flux |
| Option inconnue, valeur absente ou ambiguë, aucun opérande | Statut 1, diagnostic sur stderr. `--f` peut désigner deux options | L’ELF emploie le statut natif 3 pour la syntaxe. À l’invite publique, une commande métier valide aboutit à 127 |
| Chemin absent ou accès refusé | Cas simple et groupe mixte, préfixe de sortie, errno et statut comparés | Chemin absent testé, statut 20. Lecture d’un objet existant sans canaux métier, statut 5. La révocation en cours de lecture reste non qualifiée |
| Pipe | Producteur stat vers wc, NUL et descripteur d’entrée fichier couverts | Syntaxe ordinaire `stat temoin \| wc -l` refusée avec 3. Le lanceur et les rôles de pipe demeurent ceux de la base |

## Directives

Chaque directive figure individuellement dans les sorties brutes. Sur Darwin, la comparaison utilise les mêmes objets, `LC_ALL=C`, `TZ=UTC` et le même style de protection des noms. Les fautes récupérables et le statut non nul de GNU pour `%C` sont inclus.

| Champs fichier | Oracle ou correspondance |
|---|---|
| `%n %Qn %N` | Nom brut ou protégé. Darwin lit également la destination d’un lien pour `%N`. P9 affiche le nom reçu avec la délégation |
| `%i %s %F` | Darwin utilise l’inode, la taille et le type. P9 fournit l’identifiant d’objet, la taille en octets et la nature fichier/répertoire. Un fichier de taille zéro est distingué |
| `%w %W %x %X %y %Y %z %Z` | Naissance, accès, modification et changement. P9 code dans l’ordre naissance, modification, changement, accès, sur quatre fois douze octets. La recette relit indépendamment l’ID, la taille et la date de modification de `alpha` sur l’image. Valeur attendue `2001-01-01 00:00:00.800000001 +0000`. En cas de date inconnue, `%w` donne `-`, `%W` donne 0 et les autres champs donnent `?` |
| `%a %A %f %u %U %g %G %h` | Permissions, mode, identités et nombre de liens de Darwin. Indisponibles dans la réponse P9 |
| `%b %B %o %m` | Allocation, unité, taille E/S et point de montage Darwin. Indisponibles dans la réponse P9. La géométrie du disque virtuel ne donne pas l’allocation de l’objet |
| `%d %D %Hd %Ld %r %R %Hr %Lr %t %T` | Numéros de périphérique et de périphérique spécial Darwin. Indisponibles dans la réponse P9 |
| `%C` | SELinux indisponible sur Darwin, diagnostic, `?` et statut 1. P9 affiche `?` sans contexte SELinux |
| Volume `%a %b %c %d %f %i %l %n %Qn %s %S %t %T` | Les treize conversions sont exercées. Darwin utilise statfs. Sur les volumes essayés, GNU donne `?` pour la limite de nom. Pour les compteurs, le banc interroge également statvfs. Sur Darwin GNU utilise la taille fondamentale du volume pour `%s` et `%S`. Les champs de volume sous P9 restent bloqués ensemble par l’absence de délégation correspondante |
| `%%`, `%` final, champ inconnu, modificateurs, précision, largeur, échappements | Moteur commun. Les bancs couvrent notamment `%.Y`, `%013.6Y`, `%#06a`, `%-10.3n`, octal, hexadécimal, avertissements et préfixe émis avant une directive invalide |

Les largeurs/précisions unitaires sont bornées à 65 536. Le transport de l’ELF accepte jusqu’à 32 arguments et 2 560 octets encodés. La comparaison numérique a été menée en locale C. Le regroupement des chiffres et le séparateur localisé en dehors de C, les caches d’un serveur distant et les erreurs de sortie extrêmes restent hors qualification. Les effets sur les noms hors UTF-8 dépendent du système de fichiers de l’hôte.

La recette ne qualifie pas un lancement générique depuis une installation dynamique de stat. L’entrée métier, les pipes génériques, les attributs de volume et les métadonnées absentes empêchent de déclarer une compatibilité GNU complète sur NovaOS.
