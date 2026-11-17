# NovaOS v0.24.0 — Qualification du raccord VFS privé et de l’amorçage QEMU

**Branche** `codex/lancement-generique-elf`. Les sources qualifiées incluent le diagnostic des racines, le correctif du chargeur `ad61c8bd76d0bb2a95daa198f7029f368cb801ba`, le raccord `9e4165f1850c59f0b6625d3845720cd6eeb1e60f` et sa sonde interprocessus `a04fcf5fbaec2deda594307db12a36548e8e8623`. Les mesures concernent QEMU 11.1.3 en TCG, sur Apple M4 Max, avec deux vCPU, 256 Mio, un volume P9 jetable et une autorité de laboratoire séparée. Les arguments QEMU, les empreintes des firmwares, des ISO et des volumes initiaux se trouvent dans `conditions.json`. [Diagnostic du passage de relais](limine-passage-relais-qemu-v0.24.0.md) · [Diagnostic initial des quotas](diagnostic-racines-ipc-0240.md)

## Passage de relais

Le correctif complète la carte directe au niveau de Limine avant le passage à EL1. La comparaison avant/après utilise le même noyau AArch64. Le contrôle de traduction du noyau, le moment de restitution des cadres et les protections de pages sont conservés. La sonde matérielle porte sur le premier mot de huit octets de la page physique. L’analyse des deux arbres de traduction, des piles et des réponses du chargeur complète l’observation de cette écriture.

| Contrôle | AArch64 P9/512 | AArch64 P9/4096 | x86_64 P9/512 | x86_64 P9/4096 |
| --- | --- | --- | --- | --- |
| Démarrage à deux vCPU, NovaFS et session authentifiée | réussi | réussi | réussi | réussi |
| Allocateur, tas et protections | 7/7 | 7/7 | 7/7 | 7/7 |
| Requête NovaAuth entre préparation et admission du lecteur | 3/3 | 3/3 | 3/3 | 3/3 |
| Pipes historiques, octets et statuts des deux processus | 3/3 | 3/3 | 3/3 | 3/3 |
| Quota, collision, refus partiel, expiration et révocation | 5/5 | 5/5 | 5/5 | 5/5 |
| Courtier nominal et refus d’authentification sans facteur | 2/2 | 2/2 | 2/2 | 2/2 |
| Admission pendant la validation par le processus NovaFS | réussi | réussi | réussi | réussi |
| Onze groupes ELF et redirections | 11/11 | 11/11 | 11/11 sans compteur exhaustif, 10/11 avec | 11/11 |

Les sept contrôles mémoire couvrent `mem-test`, `heap-test`, `mem-ro`, `mem-alias`, `mem-page-ro`, `mem-nx` et `mem-absent`. Le tas détecte la double libération et vérifie fragmentation, fusion et réutilisation sur 1 500 opérations. Pour les fautes attendues, le lecteur du banc historique a validé les journaux série originaux après normalisation CRLF. Sur AArch64, les instantanés avant/après drainage des services montrent zéro cadre conservé par le chargeur et 8 917 cadres restitués.

## Autorités et budget IPC

Le noyau lie chaque réservation à l’identité complète de l’instance du shell, à la session NovaAuth vivante, à la racine exacte du pipe et à l’image du producteur. NovaFS résout les objets du lot, vérifie le dossier autorisé et relit la politique de lecture pour l’installation attestée. La validation de NovaFS est acceptée par sa voie privée. Le producteur reçoit ses deux racines VFS avant sa première instruction. Les feuilles opposées sont remises à NovaFS. L’expiration, le retrait de session, le refus du lecteur et l’échec partiel suivent le même chemin de nettoyage. Le courtier n’ouvre aucun des fichiers de ce raccord.

| Acteur | Pic nominal de racines pendant les trois pipes | Témoin d’échec |
| --- | ---: | --- |
| `nsh` | 3 | 4 pendant le test de la cinquième demande, puis retour à 2 après fermeture du contrôle |
| NovaFS | 4 | 4 |
| NovaAuth | 4 | 4 |
| Courtier | 4 | 4 |
| Producteur | 2 | 2 puis 0 |
| Lecteur | 2 | lecteur absent si son admission échoue |

Les journaux détaillent chaque création et retrait. Le refus `LIMIT=6` laisse strictement identiques les trois compteurs, soit 43 canaux, 84 nœuds et 10 handles locaux au shell au moment du refus. Le premier appel de chacune des cinq épreuves retrouve 41 canaux, 82 nœuds et 8 handles au shell. Après l’arrêt ordonné, chaque PID observé revient à zéro racine. Les reçus de fin du producteur retiré se collectent une fois. Ces mesures portent sur les parcours exercés.

L’export supplémentaire `profile.py --selection-smp DESTINATION` insère une sonde dans les deux acteurs. Pendant la demande VFS en vol, `nsh` présente la réservation. Une présentation avant validation est refusée, puis un seul PID est publié. NovaFS ne peut ni utiliser la réservation comme lanceur, ni valider deux fois la sélection. La sonde vérifie aussi que le shell conserve son statut de titulaire unique. Deux threads hôtes sollicitant simultanément la même réservation n’obtiennent qu’une consommation. Les résultats du test d’état et les quatre séries SMP en invité sont conservés séparément.

## Octets, statuts et compatibilité

Sur chacun des quatre profils, `cat donnees.tsv | wc -l` a produit `32 0a` sur un fichier contenant deux LF. Le shell a collecté deux PID distincts, aux statuts 0/0, et a retourné 0. Les deux essais `cat | head -n 1` ont produit exactement `41 09 42 09 43 0a`, avec stderr vide. Sur l’entrée courte les statuts sont 0/0. Sur 64 Kio, la fermeture interrompt l’écriture du producteur, ses statuts sont 11/0 et le pipeline retourne 0. Chaque admission a reçu une réponse NovaAuth indépendante.

Les onze groupes ELF couvrent les quatre formes de `cut`, les pipes entre ELF, l’identité de l’auteur des ouvertures, les arguments depuis script et `nsh -c`, les redirections combinées, les droits, les octets binaires, la syntaxe, les tickets, les redirections sur cibles neuves et l’installation à chaud. La géométrie et l’ISO de chaque résultat figurent dans le bilan machine.

Sur l’ISO à trace exhaustive, 51 des 52 groupes passent au premier passage. L’installation à chaud sur x86_64/P9-512 publie le témoin et réussit le pipe. Le dernier lancement par nom est refusé. Les deux répétitions horodatées atteignent ce refus à 65,631 et 67,948 s depuis le démarrage de QEMU. Deux autres essais avec les vCPU en QoS User Initiated et un essai démarré sans autre QEMU actif reproduisent le refus. Le dernier essai a partagé une partie de son exécution avec un témoin ultérieur. La limite de session de 60 s est demeurée active.

Sur cette même géométrie, les onze groupes passent avec le comptage exhaustif désactivé. L’installation à chaud a terminé en 55,705 et 56,171 s. Dans cette première reconstruction faite dans un autre répertoire, le noyau et l’image du service de stockage ont des empreintes différentes. Un témoin supplémentaire reconstruit au même emplacement que l’ISO instrumentée élimine ce biais. Les octets de tous les ELF et services extraits correspondent, seul le noyau change avec l’option de traçage. Les deux essais à chaud terminent en 57,781 et 58,819 s, sans renouvellement de session. La trace exhaustive reste une condition défavorable reproductible du laboratoire. Les chronologies sont des temps écoulés côté hôte.

Le registre ABI v1 conserve 48 sélecteurs. Les neuf contrôles passent sur les sources ordinaires et dans l’export de qualification. Les ELF historiques `sdk-example` et `threads-test` de chaque architecture ont été vérifiés par rapport aux SHA-256 v0.20.0 et exécutés sans recompilation des applications. Les quatre témoins réussissent à la fois dans le profil historique et dans un nouvel export cumulant `thread-probe` et le raccord QEMU. Les journaux de threads vérifient le retour au nombre initial de cadres et la restitution des TCB. Le noyau ordinaire reconstruit garde les SHA-256 `17ab08f45b11cd5bb089a2d384a2df0c95e5c123e013f15dc475d85a44d395e2` sur AArch64 et `32bad229fee23fc3c9e4f74b2ae20518f414a2e38934d6c71102138b4e6b94ca` sur x86_64. Le format NovaFS P9 et les interfaces ordinaires du lanceur sont conservés.

## Essais intermédiaires et conditions

L’archive comprend la panique AArch64 d’origine et les essais infructueux. Un premier témoin d’expiration confondait millisecondes et ticks. L’attente de 2 600 ticks à 250 Hz exerce maintenant les 10 secondes de la réservation à l’intérieur de la session de 60 secondes. Une première sonde SMP a constaté le refus de créer un thread dans le shell supervisé. La sonde interprocessus suivante utilisait un tag VFS hors séquence et la requête suivante échouait. La sonde finale réserve le tag dans le client avant l’envoi. Ces corrections touchent les témoins de laboratoire. Les premières traces de refus, d’expiration et de redirection sont conservées avec leurs causes établies ou leurs limites d’interprétation.

La validation couvre les scénarios QEMU ci-dessus. Elle ne qualifie ni une installation UTM, ni un firmware physique, ni une autorité NovaAuth de production. Une éventuelle mise à jour du Limine distribué ou une généralisation du raccord privé exige un arbitrage séparé.

## Preuves

Le répertoire `OS-worktrees/preuves-lancement-generique-elf/selection-privee-v_5rasw6/` contient les données brutes, `conditions.json`, `bilan-qualification.json`, les cartes mémoire, les chronologies, les sondes et les ELF de référence. Les 1 478 fichiers du manifeste ont été relus après décompression et comparés à leur original. Le manifeste lui-même a également été vérifié. [Manifeste individuel de qualification](validation/limine-raccord-0240.sha256)

La copie durable `OS-worktrees/archives-lancement-generique-elf/limine-raccord-a04fcf5.tar.gz` occupe 174 682 297 octets. SHA-256 de l’archive `43eab20a8ac7ab331c8c0c20258180d7366b199618ad0c69fb7f841f3195a70f`. SHA-256 du manifeste `abcdb0b04488c724e40179aa8b199280e1d8ea7055683b028bdd01ecb45f39e8`. Les fichiers P9 et les autorités contenus dans cette archive sont ceux des invités de qualification.
