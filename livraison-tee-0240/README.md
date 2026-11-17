# tee natif · ARC-027

**17 novembre 2026.** Mach-O Darwin ARM64 et ELF statiques NovaOS AArch64 et x86_64. Les 191 comparaisons hôtes avec GNU tee 9.12 réussissent. Les quatre profils QEMU ont exécuté les ELF exacts, le témoin du moteur avec une destination P9 et les consultations documentaires du vrai shell. Le raccord métier de `cat entree | tee sortie` reste ouvert.

La liaison Darwin élimine désormais le code inutilisé. Sur chacune des six charges de 1 à 32 Mio, le Mach-O livré respecte la limite de 1,10 × la médiane GNU dans deux campagnes indépendantes. Les fichiers de `preuves/` et l’archive décrivent la livraison initiale. [Rapport de performance et mesures brutes du binaire corrigé](performance-darwin/README.md)

| Référence | Valeur |
|---|---|
| Base vérifiée sur `origin/main` au départ | `425e4e7cc5c03e246efb64d3638376906344bf47` |
| Sources, binaires et bancs de la livraison initiale | `18818d4d8a8b345d467a46d00daa9cab343ef306` |
| Branche | `codex/tee-natif-0240` |
| Version affichée | `nova-cli 0.20.0`, issue de `components/Cargo.toml` |
| [Mach-O ARM64](bin/tee) | `fc23a98542fe39e98ab7aae9ff61dbaa5421abd74fa501e94d2234ac7e566bc9` |
| [ELF AArch64](bin/tee-aarch64.elf) | `ddaf701f1e995e084178172b83ec031b1ad4a196f5ad6808604295d210add2d9` |
| [ELF x86_64](bin/tee-x86_64.elf) | `352d592a70966d1b4307a163ec840050cca4ea57b937b599214b0d3c901dbb12` |

[Recette et résultats par option](MATRICE.md) · [Instructions de reconstruction et de vérification](RECONSTRUIRE.md) · [Script fonctionnel](tester-options-tee.sh) · [Script de performances](mesurer-performances-tee.sh) · [Archive des preuves](preuves-tee.tar.xz) · [SHA-256](SHA256SUMS)

## Références et choix de mise en œuvre

L’analyse part de `src/tee.c` au tag GNU v9.12, commit `c0f8514d989184921d9b12a4d103a7b23abc5af8`. Le parseur distingue argument facultatif attaché et opérande suivant. Le traitement des erreurs à l’ouverture peut arrêter avant la lecture. L’ordre d’écriture commence par stdout. La disparition de la dernière sortie en mode nopipe peut arrêter une entrée encore silencieuse. `src/iopoll.c` explique également l’attente sur EAGAIN et la reprise après une écriture courte. Le manuel texinfo, `man/tee.x` et les quatre suites de `tests/tee/` sont inventoriés dans les preuves. La copie gnulib de travail pointe vers une autre révision. Les auxiliaires externes argmatch, closeout et init.sh ont été lus dans l’objet Git exactement épinglé, `106e9b2384d08a1696fcbd40cbab52237943f208`. Le comparateur installé est la bouteille Homebrew 9.12_1, SHA-256 `d9420927d43bc432761195ea7ae1ecfbc048ed6c09cef6da366e5bddef8624a9`. [Source GNU](https://github.com/coreutils/coreutils/blob/c0f8514d989184921d9b12a4d103a7b23abc5af8/src/tee.c) · [Empreintes des sources consultées](preuves/references.json)

L’évaluation uutils porte sur `4735bbe40d764a523c5ece37d47465b32452d1a2`. Le code MIT de uu_tee utilise std, clap, fluent et uucore, avec rustix pour les descripteurs. Son parseur accepte également `-h` et donne la priorité au mode long explicite sur `-p`. Ces choix modifieraient deux résultats GNU. Le port emploie un moteur Rust original sous MIT pour les deux systèmes, un adaptateur Darwin pour les signaux et descripteurs, et l’API de capacités existante sur NovaOS. Le lien GNU reste une référence documentaire. [Sources uutils examinées](https://github.com/uutils/coreutils/tree/4735bbe40d764a523c5ece37d47465b32452d1a2/src/uu/tee) · [Moteur](../components/coreutils/tee/engine.rs) · [Notices](licences)

## Ce qui a tourné

| Niveau | Résultat |
|---|---|
| Darwin ARM64 | 191/191 comparaisons. SIGINT réel, SIGPIPE hérité ou ignoré, pipe sans lecteur, EAGAIN, écriture partielle sous RLIMIT_FSIZE, erreurs de lecture et d’ouverture. Un premier essai avait révélé un faux échec en fermant stdout déjà fermé sur entrée vide. La fermeture tient maintenant compte du EBADF dans ce cas et la recette le reproduit. |
| Bancs amont sur les deux exécutables | `append.sh` et `tee.sh` réussissent. `short-write.sh` et `write-eagain.sh` rendent 77 sur Darwin, où leur injection dépend de strace. [Journaux](preuves/amont.txt) |
| QEMU TCG, deux vCPU et 256 Mio | AArch64 et x86_64, chacune en P9/512 et P9/4096. Douze démarrages de qualification. Dans chaque profil, le banc SDK vérifie l’inclusion octet pour octet de l’ELF livré dans le noyau extrait de l’ISO, exerce 148 argv dans des processus isolés et contrôle la collecte. Le disque du scénario SDK est en lecture seule. |
| Moteur et P9 | Le témoin dans le cp de recette reçoit le fichier source et une destination par les délégations ordinaires de cp. Il charge le corpus, alimente le moteur, capture stdout et le miroir, et appelle l’adaptateur P9 du port. Il vérifie 33 cas sur chacun de trois appels, dont un corpus vide. Les fichiers créés de 0 et 1 030 octets et le contenu initial du nom occupé sont relus indépendamment sur l’image finale. |
| Vrai nsh | Pour fr-CA, en-US et fr--CA, aide, version, alias et manuel réussissent. Les octets du volume et les compteurs QMP d’écriture et de FLUSH sont stables pendant ces 18 consultations. Le pipeline demandé rend 3, la destination est absente et le témoin historique `cat entree \| wc -l` rend 5 lignes. |
| Régressions | Neuf tests du registre ABI réussis. `make -C novaOS ARCH=… nsh-scripts-test cli-p9-test` réussit sur les deux architectures, les deux géométries et après redémarrage. Les bancs concernés sont ceux du shell et du stockage CLI. Les profils matériels UTM et les essais de panne électrique se situent hors de cette qualification TCG. |

## Capacités P9 et raccord à reprendre

Le service exige TRAVERSE, ADD et FILE_WRITE pour sélectionner un nom libre avec SELECT_NEW. Dans le GROUP, CREATE_NEW rend un handle WRITABLE, PUBLISH_COPY rend le fichier visible dès l’ouverture, WRITE + SYNC transfèrent les octets par offset puis CLOSE restitue le handle. Les noms sont résolus par le délégant. Le client travaille uniquement sur ses slots sélectionnés, sans droit de parcours global, d’effacement ou de remplacement. Le témoin exerce le même adaptateur sur le slot 2 de cp. L’ELF attend les destinations dans l’ordre des opérandes à partir du slot 1. La qualification physique du témoin couvre une destination P9 à la fois. [Sélection côté service](../novaOS/user/novafs/files/selected.rs) · [Adaptateur de tee](../components/coreutils/tee/native_io.rs)

Sur nom occupé, CREATE_NEW renvoie EXISTS. L’ajout atomique à l’extrémité courante et la troncature sous capacité nommée attendent un contrat de stockage. Le port refuse donc ces destinations, conserve les octets existants et applique la politique d’erreur demandée aux autres sorties. La console native présente les contrôles et les octets invalides sous forme échappée. Les fichiers P9 et un canal de sortie binaire reçoivent les octets bruts. L’ABI ne fournit ni SIGINT POSIX, ni attente combinée entrée/fermeture équivalente à poll sur Darwin.

Le main de départ limite le pipeline aux lecteurs wc et head. Le travail parallèle de lancement générique à `0bd8d679ea3b1fade9789557f506e9b66be099cd` qualifie le retrait du producteur et distingue encore le raccord au shell. La branche tee conserve le lanceur, les rôles, l’ABI v1 et le format P9. Pour la réception de bout en bout, le lanceur générique devra admettre l’ELF livré, lui remettre RECEIVE et les seules destinations nommées, puis exposer une sortie binaire sélectionnée lorsqu’elle existe. Rejouer alors les tests d’ordre, révocation, lecteur fermé, sorties multiples et restitution depuis le vrai nsh. [Parseur du shell](../components/shell/parser.rs) · [Admission sur la base](../novaOS/kernel/src/sched/process/ipc/spawn.rs)

## Profil court de la livraison initiale

Médianes Darwin en millisecondes, trois passages après échauffement, ordre alterné. Le chronométrage inclut le démarrage et la terminaison du programme. Les deux exécutables utilisent le même corpus binaire et les mêmes chemins. Les SHA-256 de toutes les destinations sont vérifiés après chaque mesure. Les fichiers sont en cache et aucune persistance par fsync n’est demandée.

| Entrée | Sorties et volume total écrit | Nova tee | GNU tee |
|---|---|---:|---:|
| 1 Mio | 1 sortie, 1 Mio | 8,020 ms | 6,721 ms |
| 1 Mio | 4 sorties, 4 Mio | 19,941 ms | 17,030 ms |
| 8 Mio | 1 sortie, 8 Mio | 34,948 ms | 35,907 ms |
| 8 Mio | 4 sorties, 32 Mio | 209,324 ms | 65,938 ms |

Pics RSS maximaux Darwin hors échauffement, 2 195 456 octets pour Nova et 2 064 384 pour GNU. Trois processus QEMU AArch64, Cargo et rustc étaient présents au début, quatre QEMU à la fin. La dispersion de cette campagne, notamment à quatre sorties, limite la portée des écarts. Les durées de chaque processus, RSS, débits d’entrée et de sortie figurent dans les données brutes. [Mesures](preuves/mesures.json) · [Tableau terminal](preuves/performances.txt)

Le témoin invité mesure également 1 030 octets en entrée depuis la mémoire. Une sortie de capture écrit 1 030 octets. La capture et une destination P9 synchrone écrivent 2 060 octets. La mesure va d’un marqueur série à l’autre, inclut l’ouverture et la fermeture P9, et exclut la validation. La réception série limite la précision des mesures courtes. Les valeurs suivantes sont en ms, avec trois passages après échauffement. Ce profil ne mesure pas l’admission de tee par nsh ni une exécution GNU dans l’invité.

| Invité | Capture seule | Capture + P9 |
|---|---:|---:|
| AArch64 / 512 | 0,456 | 74,448 |
| AArch64 / 4096 | 0,443 | 67,421 |
| x86_64 / 512 | 0,485 | 58,376 |
| x86_64 / 4096 | 0,481 | 58,254 |
