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

Les quatre profils publiés, AArch64 et x86_64 avec P9/512 et P9/4096, ont chacun passé 3 652 cas dans la sonde SDK. Les 64 mesures disponibles portent sur le moteur Nova en mémoire avec un passage par charge et par profil. Aucune mesure GNU comparable en invité n’existe dans ces rapports. Les 22 régressions du système appartiennent à la livraison publiée. Le rejeu du candidat figure ci-dessous. [Résultats dans les invités](../couverture-qemu.json) · [Mesures de la sonde](../tableau-qemu.txt)

Le vrai nsh de la livraison rend 3 avant le lancement des deux processus. Le scénario `cat donnees.txt | tr 'a-z' 'A-Z'` est à qualifier avec le lanceur commun. L’attendu est `ABC-123` suivi de LF, stderr vide, deux PID distincts, cat=0 et tr=0, restitution du pipe et droit de lecture du fichier réservé à cat.


### Qualification du candidat · 17 novembre 2026

**3 684 comparaisons réussies dans chacun des quatre invités, soit 14 736 exécutions et huit démarrages QEMU.** Le code candidat est `47caae68d9193cae276a066b0b808c1173a59595`. La base du banc est `1f6bd01196942030414d32344d2201bef6b6255c`. Les empreintes des sources de traduction, de contraction et des adaptateurs sont comparées au Git du candidat. Le moteur et son adaptateur de lecture figurent dans les dépendances de construction du véritable ELF et de l’ELF de sonde. La sonde confronte les sorties et diagnostics bruts aux octets GNU archivés, ainsi que le statut ou l’action documentaire. Elle exerce également trame entièrement supprimée, EOF, révocation de lecture et fermeture.

| Invité QEMU TCG · 2 vCPU · 256 Mio | Secteurs P9 | Cas du moteur | Niveau du témoin | Statut et restitution | Contrôles du vrai nsh |
|---|---:|---:|---|---|---:|
| aarch64 | 512 | 3 684 | EL0 | 0 · OK | 27 |
| aarch64 | 4096 | 3 684 | EL0 | 0 · OK | 27 |
| x86_64 | 512 | 3 684 | Ring3 | 0 · OK | 27 |
| x86_64 | 4096 | 3 684 | Ring3 | 0 · OK | 27 |

La matrice compte 1 842 cas en C et autant en en_US.UTF-8. Les 27 contrôles du shell par profil comprennent 21 demandes de documentation ou d’option inconnue, 4 refus de pipeline et 2 témoins positifs. Le vrai nsh lit le fichier `donnees.txt` avec cat et retourne 0. Il refuse `cat donnees.txt | tr 'a-z' 'A-Z'` avec le statut 3 avant de créer les deux enfants. Les contrôles documentaires conservent le SHA du volume et les compteurs d’écriture et de flush. Le pipeline de production reste à qualifier avec le lanceur commun.

| Architecture | SHA-256 ELF de production reconstruit | SHA-256 ELF de la sonde exécutée |
|---|---|---|
| aarch64 | `78bbd8c60fa3402793f69bd50d1318bb1b84f28ab5ee6e5bef353a0c24211286` | `4f5817dbaebe2ed281f350280692a196d97498e770cc9ff554ff6de580893666` |
| x86_64 | `b25c675e02b4bf9d3c6dddfa5b107f0c5ad5d0d1b8a1df081af498ef10687cb9` | `0f95bc18c3d1b22bdbfd4f4cb322b386e0e7e4ed79987d23463eb089a63166ed` |

Les deux ELF de production sont des exécutables statiques ELF64 dont le point d’entrée appartient à un segment exécutable. Leurs octets diffèrent de ceux publiés à `61c8211`. Le noyau de chaque ISO a été extrait et identifié. La sonde est présente dans l’ISO SDK et sort avec le PID 2. Les deux images de chaque profil excluent l’ELF de production du candidat. Le PID observé est celui du témoin SDK. Les 22 régressions système et les anciennes campagnes QEMU restent rattachées à leur livraison d’origine. Aucun nouveau chronométrage Darwin ou QEMU durant cette qualification fonctionnelle.

[Résultats, SHA des huit images et des noyaux extraits](creneau-qemu/resultats.json) · [Sessions AArch64](creneau-qemu/aarch64.tar.xz) · [Sessions x86_64](creneau-qemu/x86_64.tar.xz) · [ELF de production AArch64](creneau-qemu/elf-production/tr-aarch64.elf) · [ELF de production x86_64](creneau-qemu/elf-production/tr-x86_64.elf)

Pour reproduire depuis `livraison-tr-0240`, créer un sous-répertoire propre dans la racine temporaire OS. Chaque architecture dispose de son cache et chaque géométrie de son volume P9, de son ISO ordinaire et de son ISO SDK. Les deux archives conservées incluent chacune 33 fichiers et un manifeste SHA-256 interne. Elles contiennent les volumes, les configurations, les traces UART, les sorties de construction et les ELF de sonde.

~~~sh
set -e
q=$(mktemp -d "$(cd ../.. && pwd)/tmp/tr-qualification-3684.XXXXXX")
export NOVAOS_TMPDIR="$q" TMPDIR="$q/" NOVA_TR_BIN_DIR="$q/elf-production"
for isa in aarch64 x86_64; do
  ./reconstruire.sh "$isa"
  for secteur in 512 4096; do
    python3 -B outils/invite.py "$isa" "$secteur" "$q/$isa-$secteur" --matrice preuves/seuil-0300/fonctionnel-candidat.jsonl.gz
  done
done
~~~

La livraison qualifiée est disponible localement sur `codex/tr-bancs-0300`. La branche distante historique `origin/codex/tr-natif-0240` pointe sur `61c8211`. La publication de la branche de recette couvrirait les bancs et le correctif du moteur, le Mach-O candidat et les deux ELF reconstruits, les preuves Darwin et QEMU, le rapport et son manifeste SHA-256.
