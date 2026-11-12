# Audit fonctionnel de nsh pour la préparation de NovaOS v0.16.3

**12 novembre 2026.** Système testé sur la branche `main` au commit `910deaa731d8042d93ae3b863ab046a9cf9e1ee7`, propre au départ. Le système et les commandes annoncent NovaOS v0.16.2. Les volumes de travail utilisent NovaFS v0.9.13 P9.

## Résultats

| Environnement | Secteur | Essais exécutés | PASS | FAIL | ABSENT |
|---|---:|---:|---:|---:|---:|
| QEMU 11.1.3, invité AArch64 | 512 | 316 | 297 | 5 | 14 |
| QEMU 11.1.3, invité AArch64 | 4 096 | 316 | 297 | 5 | 14 |
| QEMU 11.1.3, invité x86_64 | 512 | 316 | 297 | 5 | 14 |
| QEMU 11.1.3, invité x86_64 | 4 096 | 316 | 297 | 5 | 14 |
| Application UTM 5.0.8, AArch64, port série PTY | 512 et 4 096 | 45 | 17 | 0 | 28 |
| Fenêtre du terminal UTM, AArch64 | 512 | 24 | 13 | 2 | 9 |
| Export à froid de la VM du test en fenêtre | 512 | 1 | 1 | 0 | 0 |

Les 316 essais du premier profil sont répétés sur chacun des trois autres. Les cinq échecs correspondent à **deux défauts**. Les quatorze cas d’absence correspondent aux neuf gestes de complétion et à cinq commandes. Les régressions historiques et les cas non testés sont comptabilisés séparément. Les essais effectués sur les volumes jetables n’ont révélé ni corruption, ni remplacement de destination, ni élévation de droits.

Le **tableau exhaustif** donne la commande ou les touches, la précondition, l’attendu, l’observation, le statut, l’architecture, le secteur, la gravité et le chemin de la preuve pour chaque essai. Il contient 1 334 observations exécutées et 13 entrées « NON TESTÉ ». Les identifiants natifs se lisent avec l’architecture et le secteur. Les lignes conservent les échappements, les commandes citées et les diagnostics. Les sorties longues portent leur taille et leur SHA-256, le journal série contient les octets d’origine. [Matrice TSV](nsh-audit-v0.16.3/matrice.tsv) · [Journaux, contrôles des volumes et banc de reproduction](nsh-audit-v0.16.3/preuves.tar.gz) · [Empreintes des 59 fichiers des fixtures](nsh-audit-v0.16.3/empreintes.tsv)

## Anomalies, fonctions absentes et priorités proposées

| Priorité | Cas | Reproduction et diagnostic | Critère proposé pour v0.16.3 |
|---|---|---|---|
| P2 | T10, GUI-13 | Taper `cat xyz`, Tab, trois retours arrière, `alpha.txt`, Entrée. La ligne affiche `cat alpha.txt`, puis `saisie refusée : argument invalide` (code 3 sur les quatre profils). La même commande ressaisie lit `alpha A`. Le décodeur classe HT (0x09) comme touche invalide. L’éditeur conserve l’erreur même après correction du texte. Entrée renvoie cette erreur avant l’analyse syntaxique. | En premier, rendre la ligne éditable après une touche sans candidat. Tester la ligne corrigée, le texte réellement soumis et l’absence d’exécution avant Entrée. Conserver le refus des séquences malformées. |
| P3 | T01–T09 | Aucune complétion de commande, de répertoire ou de fichier. Aucun candidat au deuxième Tab. Sur un préfixe sans correspondance, la ligne reste affichée mais Entrée la refuse. Reproduit sur les deux géométries dans UTM et au clavier dans sa fenêtre en 512. | Si la complétion est retenue, réutiliser les noms de commandes connus et la lecture de répertoire déléguée. Préserver précisément la citation, le curseur et les limites du parcours. |
| P3 | V16–V19, GUI-23 | Un fichier contenant les cinq octets `AB\nCD` laisse `CDnsh> ` à l’écran avec `cat`, `head`, `tail` et `more`. Dans UTM, la dernière ligne de la page donne `P30nsh> `. Les octets lus sont exacts. L’éditeur de ligne écrit directement l’invite, quelle que soit la colonne laissée par la commande précédente. | Donner à l’invite une ligne distincte lorsque nécessaire. Vérifier les octets produits par les utilitaires et le positionnement du terminal séparément. |
| P3 | Y01, Y02 | `ps` et `top` sont tous deux **absents**. Chacun rend `commande inconnue`, code 127 sur les quatre profils. Aucun PID, état, pourcentage CPU ou compteur mémoire n’est présenté par ces commandes. | Décider des données que chaque client peut observer avant de définir une commande d’inspection. |
| P3 | Y03–Y05 | `clear`, `history` et `help` sont **absents**, code 127. La flèche haut rappelle une commande et l’aide propre à chaque utilitaire testé fonctionne. | Arbitrer séparément l’effacement de l’écran, l’affichage de l’historique et l’aide générale. |

La priorité P2 concerne le rejet d’une commande valide. La priorité P3 de la complétion concerne la fonctionnalité demandée. Cette distinction permet de tester la correction de la saisie indépendamment d’une décision sur les suggestions. Le défaut d’invite concerne l’ergonomie du terminal. Les fichiers sans LF constituent des entrées valides. [Décodage des touches](../../../components/shell/input.rs) · [Édition de ligne](../../../components/shell/line_editor.rs) · [Boucle du terminal](../../../components/shell/main.rs) · [Commandes isolées reconnues](../../../components/libraries/command.rs) · [Commandes intégrées](../../../components/shell/builtins.rs)

## Banc, données et critères de comparaison

L’hôte Apple Silicon exécutait macOS 27.2 (26B123). Les quatre invités de développement ont démarré avec QEMU 11.1.3, TCG, un disque VirtIO brut et le profil CLI ordinaire. Les commandes sont envoyées à la véritable invite. La sonde historique est réservée aux suites en profil test. Le port virtuel de l’application UTM et sa fenêtre macOS constituent deux essais distincts. UTM utilise son propre QEMU 10.0.12, le disque système QCOW2 installé, le firmware privé du projet, TPM CRB et GICv3 sans ITS. Le profil 4 096 utilise une GPT préparée pour cette géométrie avec tailles logique et physique du périphérique fixées à 4 096. [Lancement des invités](../../scripts/bloc_qemu.py) · [Profil d’installation](../installer-v0.15.0.md)

Le banc a créé deux volumes de 2 Mio. Il a peuplé les images hors ligne, avec le moteur NovaFS, avant tout démarrage. Il a vérifié les contenus attendus, remonté en lecture seule et appelé la vérification du moteur. Chaque scénario de modification démarre une **VM neuve sur sa propre copie**, avec sa précondition reconstituée. L’inventaire complet avant et après contient chemin, type, identifiant, taille, révision, quatre dates et empreinte du contenu. Une copie doit créer une nouvelle identité, un renommage conserver celle de chaque objet déplacé. Une lecture autorisée peut faire évoluer `atime`. Pour les refus et les commandes informatives, le contrôle porte sur l’arbre, l’identité, les octets, la naissance, `mtime` et `ctime`. Les différences de métadonnées sont archivées. Le test porte sur les effets observables dans P9, il ne mesure pas le nombre d’écritures physiques du périphérique.

`/test-nsh` contient les 14 fichiers et les six répertoires demandés, racine comprise. `.cache` contient `caché\n`. Le nom `notes 2026.txt` est un seul composant. `image.png` contient les huit octets de la signature PNG, suffisants pour vérifier une copie binaire. `/aux` ajoute entre autres un fichier vide, un fichier sans LF final, des caractères multioctets, un nom commençant par un tiret, un nom de 150 octets, 34 candidats au même niveau et 27 répertoires imbriqués. Le fichier volumineux fait 69 632 octets et 4 096 lignes. Les lecteurs hôtes calculent le SHA-256 après lecture de tous les octets. Les deux géométries retrouvent les mêmes empreintes de contenu. [Manifeste des fichiers](nsh-audit-v0.16.3/empreintes.tsv)

| Domaine | Vérifications et résultats par profil QEMU |
|---|---|
| Navigation (N, K) | Succès de `cd` confirmé par une seconde commande `pwd`. Répertoire courant conservé après nom absent (20), fichier ordinaire (29) et tentative de sortie de racine (5). Les chemins avec espace, Unicode et tiret littéral fonctionnent. Le séparateur répété est explicitement refusé (3). |
| Listes (L, O, Oa) | `ls -a` énumère `.` et `.cache`. L’entrée `..` n’est pas énumérée par ce profil. `ls -A` omet le point. Tailles et colonnes du format long, couleur par type, tri par taille, tri temporel et tri inversé contrôlés sur les vraies entrées. Les 61 formes GNU documentées comme non prises en charge rendent un diagnostic et le code 3. Les 16 contrôles complémentaires exercent les équivalences, l’ordre des options, l’aide et la version. |
| Globbing (G, K) | `**/*.txt` renvoie huit noms triés, dont ceux du répertoire courant. `[0-9]`, `[!5]`, `?`, combinaisons récursives, littéraux protégés et accès explicite au fichier caché vérifiés. Une absence de correspondance conserve l’opérande littéral, puis `ls` rend 20. Les 34 candidats dépassent le budget d’arguments (6). Recherche au fond des 27 répertoires réussie. Motif de 33 composants refusé (6). |
| Fichiers (F01–F21) | 21 × 4 remontages indépendants conformes. Couverture de `mkdir`, `rmdir`, `touch`, copies ordinaires, récursives et multisources, déplacements, retraits et refus de remplacement. F20 et F21 passent **deux opérandes existants** à `rm --help` et `rm --version`. Les deux fichiers restent identiques. |
| Lecture (CAT, HEAD, TAIL, MORE, WC, V) | Les cinq commandes traitent le vide, une ligne, plusieurs lignes, l’absence de LF final, UTF-8, plusieurs fichiers et la grande fixture. Absence (20), accès refusé (5), option invalide (3) et absence d’opérande contrôlés séparément. Les retours de ligne et les octets du fichier sans LF sont conservés. Le comportement de l’invite figure en V16–V19. |
| Scripts (S, E, K) | Fichier simple, boucle avec motif, `-c`, variable locale, export entre appels de nsh, retrait, statut, branches, boucles, interruption de boucle, court-circuit, citations, enfant avec retour 17, fichier vide, malformé et inaccessible. La faute de syntaxe est reconnue avant la première commande témoin. Le dépassement numérique est refusé (3). La boucle infinie et l’expansion excessive atteignent la limite (6). |
| Système (Y, T) | `echo`, définition et suppression d’un alias, rappel au clavier, `exit` dans un appel enfant et fermeture du terminal fonctionnent. Les cinq commandes absentes figurent dans les priorités. |

Les oracles de lecture ont exécuté GNU Coreutils 9.12 sur les mêmes octets, avec locale UTF-8. Pour `head` et `tail`, la comparaison tient compte des noms absolus des opérandes dans les en-têtes. Pour `wc`, elle porte sur les nombres, noms et totaux après normalisation de l’espacement. Le témoin de contenu de `more` est GNU `cat`, les gestes de pagination ont leur propre essai. Les options de `ls` ont été confrontées au manuel Info de Coreutils 9.12 et au contrat du projet. L’oracle temporel utilise les dates P9 réelles. GNU Bash 3.2.57 a également exécuté le programme commun de la suite historique, avec onze lignes de sortie identiques (variables, citations, boucles et statuts). Ces comparaisons portent uniquement sur les constructions et options testées. [Manuel GNU de ls](https://www.gnu.org/software/coreutils/manual/html_node/ls-invocation.html) · [Contrat CLI](../cli-v0.14.0.md) · [Options et limites de nsh](../cli-v0.15.1.md) · [Contrat des scripts](../nsh-scripts-v0.16.0.md)

## Interactions observées dans UTM

Le système installé a démarré depuis son disque. Dans la fenêtre du terminal UTM, la saisie d’un chemin cité a lu le fichier avec espace. Les neuf préfixes avec Tab ont tous rendu l’invite après un refus à Entrée. Le rappel par flèche haut a réexécuté `echo rappel-UTM`. `nsh scripts/boucles.nsh` a affiché les trois noms attendus. Un premier passage plein écran a laissé une saisie refusée et le répertoire courant à `/`. Le banc a resaisi `cd /test-nsh` et vérifié `pwd` avant la série de mesures. [Capture du clavier](nsh-audit-v0.16.3/utm-clavier.jpg)

`vi /page-audit` a enregistré trente lignes, `P01` à `P30`, sans LF après `P30`. À la demande de suppression interactive de `alpha.txt`, la réponse `n` a conservé `alpha A`. Après un second démarrage, Entrée dans `more` a affiché `P25` et `q` a rendu la main avant la fin du fichier. Un second appel suivi d’Espace a affiché jusqu’à `P30`. Les commandes marqueurs `FIN-Q` et `FIN-ESPACE` ont été exécutées aux deux retours dans le shell. La capture de pagination montre l’invite accolée au dernier octet. [Refus de suppression](nsh-audit-v0.16.3/utm-refus.jpg) · [Pagination après redémarrage](nsh-audit-v0.16.3/utm-pagination.jpg)

La relecture hôte de l’export UTM éteint a retrouvé les **20 objets de `/test-nsh`**, avec les mêmes chemins, identités, types, tailles, contenus, dates de naissance, de modification et de changement de métadonnées. Le nouveau fichier `/page-audit` fait 119 octets, SHA-256 `10300d0bb29e88b4a9b45fb509bd42e944456bf61b9e88864584c1b1f836fffa`. Les deux sessions supplémentaires par port série ont utilisé l’application UTM avec des disques de 512 et 4 096 octets par secteur. La fenêtre a été observée en 512 uniquement. [Matrice GUI et PTY](nsh-audit-v0.16.3/matrice.tsv) · [Export contrôlé et journaux UTM](nsh-audit-v0.16.3/preuves.tar.gz)

## Sécurité et limites d’observation

| Cas | Préconditions et commande | Résultat observé | Couverture et preuve |
|---|---|---|---|
| Racine et données inaccessibles | Depuis `/test-nsh`, `cd ../../..`, `cat ./../../test-nsh/alpha.txt`, `ls /../../*.txt` et lecture par chaque utilitaire hors racine | Code 5. `pwd` conserve le répertoire initial. | PASS, quatre profils, N10/N10b, G16, K04 et cas 09 des lecteurs. |
| Droits minimaux | Profil de sondes, `@probe droits`, `nsh -c '@probe droits'`, `@probe arbre`, `@probe vide`, `@probe ancien-retrait` | Les opérations contrôlées sur des objets ou droits non délégués restent refusées. | PASS, quatre profils, journaux historiques `session`, `scripts` et `nsh`. |
| Révocation | Profil de sondes, `nsh -c '@probe revocation'`, `@probe arbre-revoque`, `@probe vide-revoque` | La sonde termine en constatant le refus ou la révocation. Le banc accepte aussi une révocation déjà effective à l’ouverture du client. | PASS, quatre profils, journaux `scripts` et `nsh`. |
| Retrait récursif interrompu | `rm -riv coupure` sur fixture dédiée. Coupure après le retrait confirmé d’alpha, avant autorisation pour beta. | Après contrôle hôte et nouveau démarrage, alpha est absent, beta et le témoin hors de l’arbre gardent leur contenu. | PASS, quatre profils, suite `nsh-test`, journaux `nsh` et `nsh-remontage`. |
| Deux clients de répertoire | Profil `namespace-test`, client avec mutations et seconde session RO dans `/public`, renommage, départ du pair et remontage | Identité retrouvée à 1 074 octets de chemin après redémarrage. Refus des opérations non autorisées et survie de l’autre client. | PASS, deux architectures × deux secteurs, `novaos-ARCH.log`. |
| Limites de ressources | Nombre d’arguments, croissance d’une variable, 33 composants, composant de 256 octets, entier hors plage et boucle sans fin | Refus explicite. Les lectures autorisées et une nouvelle ligne de commande restent possibles. Le test de namespace exerce 1 023, 1 024, 1 025, 4 096 et 8 192 octets. | PASS, quatre profils, G14, K01–K03, E01–E02, `namespace-test`. |
| Tab et absence d’effet | `touch temoin-tab<Tab>`, attente d’une seconde, Entrée, puis `test -e temoin-tab` | Écho de la seule saisie pendant l’attente, refus à Entrée, fichier absent (1). La relecture du volume conserve les identités et les empreintes. Aucun candidat ni chemin divulgué dans ces essais. | PASS pour l’absence d’exécution sur les quatre profils, K05. La complétion elle-même est ABSENT. |
| Protection des données | F08, F09, F13, F18–F21 sur copies fraîches avec destinations ou opérandes existants | Refus de remplacement et commandes informatives sans disparition ni changement de contenu. | PASS, quatre profils, différences d’inventaire dans `disques/`. |
| Ordonnancement précis | Écrivain intercalé entre deux réponses de lecture du répertoire, pénurie physique exactement pendant la collecte d’un glob, révocation exactement après le premier fragment de `cat` | La production ne fournit pas de point de rendez-vous au banc pour ces intercalages. Les essais de quotas, de deux clients et de révocation ci-dessus ont été exécutés. | NON TESTÉ, NT01–NT03 pour chacun des quatre profils. |

Le terminal de production et les sondes utilisent les mêmes contrats de capacités. Les identifiants ci-dessus précisent le profil qui a exercé chaque opération. [Espace de noms](../namespace-v0.12.0.md) · [ABI native](../abi.md) · [Contrats IPC](../ipc-security.md) · [Sonde de droits et de révocation](../../../components/coreutils/probe.rs)

## Régressions rejouées

| Banc | AArch64 | x86_64 | Résultat et preuve |
|---|---|---|---|
| Démarrage, bloc et processus NovaOS | 25 démarrages, 24 groupes bloc, 3 contrôles ELF/cycle de vie/IPC | Mêmes effectifs | PASS. Les 24 groupes bloc comprennent 12 groupes par secteur. Journaux `regressions/novaos-ARCH.log`. |
| CLI et CLI P9 | 4 essais (2 suites × 2 géométries) | 4 essais | PASS. Commandes isolées, contenu et dates, refus, remontage et lecture indépendante Darwin. |
| nsh historique | 113 commandes par secteur et interruption | Mêmes effectifs | PASS. Les deux retraits délégués, les refus et l’état après coupure figurent dans la suite. |
| Scripts | 31 commandes par secteur et reprise | Mêmes effectifs | PASS. Oracle Bash de 11 lignes. Les journaux incluent les refus de syntaxe et le script après redémarrage. |
| Espace de noms | 2 clients, grandes longueurs et 10 scénarios de coupure/erreur par secteur | Même couverture | PASS. Le lecteur hôte contrôle les données et les identités. |
| vi et terminal | 14 ELF collectés et 13 fichiers relus par secteur, plus test d’écran | Même couverture | PASS. Test d’écran par architecture en 512 octets avec trois dimensions du terminal, 20 frappes, 20 lignes et retour à nsh. |
| DMA/GICv3, moteur QEMU d’UTM 10.0.12 | 11 scénarios GICv3 et 23 scénarios DMA séquentielle | Spécifiques à AArch64 | PASS. Firmware et injection appariés pour chacun des deux bancs, y compris quarantaine et reprise. Résultats dans `regressions/dma-gicv3.json` et `regressions/dma-sequentiel.json`. |
| NovaFS Darwin | ARM64 natif | x86_64 sous Rosetta, indicateur de traduction 1 | Par architecture, 196 standards et 197 instrumentés réussis, 6 marqués ignorés dans chacune de ces deux passes. Les suites ciblées sont comptées séparément. |
| NovaFS différentiel et ancres | 1 000 000 d’opérations, 2 304 injections d’ancres | Mêmes effectifs | Par architecture, 100 graines, 345 183 commits observés et 1 000 remontages/audits dans le différentiel. Les injections couvrent les 2 secteurs, 2 opérations, 2 slots, 32 plages et 9 phases. Les 360 refus de montage font partie des résultats attendus. |

Les campagnes NovaOS, NovaFS, GIC et DMA ont produit des journaux indépendants. Les journaux NovaFS distinguent les essais standards, instrumentés, optionnels, de migration, permutations et saturation. [Makefile NovaOS](../../Makefile) · [Banc nsh](../../tests/nsh.py) · [Banc scripts](../../tests/nsh_scripts.py) · [Banc namespace](../../tests/namespace.py) · [Suites NovaFS](../../../novaFS/lab/p51-validation-suites.py) · [Preuves de cette exécution](nsh-audit-v0.16.3/preuves.tar.gz)

## Reproduire l’audit

Depuis la racine de la révision auditée, sur le Mac de développement équipé des cibles Rust du projet, de QEMU et de GNU Coreutils, ces commandes créent deux nouvelles fixtures. Utiliser un répertoire de banc neuf à chaque campagne. La reconstruction depuis l’archive a reproduit les 59 empreintes sur chacun des deux secteurs logiques. Les programmes sous `banc/` opèrent sur les copies jetables créées sous `novaOS/build/`.

```sh
audit_dir=$(mktemp -d "$PWD/novaOS/build/rejeu-nsh-XXXXXX")
tar -xzf novaOS/docs/validation/nsh-audit-v0.16.3/preuves.tar.gz -C "$audit_dir" --strip-components 1 banc
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --lib --bin novafs
lib="$PWD/novaFS/target/aarch64-apple-darwin/release"
rustup run stable rustc --edition 2024 "$audit_dir/fixture.rs" \
  -L "dependency=$lib/deps" --extern "novafs=$lib/libnovafs.rlib" -O -o "$audit_dir/fixture"
python3 -B "$audit_dir/preparer.py"
for arch in aarch64 x86_64; do
  make -C novaOS ARCH="$arch" kernel
  for secteur in 512 4096; do
    for banc in campagne options interaction mutations; do
      python3 -B "$audit_dir/$banc.py" "$arch" "$secteur" || exit 1
    done
  done
done
```

Pour reproduire T10 au clavier dans une copie jetable du système UTM, placer la fixture dans `/test-nsh`, saisir `cd /test-nsh`, puis `pwd`. Taper `cat xyz`, presser **Tab**, effacer les **trois** lettres, saisir `alpha.txt` et valider. Ressaisir ensuite `cat alpha.txt` depuis l’invite neuve. Pour reproduire le second défaut, créer un fichier texte sans LF final et lancer `cat` ou atteindre la dernière page avec `more`. Les captures documentent les deux comportements. [Procédure du profil UTM livré](../vi-v0.16.2.md) · [Matrice de diagnostic](nsh-audit-v0.16.3/matrice.tsv)
