# Plafond de performance CMP du 17 novembre 2026

Le plafond est inclusif, par charge. Le quotient utilise la médiane des temps individuels Nova divisée par la médiane GNU de la même charge. La référence est GNU cmp de Diffutils 3.12, SHA-256 `e986a20d14dd933c057b9b80f7445ac9c23821812ff379258ce9304c021e01d8`. Le Mach-O Nova 0.24.0 livré porte l’empreinte `58199b0810ccfc8385521950aa706983a123e8bb5c158a1394d225631c73b29e`. La tête publiée de `origin/codex/cmp-natif-0240` est `cf259fbd53cdc8f60ae96cdd1d9c255862c2aa81`. Les bancs de départ sont au commit local `f2c25331056bff448cf29b35af2cf13471d39f36`.

## Mac ARM64

Les deux archives vérifiées contiennent 30 et 10 charges. Chaque charge respecte 1,10. Le maximum de la campagne complète vaut 1,091680 environ pour le premier octet différent à 1 Gio. Le maximum de la campagne rapide vaut 1,08892 pour les sauts distincts à 1 Mio. Chaque médiane inclut le démarrage et la collecte de cmp, après un échauffement et sur trois passages alternés. [Détail des 40 comparaisons et empreintes des JSON](preuves/seuil-v0300/rapports-par-charge.md)

Les fichiers gauche et droite sont écrits séparément avec la graine 270240. Pour une taille N, les modifications se situent aux offsets 0, N/2 et N−1 selon le libellé. « EOF » tronque droite d’un octet. « limite » utilise `-n N/4`, « deux sauts » utilise `-i 17:17`, « sauts distincts » utilise `-i 17:23`. « liste clairsemée » utilise `-l` avec les trois positions modifiées. « même objet » présente deux fois gauche. Les deux fichiers de la charge égale sont des objets distincts.

Les sorties terminal de préparation du banc avaient aussi affiché deux dépassements à 1 Mio. La différence au premier octet a donné 1,387 ms pour Nova et 1,211 ms pour GNU, soit environ 1,145×. Les sauts distincts ont donné 1,565 ms et 1,422 ms, soit environ 1,101× dans un passage interrompu plus loin. Les affichages sont arrondis et leurs échantillons individuels ne figurent pas dans les archives conservées. Ces signaux motivent la vérification au créneau réservé. Aucun gain du candidat n’a été chronométré.

## Fonctionnel et QEMU

- Sur le Mac, la livraison couvre 467 cas avec le saut à plus de 4 Gio. Le banc rapide couvre 482 cas et 12 contrôles de flux. Les résultats attendus sont satisfaits dans les deux campagnes. Sur le témoin de 256 Kio, les deux stdout valent `20202020203120202030203133300a`, les deux stderr sont vides, les processus Nova et GNU rendent respectivement 1 et 0.
- Sur entrées mémoire, le moteur SDK valide 455 vecteurs sur chacune des deux architectures.
- Dans le vrai nsh de recette, les quatre profils P9, AArch64 et x86_64 en secteurs de 512 et 4096, ont chacun une mesure Nova par charge. À 1 Mio, les parcours égaux durent respectivement 7,311 s, 7,293 s, 6,143 s et 6,395 s. Les fichiers de 100 Mio et de 1 Gio ont été exercés sur différence initiale, identité, limite et sauts. Les parcours complets égaux et les différences au dernier octet restent à mesurer à ces deux tailles sur chacun des quatre profils. Une médiane comparative invitée nécessiterait aussi une référence GNU comparable dans le même environnement et des répétitions. [Charges QEMU individuelles](preuves/tableau-invites.txt)
- Le nsh ordinaire de la base de livraison refuse les deux témoins d’admission. L’archive séparée de pause conserve 501 observations par profil de la version intermédiaire à cinq racines IPC. Sur fichiers égaux, le vrai nsh y rend 0 pour `cmp a.bin b.bin` et pour `cat a.bin | cmp -s b.bin -`. Le correctif à quatre racines et la recette du lanceur commun restent en attente. Empreinte relue de l’archive de pause : `59f4d03a52212b9cb4ff14411172d5460544c5cd4dae080ac385ac281300abae`.

## Candidat et contrôles au créneau

Sur Darwin, le programme consultait systématiquement les métadonnées de /dev/null avant la comparaison, y compris lorsque stdout était un fichier ordinaire comme dans le banc. Le candidat évite cette recherche pour les sorties qui ne sont pas des périphériques caractère. Les charges d’arrêt immédiat incluent ce coût fixe. Sa contribution au dépassement demande une mesure. Le candidat ARM64 porte le SHA-256 `65074cef88581276d828adf0926a382e839a101a305c059b5a17607cdc1ba135`. Ses 483 cas fonctionnels, les 12 observations de flux et les 9 contrôles par outil sur fichier, pipe et /dev/null passent. [Binaire et contrôles](preuves/seuil-v0300/)

Pour recalculer les rapports déjà archivés, depuis la racine du worktree :

```sh
d=livraison-cmp-0240
python3 -B "$d/verifier-seuil.py" "$d/preuves/performances.json" "$d/preuves/bancs-v0300/performances.json"
```

Au créneau Mac, commencer par le Mach-O livré sur les dix charges de 1 Mio, avec onze passages par outil et les temps bruts dans un répertoire neuf. Si le plafond est dépassé, mesurer le candidat face au même GNU, sur la même graine et avec le même protocole. Avant de retenir un candidat modifié, vérifier aussi ses dix charges à 100 Mio et à 1 Gio. Le vérificateur retourne 1 dès que le bilan comporte un dépassement.

```sh
d=livraison-cmp-0240
p=$(mktemp -d ../livraisons/cmp-seuil.XXXXXX)
NOVA_CMP="$PWD/$d/bin/cmp" "$d/mesurer-performances-cmp.sh" --mio 1 --repetitions 11 --json "$p/livre.json"
python3 -B "$d/verifier-seuil.py" "$p/livre.json"
# Si nécessaire, sur le même créneau :
gzip -dc "$d/preuves/seuil-v0300/cmp-candidat-macos-arm64.gz" > "$p/cmp"
chmod +x "$p/cmp"
NOVA_CMP="$p/cmp" "$d/mesurer-performances-cmp.sh" --large --repetitions 11 --json "$p/candidat.json"
python3 -B "$d/verifier-seuil.py" "$p/candidat.json"
```
