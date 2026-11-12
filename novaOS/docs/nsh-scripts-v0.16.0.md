# NovaOS v0.16.0 — Scripts nsh

Depuis le terminal série `nsh>`, `nsh /verification.nsh` exécute le script installé sur NovaFS P9. Il lit `/systeme` et affiche `NovaOS`. Une commande courte peut aussi s’exécuter avec `nsh -c 'test -f /systeme && echo présent'`. Le terminal, les fichiers et `-c` partagent le découpage lexical, l’analyseur, les expansions et l’exécuteur. [Analyseur](../../components/shell/parser.rs) · [Moteur](../../components/shell/script.rs) · [Script installé](../user/novafs/verification.nsh)

## Grammaire

```ebnf
source      = [ liste ] ;
liste       = chaîne, { séparateur, chaîne }, [ séparateur ] ;
chaîne      = terme, { ("&&" | "||"), terme } ;
terme       = [ "!" ], commande ;
commande    = simple | condition | tant_que | pour ;
simple      = mot, { mot } ;
condition   = "if", bloc, "then", bloc, { "elif", bloc, "then", bloc },
              [ "else", bloc ], "fi" ;
tant_que    = "while", bloc, "do", bloc, "done" ;
pour        = "for", identifiant, "in", { mot }, séparateur, "do", bloc, "done" ;
bloc        = chaîne, séparateur, { chaîne, séparateur } ;
séparateur  = ";" | fin_de_ligne ;
```

Les lignes vides sont admises, y compris après un opérateur logique. Un bloc contient au moins une commande. Les mots clés doivent être littéraux et occuper la position prévue par la grammaire. Un commentaire commence par `#` au début d’un mot et finit à la fin de la ligne. Enregistrer les fichiers en UTF-8 sans marque d’ordre des octets (BOM), avec fins de ligne LF ou CRLF. L’analyseur rejette l’UTF-8 malformé au chargement, les contrôles interdits et les constructions incomplètes avant la première commande du fichier. Le terminal affiche `...>` lorsqu’une structure attend sa suite. Une ligne interactive ayant un guillemet ou un échappement inachevé est refusée.

`!` inverse le succès d’une commande. `&&` et `||` s’évaluent de gauche à droite au même niveau de priorité. Une branche ignorée ne développe aucun motif, ne consulte aucun fichier et ne lance aucun enfant. Le statut d’une liste est celui de sa dernière commande exécutée. Un fichier vide réussit. [Découpage et expansions](../../components/shell/lexer.rs) · [Analyse des listes](../../components/shell/parser.rs)

## Variables et appels

`nom=valeur` constitue une commande autonome. Le nom commence par une lettre ASCII ou `_` et peut ensuite contenir aussi des chiffres. `$nom` et `${nom}` donnent sa valeur, vide pour une variable absente. Les apostrophes protègent tout le mot. Les guillemets doubles autorisent l’expansion des variables et protègent les motifs. Hors apostrophes, une barre oblique inverse protège le caractère suivant. Une valeur développée reste un seul mot, même vide ou contenant des espaces. Une valeur ne devient jamais du code par simple expansion.

`export nom` et `export nom=valeur` transmettent la variable aux appels imbriqués de `nsh`. `unset nom` retire la variable. Les variables locales et les alias de l’appelant restent privés. Le script hérite de son répertoire courant et les modifications du répertoire ou des variables du script prennent fin au retour. `$$` affiche le PID réel du terminal, commun aux appels imbriqués. Les profils de terminal 41 et 42 utilisent la session principale sur volume accessible en écriture. La sauvegarde du répertoire courant épingle un handle privé de cette session pour restituer son identité et ses droits au retour. Les programmes ELF reçoivent leurs arguments et les capacités déléguées selon l’ABI existante. [ABI de lancement](../abi/terminal.rs) · [Exécuteur des commandes isolées](../../components/shell/executor.rs)

```sh
export dossier=/documents
nsh -c 'if test -d "$dossier"; then ls "$dossier"; else return 7; fi'
echo "statut=$?"
```

`nsh chemin` accepte un fichier sans ligne d’interpréteur ou avec une première ligne exacte `#!/bin/nsh`. L’appel direct par chemin, par exemple `./recettes/preuve.nsh`, exige cette ligne. Le terminal ouvre le fichier en lecture seule et vérifie son identité, sa taille et la validité du texte avant de l’exécuter. Une lecture P9 peut avancer la révision de l’objet en actualisant sa date d’accès. Si la révision varie, le chargeur relit le contenu sur le même handle et compare les octets jusqu’à la fin du fichier. Ce contrôle corrige le refus de scripts intacts rencontré sur des volumes avec mise à jour de la date d’accès.

## Commandes et statuts

| Forme | Effet |
| --- | --- |
| `echo mots…`, `true`, `false` | Affichage des arguments séparés par une espace et terminé par un saut de ligne, succès ou échec. |
| `test expression`, `[ expression ]` | Le dernier crochet est un argument séparé. Une chaîne seule est vraie si elle contient au moins un caractère. `-z` et `-n` testent sa longueur. `=`, `==` et `!=` comparent deux chaînes littéralement. |
| `-lt`, `-le`, `-eq`, `-ne`, `-ge`, `-gt` | Comparaisons décimales d’entiers signés sur 64 bits. Un entier invalide produit une erreur. |
| `test -e chemin`, `-f`, `-d` | Existence, fichier ou répertoire. Une absence donne 1. Une erreur de droits ou de résolution conserve son code. |
| `if`, `elif`, `else` | La condition est une liste de commandes. Un statut nul exécute le bloc correspondant. |
| `for`, `while`, `break`, `continue` | `for` développe la liste une fois. `while` réévalue la condition. Les sauts concernent la boucle la plus proche du même script. Un saut réussi rend 0 même après une itération précédente en échec. Une boucle sans itération réussit. |
| `cd`, `alias`, `unalias` | Navigation et alias de mots selon le contrat interactif. Une définition d’alias ne peut pas introduire de structure de contrôle. |
| `exit [n]`, `return [n]` | Dans un fichier ou `-c`, fin de l’appel avec le statut précédent ou le nombre explicite de 0 à 255. Au prompt, `exit` ferme la session et `return` est une erreur. |
| `$?` | Statut précédent. 0 indique le succès, 1 un test faux, 3 une syntaxe ou un argument invalide, 5 un accès refusé, 6 une limite atteinte, 127 une commande inconnue, 128 une faute du processus enfant. Les codes natifs des commandes ordinaires sont conservés. |

```sh
#!/bin/nsh
for fichier in **/*.txt; do
    test -f "$fichier" || continue
    echo "$fichier"
done

attente=oui
while [ "$attente" = oui ]; do
    if test -f /systeme; then
        cat /systeme
        break
    fi
    attente=non
done
```

## Globbing, ressources et isolation

Le moteur hérite de `*`, `?`, des classes, des plages, de leur négation et de `**` comme composant récursif. `**/*.txt` inclut le répertoire courant. Un nom caché exige un point littéral au début du composant. La récursion parcourt uniquement les répertoires visibles et contrôle leurs identités. Les résultats sont triés par octets UTF-8 et un motif sans résultat garde sa forme littérale. Ainsi `echo "$motif"` affiche la valeur, tandis que `for i in $motif` applique les jokers actifs. [Contrat des motifs](cli-v0.15.1.md#expansion-des-noms)

| Ressource | Borne |
| --- | ---: |
| Source par appel / ligne de saisie | 8 192 / 2 048 octets |
| Jetons par source / niveaux de structures composées | 512 / 8 |
| Appels de scripts simultanés sous le terminal | 4 |
| Termes exécutés par saisie complète, appels imbriqués compris | 1 024 |
| Variables par cadre / somme des octets des noms et valeurs | 32 / 4 096 |
| Alias par cadre de script | 64 |
| Arguments après expansion / trame des arguments | 32 / 2 560 octets |
| Globbing par liste de mots développée | 32 niveaux, 128 énumérations, 1 024 entrées examinées |

Le texte de `nsh -c` occupe aussi un argument. La taille de la trame et celle de la ligne interactive peuvent donc limiter cet appel avant le plafond de 8 192 octets applicable à un fichier. Le budget d’exécution inclut les conditions et les corps de boucle. Une limite termine l’exécution en cours et restitue le prompt. Les commandes déjà acquittées conservent leurs effets. Les tests de chemin et le globbing utilisent la racine de session. Chaque commande externe obtient sa propre délégation, limitée aux objets et opérations sélectionnés. Les refus, la révocation, l’interdiction de remplacer une destination et la collecte des enfants restent exercés par les bancs de régression. [Délégations](../../components/shell/capabilities.rs) · [Résolution des chemins](../../components/shell/paths.rs)

### Différences avec Bash

Les arguments positionnels, l’arithmétique, les substitutions de commande, la recherche dans un `PATH`, les pipelines, les redirections, les fonctions, les sous-shells et le contrôle de travaux sortent du langage v0.16.0. PC1 et le remplacement transactionnel de destination pour `cp` et `mv` restent désactivés. Il n’existe pas d’environnement exporté aux ELF. `Ctrl-C` pendant la saisie invalide la ligne selon le contrat v0.15.1. La console native ne fournit pas de signal asynchrone pour interrompre une commande déjà lancée. Les scripts s’exécutent sur demande de l’utilisateur avec l’autorité de sa session.

## Installation dans UTM

Cette section décrit l’image d’origine du moteur de scripts. Pour la distribution incluant l’éditeur, suivre [l’installation avec vi](vi-v0.16.0.md#installation-dans-utm).

Sur Mac Apple Silicon avec UTM 5.0.8 et `qemu-img`, exécuter depuis la racine du dépôt. L’archive AArch64 a pour SHA-256 `a1d462e7913fd6b77c39420dde67cb7d4b6050c50d597637bd81a970bbefe1a9`. [Image installable](../../distribution/images/novaos-0.16.0-utm-aarch64.tar.gz)

```sh
NOVA_TRAVAIL="$(mktemp -d "$HOME/Documents/NovaOS-0.16.0.XXXXXXXX")"
NOVA_DIST="$NOVA_TRAVAIL/novaos-0.16.0-utm-aarch64"
NOVA_VM="$NOVA_TRAVAIL/machine/NovaOS-installation-AArch64.utm"
tar -xzf distribution/images/novaos-0.16.0-utm-aarch64.tar.gz -C "$NOVA_TRAVAIL"
(cd "$NOVA_DIST" && shasum -a 256 -c SHA256SUMS) &&
    python3 novaOS/scripts/paquet_installation.py creer "$NOVA_DIST" "$NOVA_TRAVAIL/machine"
```

1. Ouvrir le paquet dans UTM et démarrer. Attendre la vérification complète du nouveau disque de 64 Mio. Contrôler la série `NOVA-CIBLE-000000001`, le SID 56 et la taille 67 108 864 octets avant de saisir `AUTORISER NOVA-CIBLE-000000001 67108864`.
2. Après `INSTALLATION bilan Ok(())`, arrêter la VM. Retirer son raccourci de la bibliothèque UTM en conservant les fichiers. Exécuter `python3 novaOS/scripts/paquet_installation.py systeme "$NOVA_VM"`, puis rouvrir ce paquet dans UTM.
3. Démarrer depuis le disque seul. Au prompt, exécuter `nsh /verification.nsh`, puis `mkdir recettes` et `nsh -c 'test -f /verification.nsh && cp /verification.nsh recettes/preuve.nsh'`. Exécuter `exit` et arrêter la VM après `SYSTEME bilan Ok(())`.
4. Redémarrer et exécuter `./recettes/preuve.nsh`. La copie persistante doit afficher `Lecture du disque système` puis `NovaOS`.

Le profil utilise le firmware EDK II expérimental propre à la VM, le TPM CRB et GICv3 sans ITS. Le support est en lecture seule. Le domaine UEFI est révoqué avant la prise en charge DMA par NovaOS. Pour reconstruire depuis ces sources, utiliser le `NOVA_DIST` de cette version avec les commandes de construction des ELF `systeme-gpt` et `installation-gpt` de la procédure antérieure. [Construction de l’image](cli-v0.15.1.md#image-et-installation-dans-utm) · [Qualification du stockage](installer-v0.15.0.md)

## Qualification

Le banc dédié a exécuté 31 commandes contrôlées par combinaison architecture/secteur, plus une saisie multiligne, une coupure pendant une question de retrait et un remontage. Chaque profil exécute aussi un ELF au fond de quatre scripts imbriqués contenant chacun huit conditions. Les quatre sessions ont reproduit les onze lignes de référence du scénario commun exécuté par Bash. La VM interrompue après la création de `avant` retrouve ce fichier et le témoin `garde` intact. La commande suivante, qui devait créer `apres`, n’a pas été exécutée. Un lecteur Darwin vérifie le volume avant le remontage de l’invité. Ce scénario exerce un arrêt de VM sur l’hôte alimenté. [Banc de scripts](../tests/nsh_scripts.py)

```sh
make -C novaOS ARCH=aarch64 nsh-scripts-test
make -C novaOS ARCH=x86_64 nsh-scripts-test
```

Les relevés du PID de `nsh` donnent un maximum observé de 108 pages privées avec pile sur AArch64 (432 Kio), et de 110 sur x86_64 (440 Kio), aux deux tailles de secteur. La mesure provient des échantillons du noyau pendant les sessions. Le banc CLI P9 historique relevait 400 et 408 Kio pour v0.15.1. [Mesures de référence](cli-v0.15.1.md#résultats-de-qualification)

L’application UTM a installé l’image en secteurs de 512 octets puis démarré deux fois sur la cible seule. Le second démarrage a exécuté le script copié au premier. Le SHA-256 du support QCOW2, relu après les essais, correspond au fichier de l’archive. Le moteur QEMU 10.0.12 fourni par UTM a également exercé l’installation et la persistance en secteurs de 512 et 4 096 octets. [Banc UTM](../tests/nsh_utm.py) · [Banc du moteur](../tests/installation_gpt.py)

| Suite | AArch64 | x86_64 |
| --- | ---: | ---: |
| Scripts, secteurs de 512 / 4 096 octets | 31 / 31 | 31 / 31 |
| Commandes du banc interactif v0.15.1, mêmes secteurs | 113 / 113 | 113 / 113 |
| Démarrages / groupes bloc / contrôles ELF, processus et IPC / essais CLI historiques | 25 / 24 / 3 / 4 | 25 / 24 / 3 / 4 |
| Groupes fichiers et espaces de noms réunis | 68 | 68 |
| NovaFS sur Darwin, tests standards / instrumentés | 196 / 197 | 196 / 197 |
| Injections sur les ancres / opérations différentielles | 2 304 / 1 000 000 | 2 304 / 1 000 000 |
| DMA/GICv3 / DMA séquentielle, moteur UTM | 11 / 23 | Profil AArch64 |

Les invités de développement tournent sous QEMU 11.1.3. Les exécutables Darwin de NovaFS ont tourné sur macOS 27.2 en ARM64 natif et en x86_64 sous Rosetta. Les relectures croisées P8 et P9 couvrent les deux sens aux deux tailles de secteur. Sur le banc CLI P9, le pic résident échantillonné est de 4 908 Kio sur AArch64 et de 5 084 Kio sur x86_64. Le pic mesuré au remontage est respectivement de 5 028 et 5 204 Kio. La matrice conserve les relevés par géométrie, les empreintes de l’image et les scénarios de refus sans écriture. [Résultats détaillés](validation/nsh-v0.16.0.json)
