# Requalification des performances de diff 0.24.0

Référence publiée : c9655770067df3faf012335b90a496797f746a04. Les jeux de données, les binaires et les résultats de la livraison initiale se trouvent dans ../livraison-diff-0240. Le nouveau banc vérifie les empreintes du binaire publié et du GNU 3.12 de CMP, ainsi que la taille des entrées et l’empreinte de la sortie de chaque charge historique.

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
