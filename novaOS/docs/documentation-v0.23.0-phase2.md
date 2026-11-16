# NovaOS v0.23.0 — Qualification ARC-026, phase 2

Les 37 noms du catalogue public ont une aide, une version de composant et une page du manuel natif. Les 29 ajouts ont passé la recette documentaire dans les deux invités. La clôture de la phase 2 est proposée à la MOA/MOE avec les réserves de régression ci-dessous. L’image UTM distribuée reste en v0.17.0.

## Révisions qualifiées

Le candidat de phase 1 `c748b436659366e7c18bda0d44c17f3a320e8dc5` descend de la clôture v0.22.0 `ce8f2d83792e4d83f6e4d7ffbce6400a288fbd27`. Le commit d’intégration publié `3bd662503b5fc6df0578e814b1e9d39243009dd4` a exactement son arbre. Les 153 entrées des deux archives (129 et 24) ont été relues et vérifiées par SHA-256. Le candidat fonctionnel de phase 2 est `929a6042adb8d19aaf97c5f83c5523647e31fe42`. [Clôture de phase 1](cloture-documentation-v0.23.0-phase1.md)

| Lot | Commandes | Commit | Observations finales AArch64 / x86_64 |
|---|---|---|---:|
| 1 · Lecture, ELF | `pwd cat head tail wc more` | `aa72194fff79b6326b196145e49d6ab966ed2dba` | 129 / 129 |
| 2 · Mutation, ELF | `mkdir rmdir touch cp mv` | `dac10d61a475d38f0e12ff30a236cad480a51a17` | 108 / 108 |
| 3 · Terminal et observation, internes | `help history clear ps top true false` | `b98bcc4454dab855c27bfe068540ce99f85cc2d8` | 99 / 99 |
| 4 · Langage et session, internes | `alias unalias export unset echo test [ break continue exit return` | `6f3fed287ea71eb4e08774fe89e21c4f289144c2` | 174 / 174 |

Les premières recettes ont enregistré respectivement 119, 108, 99 et 174 observations par invité. Les dix observations supplémentaires du lot 1 portent sur les noms à tiret et leur interprétation dans un répertoire isolé. Le correctif conserve le caractère actif ou protégé d’un motif lors du contrôle des options. Le contrôle des arguments développés précède le lancement métier de l’ELF. Avant cette correction, `cat -n*` refusait le motif avant de découvrir le fichier `-nom`. Le banc historique et le nouveau témoin lisent ce fichier, rejettent les options invalides et contrôlent son contenu. [Banc des quatre lots](../tests/documentation_lots.py) · [Matrice par commande](documentation-v0.23.0-phase2-plan.md)

## Protocole et résultats

- Chaque lot a été compilé pour les deux architectures. Le rejeu des quatre lots utilise QEMU AArch64 et x86_64, 2 vCPU, 256 Mio, un volume NovaFS P9 jetable à secteurs de 512 octets et les sondes documentaires. Les fichiers `session.vm.json` décrivent les profils. Les constructions de production du correctif ont également réussi.
- Chaque commande a été appelée avec `--help`, `--version`, `man NOM` et une option inconnue dans les trois locales. Les sorties et les marqueurs de statut exécutés par le shell figurent dans les JSON et les traces série. Le lecteur revient au shell avec le code 0. Le refus d’option renvoie 3. `fr-CA` et `fr-zzzz` donnent le français, `fr-zzzzz` l’anglais. Les versions affichées proviennent de `nova-cli 0.20.0` pour les nouveaux ELF et de `nsh 0.20.0` pour les intégrées. Le catalogue décrit options, effets, capacités, erreurs et statuts dans les deux langues. [Catalogue](../../components/libraries/documentation.rs)
- La consultation publique précède le lancement métier. Chaque nouvel ELF a également exécuté en processus autonome une aide et une version avec les deux ports métier nuls et la préférence linguistique transmise. Les traces identifient le PID et la langue. Sur les huit fenêtres lot/architecture, les SHA-256 du disque complet avant/après concordent. Compteurs VirtIO supplémentaires pendant la consultation : **0 écriture, 0 octet, 0 FLUSH**. Les opérations métier témoins ont ensuite fait varier les compteurs et le SHA. Les fichiers de la recette de transfert ont été relus hors invité après arrêt.
- Le banc des huit pilotes a réussi sur le correctif, avec 71 observations par invité, neuf parcours de pages, la recherche, le retour à l’invite, zéro requête STOP pour l’aide et une vraie requête STOP de maintenance comme témoin. Les cinq tests Rust du catalogue et de l’admission ont réussi sur Darwin ARM64 et sous Rosetta x86_64. Les neuf contrôles du registre ABI ont réussi. Le diff contre la base de phase 2 est vide pour `novaOS/abi` et `novaFS`. L’ABI native v1 conserve 48 sélecteurs, le stockage reste en v0.9.13 P9. [Banc des pilotes](../tests/documentation_cli.py) · [Contrôles ABI](../tests/abi_registre.py)

### Scripts et continuité de session

| Commande ou parcours | Observation commune aux deux invités |
|---|---|
| `false`, `false --help`, `true` | Respectivement 1, 0 avec aide, 0 |
| `exit --help` puis `echo SESSION-VIVANTE` | Aide, statut 0, puis témoin affiché dans la même session |
| `exit` interactif | Fin de session réussie puis arrêt normal avec acquittement des deux vCPU |
| `nsh -c 'exit 42; echo impossible'`, `nsh -c 'exit'` | Retour du script au shell avec 42 et 0 |
| `return 37` en script, `return 1` au niveau interactif | Respectivement 37 et refus 3 |
| Alias, variables, `break`, `continue`, tests numériques et courts-circuits | Les témoins exercés conservent leur effet normal, leur portée et leurs statuts. L’aide des contrôles de boucle laisse la boucle se poursuivre. |

## Régressions et réserves

| Parcours | Résultat et portée |
|---|---|
| `nsh.py`, P9/512 | Réussite sur les deux architectures après correction, y compris globbing, transferts, retraits, coupure et relecture. |
| `nsh.py`, P9/4096 | AArch64 réussi. Sur x86_64, premier passage en échec à `ls src/` (« capacité révoquée », code 10), puis rejeu intégral réussi sur le même code. La base a également réussi lors de la comparaison. L’origine de cet échec intermittent reste indéterminée. |
| `nsh_scripts.py` | Le glob récursif depuis la racine dans `p='**/*.txt'; for i in $p; do echo [$i]; done` renvoie « capacité révoquée ». Reproduit sur la base de phase 2 et sur les deux architectures. Également observé sur le correctif en P9/4096. La suite s’interrompt à cette assertion. Le banc des lots exerce séparément les contrôles de flux et les cas documentaires. |
| `cli_p9.py` | Le préambule historique essayait de démarrer le lanceur P9 avec un volume P8. Sélection explicite du profil P8 pour ce passage et maintien des assertions. Ensuite `ls -latr --color /` retourne 10 en P9/512 sur la base et sur le correctif, sur les deux architectures. La suite de ce parcours reste non qualifiée. |
| `vi.py`, fichier UTF-8 tronqué, et `tmp_prive_cli.py`, `ls -a /` | Anomalies de phase 1 conservées. Toutes deux préexistantes, cause non déterminée. Les reproductions, les deux architectures et le risque sont détaillés dans le registre de phase 1. Aucune cause commune n’a été établie pour les différents refus. |

Les premiers passages interrompus, leurs journaux et les rejeux sont conservés. L’acceptation proposée porte sur ARC-026 pour les 37 commandes distribuées et sur les profils mesurés. Les fonctionnalités historiques dont le banc s’interrompt gardent les réserves ci-dessus. La campagne ne comporte pas de qualification UTM ni de nouvelle image installable.

## Preuves durables

Sous `OS-worktrees/archives-documentation-0230-phase2/`, chaque archive a un manifeste homonyme `.sha256` par fichier. Les entrées ont été relues depuis les archives et comparées aux empreintes des fichiers du dossier `preuves-documentation-0230-phase2/`. Les archives des lots contiennent les états intermédiaires. Le rejeu sur le correctif est dans `lotfinal`. Les résultats de comparaison avec la base, les erreurs et leurs rejeux sont dans `regressions`.

| Archive | Fichiers | SHA-256 |
|---|---:|---|
| lot1.tar.gz | 17 | `3702ae986e0376bdc8ff256d60c3e79822a22dfe6c595bb1cbc19ea187dc2bce` |
| lot2.tar.gz | 24 | `9a90a772e93cca30b39c362881ade6eb41783a6782c3f9bdc18e0ca0535ee707` |
| lot3.tar.gz | 17 | `5ab3c8baf9d3ac3c6b239e480cc347e6c14af8e6500c9820769518fc17681f84` |
| lot4.tar.gz | 33 | `15575e40d51a34beb0e2bfd584d9f615dd2d70c5cb8f1ed38905f42004d21a75` |
| lotfinal.tar.gz | 76 | `d58bd215b10ec780fb1b9125e6b41fb8d22ad5a3627fcbc909246e4ba1272d3f` |
| regressions.tar.gz | 239 | `c8effea8709cd5a14c1c9152056a67c628805c1d69969992eb4b9aa230796993` |
