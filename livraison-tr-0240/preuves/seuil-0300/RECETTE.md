# TR · Plafond de temps médian · 17 novembre 2026

La livraison publiée est `61c8211ac31ee77638a8e864e793b9729d8472f4`. Les bancs de départ sont à `949282d483eef9b27996d6925717e5db255c4bbe`. Sur Darwin ARM64, le critère s’applique séparément à chaque charge comparable, sur les médianes brutes de trois passages par outil après échauffement. Le statut dépend de Nova ≤ 1,10 × GNU avant arrondi. GNU est `/opt/homebrew/bin/gtr`, Coreutils 9.12.

Le Mach-O publié dépasse le plafond sur quatre charges parmi les 24 mesurées. Le [tableau des 24 charges](ratios-livraison.md) indique les arguments, les tailles d’entrée et de sortie, les médianes, le ratio et le SHA-256 de chaque sortie. Les [mesures historiques](../mesures-mac.json) et le [profil court](../recette-courte-2026-11-17/mesures.json) contiennent les temps individuels, les pics RSS, les empreintes d’entrée, le motif et son nombre de répétitions. Pour reproduire le tableau depuis `livraison-tr-0240`, la commande suivante rend 1 car au moins une ligne dépasse le seuil.

~~~sh
python3 -B outils/evaluer_seuil.py preuves/mesures-mac.json preuves/recette-courte-2026-11-17/mesures.json
~~~

## Correctif candidat

Sur le flux binaire cyclique 00 à ff, la suppression de X retire un octet sur 256. La contraction de a et X conserve tous les octets. Sur le texte historique de 1 million de lignes, la suppression de X retire un octet sur huit. La boucle livrée exécutait aussi les consultations des tables de traduction et de contraction et maintenait l’octet précédent dans le chemin de suppression seule. Ce travail superflu est visible dans le code. Son coût individuel reste à mesurer.

Le candidat compacte directement les octets dans le tampon pour `-d`. Pour la contraction seule, il compare les octets successifs et conserve le précédent entre lectures. Les chemins combinés utilisent la boucle existante. La suite fonctionnelle complète du Mach-O candidat compte 3 684 comparaisons avec GNU, en C et en en_US.UTF-8, dont 30 contrôles supplémentaires des deux chemins directs. Les entrées couvrent notamment les répétitions à cheval sur des blocs de 1, 128 et 129 octets, les octets NUL, le complément et les ensembles vides. [Résultats individuels](fonctionnel-candidat.jsonl.gz) · [Bilan](fonctionnel-candidat.txt)

## Créneau Mac coordonné

Depuis `livraison-tr-0240`, utiliser le Mach-O candidat dont le SHA-256 est `7e22a2d593abcd0770d987a5e052ec53b5ae7fbdf2a0940cb30bdbbf834d7714`. Pour vérifier le comportement puis, pendant le créneau, les huit charges courtes et les dix-neuf charges complètes, exécuter les commandes ci-dessous. Chaque exécution emploie un nouveau dossier. Le comparateur rend 1 si une charge dépasse 1,10.

~~~sh
export NOVA_TR_BIN="$PWD/preuves/seuil-0300/tr-candidat"
r=$(mktemp -d ./tr-seuil.XXXXXX)
./tester-options-tr.sh --large --rapport "$r/fonctionnel.jsonl.gz"
./mesurer-performances-tr.sh --comparer --rapport "$r/court.json" &&
./mesurer-performances-tr.sh --comparer --large --rapport "$r/complet.json" &&
python3 -B outils/evaluer_seuil.py "$r/court.json" "$r/complet.json"
~~~

Les seize mesures historiques du profil complet portent sur les textes de 1, 10 et 65 millions de lignes et sur 33 554 432 octets binaires. Les trois mesures supplémentaires portent sur la traduction ASCII de journaux de 52 octets par ligne. Leurs entrées font respectivement 52 000 000, 520 000 000 et 3 380 000 000 octets. Les durées, ratios et empreintes des sorties issues de ces trois passages restent à relever. Le créneau doit également établir le respect du plafond par le nouveau binaire sur les 24 charges déjà mesurées avec l’ancien.

## NovaOS QEMU

Les quatre profils publiés, AArch64 et x86_64 avec P9/512 et P9/4096, ont chacun passé 3 652 cas dans la sonde SDK. Les 64 mesures disponibles portent sur le moteur Nova en mémoire avec un passage par charge et par profil. Aucune mesure GNU comparable en invité n’existe dans ces rapports. Les 22 régressions du système appartiennent à la livraison publiée. Le candidat devra rejouer les contrôles du moteur sur les quatre profils. [Résultats dans les invités](../couverture-qemu.json) · [Mesures de la sonde](../tableau-qemu.txt)

Le vrai nsh de la livraison rend 3 avant le lancement des deux processus. Le scénario `cat donnees.txt | tr 'a-z' 'A-Z'` est à qualifier avec le lanceur commun. L’attendu est `ABC-123` suivi de LF, stderr vide, deux PID distincts, cat=0 et tr=0, restitution du pipe et droit de lecture du fichier réservé à cat.
