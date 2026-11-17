# CUT — contrôle du plafond de 1,10 le 17 novembre 2026

La livraison publiée et vérifiée sur origin est `008d62b82d5d7c7ffb601bb15a0bdee09af1b6c1`. Les bancs de départ sont à `fb7d050c6edf578bb2aa7e4eabe4be0b039d4bc0`. Chaque verdict compare les médianes des passages individuels avec la précision du JSON. Un rapport supérieur à 1,10 entraîne un échec pour la charge. Les sorties et leurs SHA figurent dans les preuves de chaque campagne.

| Campagne du Mach-O livré | Mesures sous plafond | Dépassements |
|---|---:|---:|
| Grande campagne sur fichier | 9 | 6 |
| Profil court sur fichier et stdin | 4 | 2 |

Les neuf traitements sur grands fichiers respectent le seuil. Les six dépassements du premier banc correspondent à des lignes de 1, 16 et 64 Mio, avec un TAB suivi de `fin LF`. Les deux dépassements du second banc concernent de nouveau la ligne de 1 Mio. Le maximum observé est 3,4187 sur `-sf2`, 67 108 869 octets. Les charges très courtes incluent le lancement du processus. Les tableaux donnent chaque médiane, la taille exacte et le verdict. [Grande campagne](ratios-grands.txt) · [Profil court](ratios-courts.txt) · [Passages historiques](../mesures.json) · [Passages courts](../recette-2026-11-17/mesures.json)

## Correctif candidat

Le chemin des champs simples recherchait la fin de ligne, puis parcourait chaque octet pour trouver le délimiteur. Avec `-sf1`, le premier champ était aussi recopié intégralement du tampon d’attente vers la sortie lors de la découverte du TAB. Le candidat traite d’un seul tenant le contenu d’un bloc qui ne contient ni délimiteur ni fin de ligne. Il transfère le tampon du champ à une sortie vide quand sa capacité est insuffisante. Le gain réel attend la mesure coordonnée.

Le candidat Darwin 0.24.0 a le SHA-256 `64397fc6446ae26a8c0e7bbb594f114776f80d2d248dbfcdfddb0e761bfb5d9d`. GNU est `/opt/homebrew/bin/gcut` 9.12, SHA-256 `d84adc05e56f93e57fabbfdd555d443d3d5475947d3374ac0a68f461c7ef8335`. Le différentiel complet a réussi 2 013 cas et les 933 cas amont passent. Entrée du témoin TSV de 12 octets : `41 09 42 09 43 0a 44 09 45 09 46 0a`. Pour `-f1,3` sur fichier et sur stdin, stdout vaut `41 09 43 0a 44 09 46 0a`, stderr est vide et le statut vaut 0. Les cas mixtes vérifient aussi la restitution du champ suivi d’un TAB tardif, la ligne longue sans TAB, l’EOF sans LF et le séparateur d’enregistrements NUL. [Binaire testé](cut-darwin-arm64) · [Capture fonctionnelle](options-candidat.jsonl.gz) · [Suite GNU](amont-candidat.txt)

## Charges du créneau Mac demandé

- Rejouer le profil court puis le profil complet avec le **Mach-O candidat**, GNU 9.12 et sortie vers `/dev/null`. Compter un échauffement et trois passages en alternant Nova/GNU. La validation des octets précède les mesures. Le banc relève chaque durée, le débit, le pic RSS, le rapport et le verdict et rend un statut non nul après le dernier cas si un seuil échoue.
- Pour `-f1,3,8,9` la campagne publiée a mesuré 1, 10 et 65 millions de lignes **sur fichier**. Pour ces trois charges, la lecture du même fichier par redirection **sur stdin** manque. Volumes respectifs de 23 175 529, 231 755 463 et 1 506 410 411 octets, graine 27027. Le profil court couvre déjà les deux entrées à 10 000 lignes pour la livraison publiée.
- À chaque grande taille rejouer aussi le complément de champs et la sélection UTF-8 sur fichier pour contrôler les régressions du candidat.
- Aux tailles 1, 16 et 64 Mio, mesurer `-sf1` et `-sf2` avec TAB tardif. Pour la ligne entièrement sans TAB, mesurer `-f1,3` (ligne entière attendue), `-sf1` et `-sf2` (sortie vide attendue). Ces trois dernières charges ont pour source exactement N Mio de `x` puis LF.

## Invités et lancement

Dans la **livraison 008d62b**, sur chacun des profils AArch64/512, AArch64/4096, x86_64/512 et x86_64/4096, la sonde SDK a validé 1 954 cas. Elle a traité 272,002 Mio en mémoire avec 256 Mio de RAM. Pour la projection de ce flux, les médianes respectives sont de 7,7350, 7,8878, 8,2228 et 8,1905 s. Aucun GNU correspondant n’a tourné dans ces invités. Le rapport de 1,10 ne peut pas être calculé pour ces lignes.

Dans les quatre démarrages historiques du vrai nsh, la commande métier sur fichier et son pipe ont rendu 3 avant lancement, l’aide et la version ont rendu 0. Sur le candidat, les deux cibles ELF compilent. La recette moteur invitée du candidat et la qualification métier du lanceur commun restent distinctes. La fourniture de stdout brut de l’ELF installé et l’ouverture autorisée à la demande relèvent de ce dernier chantier. [Traces par profil](../qemu) · [Durées par profil et traitement](../tableau-complet.txt)

## Relecture et commandes du créneau

Depuis `livraison-cut-0240`, les deux premières commandes relisent les chiffres archivés. Chacune doit rendre 1 pour signaler les dépassements. Les commandes fonctionnelles peuvent être exécutées indépendamment des chronométrages.

```sh
./mesurer-performances-cut.sh --relire preuves/mesures.json
./mesurer-performances-cut.sh --relire preuves/recette-2026-11-17/mesures.json
export CUT_NOVA_BIN="$PWD/preuves/plafond/cut-darwin-arm64"
./tester-options-cut.sh --novaOS --large
./tester-amont-cut.sh --novaOS
```

Lors du créneau accordé, lancer les deux profils successivement. Conserver les deux JSON, y compris si le profil court rend 1. Les preuves définitives devront être copiées dans le dossier de livraison et relues.

```sh
export CUT_NOVA_BIN="$PWD/preuves/plafond/cut-darwin-arm64"
d=$(mktemp -d ../../tmp/cut-plafond.XXXXXXXX)
./mesurer-performances-cut.sh --novaOS --macOS --json "$d/court.json"
./mesurer-performances-cut.sh --novaOS --macOS --large --json "$d/complet.json"
```
