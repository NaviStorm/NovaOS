# NovaOS v0.23.x — Second pipe et stabilisation des lectures

## Références de qualification du 16 novembre 2026

| Objet | SHA Git |
|---|---|
| Premier parent, `origin/main` avant intégration | `4d29fc250d673cbe283261fc0f84adcdbaeb4758` |
| Second parent, candidat `cat fichier \| head -n 1` | `37f91b7593fb110c6b326f0ee500cd9d73909330` |
| Commit de fusion publié sur `origin/main` | `816843fa6f5a8ae35f64ef0c3f96a67652bbc384` |
| Arbre commun au candidat et à la fusion | `691ee243436d0cb51c1f2c0ab1373ab63af15eee` |
| Correctif et bancs sur `codex/stabilisation-023x` | `15408d88c9bad4a112d245f378f277bc7e87e7ac` |

Les deux références ont été lues sur le distant avant la fusion `--no-ff`. Les deux parents dans cet ordre et l’arbre ont été vérifiés sur le commit récupéré après le push. Les 170 membres de preuve décompressés correspondent aux originaux, au manifeste et à l’archive versionnée de SHA-256 `fd5fe14a4c17bd00fd47562a099a499e7dabfc95446b3513a8c1f369e46ea07a`. [Qualification du second pipe](pipe-head-v0.23.x.md)

## Origine des cinq observations

| Scénario sur le code fusionné | Mécanisme observé sur la série instrumentée x86_64 |
|---|---|
| `ls -a /` | Entre deux pages de READDIR, la génération P9 passe de 66 à 67. La racine reste à la révision 66, le journal publie 67. Selon l’ordonnancement, le refus peut aussi se produire au second listage du banc. |
| `ls -latr --color /` | Après le nom `-nom`, la publication du journal fait passer le volume de 68 à 69. La racine reste à 68. Le refus intervient avant le tri et la coloration. |
| Glob récursif depuis `/` | Pendant l’expansion de `**/*.txt`, le curseur du répertoire 1 porte 99 et le journal publie 100. Le répertoire reste à 99. L’échec précède la boucle du script. |
| `ls src/` sur P9/4096 | Pendant la pagination de `/g/src`, le journal publie 160. Le répertoire 3 reste à 159 et la dernière publication du temporaire observée est 157. Quatre captures isolent cette même interposition. |
| `vi tronque` | La fenêtre du fichier 11 contient une copie de ses octets. Après les 88 premiers octets, READ/CONTINUE compare la génération du volume 115 à la valeur initiale 114. Le fichier reste à la révision 114. Le refus empêche la validation UTF-8 des 4 095 octets ASCII suivis des deux premiers octets de 猫. |

Le code public 10 provient dans ces captures du contrôle de génération du service. La sonde de mise au point relève les générations de la couche privée et la révision de l’objet au point du refus. Les noyaux soumis à la qualification finale utilisent le code sans sonde. La vérification couvre séparément une véritable révocation.

**Énumération.** READDIR compare le jeton à la révision du répertoire associé au handle autorisé. Il renvoie la révision après l’éventuelle publication de la date d’accès. Création, retrait et renommage actualisent déjà la révision du parent dans le moteur NovaFS. Le filtre des objets privés, le contrôle du droit d’énumération et le refus d’une continuation obsolète restent exercés. Le comportement demeure conservateur si une autre opération change la révision du répertoire lui-même.

**Lecture groupée.** Le service conserve la révision du fichier après sa lecture dans la fenêtre privée du client. Tant que la génération du montage est identique, la copie se poursuit. Si elle change, le service contrôle la révision du fichier avant de transmettre un autre octet. La fenêtre reste liée au handle vivant, à la session, au droit de lecture, au total et à l’offset attendus. Une écriture du fichier pendant la fenêtre invalide la continuation. Le traitement des fichiers malformés atteint alors le diagnostic UTF-8 prévu. [Service de fichiers](../user/novafs/files.rs) · [Curseur de répertoire](../user/novafs/files/namespace.rs)

## Reproduction de l’intermittence

QEMU 11.1.3 TCG, x86_64, 1 vCPU, 256 Mio, VirtIO-blk, volume P9 neuf, secteurs logiques et physiques de 4 096 octets. Chaque passage repart du début du banc historique, change plusieurs fois de répertoire, lit un fichier, vérifie les refus de traversée, puis exécute `ls src/` depuis `/g`. Un succès exige `lib`, `test1.rs` et le code 0. Les sorties individuelles et le SHA-256 de chaque série figurent dans `bilan-qualification.json` de l’archive.

| Noyau et parcours | Essais | Numéros donnant le statut 10 | Autres essais |
|---|---:|---|---|
| Fusion, suite entière sans instrumentation | 3 | 03 | 01, 02 réussis |
| Fusion instrumentée, trace détaillée, suite entière | 5 | 02, 03, 05 | 01, 04 réussis |
| Fusion instrumentée, trace limitée aux refus, suite entière | 5 | Aucun | 01 à 05 réussis |
| Fusion instrumentée, préfixe historique jusqu’à `ls src/` inclus | 18 | 03, 09, 10, 12 | Les 14 autres réussis |
| Correctif initial, suite entière | 5 | Aucun | 01 à 05 réussis |
| Noyau final, avant correction de la lecture d’invite du banc, suite entière | 5 | Aucun | 01 à 05 réussis |
| Livraison, suite entière | 5 | Aucun | 01 à 05 réussis |
| Livraison, même préfixe historique | 18 | Aucun | 01 à 18 réussis |

La sonde détaillée a consommé le quota de messages de débogage du service avant certains refus. Les quatre captures du préfixe avec la sonde ciblée contiennent la comparaison qui renvoie 10. La commande, la géométrie et les droits sont identiques dans les témoins réussis. Un rendez-vous distinct entre deux clients impose ensuite l’ordre des opérations indépendamment de l’ordonnancement.

## Assertions retrouvées et corrections du banc

- `cli_p9.py` exige toujours le lancement puis la collecte des quatorze images CLI historiques. Il comptait auparavant sur l’aide pour lancer les ELF de `rm` et `rmdir`. ARC-026 sert désormais l’aide publique sans lancement ni écriture. Le banc crée un répertoire et un fichier fictifs, les retire avec les deux commandes métier et vérifie leur disparition. Les assertions sur les quatorze images, les statuts et la restitution sont conservées.
- Une trace asynchrone de retrait du temporaire pouvait terminer une ligne juste avant l’écho de `xxpwd`. Le banc prenait ensuite l’invite redessinée par la touche Début comme fin de commande. Il attend désormais la séquence complète CR LF suivie de l’invite. Les redessins utilisent CR seul. La sortie de `pwd`, le statut requis et les anciennes assertions sont toujours vérifiés.
- Deux clients natifs exercent START/CONTINUE sur le même serveur. Une écriture dans un autre objet autorise la suite et ses 88 octets exacts. Une mutation du fichier ouvert refuse la suite avec REVOKED. Le même banc valide le curseur après une mutation dans un sous-répertoire et exige REVOKED après une mutation du répertoire énuméré. Les témoins négatifs de handle étranger et de droits réduits restent actifs. [Banc natif](../user/storage/names.rs) · [Banc CLI P9](../tests/cli_p9.py) · [Lecture de l’invite](../tests/cli.py)

## Qualification de la branche de stabilisation

Les commandes ordinaires ont été exercées sous QEMU direct 11.1.3 TCG, en AArch64 et x86_64. Chaque cellule P9 utilise les deux géométries logiques et physiques de 512 et 4 096 octets. Les sessions shell disposent de 1 vCPU et de 256 Mio. Les pipes et les quatre témoins binaires historiques utilisent 2 vCPU. Les fichiers VM associent le SHA de l’ELF au lancement exact. Les premiers lancements sur modifications locales portent encore le SHA du parent dans le champ de provenance du lanceur. Le diff exact du commit de correctif et les deux empreintes de noyau sont archivés.

| Contrôle | AArch64 | x86_64 | Vérification effectuée |
|---|---|---|---|
| `tmp_prive_cli.py` | 512 et 4096 | 512 et 4096 | Deux listages, objets privés masqués, fichier témoin, refus de droits, révocation, relecture par l’oracle |
| `nsh_scripts.py` | 512 et 4096 | 512 et 4096 | 31 commandes par profil, 11 lignes comparées à Bash, reprise après redémarrage |
| `nsh.py` | 512 et 4096 | 512 et 4096 | 113 commandes par parcours, isolement des retraits, coupure, vérification indépendante de la cible et du voisin |
| `vi.py` | 512 et 4096 | 512 et 4096 | UTF-8 tronqué refusé et frontière valide acceptée, édition, historique, refus concurrent, révocation, collecte de 16 ELF et relecture de 16 fichiers par profil |
| `cli_p9.py` | 512 et 4096 | 512 et 4096 | Tri, dates à la nanoseconde, options, alias, édition de ligne, 14 images exécutées et collectées, copie de 67 509 octets, coupure de VM, comparaison après remontage |
| Terminal et édition paginée | 512 et 4096 | 512 et 4096 | `nsh_terminal.py` et `vi_pages.py` en profil ordinaire, erreurs de lecture et de saisie, octets de la copie |
| Continuations avec deux clients réels | P9/512 et P9/4096 | P9/512 et P9/4096 | Objet étranger modifié, contenu exact de la suite, refus après modification de la cible, curseur périmé refusé, relecture de P9 hors ligne |
| Pipes natifs | 512 et 4096 | 512 et 4096 | Par profil, 41 commandes du banc `cat \| head`, 11 fichiers relus et 40 observations du banc `cat \| wc`. Les deux bancs vérifient les PID, statuts et restitutions |
| IPC, processus, ELF | Réussi | Réussi | 18 cas IPC et courses, collecte des 128 processus du banc de cycle de vie, contrôle ELF |
| Registre natif et binaires SDK | 2 ELF archivés | 2 ELF archivés | 9 groupes ABI, 48 sélecteurs, exécution sans recompilation des exemples SDK et des témoins de threads v0.20.0 après contrôle du SHA-256 |
| Banc d’espace de noms | 512 et 4096 | 512 et 4096 | Deux clients, renommage, chemins profonds, coupures et erreurs d’E/S autour de F1/F2 |
| Banc de fichiers | 512 et 4096 | 512 et 4096 | Remontage indépendant, ancienne ou nouvelle génération attendue, EIO et refus sur support en lecture seule |
| CLI de compatibilité P8 | 512 et 4096 | 512 et 4096 | Lancement d’ELF isolés, révocation, comportement après faute et remontages |

### Reproduction et lecture des preuves

Depuis la racine du worktree du correctif, configurer le répertoire temporaire selon AGENTS.md. Les cibles Make reconstruisent les noyaux et préparent les outils hôtes. Pour rejouer les trois suites ci-dessous sur les deux géométries et les deux architectures, utiliser les entrées ordinaires du dépôt.

```sh
for arch in aarch64 x86_64
do
    make -C novaOS ARCH="$arch" cli-p9-test nsh-test nsh-scripts-test
done
```

L’archive contient les commandes exactes des autres régressions dans chaque résultat JSON, les journaux série, les noyaux de livraison, les quatre applications SDK historiques et les scripts de campagne. Les sous-répertoires `initial`, `diagnostic`, `sonde` et `prefixe-sonde` conservent les essais antérieurs au correctif. `livraison`, `prefixe-livraison`, `navigation`, `regressions` et `abi-finale` portent les résultats de la qualification. `bilan-qualification.json` donne les 64 observations de `ls src/`, y compris chaque échec. `controle-170.json` consigne la vérification des preuves du second pipe.

### Périmètre et réserves

Les coupures exercées concernent des VM QEMU et les points de panne injectés par les bancs. Le matériel physique, l’application graphique UTM et une perte d’alimentation de l’hôte restent hors de cette qualification. Les tests différentiels longs du moteur NovaFS ont été laissés à leur qualification existante. Le diff fonctionnel concerne le service de fichiers en espace utilisateur et les bancs. Le registre ABI v1 conserve ses 48 sélecteurs et le format P9 sa version v0.9.13.

Un objet effectivement modifié, un handle révoqué ou une ancienne continuation sur un répertoire modifié déclenchent toujours un refus. Le diagnostic de cette campagne explique les cinq observations reproduites. Il ne généralise pas cette cause à tous les futurs retours REVOKED. Le second pipe est intégré sur `main`. L’intégration du correctif de stabilisation et la décision de clôture de v0.23.0 relèvent d’un nouvel arbitrage MOA/MOE.

### Archives vérifiées

Les cinq suites historiques directement concernées ont produit 24 passages de livraison réussis, dont cinq parcours complets de `nsh.py` sur x86_64/4096. Les huit passages de navigation et les 32 régressions complémentaires ont également réussi. Les échecs historiques apparaissent avec leur sortie d’origine. Le worktree principal a conservé son HEAD, son état Git, le contenu local de `.gitignore` et les cibles de ses deux liens non suivis.

| Preuve | Contenu | SHA-256 |
|---|---|---|
| Archive versionnée de stabilisation | 1 545 fichiers issus de la campagne, inventaire des originaux, référence de la copie intégrale et manifeste de l’archive | `b018b1f0b9277c68637716ab467b711db3cd744632a4ab5024de8e842ed6b4f7` |
| Copie durable intégrale `archives-stabilisation-pipe-023x/campagne-complete.tar.gz` | 2 753 fichiers, dont les ISO, images de volumes, sondes et journaux, et un manifeste. 950 575 717 octets compressés | `24b565c09430d8fd98de958a1b1b8b48fe21c466541d6e5889b22ac54d12c1d1` |
| Noyau ordinaire AArch64 | ELF de la campagne de livraison | `808e282e738fee05e36e67245e57f53288147a42331aa90f2d69eddf471c7467` |
| Noyau ordinaire x86_64 | ELF de la campagne de livraison | `1526bf06973ed5f92a3247244c5d9865233647f73597cc6c49f8d74c718f0048` |

Les originaux sont conservés dans `OS-worktrees/preuves-stabilisation-pipe-023x` et la copie intégrale dans `OS-worktrees/archives-stabilisation-pipe-023x`. Chaque membre des deux archives a été décompressé, relu et comparé au SHA-256 du fichier source ou du manifeste correspondant. La consultation des traces et des résultats individuels peut se faire depuis la branche publiée. [Archive de qualification](validation/stabilisation-v0.23.x.tar.xz) · [Empreinte de l’archive](validation/stabilisation-v0.23.x.sha256)
