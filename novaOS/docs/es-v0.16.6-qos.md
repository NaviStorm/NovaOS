# Politique hôte et latences de sauvegarde sous UTM

Qualification sur `codex/es-groupees-0166`, à partir de `262a335a01f4b66a0dba0d3a460cafb443f6eee6`. L’expérience porte sur la politique du moteur hôte et l’observation du thread TCG. Les sauvegardes passent par le prototype E/S groupées avec scrutation, sur des supports jetables. Le jalon publié demeure NovaOS v0.16.5 et NovaFS v0.9.13 P9. [Campagne précédente](es-v0.16.6-irq.md)

## Contrats macOS et observations possibles

Apple documente l’affectation d’une classe QoS au travail. Cette indication influence l’ordonnancement sur Apple silicon. Une tâche de fond favorise l’efficacité énergétique. Une tâche déclenchée par l’utilisateur peut demander `User Initiated`. Le système reste responsable du placement. L’API publique `pthread_set_qos_class_self_np` s’applique au thread appelant. Un helper XPC et ses threads possèdent leur propre contexte d’exécution. La priorité visible de l’application ne suffit donc pas à établir celle de son moteur. [Recommandations Apple silicon](https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon) · [Classes QoS Apple](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheTaskLevel.html)

`taskpolicy -b -p PID` place un processus existant sous `PRIO_DARWIN_BG`. `-B` le fait sortir de cette politique. Au lancement d’un nouveau programme, `-c utility` ou `-c background` applique un plafond QoS. Les réglages affectent aussi les E/S et la gestion des ressources. Dans la campagne UTM, la comparaison `-b/-B` mesure ainsi l’effet de la politique processus complète. Une priorité élevée ou la réussite de `-B` doit être accompagnée d’une observation effective du placement pour étayer une attribution aux cœurs. [Manuel Apple de taskpolicy](https://github.com/apple-oss-distributions/system_cmds/blob/main/taskpolicy/taskpolicy.8)

UTM 5.0.8 lance QEMU dans un `QEMULauncher` enfant du helper XPC. Le moteur signé observé appartient à l’équipe `WDNLXAD4W8`, possède le hardened runtime, le sandbox hérité et l’autorisation JIT. La validation de sa signature a réussi. Les réglages de cette expérience visent uniquement le PID associé à l’UUID de la VM jetable. Dans le code amont, `startQemuFork` demande `User Interactive` pour son propre thread après le fork. Le second point d’entrée `startQemuProcess` appelle directement `runQemu`. Ces deux chemins et l’existence d’un thread TCG distinct imposent d’observer la QoS de ce dernier en exécution. [Architecture UTM 5.0.8](https://github.com/utmapp/UTM/blob/v5.0.8/Documentation/Architecture.md) · [Code du lanceur distribué](https://github.com/utmapp/UTM/blob/v5.0.8/QEMULauncher/Bootstrap.c)

Le témoin QEMU direct applique les classes publiques depuis le thread TCG lui-même, à l’aide de l’interface officielle de plugins de QEMU. Il relève la classe demandée, la priorité Mach et le cœur courant à des marqueurs invités. Le volet UTM observe le moteur distribué avec le modèle `System Trace` d’Instruments. Il mesure les intervalles `Running`, `Runnable`, `Blocked`, `Preempted` et `Interrupted`, le type de cœur et la priorité ordonnancée. Le temps de résidence P/E provient de ces intervalles. [Protocole QEMU](es-v0.16.6-qos-qemu.md) · [Observation publique](es-v0.16.6-qos-observation.md)

## Environnement et protocole UTM

Hôte Apple M4 Max, 128 Gio, macOS 27.2 build 26B123. Le relevé initial indique l’alimentation secteur et une batterie à 80 %. `pmset -g therm` rapporte l’absence d’avertissement thermique et de performance enregistré. Ces relevés ponctuels ne renseignent pas la fréquence instantanée pendant chaque sauvegarde. Le QCOW2 se trouve sur le volume APFS interne chiffré FileVault, unité de bloc annoncée de 4 096 octets. `qemu-img` 11.1.3 prépare et relit les images. UTM exécute son propre QEMU 10.0.12.

La trace utilise Xcode/Instruments 27.2 build 27B52 présent dans `/Applications/Xcode.app`. Le chemin précédemment indiqué pour Xcode 27.1 Beta est absent. L’outil hôte n’intervient pas dans la construction des ELF invités de référence. Le pilote UTM préalable est conservé dans `novaOS/build/qos-0166/pilote-a` et exclu de la distribution finale, car les régressions tournaient encore.

Dans la campagne antérieure du commit de départ, les 30 sauvegardes UTM en polling ordinaire hors sonde invitée avaient toutes comptabilisé leur CPU sur E. Neuf dépassaient la seconde, dont 5/20 sous charge stockage, 4/6 au repos pour les éditions multiples et 0/4 pour l’insertion localisée. La pire sauvegarde sous charge avait pris 1 951,191 ms, dont 801,267 ms de lectures cumulées du backend et 3,231 ms de FLUSH. Cette lecture des données archivées motive les mesures appariées. Une classe de cœur identique pour les trente essais ne peut expliquer à elle seule leurs différences. [Données historiques, conservées séparément](validation/es-v0.16.6-irq-utm.json)

La matrice UTM planifie 93 essais. Chaque essai possède son nouveau QCOW2, un vCPU TCG Cortex-A72 et 256 Mio, sur `virt-10.0`, avec le firmware expérimental UEFI, GICv3 sans ITS, le TPM CRB et le chemin DMA du profil déjà qualifié. Le banc utilise la console série de la vraie VM. Il conserve les événements QEMU IOMMU et VirtIO-blk historiques, y compris les traces par lecture et écriture. Il alterne les trois politiques `ordinaire/-b/-B` selon une permutation déterministe et répartit aussi les cellules dans le temps. Les benchmarks QEMU et UTM occupent deux créneaux successifs, hors régressions.

| Cellule | Modification | Secteurs | Charge ajoutée | Instruments | Sonde invitée | Répétitions par politique |
|---|---|---:|---|---|---|---:|
| C1 | 4 Mio insérés et éditions locales, destination 8 Mio + 1 octet | 4 096 | Stockage | Sans | Sans | 8 |
| C2 | Même opération | 4 096 | Stockage | Avec | Sans | 8 |
| C3 | Même opération | 4 096 | CPU | Avec | Sans | 6 |
| C4 | Même opération | 4 096 | Repos | Sans | Sans | 3 |
| C5 | Création de 4 Mio | 512 | Repos | Avec | Avec | 3 |
| C6 | Même opération que C1 | 4 096 | Stockage | Avec | Avec | 3 |
| C7 complémentaire | Même opération que C1 | 4 096 | Repos | Avec | Sans | 6 |

C7 est un complément prospectif de 18 essais, décidé après le dépassement de 1 846,086 ms au premier passage C4 sous `-B`. C4 avait conservé les compteurs disque mais aucune résidence du vCPU. C7 enregistre le même scénario au repos sous les trois politiques. Il vient après la matrice principale et les deux bancs firmware, avec les six permutations possibles. Cette série ne reconstitue pas la trace de l’essai C4 antérieur.

La charge stockage écrit des blocs de 4 Mio dans un fichier temporaire distinct de 32 Mio sur le même volume, avec `F_NOCACHE` et `fsync`. La charge CPU lance douze travailleurs SHA-256 bornés. Le banc attend leur démarrage et vérifie qu’ils restent actifs pendant toute la commande. « Repos » désigne l’absence de charge ajoutée. Deux relevés publics de CPU global, de mémoire virtuelle et de disque encadrent chaque sauvegarde. Leurs fenêtres sont plus larges que la commande. L’activité des autres applications demeure possible et les relevés individuels la conservent. [Charges hôtes](../tests/charge_hote.py)

Le chronomètre commence immédiatement avant l’envoi de `:w destination` et finit à la réception du message de succès. La saisie automatisée de `OUI` constitue une borne intermédiaire. Les acquisitions de priorité, les requêtes HMP, l’armement d’Instruments, son export et son analyse se font hors de cette fenêtre. L’enregistrement des événements système se poursuit pendant la fenêtre et a son coût propre. C1/C2 permettent d’en comparer l’effet sur les distributions. Les CPU `Running` mesurés incluent le calcul invité, TCG et la scrutation active.

Après chaque succès, le banc ferme la VM, la redémarre, ouvre la destination, puis exporte le support arrêté. Un lecteur indépendant valide les deux générations P9 et compare intégralement source et destination aux octets attendus. Les compteurs HMP encadrent la sauvegarde et doivent afficher deux FLUSH. La variante invitée instrumentée vérifie aussi le nombre d’échanges IPC, F1/F2 et la concordance des compteurs bloc. Les empreintes des binaires restent celles de la référence précédente.

Les percentiles empiriques sont calculés au rang supérieur `ceil(p*n)`, sans retrait de valeur extrême. Pour trois à huit mesures par cellule, p95 et p99 sont le maximum observé. Ces petits effectifs décrivent la campagne et ne démontrent pas une borne de latence générale. Les relevés individuels, les écarts des triples appariés et les empreintes des journaux accompagnent le rapport. [Banc UTM](../tests/qos_utm.py) · [Dépouillement](../tests/qos_utm_analyse.py)

## QEMU direct sur image brute

Les 120 essais QEMU 11.1.3 ont atteint l’acquittement et passé la relecture indépendante. Sur les éditions multiples, la politique ordinaire donne un maximum de 144,308 ms en AArch64 et de 209,952 ms en x86_64. Pour la même charge x86_64, les maximums sous plafond processus `background` et sous demande `background` du seul thread sont respectivement de 1 237,452 et 1 263,272 ms. Les quatre dépassements QEMU sont détaillés individuellement. Les 3 024 marqueurs des deux politiques de fond ont tous observé E. Les 4 536 marqueurs des trois autres politiques ont tous observé P. La durée entre marqueurs et la durée de résidence mesurée sous UTM sont des métriques différentes. [Résultats QEMU et tous les percentiles](es-v0.16.6-qos-qemu.md) · [Séries individuelles](validation/es-v0.16.6-qos-qemu.json)

Les variantes qui changent uniquement la demande du vCPU fournissent un contrôle causal supplémentaire. À opérations et géométrie égales, le rapport médian apparié de temps CPU entre `thread-background` et `thread-user-initiated` va de 4,19 à 5,11 selon l’architecture et l’édition. Lors du maximum x86_64, le vCPU a consommé 1 214,3 ms de CPU pendant 1 240,6 ms de publication. Ses deux FLUSH ont pris 3,625 et 0,525 ms. Sur ces images brutes et ce moteur, le ralentissement provoqué par les politiques de fond touche surtout l’exécution du vCPU. L’extension de ce résultat aux dépassements d’UTM dépend de ses propres traces.

## Résultats UTM AArch64 sur QCOW2

Les 111 sauvegardes ont été acquittées après deux FLUSH, redémarrées et relues intégralement. Le plafond de 64 échanges est respecté dans la série instrumentée. La règle de latence stricte échoue dans deux essais. Les tableaux séparent les conditions, y compris les essais avec observateurs. [111 mesures individuelles et comparaisons appariées](validation/es-v0.16.6-qos-utm.json)

| Cellule | Politique demandée | n | p50 (ms) | p95 (ms) | p99 (ms) | Maximum (ms) | ≥ 1 s |
|---|---|---:|---:|---:|---:|---:|---:|
| C1 | ordinaire | 8 | 152,079 | 789,429 | 789,429 | 789,429 | 0 |
| C1 | `-b` | 8 | 734,171 | 942,323 | 942,323 | 942,323 | 0 |
| C1 | `-B` | 8 | 142,601 | 627,837 | 627,837 | 627,837 | 0 |
| C2 | ordinaire | 8 | 158,099 | 161,588 | 161,588 | 161,588 | 0 |
| C2 | `-b` | 8 | 808,225 | 873,827 | 873,827 | 873,827 | 0 |
| C2 | `-B` | 8 | 164,207 | 884,962 | 884,962 | 884,962 | 0 |
| C3 | ordinaire | 6 | 185,694 | 570,696 | 570,696 | 570,696 | 0 |
| C3 | `-b` | 6 | 577,167 | 617,093 | 617,093 | 617,093 | 0 |
| C3 | `-B` | 6 | 184,993 | 558,949 | 558,949 | 558,949 | 0 |
| C4 | ordinaire | 3 | 619,902 | 674,087 | 674,087 | 674,087 | 0 |
| C4 | `-b` | 3 | 717,117 | 730,743 | 730,743 | 730,743 | 0 |
| C4 | `-B` | 3 | 700,958 | 1 846,086 | 1 846,086 | 1 846,086 | 1 |
| C5 | ordinaire | 3 | 430,427 | 436,502 | 436,502 | 436,502 | 0 |
| C5 | `-b` | 3 | 415,895 | 424,418 | 424,418 | 424,418 | 0 |
| C5 | `-B` | 3 | 436,823 | 536,297 | 536,297 | 536,297 | 0 |
| C6 | ordinaire | 3 | 761,728 | 824,548 | 824,548 | 824,548 | 0 |
| C6 | `-b` | 3 | 861,426 | 911,301 | 911,301 | 911,301 | 0 |
| C6 | `-B` | 3 | 778,785 | 846,043 | 846,043 | 846,043 | 0 |
| C7 | ordinaire | 6 | 164,129 | 166,787 | 166,787 | 166,787 | 0 |
| C7 | `-b` | 6 | 755,340 | 1 762,131 | 1 762,131 | 1 762,131 | 1 |
| C7 | `-B` | 6 | 165,712 | 727,478 | 727,478 | 727,478 | 0 |

En C1, hors instrumentation, la mise en arrière-plan ajoute 476,0 ms à la médiane des différences appariées avec la politique ordinaire. En C2, avec Instruments, cet écart apparié vaut 653,4 ms. L’écart de p50 entre C1 et C2 dépend de la politique et du placement effectivement obtenu dans chaque série. Il ne constitue pas un coût constant de la sonde. En C7, l’écart apparié au repos est de +588,8 ms avec `-b` et de +1,6 ms avec `-B`. Les résultats individuels conservent aussi les inversions où la politique ordinaire s’est exécutée en arrière-plan.

Les opérations bloc sont constantes. La création de 4 Mio totalise 19 lectures, 22 écritures et deux FLUSH. L’édition multiple totalise 143 lectures, 45 écritures et deux FLUSH. Les neuf créations instrumentées utilisent chacune 36 échanges IPC, et les neuf éditions multiples instrumentées, 56. Les temps individuels en C5/C6 donnent un maximum invité de 3,385 ms pour F1 et de 0,708 ms pour F2. Les variantes ordinaires ont conservé la télémétrie HMP et la relecture. Le pic invité sur session entière varie de 4 014 à 9 736 cadres de 4 Kio.

Les deux formats de secteur utilisent des images clairsemées à clusters QCOW2 de 65 536 octets, vérifiées à froid après l’importation. La capacité virtuelle vaut 81 788 928 octets en 512 et 295 698 432 en 4 096. Les relevés d’activité CPU de l’hôte encadrant chaque commande vont de 7,3 à 30,8 % au repos, de 78,9 à 90,7 % sous charge CPU et de 10,2 à 71,5 % sous charge stockage. Le 14 novembre à 04 h 33 CET, un relevé ponctuel de Launch Services situait l’application UTM hors premier plan. Le banc n’a pas demandé de changement de focus.

## Placement mesuré et politique effective

Les 78 fenêtres enregistrées par Instruments couvrent intégralement le vCPU, avec une incertitude de corrélation de 1,001 à 1,009 ms par borne. Dans 41 fenêtres, la QoS effective était `Background` et le vCPU a passé 100 % de son temps d’exécution sur E. Dans les 37 autres, la classe effective était `Default` et le placement était mixte, avec une majorité du temps d’exécution sur P. Sur la phase après consentement de ces 37 essais, au moins 70,7 % du CPU tournait sur P. La classe demandée du vCPU était `Unspecified` dans les 78 fenêtres, malgré les différences de classes effectives. Les fréquences individuelles de fonctionnement ne sont pas mesurées.

| Politique demandée au moteur UTM | Fenêtres | Classe effective Background, tout le CPU sur E | Classe effective Default, majorité P |
|---|---:|---:|---:|
| Ordinaire | 26 | 5 | 21 |
| Mise en arrière-plan `-b` | 26 | 26 | 0 |
| Sortie d’arrière-plan `-B` | 26 | 10 | 16 |

Avant la commande, `ps` donne nice 0 pour UTM et son moteur. Les priorités processus observées sont 4, 46 ou 47 pour le contrôleur UTM et 4 ou 31 pour QEMU. Sur les traces, la priorité ordonnancée du vCPU était principalement 4 en classe effective de fond et 31 en classe par défaut, avec de brefs changements. Instruments fournit aussi les classes des threads UTM effectivement observés. Les intervalles `Default` et `Background` du contrôleur, leur durée de couverture et leur TID sont consignés dans chaque fiche. La commande `-B` a réussi même dans dix fenêtres où le vCPU est resté sur E en classe effective de fond. [Mesures de processus et de threads](validation/es-v0.16.6-qos-utm.json)

| Cellule | Classe effective du vCPU | n | Sauvegarde p50 (ms) | vCPU Running p50 (ms) |
|---|---|---:|---:|---:|
| C2 · stockage | Default / Background | 12 / 12 | 158,3 / 807,2 | 148,7 / 690,6 |
| C3 · CPU | Default / Background | 10 / 8 | 185,0 / 570,7 | 172,2 / 459,9 |
| C5 · création | Default / Background | 2 / 7 | 116,7 / 430,4 | 109,8 / 412,8 |
| C6 · stockage avec sonde invitée | Default / Background | 2 / 7 | 160,2 / 835,1 | 150,9 / 726,4 |
| C7 · repos | Default / Background | 11 / 7 | 165,5 / 755,3 | 156,5 / 587,8 |

Un incident d’observation a touché C2-r06 sous `-b`. Instruments avait écrit son archive mais son processus de clôture a dépassé 120 s. La sauvegarde de 733,205 ms a été acquittée puis relue. Un nouvel export de l’archive après les benchmarks a restitué des intervalles complets et 575,960 ms de CPU, exclusivement sur E. Le dépouillement conserve l’erreur initiale, l’empreinte de la fiche, les empreintes des XML et les résultats de récupération. L’opération a pris 6,409 s hors chronométrage. Pour reproduire cet export, lancer `python3 -B novaOS/tests/qos_utm_reexport.py "$SORTIE/c2-r06-fond.json"` avant le dépouillement. Les sept tests de l’orchestrateur et les quatorze tests du lecteur vérifient notamment les erreurs d’observation, l’absence de faux acquittement, les PID/TID, la couverture temporelle et la relecture des preuves.

## Analyse des dépassements UTM

**C7-r03, au repos, politique de fond.** La commande a pris 1 762,131 ms. Le vCPU a exécuté 497,039 ms, exclusivement sur E. Instruments le classe `Preempted` pendant **1 140,987 ms**, dont 942,68 ms après consentement. Les autres états couvrent 94,157 ms bloqué, 24,832 ms runnable et 5,122 ms interrompu. La QoS demandée du vCPU était `Unspecified`, la QoS effective `Background`. Les intervalles couvrent la commande entière. La préemption représente 64,8 % de l’intervalle et dépasse la seconde à elle seule. Le type de cœur caractérise les 497 ms où le vCPU s’exécutait. La trace ne désigne pas la tâche préemptrice.

Les deux autres passages du même triple donnent 164,129 ms en ordinaire et 165,747 ms sous `-B`. Tous deux ont exécuté la majorité de leur temps CPU sur P, avec une QoS effective `Default`, et ont passé respectivement 0,026 et 0,038 ms en préemption. Les trois invités ont émis 143 lectures, 45 écritures et deux FLUSH. Sur la sauvegarde lente, QEMU comptabilise 171,864 ms de lectures, 143,025 ms d’écritures et 11,287 ms de FLUSH. `disk0` enregistre une activité globale de 140,49 Mio contre 8,43 et 8,44 Mio sur les passages rapides. Le CPU global encadrant la commande était occupé à 24,8 %, avec zéro échange de swap. D’autres processus et les périphériques hôtes participent à ces compteurs. Les temps de service QEMU peuvent se recouvrir avec la préemption ou la scrutation du vCPU. Ils ne s’additionnent pas aux états Instruments.

**C4-r01, au repos et sans Instruments, sortie d’arrière-plan.** La commande a pris **1 846,086 ms**, dont 1 776,633 ms après consentement. Pour les mêmes quantités et opérations bloc, le backend rapporte 425,601 ms de lectures, 153,290 ms d’écritures et 6,922 ms de FLUSH. `ps` a relevé une priorité 4 pour l’application et le moteur au début de la commande. Le cumul CPU du processus QEMU a augmenté d’environ 960 ms entre les deux relevés externes, résolution 10 ms. Le CPU hôte global était actif à 23,1 %, sans échange de swap. L’absence de trace dans C4 laisse inconnues la durée sur E, la durée sur P et les préemptions de cet essai. Le résultat C7 ne permet pas de lui attribuer rétroactivement une cause unique.

## Décision proposée

**Conserver le polling par défaut et l’IRQ sur activation explicite au laboratoire.** La politique hôte et le placement P/E expliquent l’essentiel de la différence de latence courante entre les deux classes effectives. Dans l’unique franchissement tracé, la préemption du vCPU par macOS est le premier poste mesuré. Le second franchissement comporte un temps de service du backend élevé et aucune mesure de résidence. Les deux franchissements ne suffisent donc pas à estimer la proportion générale des dépassements causés par l’exécution sur E. Le profil ordinaire a respecté le seuil dans ses 37 essais de cette campagne, tandis que l’archive précédente comporte des contre-exemples sur le même invité.

La charge cryptographique, le CoW et la scrutation consomment du CPU invité dans tous les cas. Les essais à binaire, données, requêtes et barrières identiques isolent une forte sensibilité à la planification hôte. Aucune incohérence NovaFS ou hausse du nombre d’IPC n’accompagne les deux pointes. La limite stricte reste ouverte pour le profil distribué. Une déclaration de QoS `User Initiated` directement par le moteur au moment d’un travail utilisateur est une piste pour un essai ultérieur dans une version d’UTM qui l’exposerait. Le témoin QEMU direct mesure son effet. La configuration distribuée d’UTM testée ne propose pas au banc une telle demande sur le thread TCG. Le changement de contexte premier plan/arrière-plan et les préemptions en charge concurrente resteraient à qualifier sur cette éventuelle version. Le prototype reste sur sa branche pour arbitrage MOE/MOA.

## Reproduction

Construire les archives polling ordinaires et instrumentées selon la campagne IRQ précédente. Les variables `PROFIL_UTM`, `GENERATEUR`, `POLLING` et `POLLING_SONDE` désignent respectivement le profil système privé, le générateur d’images et les deux ELF. Choisir un dossier de sortie neuf. L’application UTM et Instruments doivent être accessibles dans la session macOS.

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
caffeinate -i -s python3 -B novaOS/tests/qos_utm.py \
  "$PROFIL_UTM" "$GENERATEUR" "$POLLING" "$POLLING_SONDE" "$SORTIE"
caffeinate -i -s python3 -B novaOS/tests/qos_utm.py \
  "$PROFIL_UTM" "$GENERATEUR" "$POLLING" "$POLLING_SONDE" "$SORTIE" --repos-trace
python3 -B novaOS/tests/qos_utm_analyse.py \
  "$SORTIE" novaOS/docs/validation/es-v0.16.6-qos-utm.json --repos-trace
```

L’option `--pilote=-B` réalise un unique essai d’instrumentation préalable. Les archives `.trace`, XML, consoles série et images jetables restent dans le dossier de qualification local. Le rapport JSON conserve les résultats individuels et les empreintes des preuves exportées. En cas d’indisponibilité d’Instruments, le banc consigne l’échec de l’observateur et poursuit l’acquittement puis la relecture du disque. Une rupture de la console, une charge prescrite incomplète, des compteurs HMP incohérents ou une divergence de relecture invalident la qualification de l’essai.

## Régressions et portée de la persistance

Les 84 premières commandes de régression ont réussi avant les mesures contrôlées. Elles couvrent 40 cibles NovaOS sur les deux architectures, 36 coupures QEMU et 20 essais de défaut ou de révocation, les écritures concurrentes et les deux géométries. NovaFS passe par architecture 196 tests standards, 199 instrumentés, 2 304 injections historiques, 486 injections CoW et un million d’opérations différentielles. Les compilations portables et les échanges P9 interarchitectures passent également. Le détail distingue les commandes du banc et les scénarios inclus. [Résultats des régressions](validation/es-v0.16.6-qos-regressions.json)

Le témoin GICv3 EL1 réussit avec QEMU public. Les deux suites firmware historiques ont ensuite réussi sur le moteur QEMU 10.0.12 livré avec UTM, dans des processus de test indépendants, soit **86 commandes de qualification réussies** au total. Elles couvrent 11/11 scénarios DMA/GICv3 en 62,400 s, puis 23/23 scénarios DMA séquentiels et la reprise en 147,551 s. Le banc retrouve 123 cadres firmware réalloués sur 123, et l’INTID 37 en secteur 4 096 sur la cible déplacée. Deux préparations initiales ont échoué avant l’exécution de l’invité à la création du socket local de swtpm. Leurs traces sont archivées séparément. Les deux suites ont été rejouées après la dernière mesure avec ce socket accessible. Le rapport distingue aussi la relecture hors ligne des preuves anciennes des nouveaux essais.

Le banc mémoire a vérifié les profils de 128 et 256 Mio. La qualification du démarrage à 64 Mio reste ouverte sur les deux architectures. Les 111 sauvegardes UTM utilisent le profil 256 Mio.

L’acquittement suit les deux barrières de publication. Le test vérifie les FLUSH et la persistance après arrêt/redémarrage sur un Mac alimenté. L’absence de banc de coupure électrique laisse la perte physique d’alimentation hors qualification. La conservation du format P9 implique aussi ses critères existants de refus après ancre déchirée. [Contrat de récupération](../../novaFS/docs/novafs-p9-ancres-dechirees.md)
