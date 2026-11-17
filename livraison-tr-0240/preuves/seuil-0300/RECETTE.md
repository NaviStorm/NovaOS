# TR · Plafond de temps médian · 17 novembre 2026

La livraison publiée est `61c8211ac31ee77638a8e864e793b9729d8472f4`. Les bancs de départ sont à `949282d483eef9b27996d6925717e5db255c4bbe`. Sur Darwin ARM64, le critère s’applique séparément à chaque charge comparable, sur les médianes brutes de trois passages par outil après échauffement. Le statut dépend de Nova ≤ 1,10 × GNU avant arrondi. GNU est `/opt/homebrew/bin/gtr`, Coreutils 9.12.

Le Mach-O publié dépasse le plafond sur quatre charges parmi les 24 mesurées. Le [tableau des 24 charges](ratios-livraison.md) indique les arguments, les tailles d’entrée et de sortie, les médianes, le ratio et le SHA-256 de chaque sortie. Les [mesures historiques](../mesures-mac.json) et le [profil court](../recette-courte-2026-11-17/mesures.json) contiennent les temps individuels, les pics RSS, les empreintes d’entrée, le motif et son nombre de répétitions. Pour reproduire le tableau depuis `livraison-tr-0240`, la commande suivante rend 1 car au moins une ligne dépasse le seuil.

~~~sh
python3 -B outils/evaluer_seuil.py preuves/mesures-mac.json preuves/recette-courte-2026-11-17/mesures.json
~~~

## Correctif candidat

Sur le flux binaire cyclique 00 à ff, la suppression de X retire un octet sur 256. La contraction de a et X conserve tous les octets. Sur le texte historique de 1 million de lignes, la suppression de X retire un octet sur huit. La boucle livrée exécutait aussi les consultations des tables de traduction et de contraction et maintenait l’octet précédent dans le chemin de suppression seule. Ce travail superflu est visible dans le code. Son coût individuel reste à mesurer.

Le candidat compacte directement les octets dans le tampon pour `-d`. Pour la contraction seule, il compare les octets successifs et conserve le précédent entre lectures. Les chemins combinés utilisent la boucle existante. La suite fonctionnelle complète du Mach-O candidat compte 3 684 comparaisons avec GNU, en C et en en_US.UTF-8, dont 30 contrôles supplémentaires des deux chemins directs. Les entrées couvrent notamment les répétitions à cheval sur des blocs de 1, 128 et 129 octets, les octets NUL, le complément et les ensembles vides. [Résultats individuels](fonctionnel-candidat.jsonl.gz) · [Bilan](fonctionnel-candidat.txt)

## Verdict Darwin du 17 novembre

**27 charges sur 27 respectent le plafond de 1,10 dans chacune des deux séries indépendantes.** La série A commence par Nova, la B par GNU. Chaque série recrée les corpus sur APFS. Chaque outil bénéficie d’un échauffement, puis de sept passages mesurés par charge en ordre alterné. Les 756 passages chronométrés incluent le lancement de la commande. La génération, la mesure des SHA-256 et la vérification des flux sont hors chronométrage. Les deux outils lisent le même fichier sur stdin et écrivent sur `/dev/null`. Chaque vérification séparée a observé un statut 0, stderr vide et le même nombre d’octets en sortie avec le même SHA-256. Les 24 paires d’empreintes entrée/sortie historiques concordent également.

Le rapport maximal en série A est 1,06523 pour `-ds X a` sur le binaire de 1 Mio. Celui de la série B est 0,99307 pour les mêmes arguments sur 32 Mio. Sur les deux passages indépendants de 65 millions de lignes de journal, les sorties font exactement 3 380 000 000 octets et portent le SHA-256 `a716c68c0d007cadd829d50c017f0208b345779a514b60f1e477bf8e99dfd9c7`. La dispersion de la série B est visible sur cette charge, avec des durées individuelles GNU de 2,308 à 3,625 s. Son rapport médian reste 0,36510. Les résultats utilisent macOS 27.2.1, un Mac16,5 ARM64 sur secteur, 128 Gio de RAM et la locale C.

[Les 27 comparaisons Nova/GNU, médianes, minima, maxima et SHA-256](creneau-darwin/comparaison.md) · [Environnement](creneau-darwin/environnement.txt) · [A court](creneau-darwin/A-court.json) · [A complet](creneau-darwin/A-complet.json) · [B court](creneau-darwin/B-court.json) · [B complet](creneau-darwin/B-complet.json)

Le candidat fonctionnel et chronométré est celui du commit local `47caae68d9193cae276a066b0b808c1173a59595`. Son Mach-O a le SHA-256 `7e22a2d593abcd0770d987a5e052ec53b5ae7fbdf2a0940cb30bdbbf834d7714` et GNU `075bd855ba66facf7309fcc0eb6f06ca98195e58c3fbe949b3cd65fe1568c77f`. La dernière série Darwin a pris fin à 05 h 40, heure de Paris.

Depuis `livraison-tr-0240`, la première commande relit les résultats sans chronométrage. Pour refaire les mesures lors d’un créneau réservé, lancer le bloc suivant.

~~~sh
python3 -B outils/confirmer_seuil.py preuves/seuil-0300/creneau-darwin
~~~

~~~sh
set -e
export NOVA_TR_BIN="$PWD/preuves/seuil-0300/tr-candidat"
r=$(mktemp -d ./tr-seuil.XXXXXX)
./mesurer-performances-tr.sh --comparer --passages 7 --rapport "$r/A-court.json"
./mesurer-performances-tr.sh --comparer --passages 7 --large --rapport "$r/A-complet.json"
./mesurer-performances-tr.sh --comparer --passages 7 --gnu-dabord --rapport "$r/B-court.json"
./mesurer-performances-tr.sh --comparer --passages 7 --gnu-dabord --large --rapport "$r/B-complet.json"
python3 -B outils/confirmer_seuil.py "$r"
~~~

## NovaOS QEMU

Les quatre profils publiés, AArch64 et x86_64 avec P9/512 et P9/4096, ont chacun passé 3 652 cas dans la sonde SDK. Les 64 mesures disponibles portent sur le moteur Nova en mémoire avec un passage par charge et par profil. Aucune mesure GNU comparable en invité n’existe dans ces rapports. Les 22 régressions du système appartiennent à la livraison publiée. Le candidat devra rejouer les contrôles du moteur sur les quatre profils. [Résultats dans les invités](../couverture-qemu.json) · [Mesures de la sonde](../tableau-qemu.txt)

Le vrai nsh de la livraison rend 3 avant le lancement des deux processus. Le scénario `cat donnees.txt | tr 'a-z' 'A-Z'` est à qualifier avec le lanceur commun. L’attendu est `ABC-123` suivi de LF, stderr vide, deux PID distincts, cat=0 et tr=0, restitution du pipe et droit de lecture du fichier réservé à cat.


### Fenêtre QEMU à réserver

La préparation à sec a sérialisé les 3 684 comparaisons GNU du candidat. Pour chaque architecture et chaque géométrie, le banc démarre le nsh ordinaire sur un volume jetable puis la sonde SDK sur son ISO privée. Le contrôle de la sonde vérifie le nombre de cas du fichier de référence, le succès du moteur et la restitution du processus. Il consigne aussi l’empreinte du fichier. Le chemin ordinaire conserve le témoin de refus. Les régressions système non affectées sont reprises au dossier de `61c8211`. Les quatre relectures du moteur restent en attente d’une fenêtre dédiée.

~~~sh
set -e
export NOVAOS_TMPDIR="$(cd ../.. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
q=$(mktemp -d "$NOVAOS_TMPDIR/tr-3684.XXXXXX")
for isa in aarch64 x86_64; do
  for secteur in 512 4096; do
    python3 -B outils/invite.py "$isa" "$secteur" "$q/$isa-$secteur" --matrice preuves/seuil-0300/fonctionnel-candidat.jsonl.gz
  done
done
~~~
