# Requalification des performances de diff 0.24.0

Référence publiée : c9655770067df3faf012335b90a496797f746a04. Les jeux de données, les binaires et les résultats de la livraison initiale se trouvent dans ../livraison-diff-0240. Le nouveau banc vérifie les empreintes du binaire publié et du GNU 3.12 de CMP, ainsi que la taille des entrées et l’empreinte de la sortie de chaque charge historique.

## Décision Darwin ARM64

Le candidat `999f6e6cd41dab2b8d11812e00bfd3f557563fba` satisfait le plafond de 1,10 pour chacune des dix comparaisons. La référence « avant » est l’exécutable publié, remesuré sur le même corpus à côté du candidat et du GNU de CMP. Chaque valeur est la médiane de 18 lancements. Les valeurs affichées sont arrondies après la décision.

| Charge | Avant S1 ms | Après S1 ms | GNU S1 ms | Ratio S1 | Avant S2 ms | Après S2 ms | GNU S2 ms | Ratio S2 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| égaux | 3.2426 | 3.1832 | 3.1671 | 1.005085 | 3.1783 | 3.1641 | 3.0331 | 1.043205 |
| début | 4.9078 | 3.2326 | 3.7991 | 0.850888 | 4.9253 | 3.3056 | 3.8293 | 0.863226 |
| fin | 4.9389 | 3.9526 | 4.1245 | 0.958328 | 4.8725 | 3.9571 | 3.9271 | 1.007628 |
| dispersés | 15.2270 | 5.6290 | 7.9201 | 0.710720 | 15.2708 | 5.5973 | 7.8584 | 0.712265 |
| arborescence | 5.1074 | 4.8140 | 4.6583 | 1.033435 | 5.1989 | 4.8464 | 4.7078 | 1.029446 |

Le pire ratio est 1,0334350448911194 en série 1 (arborescence) et 1,04320451688681 en série 2 (fichiers égaux). Apple M4 Max, macOS 27.2.1, locale C, fuseau UTC0. Aucun compilateur ni QEMU à au moins 5 % CPU aux points de contrôle des deux séries. La charge générale et les dispersions individuelles figurent avec les temps bruts. [Série 1](mesures/999f6e6/serie-1/mesures.json) · [Série 2](mesures/999f6e6/serie-2/mesures.json)

Les [trois exécutables qualifiés](bin) ont été reconstruits avec des empreintes identiques. Le Mach-O mesuré a le SHA-256 `ffd2a39e421d922d3a6c5a686b654a41fb6fcb70aa28b42bd9e705c80ab55c96`. Le parseur ELF du noyau accepte chacun des deux fichiers de distribution sur son ISA. Le registre ABI v1 conserve ses 48 sélecteurs. [Reconstruction](reconstruction.json) · [Contrôle ABI et ELF](controle-abi.txt)

Pour le cas dispersé en série 1, le CPU utilisateur médian passe de 11.93 à 2.86 ms (GNU 4.86 ms) et le pic RSS médian de 19.99 à 9.08 Mio (GNU 14.27 Mio). Les entrées et les 50 488 octets émis sont les mêmes pour les trois outils.

## Diagnostic initial

Médianes tirées des cinq observations brutes par charge sur Darwin ARM64. Les temps CPU système du cas dispersé sont proches ; la différence de CPU utilisateur est de 7,04 ms.

| Charge | CPU utilisateur publié / GNU (ms) | CPU système publié / GNU (ms) | Pic RSS publié / GNU (Mio) | Sortie (octets) |
|---|---:|---:|---:|---:|
| Égaux | 0,798 / 0,772 | 1,238 / 1,240 | 6,77 / 6,61 | 0 |
| Début | 2,291 / 1,314 | 1,328 / 1,210 | 8,91 / 6,61 | 90 |
| Fin | 2,293 / 1,426 | 1,299 / 1,201 | 8,91 / 6,61 | 98 |
| Dispersés | 11,842 / 4,800 | 1,778 / 1,633 | 20,00 / 14,27 | 50 488 |
| Arborescence | 1,355 / 1,025 | 2,590 / 2,538 | 2,58 / 2,19 | 368 |

Le chemin égal lit les deux fichiers et vérifie leurs octets. Une différence déclenche en plus la construction des deux tables de lignes. Aux extrémités, le binaire publié construit 131 072 entrées de ligne pour ne conserver qu’une ligne par fichier dans la comparaison. Sur la charge dispersée, 517 substitutions séparées traversent presque toute la fenêtre. Le classement des lignes dans un arbre ordonné, la construction des tables d’équivalence et la sélection des ancres s’appliquent alors à presque toute l’entrée. GNU 3.12 délimite d’abord les extrémités identiques au niveau des octets puis construit les classes des lignes de la fenêtre dans une table de hachage (src/io.c, find_identical_ends et find_and_hash_each_line). La chaîne de lecture Darwin et les écritures de résultats restent celles du binaire initial.

Le candidat délimite en amont les lignes que le format normal doit examiner et conserve le décalage pour l’affichage des numéros. Pour des substitutions isolées, il vérifie en octets que les lignes remplacées ne peuvent correspondre à aucune ligne de l’autre côté. L’absence de telles correspondances impose l’alignement des lignes communes. En cas d’ambiguïté, de changements voisins ou de densité élevée, le moteur général prend le relais. Les dates de fichier sont préparées uniquement pour les formats qui les affichent.

Les 77 témoins incluent des positions autour des frontières de blocs, le dernier LF manquant, l’ajout, le retrait, les substitutions, des lignes déplacées ou répétées, les options et un échec d’ouverture. Le rapport initial compare le candidat de développement au binaire publié et à GNU. [Témoins fonctionnels](temoins-initial.json)

## Protocole de décision

Lancer pendant le créneau attribué, une fois pour chacune des séries 1 et 2, avec un répertoire de sortie neuf. Chaque série recrée les fichiers, lit et valide les sorties, et effectue un échauffement exclu des mesures. Pour chaque charge, trois blocs randomisés comprennent chacun les six permutations des trois binaires. Chacun occupe six fois chaque position sur 18 passages. Les deux séries ont des graines distinctes. La mesure couvre la création et l’attente du processus. Les empreintes et les contrôles de statut sont calculés après l’arrêt du chronomètre.

Le banc enregistre la charge hôte avant et après chaque charge, les concurrents de compilation et QEMU, l’ordre, les temps individuels, les pics RSS, les quartiles et l’écart absolu médian. Le verrou commun cut-performances.lock sérialise les bancs participants. Le seuil est évalué sur chaque charge de chaque série par multiplication croisée des médianes en nanosecondes.

~~~sh
python3 -B qualification-diff-perf-0240/comparer.py --serie 1 \
  --gnu "$NOVAOS_TMPDIR/cmp-0240/gnu/bin/diff" \
  --candidat "$candidat" --sortie "$resultats/serie-1"
python3 -B qualification-diff-perf-0240/comparer.py --serie 2 \
  --gnu "$NOVAOS_TMPDIR/cmp-0240/gnu/bin/diff" \
  --candidat "$candidat" --sortie "$resultats/serie-2"
~~~

## Régressions du candidat livré

Le binaire Darwin passe les 775 cas historiques et les 15 nouveaux essais sur fenêtres, avec comparaison du statut, de stdout et de stderr. Les cinq cas de pipes et les 77 témoins ciblés passent également. La suite amont retrouve les résultats du binaire publié, dont l’échec attendu de large-subopt, le test d’identité GNU incompatible avec la version Nova et les deux sauts propres au Mac. Les trois sorties détaillées de large-subopt coïncident avec GNU. [Cas Darwin](preuves-darwin/complet/options.json) · [Tests des flux](preuves-darwin/flux-darwin.json) · [Témoins ciblés](temoins-binaires-livres.json) · [Suite GNU](preuves-darwin/amont)

Les ELF de recette ont été construits par extraction du commit moteur qualifié. Chacun des quatre profils (AArch64 et x86_64, secteurs P9 de 512 et 4 096 octets) a réussi les 790 cas sur ses vecteurs chargés depuis le disque. Les cinq charges sur fichiers P9 réels comparent entre 30 240 et 30 272 octets. Chaque charge a passé quatre contrôles de statut et des deux flux, dont trois passages mesurés. Les médianes invitées vont de 124,77 à 181,42 ms, avec 64 Kio de pic de tas. Ces durées englobent le lancement et l’UART dans une VM QEMU TCG à deux vCPU et 256 Mio. [Matrice des quatre profils](preuves-invites/MATRICE.md) · [Tableau invité](preuves-invites/QEMU.md) · [Journaux et empreintes](preuves-invites/qemu)

Une première charge P9 de 19 substitutions produisait 1 962 octets et la trace n’en transmettait que 960. Le noyau limite chaque processus à 16 messages de diagnostic, dont un utilisé par la sonde de tas. Le banc a conservé les 19 substitutions dans les essais comparés en mémoire et exercé huit substitutions sur les fichiers P9 réels pour contrôler intégralement leurs 826 octets de sortie. La qualification du binaire Darwin n’utilise pas ce canal.

La commande de production attend toujours le raccord du lanceur à la session authentifiée de nsh. Sur les quatre invités ordinaires, l’appel métier a rendu 127 et la syntaxe du pipe a été refusée avant lancement. Les neuf consultations documentaires par profil ont réussi avec empreinte de disque et compteurs d’écriture et de flush identiques. Le contrôle des ELF de distribution porte sur leur admission par le parseur du noyau ; les exécutions invitées de comparaison portent sur les ELF de recette.

## Reproduire et contrôler les preuves

Depuis la racine, utiliser le guide de la [livraison initiale](../livraison-diff-0240/README.md) pour les outils et l’oracle GNU. Les binaires nouveaux sont dans qualification-diff-perf-0240/bin. Le banc conserve la livraison historique comme référence. Pour un nouveau rejeu complet, donner des dossiers neufs aux preuves et aux machines. Le banc refuse de remplacer des preuves déjà présentes. Les résultats versionnés restent disponibles dans l’archive et dans Git.

~~~sh
q=qualification-diff-perf-0240
"$q/reconstruire-candidat.sh" "$NOVAOS_TMPDIR/diff-reconstruction-neuve"
python3 -B "$q/tester-fenetres.py" "$q/bin/diff" "$NOVAOS_TMPDIR/cmp-0240/gnu/bin/diff" "$NOVAOS_TMPDIR/diff-temoins-neufs.json"
resultats=$(mktemp -d "$NOVAOS_TMPDIR/diff-preuves-XXXXXX")
python3 -B "$q/requalifier.py" --resultats "$resultats" darwin
"$q/rejouer-qemu.sh" "$resultats/machines" "$resultats"
python3 -B novaOS/scripts/generate_abi.py --check
(cd "$q" && shasum -a 256 -c SHA256SUMS)
~~~

Le manifeste interne de l’archive locale couvre également les ISO réellement démarrées, les noyaux, les quatre ELF de recette embarqués dans les profils et les disques synthétiques avant et après chaque démarrage. Le script archiver-preuves.py accepte le dossier du rejeu et un nouveau chemin d’archive, décompresse l’ensemble, vérifie tous les fichiers et écrit un reçu SHA-256 adjacent.
