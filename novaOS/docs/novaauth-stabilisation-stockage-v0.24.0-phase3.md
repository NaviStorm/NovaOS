# NovaOS v0.24.0 — NovaAuth phase 3 — Stabilisation stockage

Dans le profil historique de sessions, la perte du destinataire d’une réponse privée pouvait arrêter NovaFS et couper le service public de fichiers. Le code corrigé est `04c43b38ea0d665da28ea47bb7ca6b651cad967f`. Les changements portent sur le raccord de qualification, le lanceur, le client de confiance et les bancs. L’archive conserve les échecs.

## Les trois passages sur la même ISO

L’ISO x86_64 historique a pour SHA-256 `a61032369eaf56847e546eb39b6232ab0e0605fc18a2f4d4c012e875bd7cbb43`. Les trois traces ont en commun le rejet du rejeu, du défi expiré et de la signature incorrecte. Les durées numériques varient déjà entre les passages. Le premier changement dans la **suite des événements fonctionnels** suit le message `NA3 signature incorrecte refusee` du PID 8.

| Parcours x86_64/P9-512 | Événement suivant dans la série | Relecture indépendante |
|---|---|---|
| Premier échec | NovaFS, PID 3, sort avec le code 5 | P9 intègre, registre ouvert sur la génération 2, création publiée |
| Deuxième échec | Même PID, même statut et même PC | P9 intègre, registre ouvert sur la génération 1, enrôlement seul |
| Passage réussi | Le vérificateur PID 6 accepte la preuve suivante | Génération 7, parcours terminé en 69,279 s |

Le PC `0x1042483` est l’adresse qui suit l’instruction `syscall` à `0x1042481` dans l’ELF NovaFS extrait du noyau utilisé. Le sélecteur `0x10004` est `PROCESS_EXIT`. Le point d’entrée du service relaie l’erreur de `medium::serve` à `runtime::exit`. Les 180 secondes visibles sur les deux échecs mesurent l’attente maximale de l’oracle hôte après l’arrêt du service. [Entrée du service](../user/novafs/main.rs) · [Retour à l’ABI](../user/storage/runtime.rs)

## Origine du code 5

Le lanceur historique accordait 5 000 ticks, soit 20 secondes à 250 Hz, pour deux authentifications, trois contrôles négatifs dont une expiration volontaire, et la publication d’un autre compte. Quand le délai expire, le gestionnaire PID 1 quitte avec `TIMED_OUT=8`. Le noyau retire sa famille de services et les extrémités privées du vérificateur. NovaFS peut avoir déjà accepté une requête et traité sa lecture ou sa mutation. Lors de l’envoi de la réponse, la recherche du handle retiré par le noyau donne `DENIED=5`. Le raccord remontait ce statut à la boucle de service. La révocation a donc bien eu lieu. [Lanceur](../user/novaauth/service/launcher.rs) · [Recherche des capacités](../kernel/src/sched/process/ipc/mod.rs) · [Retrait de la famille](../kernel/src/sched/process/ipc/service.rs) · [Réponse du registre](../user/novafs/files/auth_registry.rs)

Les sondes d’erreur signalent l’attente expirée, les sorties avant retrait et l’étage de retour de l’erreur. Elles ne journalisent ni chaque tour de boucle ni le contenu des messages. Deux entrelacements ciblés reproduisent la signature des séries initiales.

| Point retenu dans le service | Gestionnaire | NovaFS | État durable |
|---|---|---|---|
| Réponse de lecture avant la création, étiquette IPC 14 du parcours | Code 8 au tick 5 440 | `IPC_SEND` refusé, code 5 au tick 5 442 | Génération 1 |
| Réponse de création après la décision | Code 8 au tick 5 379 | Retour de `serve_auth`, code 5 au tick 5 381 | Génération 2 |
| Client retardé 5 100 ticks avant remise de la création | Code 8 au tick 5 470 | Retrait du temporaire confirmé, aucune sortie 5 observée | Génération 1 |

Les traces historiques ne contiennent ni le retour d’IPC interne ni la date de sortie du gestionnaire. Les témoins établissent le mécanisme et reproduisent les deux états de disque observés. L’instant précis de préemption des deux exécutions historiques reste inconnu. Les oracles ont remonté les volumes en lecture seule, exécuté `Engine::verify`, ouvert les trois bases contre l’autorité indépendante et retrouvé la décision par son identifiant. Aucune faute utilisateur, erreur d’allocation, erreur bloc ou corruption P9 n’a été observée sur ces parcours.

## Correction et sécurité

- NovaFS traite la disparition du destinataire sur l’envoi de la **réponse privée**. Il continue de servir les autres canaux. Le résultat métier inscrit dans le message, les refus d’accès, les tickets, le journal d’autorité et le contrôle de chaque opération restent effectifs. Une réponse perdue laisse l’issue à réconcilier par son identifiant. Aucun nouvel envoi de la mutation.
- Le client historique signale deux étapes au lanceur, sur son canal privé. Le lanceur vérifie le type, l’étiquette, la longueur et l’ordre. Chaque attente reste bornée à 5 000 ticks. La durée de session et celle des défis demeurent celles du profil qualifié.
- Le provisionneur détient les racines IPC du dossier. Il est collecté après l’acquittement du **dernier contrôle VFS**. Quatre essais exploratoires de l’ancien code avaient atteint un autre délai, celui du provisionneur PID 7, après son attribution. Une première tentative de correction a collecté ce propriétaire trop tôt et les accès ont été refusés sur les quatre configurations. Ces résultats et le correctif suivant sont conservés. [Origine des canaux](../kernel/src/sched/process/ipc/service/auth_lab.rs) · [Contrôle côté client](../user/novaauth/service/src/bin/client.rs)

## Protocole comparatif

Base officielle `2e3188dee6eaef859becca3919bd0d53a20674bb`. Elle contient le registre de phase 2. Elle ne contient pas le lanceur historique, le vérificateur et ses deux clients. Aucun taux d’échec du scénario NovaAuth phase 3 ne lui est attribuable. Le dernier témoin qualifié avant les politiques est `8bb64c6365c2b7e7aaf9a833a368d1440eac1930`. Son ISO archivée provient de `719c78083e3d02e2afd26e18f355f716e08fc356`, avec les mêmes sources invitées. Le candidat d’entrée est `32e85e8366782c4333899d71fbc86ecc7d5def26`. L’ISO de l’incident a été construite sur `1dc6a43c68ff09fde6a4a7bf3aa03f4b2a0cb1a4`. Les reconstructions effectuées depuis les deux commits exacts ont des empreintes ELF distinctes des archives et leur série ordinaire figure séparément.

QEMU 11.1.3, TCG, deux vCPU, 256 Mio, `-cpu max`, sans réseau. Sur x86_64, q35, Intel IOMMU et VirtIO-blk à une file. Le descripteur VMDK associe deux fichiers plats indépendants pour P9 et l’autorité logicielle. SHA-256 de la genèse x86_64/512 `5cbcae1d29e22aeadc555f58be13701e463f9fa26459b3543b2d560d2cb883d2`. Autorité initiale `fd8bfb2cd45908cce12d835c23e1c11bbc183c22c5ef9599325273aedd40eafe`. Chaque essai indépendant part d’une copie neuve. La reprise normale utilise les octets du parcours précédent. Les témoins de recul et d’autorité perdue remplacent seulement la composante prévue par leur scénario.

Les deux séries comparatives prévues comptent chacune quatre invités par révision, en deux vagues alternées. Les arguments réels, l’heure, la charge hôte et l’état mémoire de chaque vague sont archivés. Les essais avec interruption volontaire et les quatre essais exploratoires sont comptés à part. La charge des autres tâches macOS et le placement physique des vCPU lors des deux incidents de départ ne sont pas connus.

## Résultats comparatifs

Le numérateur compte les échecs du parcours complet. La colonne suivante compte uniquement les sorties 5 du service de stockage.

| Série x86_64/P9-512 | Témoin avant politiques | Candidat initial | Sorties 5 du stockage |
|---|---:|---:|---|
| Trois passages historiques sur la même ISO | — | 2/3 | 2/3 |
| ISO d’origine, quatre passages par révision | 0/4 | 0/4 | 0/4 sur chaque ISO |
| ISO avec sondes, quatre passages par révision | 0/4 | 0/4 | 0/4 sur chaque ISO |
| Reconstructions ordinaires des deux commits exacts, ordre A-B-B-A | 0/2 | 0/2 | 0/2 sur chaque ISO |
| Série exploratoire de quatre invités avant correction | — | 4/4, délai du provisionneur PID 7 | 0/4 |

Le témoin à réponse de création retardée de **3 500 ticks (14 secondes)** a échoué une fois sur une exécution avant correction, avec sortie 5, puis réussi une fois sur une exécution corrigée en 83,14 s, y compris les quatre collectes. Un second témoin attend le **retrait réel** du canal du vérificateur après décision durable. Sur le code fautif, le PID 3 sort avec le code 5 avant toute confirmation de lecture publique. Sur le correctif, dans chacun des quatre profils, le shell retrouve le répertoire `system` par `ls /`, retourne 0 et la relecture indépendante retrouve la génération 2. Cette injection termine l’invité depuis l’hôte après la preuve de disponibilité. Les scénarios nominaux vérifient séparément l’arrêt propre.

## Mémoire et durée du parcours historique

Temps hôte du démarrage au drainage des services et à l’arrêt SMP. Les différences de charge et d’ordonnancement limitent la comparaison de vitesse. Les quatre rôles et le stockage appartiennent à l’invité. Une page compte 4 096 octets.

| Invité / secteur | Avant, succès archivé | Après, succès | Pages résidentes admission → drainage, après |
|---|---:|---:|---:|
| AArch64 / 512 | 99,90 s | 94,05 s | 2 069 → 1 999 |
| AArch64 / 4 096 | 96,09 s | 63,74 s | 2 069 → 1 999 |
| x86_64 / 512 | 69,28 s | 76,23 s | 2 157 → 2 091 |
| x86_64 / 4 096 | 79,98 s | 54,12 s | 2 157 → 2 091 |

Sur AArch64, le vérificateur a 19 pages privées et les trois autres acteurs 18 chacun. Sur x86_64, chacun a 21 pages privées. Chaque acteur a également 64 pages de pile dans les deux versions. Le coût résident au drainage est identique sur les quatre couples avant/après ayant abouti. L’attente supplémentaire de deux notifications ne crée aucune racine IPC. Les chiffres d’admission x86_64 ont varié de 2 156 à 2 161 pages dans les archives. Les journaux du banc des services fournissent les compteurs d’allocation et de restitution. Le pic instantané lors des deux arrêts historiques n’a pas été échantillonné.

## Reproduction

1. Exporter le commit fonctionnel dans un répertoire de compilation neuf avec `python3 novaOS/user/novaauth/service/profile.py DESTINATION`. Construire le noyau avec le profil Cargo `novaauth-sessions-lab` et créer l’ISO `novaauth-sessions`. Pour le témoin de retrait utiliser un autre export avec `--reponse-retiree`. La sonde appartient uniquement à cet export.
2. Pour le parcours nominal lancer `python3 novaOS/user/novaauth/service/qemu.py ARCH SECTEUR ISO GENESE SORTIE`. Pour le témoin ajouter `--attendu reponse-retiree`. Le banc conserve l’image, son autorité, la série et le résultat dans un dossier neuf. Relire le témoin avec l’exemple Cargo `relire_interruption SORTIE SECTEUR` et vérifier la génération 2. Les références des genèses, de chaque ISO et des deux QEMU figurent dans les preuves.
3. Pour les politiques utiliser un troisième export avec `--politiques` et les clés de laboratoire, puis `policy_qemu.py` selon la [procédure de qualification](novaauth-politiques-v0.24.0-phase3.md). Les clés privées et les binaires de ce profil restent dans l’archive locale privée.

Les noyaux ordinaires sont identiques octet par octet à ceux de la base officielle. SHA-256 AArch64 `28218db8581cea32acbc4e4d2f96f4fa0018a72cf19c59d38adabc8b51e334ba`, x86_64 `9ad15bc241754993b004dd78bf6ead6540da75b4f70412ae681d6577f698004e`. Le diff des sources du noyau, de l’ABI et du moteur NovaFS entre le candidat initial et le correctif est vide.

## Qualification de l’arbre corrigé

| Invité et secteur P9 | Sessions historiques, reprise, recul, autorité absente | Politiques et révocations | Destinataire retiré, stockage public disponible | Services ordinaires | Pipes `cat \| wc` et `cat \| head` |
|---|---:|---:|---:|---:|---|
| AArch64 / 512 | 4/4 | 8/8 | 1/1 | 5/5 | 40 observations et 41 commandes |
| AArch64 / 4 096 | 4/4 | 8/8 | 1/1 | 5/5 | 40 observations et 41 commandes |
| x86_64 / 512 | 4/4 | 8/8 | 1/1 | 5/5 | 40 observations et 41 commandes |
| x86_64 / 4 096 | 4/4 | 8/8 | 1/1 | 5/5 | 40 observations et 41 commandes |

Les quatre répétitions finales x86_64/512 en concurrence ont réussi en **80,50, 81,42, 80,65 et 79,25 s**, soit **0/4 échec**. Le témoin de retrait réel provoquait **1/1 sortie 5** sur la version fautive. Il produit **0/4 sortie 5** sur la version corrigée. La relecture des quatre parcours historiques nominaux retrouve les deux écritures attendues et l’identité supprimée. Chaque banc `cat | head` relit aussi onze fichiers.

Les contrôles de contrat et de concurrence IPC, le cycle des 128 processus, les neuf contrôles ABI et les quatre ELF v0.20.0 vérifiés avant exécution ont réussi. Les suites hôte du registre (quatre tests) et des sessions (cinq tests) ont réussi sur ARM64 natif et x86_64 sous Rosetta. Les 48 sélecteurs ABI v1 et le format NovaFS v0.9.13 P9 sont conservés.

**Réserves.** La perte du destinataire est démontrée et couverte. Le calendrier interne des deux incidents originaux n’a pas été enregistré. Un très fort ralentissement peut encore provoquer un refus ou l’expiration d’un délai borné. Les durées hôte incluent le démarrage et les assertions du banc. La perte d’une réponse privée laisse l’issue de la mutation à vérifier dans l’autorité. Les essais concernent QEMU TCG à deux vCPU et des autorités de laboratoire. La campagne n’inclut ni UTM, ni matériel physique, ni coupure électrique réelle. L’intermittence historique du comparateur du pipe x86_64/4096 demeure suivie séparément. La décision de fusion appartient à la MOA/MOE.

**Preuves.** La copie intégrale, à accès restreint, se trouve sous `OS-worktrees/archives-novaauth-stabilisation-0240/qualification-04c43b3.tar.gz`. L’extrait de branche contient les résultats individuels, mesures, séries positives et négatives et le manifeste de la copie complète. Les 746 fichiers de la qualification précédente ont aussi été relus et comparés individuellement à leur archive. [Empreintes et nombre de membres](validation/novaauth-stabilisation-phase3-0240.json) · [Traces et mesures](validation/novaauth-stabilisation-phase3-0240.tar.gz)
