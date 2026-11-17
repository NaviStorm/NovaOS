# Plafond de performance CMP du 17 novembre 2026

Le plafond est inclusif, par charge. Le quotient utilise la médiane des temps individuels Nova divisée par la médiane GNU de la même charge. La référence est GNU cmp de Diffutils 3.12, SHA-256 `e986a20d14dd933c057b9b80f7445ac9c23821812ff379258ce9304c021e01d8`. Le Mach-O Nova 0.24.0 livré porte l’empreinte `58199b0810ccfc8385521950aa706983a123e8bb5c158a1394d225631c73b29e`. La tête publiée de `origin/codex/cmp-natif-0240` est `cf259fbd53cdc8f60ae96cdd1d9c255862c2aa81`. Les bancs de départ sont au commit local `f2c25331056bff448cf29b35af2cf13471d39f36`.

## Mac ARM64

Les deux archives vérifiées contiennent 30 et 10 charges. Chaque charge respecte 1,10. Le maximum de la campagne complète vaut 1,091680 environ pour le premier octet différent à 1 Gio. Le maximum de la campagne rapide vaut 1,08892 pour les sauts distincts à 1 Mio. Chaque médiane inclut le démarrage et la collecte de cmp, après un échauffement et sur trois passages alternés. [Détail des 40 comparaisons et empreintes des JSON](preuves/seuil-v0300/rapports-par-charge.md)

Les fichiers gauche et droite sont écrits séparément avec la graine 270240. Pour une taille N, les modifications se situent aux offsets 0, N/2 et N−1 selon le libellé. « EOF » tronque droite d’un octet. « limite » utilise `-n N/4`, « deux sauts » utilise `-i 17:17`, « sauts distincts » utilise `-i 17:23`. « liste clairsemée » utilise `-l` avec les trois positions modifiées. « même objet » présente deux fois gauche. Les deux fichiers de la charge égale sont des objets distincts.

Les sorties terminal de préparation du banc avaient aussi affiché deux dépassements à 1 Mio. La différence au premier octet a donné 1,387 ms pour Nova et 1,211 ms pour GNU, soit environ 1,145×. Les sauts distincts ont donné 1,565 ms et 1,422 ms, soit environ 1,101× dans un passage interrompu plus loin. Les affichages sont arrondis et leurs échantillons individuels ne figurent pas dans les archives conservées. Ces deux signaux ont motivé les séries A et B décrites ci-dessous.

## Fonctionnel et QEMU

- Sur le Mac, la livraison couvre 467 cas avec le saut à plus de 4 Gio. Le banc rapide couvre 482 cas et 12 contrôles de flux. Les résultats attendus sont satisfaits dans les deux campagnes. Sur le témoin de 256 Kio, les deux stdout valent `20202020203120202030203133300a`, les deux stderr sont vides, les processus Nova et GNU rendent respectivement 1 et 0.
- Sur entrées mémoire, le moteur SDK valide 455 vecteurs sur chacune des deux architectures.
- Dans le vrai nsh de recette, les quatre profils P9, AArch64 et x86_64 en secteurs de 512 et 4096, ont chacun une mesure Nova par charge. À 1 Mio, les parcours égaux durent respectivement 7,311 s, 7,293 s, 6,143 s et 6,395 s. Les fichiers de 100 Mio et de 1 Gio ont été exercés sur différence initiale, identité, limite et sauts. Les parcours complets égaux et les différences au dernier octet restent à mesurer à ces deux tailles sur chacun des quatre profils. Une médiane comparative invitée nécessiterait aussi une référence GNU comparable dans le même environnement et des répétitions. [Charges QEMU individuelles](preuves/tableau-invites.txt)
- Le nsh ordinaire de la base de livraison refuse les deux témoins d’admission. L’archive séparée de pause conserve 501 observations par profil de la version intermédiaire à cinq racines IPC. Sur fichiers égaux, le vrai nsh y rend 0 pour `cmp a.bin b.bin` et pour `cat a.bin | cmp -s b.bin -`. Le correctif à quatre racines et la recette du lanceur commun restent en attente. Empreinte relue de l’archive de pause : `59f4d03a52212b9cb4ff14411172d5460544c5cd4dae080ac385ac281300abae`.

## Candidat et contrôles au créneau

Sur Darwin, le programme consultait systématiquement les métadonnées de /dev/null avant la comparaison, y compris lorsque stdout était un fichier ordinaire comme dans le banc. Le candidat évite cette recherche pour les sorties qui ne sont pas des périphériques caractère. Les charges d’arrêt immédiat incluent ce coût fixe. Les séries ci-dessous évaluent le candidat complet. Elles ne quantifient pas isolément le coût de cet appel. Le candidat ARM64 porte le SHA-256 `65074cef88581276d828adf0926a382e839a101a305c059b5a17607cdc1ba135`. Ses 483 cas fonctionnels, les 12 observations de flux et les 9 contrôles par outil sur fichier, pipe et /dev/null passent. [Binaire et contrôles](preuves/seuil-v0300/)

Pour recalculer les rapports déjà archivés, depuis la racine du worktree :

```sh
d=livraison-cmp-0240
python3 -B "$d/verifier-seuil.py" "$d/preuves/performances.json" "$d/preuves/bancs-v0300/performances.json"
```

## Confirmation au créneau Mac du 17 novembre

Le Mach-O publié respecte le plafond sur chacune des dix charges de 1 Mio, dans chacune des deux séries. Le candidat satisfait également les vingt contrôles face au même GNU 3.12. La livraison publiée est retenue pour ce critère. Le candidat reste expérimental. Ce résultat clôt le chronométrage Darwin demandé. Le candidat concerne uniquement l’adaptateur macOS. Aucune régression QEMU lourde n’est nécessaire pour cette décision.

| Charge à 1 Mio | Publié/GNU A | Publié/GNU B | Candidat/GNU A | Candidat/GNU B |
|---|---:|---:|---:|---:|
| Différence au premier octet | 1,045944 | 1,025883 | 1,057127 | 1,070529 |
| Sauts distincts `-i 17:23` | 1,057241 | 1,066405 | 1,052043 | 1,066910 |

Les deux séries ont tourné successivement sur Mac16,5 ARM64, macOS 27.2.1, alimentation secteur. Chaque invocation indépendante du banc a recréé deux fichiers distincts avec la graine 270240. Chaque charge a eu un échauffement par outil, puis onze tours séquentiels des trois exécutables, cache chaud et sorties vers des fichiers ordinaires. À chaque tour la série B inverse l’ordre de la série A. Chacun des trois outils occupe chaque rang trois ou quatre fois par charge. Les 660 temps individuels vont de posix_spawn à wait4. Génération des entrées et vérification des flux sont hors chronométrage. Chaque statut est vérifié à la collecte.

La décision compare les médianes entières en nanosecondes par produit croisé, 10 × Nova ≤ 11 × GNU. Les tableaux indiquent la médiane en ms suivie du minimum et du maximum. Les JSON conservent chaque temps, RSS, statut, ordre de passage, empreinte des exécutables et charge système. [Médianes, étendues et décisions par charge](preuves/creneau-1mio/dispersion.md) · [Série A](preuves/creneau-1mio/A.json) · [Série B](preuves/creneau-1mio/B.json)

Une exécution fonctionnelle distincte a relu stdout en entier et collecté séparément stderr et le statut de chacun des trois processus sur le témoin de 256 Kio. Les trois sorties valent `20202020203120202030203133300a`, les trois stderr sont vides. Le publié et le candidat rendent 1, GNU rend 0. [Relevé et empreintes des entrées](preuves/creneau-1mio/temoin-256k.json)

Pour relire les preuves sans relancer les chronométrages :

```sh
d=livraison-cmp-0240
python3 -B "$d/verifier-seuil.py" "$d/preuves/creneau-1mio/A.json" "$d/preuves/creneau-1mio/B.json"
```

Pour reproduire les séries sur un créneau dédié, depuis la racine du worktree :

```sh
d=livraison-cmp-0240
p=$(mktemp -d ../livraisons/cmp-seuil.XXXXXX)
gzip -dc "$d/preuves/seuil-v0300/cmp-candidat-macos-arm64.gz" > "$p/cmp"
chmod +x "$p/cmp"
export NOVA_CMP="$PWD/$d/bin/cmp" GNU_CMP="$PWD/$d/reference-gnu/bin/cmp"
for serie in A B; do
    "$d/mesurer-performances-cmp.sh" --mio 1 --repetitions 11 --candidat "$p/cmp" --serie "$serie" --json "$p/$serie.json" || break
done
python3 -B "$d/verifier-seuil.py" "$p/A.json" "$p/B.json"
```
