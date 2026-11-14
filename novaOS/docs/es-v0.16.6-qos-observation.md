# Observation publique du vCPU et de la QoS sous macOS

## Périmètre et références

Audit du 14 novembre 2026, branche `codex/es-groupees-0166`, base `262a335a01f4b66a0dba0d3a460cafb443f6eee6`. Le module hôte [qos_observation.py](../tests/qos_observation.py) emploie les commandes publiques `ps` et `xctrace`, ainsi que la notification de démarrage de `notify.h`. Les données proviennent des exports XML d’Instruments. Le module est indépendant des sondes libproc historiques.

Mac observé sous macOS 27.2, build 26B123, noyau Darwin 27.2.0 ARM64 T6041, Xcode et Instruments 27.2, build 27B52. Le SDK est `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk`.

La QoS influence l’ordonnancement du CPU et d’autres ressources. Une priorité numérique, une classe demandée et une classe effective ont des significations distinctes. Le lecteur conserve les libellés et valeurs effectivement exportés par l’outil. Apple présente System Trace pour suivre l’état des threads et l’activité par cœur. [Guide Apple de QoS](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheTaskLevel.html) · [Ordonnancement Apple silicon](https://developer.apple.com/videos/play/tech-talks/110147/) · [Recommandations Apple silicon](https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon)

## Outils vérifiés

| Source publique | Donnée réellement disponible | Limite observée |
|---|---|---|
| `ps -L`, `ps -p PID -o pid=,ppid=,nice=,pri=,state=,time=,comm=` | PID, parent, nice, priorité `pri`, état, temps CPU agrégé et exécutable | Instantané de processus. Aucune classe QoS ni identité TID dans cette liste de colonnes |
| `ps -M` | Lignes de priorité et temps par thread | Le format ajoute ses propres colonnes. L’identité stable du vCPU est absente |
| `top -h` et `man top` | Temps et CPU de processus, compte de threads, changements de contexte, cycles, instructions, boosts | Les options exposées ne fournissent pas la classe QoS effective ni le cœur par TID |
| `sample PID 1 50 -file fichier` | TID et nom `CPU 0/TCG`, piles du témoin, alternance calcul/sommeil | Aucune classe QoS, priorité ou résidence P/E dans le rapport obtenu |
| `spindump PID 1 20 -onlyTarget -timeline -noBinary` | Manuel décrivant QoS, priorité, App Nap et états des threads | Refus effectif `spindump must be run as root when sampling the live system`. Le paramètre `-sampleWithoutTarget` permet de continuer après la fin de la cible. Le ciblage exclusif exige `-onlyTarget` |
| `powermetrics -h` | Options documentées `--show-process-qos`, `--show-process-amp`, `--show-cpu-qos` | Refus effectif `powermetrics must be invoked as the superuser`. Les compteurs par processus agrégeraient plusieurs threads |
| `footprint` | Empreinte mémoire d’un processus selon son manuel | Le manuel fournit des métriques mémoire |
| API publique `task_name_for_pid`, puis `task_policy_get(TASK_CATEGORY_POLICY)` | Port de nom obtenu sur un enfant, retour 0 | Lecture de rôle refusée avec retour Mach 4. Les flavors publics de latence 10 et débit 11 sont également refusés avec ce port |
| SDK `pthread/qos.h` | Lecture de QoS par un `pthread_t` du processus appelant | Ce contrat ne désigne pas les threads d’un PID externe |
| `xctrace record --template 'System Trace' --attach PID` | TID nommé, priorité ordonnancée, états, core P/E, intervalles QoS après événements | Instrumentation explicite, exports différés et accès au profilage requis |

Les commandes `ps` et Instruments ont initialement été refusées par le bac à sable de l’agent. L’exécution autorisée hors bac à sable a permis l’observation. Aucun privilège root n’a été utilisé pour les traces retenues. Les messages de refus de spindump, powermetrics et des lectures Mach font partie de l’audit.

Les contrats locaux consultés sont `man ps`, `man top`, `man sample`, `man spindump`, `man footprint`, `man taskpolicy`, le manuel `xctrace.1` livré avec Xcode, et les en-têtes publics `notify.h`, `pthread/qos.h`, `mach/thread_info.h`, `mach/task_policy.h`, `sys/resource.h`. Le manuel SDK de `getpriority(2)` documente uniquement `who=0` pour la lecture de l’état Darwin background. Le module s’appuie sur les données effectivement exportées par Instruments pour caractériser le vCPU.

## Contrat du banc

`relever(pid_utm, pid_qemu, tid_hmp)` retourne les instantanés des deux processus, accompagnés de leurs bornes monotones. Chaque erreur de `ps` apparaît sous `indisponible`. Le champ `tid_hmp` conserve l’identifiant annoncé par QEMU.

`horloges()` lit successivement le temps monotone, UTC, puis monotone. Le banc appelle cette fonction près des trois frontières de la commande, du début de `:w` à OUI puis au message de succès.

`tracer(pid_qemu, dossier)` démarre System Trace en mode attaché. Le contexte attend la notification Darwin publique annoncée par `--notify-tracing-started`. Un délai maximal de 60 secondes borne son démarrage. La sortie envoie SIGINT au seul processus xctrace créé, attend la fermeture de la trace pendant au plus 120 secondes et conserve son code de sortie. Le processus invité et son lanceur continuent à être gérés par le pilote UTM.

Après fermeture, `trace.extraire(tid_hmp, bornes_monotones, ancrages_horloges, pid_utm=pid_controleur)` exporte séparément les tables `thread-state` et `ThreadQoSTable`, ainsi que le TOC. Les exports disposent chacun d’un délai maximal de 120 secondes. Les défauts de lecture, de couverture, d’identité et les erreurs de sous-processus sont remontés au pilote. Le lecteur sélectionne un seul thread nommé `CPU 0/TCG` dans le PID attaché, vérifie l’existence de temps Running et rapporte l’égalité ou la différence avec le TID HMP. Une ambiguïté de nom entraîne un refus.

Le lecteur résout les références XML par identifiant. Chaque phase reçoit les temps par état, les temps Running par type P/E, les priorités ordonnancées pendant Running et les intervalles de QoS demandée/effective. Les séjours sont découpés aux frontières de la fenêtre. Les temps absents des tables sont conservés explicitement comme lacunes. Les durées par état comprennent séparément `Running`, `Runnable`, `Blocked`, `Preempted` et `Interrupted`.

Le paramètre facultatif `pid_utm` sélectionne les événements QoS du contrôleur graphique. Le champ `threads_utm` fournit pour chaque TID les noms vus, les classes demandées et effectives effectivement enregistrées, les intervalles intersectés et leur couverture. Le calcul de la durée sans observation d’aucun thread utilise l’union des intervalles. Plusieurs threads actifs en même temps ne doublent donc pas la couverture. Une trace ciblée sur QEMU peut ne contenir aucun événement du contrôleur. Les observations restent attachées aux TID qui les ont produites.

La date de début d’Instruments est exportée à la milliseconde. Le lecteur retient une incertitude par borne de 1 ms augmentée de la dispersion des trois ancrages horloges. Une dispersion supérieure à 2 ms entraîne un refus. Le début et la fin de la commande, augmentés de cette incertitude, doivent être couverts par la trace du thread. Cette corrélation dépend de la continuité observée de l’horloge UTC pendant l’essai. Un ajustement UTC aller-retour entre deux ancrages pourrait rester invisible.

Les essais chronométrés sans Instruments et les essais instrumentés doivent être présentés séparément. Le démarrage, l’arrêt et l’export ont lieu hors de la fenêtre de sauvegarde. L’enregistrement simultané pendant la commande consomme des ressources hôte. Le coût de cette instrumentation se mesure par des séries comparables.

## Étalonnages et preuve observée

Le [témoin reproductible](../tests/qos_observation_temoin.c) crée son propre thread nommé `CPU 0/TCG`, affiche PID et TID, et alterne environ 100 ms de calcul avec 100 ms de sommeil. Avec un argument, il change la classe demandée entre User Initiated et Background. Les appels pthread sont contrôlés. Il termine après environ 24 secondes.

| Essai | Résultat observé |
|---|---|
| Lancement System Trace sur `sleep` pendant 2 s | Export `thread-state` disponible, CPU 10/11 étiquetés P dans les événements observés. Le mode `--launch` interrompt l’enfant à la limite et renvoie 54. Le banc utilise `--attach` |
| Attachement à un enfant calcul/sommeil | PID 24828, TID 1356534 identifié à la fois par pthread et Instruments. Fenêtre 1005,677 ms, Running 490,325 ms, dont P 490,219 ms et E 0,105 ms. Couverture des états complète |
| Coût hors fenêtre du même attachement | Démarrage 1841,012 ms, fermeture 3106,345 ms, export/analyse des états 3828,784 ms. L’export QoS ajouté ensuite introduit un traitement supplémentaire |
| Classe stable demandée avant attachement | Table QoS vide. Le lecteur conserve une durée sans observation |
| Changements demandés pendant attachement | Quatre intervalles User Initiated/Background exportés, classe effective affichée Default sur ce témoin. Les deux champs sont conservés séparément |
| `sample` du témoin | TID 1342142 identique à pthread, nom CPU 0/TCG, 10 échantillons en calcul et 10 en sommeil |
| Premier pilote UTM de la campagne | QEMULauncher PID 28640, TID Instruments 1368080, nommé CPU 0/TCG. Classe demandée Unspecified, effective Default entre 285067000 et 882662057 ns de la trace. Cet intervalle couvre les 164,217 ms de la commande, avant et après OUI, avec une incertitude de 1,001125 ms par borne. L’identifiant HMP reçu correspondait au PID |

Les archives d’étalonnage sont dans `novaOS/build/qos-observation/`. La première trace UTM relue est dans `novaOS/build/qos-0166/pilote-a/c0-r01-sortie/`. Le lecteur filtre explicitement PID et TID. L’export ciblé contient aussi des références à des threads réveilleurs et à d’autres événements corrélés du système.

Sur le même pilote UTM, Instruments a identifié un thread du contrôleur graphique PID 2242. Le TID 1368533 présente une demande et une classe effective Default sur 133,124542 ms de la commande, avec 31,092417 ms sans observation. Cet intervalle ne permet pas d’attribuer une classe aux autres threads de l’application.

Les quatorze tests synthétiques du [lecteur](../tests/qos_observation_test.py) passent. Ils couvrent le découpage aux frontières, les références XML, la différence de TID HMP, la classe effective explicite, l’absence de QoS, les PID divergents, l’ambiguïté de nom, les ancrages instables, les chevauchements, les lacunes, le filtrage du PID du contrôleur et l’union des couvertures de plusieurs threads graphiques. Les commandes de contrôle sont les suivantes.

```sh
python3 -B novaOS/tests/qos_observation_test.py -v
clang -Wall -Wextra -Werror -O2 novaOS/tests/qos_observation_temoin.c -o novaOS/build/qos-observation/temoin-public
xcrun xctrace list templates
xcrun xctrace help record
```

## Réexport après une erreur Instruments

Le pilote peut confirmer la sauvegarde et sa relecture alors que le processus Instruments signale une erreur de clôture. La [commande de réexport](../tests/qos_utm_reexport.py) travaille exclusivement sur l’archive existante. Son entrée est la fiche JSON de l’essai. Elle vérifie les conditions de sauvegarde, la concordance des bornes avec les trois ancrages, le PID de la cible et l’identité unique du thread. Elle exige une couverture complète de la commande en états et l’identification du type de cœur pour tous les séjours Running.

La sortie se trouve dans le sous-dossier `post-traitement` de l’essai. Chaque tentative conserve les XML produits et son diagnostic. Le fichier `resultat.json` associe le résultat à l’empreinte de la fiche d’origine, au message d’erreur initial et aux empreintes des trois exports. Le lecteur recalcule les mesures à partir des XML avant de restituer le résultat. Une tentative réussie peut être relue à nouveau. Les tentatives infructueuses restent consultables dans leurs dossiers respectifs. Les 14 tests incluent une reprise après échec, la conservation de la fiche, une double relecture et le refus d’une empreinte divergente.

```sh
python3 -B novaOS/tests/qos_utm_reexport.py novaOS/build/qos-0166/utm/c2-r06-fond.json
```

L’essai C2-r06-fond a atteint le délai de fermeture de 120 s après l’écriture de l’archive. Après les 111 essais UTM, les trois exports et l’analyse ont pris 6,409 s. La fiche d’origine a conservé son SHA-256 `3ee0a3ee579e2c3192061fecbfdfe043c7bbc8798bcfea39c02339603070a082` et son erreur initiale. La relecture indépendante des XML confirme le TID Instruments 1706531 du PID QEMU 17400. Le vCPU a passé 575,960 ms en état Running sur cœur E, avec 54,398 ms avant OUI et 521,562 ms après. La QoS demandée est Unspecified et l’effective Background pendant la totalité des 733,207 ms de la fenêtre de trace. La mesure monotone d’origine vaut 733,205 ms, l’incertitude de corrélation 1,004 ms par borne et la lacune d’observation des états 0 ns. Le thread GUI 1706791 est observé en Default sur 681,266 ms.

## Portée des conclusions

La table d’états fournit une observation directe des séjours du thread sur les classes de cœurs, de sa priorité ordonnancée et de son état d’attente. Le champ Preempted est la classification publiée par Instruments. L’attribution détaillée de la cause et l’identité du thread préempteur demanderaient l’examen des autres colonnes exportées. Le module fournit les temps par état nécessaires à la comparaison.

Une table QoS vide laisse la classe inconnue sur l’intervalle concerné. Le démarrage d’Instruments après l’installation d’une politique peut manquer l’événement initial. Une priorité 31 conserve sa valeur numérique. Sa conversion en classe QoS demanderait une information distincte. Le lecteur utilise uniquement les champs demandée/effective de `ThreadQoSTable`.

Les temps P/E dépendent des événements enregistrés, de leur couverture et de la corrélation à l’horloge hôte. Une lecture réussie doit être accompagnée du compte des lacunes et de l’incertitude par borne. Les tests étalonnent le mécanisme d’observation et les erreurs de parsing. La qualification de la sauvegarde, les seuils de latence et l’effet causal d’une politique appartiennent à la campagne appariée UTM.
