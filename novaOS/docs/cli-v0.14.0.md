# Audit de la CLI v0.14.0 — tri par date de modification

**Décision du 11 novembre 2026.** Le tri `ls -t` doit utiliser la date réelle de dernière modification du fichier. Le volume que monte NovaOS ne conserve pas cette information. La livraison se limite donc à cet audit sur la base `bcea8e1d63e1c5c4a1c6051dea63c96afb579ef9`. La version publiée reste NovaOS v0.13.0, avec NovaFS v0.9.12, le format P8 et les 19 appels de l’ABI native. NovaOS et les composants CLI sont sous licence MIT, NovaFS sous MPL-2.0.

## Du disque à `ls`

| Couche examinée | Données réellement disponibles | Conséquence |
|---|---|---|
| Volume logique autonome monté par NovaOS | Le moteur P8 utilise les enregistrements d’objets du format de contenu P0. La clé est l’identifiant persistant. La valeur contient type, révision, taille, indicateur de retrait, parent, longueur du nom et nom UTF-8. | Aucun champ ne conserve l’instant de modification. [Contrat du montage natif](validation/novafs-natif-v0.11.0.md) · [Encodage et décodage](../../novaFS/src/fs/record.rs) |
| Moteur NovaFS | `Stat` fournit `id`, `kind`, `size` et `revision`. Une écriture prend la génération de la transaction. Un renommage peut aussi changer la révision de l’objet et de ses répertoires parents. | Un fichier renommé peut passer devant un fichier dont le contenu a été modifié plus récemment. Plusieurs objets peuvent recevoir la même génération. [Attribution des générations et renommage](../../novaFS/src/fs.rs) · [Écriture des données](../../novaFS/src/fs/data.rs) |
| Service utilisateur NovaFS | `STAT` et `LOOKUP_AT` partagent un résultat de 25 octets. Il encode trois `u64` little-endian (identifiant, révision, taille), puis le type sur un octet. | Le client ne peut obtenir de date par ces opérations. [Réponse du service](../user/novafs/files/namespace.rs) |
| Commande et terminal | `ls -l` énumère le nom et le type et ajoute l’identifiant et la taille. Les flux texte filtrent les caractères de contrôle, dont ESC. Le shell v0.13.0 connaît les internes `cd` et `exit`. | La couleur demanderait une opération de rendu sûre. Élargir le parseur ou ajouter un alias ne fournirait aucune date. [Commande](../../components/coreutils/ls.rs) · [Terminal](../abi/terminal.rs) · [Exécuteur](../../components/shell/executor.rs) |

Le vérificateur macOS consulte aussi `mtime` et `ctime` du **fichier image hôte**. Ces valeurs servent à détecter un changement du conteneur pendant la vérification. Tous les objets du volume occupent la même image. Sa date, celle du montage et l’horloge au moment de l’énumération ne permettent pas de retrouver les dates individuelles après redémarrage. [Vérificateur hôte](../../novaFS/src/bin/novafs/check.rs)

## Observations reproduites

- **Référence GNU.** Sur macOS, GNU coreutils 9.12 a listé deux fichiers `alpha` et `beta` de contenu identique (`NovaOS\n`). Avec des dates de modification fixées au 1ᵉʳ janvier 2000 et au 1ᵉʳ janvier 2001, `gls -latr --color -- alpha beta` a rendu `alpha`, puis `beta`. Inverser uniquement les dates a inversé l’ordre. Les deux sorties contenaient les codes de couleur demandés. Dans cette combinaison, `-t` choisit la date et `-r` place le plus ancien en tête.
- **Shell NovaOS de production.** Dans quatre VM (x86_64 et AArch64, secteurs de 512 et 4 096 octets), `ls -l documents` a renvoyé `exemple.txt` de taille 7. `ls -t documents` et `ls -latr --color` ont renvoyé « argument invalide » et le statut 3. Chacune des lignes `alias l='ls -latr --color'`, `l` et `unalias l` a donné « commande inconnue ». Après ces refus, `pwd` a rendu `/`. L’empreinte SHA-256 de chaque image avant démarrage et après arrêt forcé était identique.
- **Contenu persistant.** Pour chaque couple architecture Darwin / géométrie, deux exécutions du scénario F01 ont effectué les mêmes créations, écritures et renommage avec les mêmes identifiants FS/LV et un intervalle d’au moins 1,2 seconde. Les huit images de 1 572 864 octets ont donné la même empreinte `b90960a793d8d8243bb455903c6f96d5b4ddcf13cb800c398b004729574ec5b0`. Ce contrôle sur un volume autonome complète l’examen du codec. [Scénario F01](../../novaFS/tests/interop.rs)

## Matrice de compatibilité au point d’arrêt

| Commande | Disponible en v0.13.0 | Options ou comportements reportés et prérequis |
|---|---|---|
| `ls` | `ls [-l] [répertoire]`. Le détail fournit les champs natifs. | `-t` exige une date de modification persistante. `-a -A -h -R -r -S -1 -d -F` et les groupes comme `-latr` restent hors de la grammaire actuelle. Les quatre formes GNU de couleur sont reportées. `--color` et `--color=always` forcent la couleur, `--color=auto` dépend du terminal de sortie et `--color=never` la supprime. |
| `pwd` | Répertoire courant obtenu du service. | `-L`, `-P` reportés. |
| `cat` | Un fichier lu par fragments. | `-n -b -s -E -T -A` et plusieurs fichiers reportés. |
| `head` | Un fichier, 10 lignes par défaut ou `-n N`. | Formes accolées et longues, `-c -q -v` et plusieurs fichiers reportés. |
| `mkdir`, `rm` | Création d’un répertoire ou suppression d’un fichier, sur une entrée déléguée. | `mkdir -p/-v/-m` et `rm -i/-I/-f/-r/-R/-d/-v` reportés. Le mode persistant et la délégation récursive demandent leurs propres garanties. |
| `cp`, `mv`, `touch` | Absents du catalogue des exécutables. | Copie, renommage et création par ces commandes reportés. La mise à jour d’une date par `touch` rencontrerait le même manque de métadonnées. |
| `more`, `wc`, `tail` | Absents du catalogue. | Pagination interactive, comptage et lecture de fin de fichier reportés. |
| Shell et options communes | Saisie UTF-8, insertion, retour arrière, mouvements gauche/droite, guillemets, `cd` et `exit`. | Historique par session, édition étendue, `alias`, `unalias`, analyseur GNU, groupes courts, `--help`, `--version` et `--` reportés. La grammaire v0.13.0 traite certains arguments commençant par un tiret comme des chemins. |

Le catalogue autorisé et la sélection des droits par opérande précèdent le lancement de chaque ELF. Une éventuelle expansion d’alias devrait passer par cette même sélection en conservant les arguments supplémentaires. Aucun résultat d’exécution, de performance ou de consommation mémoire des fonctions reportées n’est disponible. [Grammaire et catalogue v0.13.0](../../components/libraries/command.rs) · [Validation du shell publié](cli-v0.13.0.md)

## Validation de la base publiée

| NovaOS v0.13.0 sous QEMU | x86_64 | AArch64 |
|---|---:|---:|
| Scénarios historiques de démarrage, mémoire, ordonnanceur, ABI et IPC | 25/25 | 25/25 |
| Groupes bloc, dont DMA, erreurs d’E/S et FLUSH | 24/24 | 24/24 |
| Contrôles fichiers, dont coupures transactionnelles et remontages | 44/44 | 44/44 |
| Contrôles d’espace de noms (12 bilans par géométrie pour 30 contrôles) | 30/30 | 30/30 |
| ELF multipage, courses IPC, cycle de vie des familles | 3/3 | 3/3 |
| Sessions CLI v0.13.0 sur 512 et 4 096 octets | 2/2 | 2/2 |
| Reproduction du refus de tri et d’alias sur ces deux géométries | 2/2 | 2/2 |

Pour chaque couple architecture / secteur, la suite CLI a démarré quatre VM, vérifié la révocation et la faute d’un enfant, puis retrouvé G43 et G44 lors des remontages. Après la fermeture de la session instrumentée, les compteurs de processus, pages utilisateur et pages IPC sont revenus à zéro. Ces essais couvrent le redémarrage de QEMU sur image virtuelle. La durabilité après coupure électrique de l’hôte reste hors de la mesure.

Sur macOS 27.2, les deux suites autonomes de NovaFS ont été rejouées avec les exécutables Mach-O correspondant à chaque architecture.

| NovaFS v0.9.12 | ARM64 natif | x86_64 sous Rosetta |
|---|---:|---:|
| Tests standards réussis / ignorés par défaut | 179 / 4 | 179 / 4 |
| Tests instrumentés réussis / ignorés par défaut | 180 / 4 | 180 / 4 |
| Modèle différentiel (100 graines) | 1 000 000 opérations en 25,566 s | 1 000 000 opérations en 40,981 s |
| Commits observés / pénuries injectées / remontages audités | 345 183 / 54 643 / 1 000 | 345 183 / 54 643 / 1 000 |

Le banc a également exécuté deux tests optionnels par architecture. La campagne P5 a contrôlé 342 coupures, 76 erreurs d’E/S et 6 écritures déchirées **par géométrie et par architecture**. Le banc P8 a relu les images historiques du commit `a33cc66f54ef779b9f67e5a940cd9437ef3f59d4` avec le lecteur de l’autre architecture, sur les deux géométries (12 rapports par combinaison). Le scrub sans clé rend explicitement un résultat incomplet pour les vues chiffrées.

## Reproduction

Depuis la racine du monorepo sur le Mac de validation, les commandes suivantes utilisent les bancs existants et leurs images de test.

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test lifecycle-test elf-test ipc-races block-test
    make -C novaOS ARCH="$arch" file-test namespace-test cli-test
    python3 novaFS/lab/p51-validation-suites.py "$arch"
done
python3 novaFS/lab/p8-validation.py cli --sortie "${TMPDIR:-/tmp}/nova-p8-audit.json"
```

Pour vérifier la reproductibilité temporelle de l’image F01, relancer le banc après une pause. Chaque passage compare toutes les images à l’empreinte publiée en P0 et remonte les volumes sur l’autre architecture.

```sh
python3 novaFS/lab/test-interop.py
sleep 2
python3 novaFS/lab/test-interop.py
```

Pour reproduire les refus à l’invite sur une image neuve, reprendre ces deux commandes avec chaque architecture et chaque taille de secteur. `cli-image` crée le fichier à un emplacement libre.

```sh
arch=x86_64
secteur=512
image="$(mktemp -d)/audit.raw"
make -C novaOS cli-image SECTOR="$secteur" IMAGE="$image"
make -C novaOS cli ARCH="$arch" SECTOR="$secteur" IMAGE="$image"
```

```text
nova> ls -l documents
nova> ls -t documents
nova> ls -latr --color
nova> alias l='ls -latr --color'
nova> l
nova> unalias l
nova> pwd
```

Pour comparer le comportement de GNU `ls` sur des dates connues, utiliser le binaire `gls` de GNU coreutils sur le Mac. Les fichiers de l’essai résident dans un répertoire temporaire.

```sh
python3 - <<'PY'
import os, subprocess, tempfile
from pathlib import Path
with tempfile.TemporaryDirectory() as tmp:
    paths = [Path(tmp) / name for name in ("alpha", "beta")]
    for path in paths:
        path.write_bytes(b"NovaOS\n")
    for older, newer in (paths, paths[::-1]):
        for path, stamp in ((older, 946684800), (newer, 978307200)):
            os.utime(path, (915148800, stamp))
        subprocess.run(["gls", "-latr", "--color", "--", "alpha", "beta"],
                       cwd=tmp, env=dict(os.environ, LC_ALL="C", TZ="UTC", LS_COLORS="fi=32"),
                       check=True)
PY
```

## Condition de reprise

Une décision ultérieure devra définir comment obtenir une heure civile fiable, la conserver pour chaque objet concerné au sein de sa transaction CoW et la restituer par l’interface de métadonnées. Elle devra préciser la sémantique des écritures, des renommages, des dates égales et des images historiques dont la date est inconnue. La qualification devra vérifier le tri après remontage et après interruption aux barrières F1/F2. Les dates historiques absentes sont irrécupérables à partir des seuls octets de ces volumes.
