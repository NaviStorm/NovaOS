# Arbitrage MOA/MOE · Admission de cat | awk

**État du 16 novembre 2026.** Le noyau reste gelé. La base examinée est `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`. La partie à décider concerne l’identité du lecteur au lancement. Le transport IPC existant suffit pour acheminer les octets.

## Fermeture, lecture et révocation

| Événement | Effet observé dans le code de la base | Contrat pour awk |
|---|---|---|
| Le producteur ferme SEND ou meurt | `pipe_close` pose `writer_closed`. La sélection de tête sert les dépôts FIFO avant EndOfStream. EOF est une réponse de type 2, longueur 0, aux=1, handle nul. | Terminer le dernier enregistrement même sans LF, exécuter ENDFILE et END, vérifier la sortie. |
| Le lecteur ferme RECEIVE ou meurt | `reader_closed` fait échouer un nouvel envoi et réveille l’offre SEND suspendue avec CLOSED (11). Les trames déjà admises restent comptabilisées. | Si le programme cesse de lire, libérer l’extrémité. Un exit explicite garde son statut et exécute END selon le langage. |
| Le propriétaire révoque | `revoke` marque le pipe, coupe les descendants, purge puis appelle `sweep`. Une attente sur un nœud retiré reçoit REVOKED (10). Un endpoint détruit, un lecteur déjà fermé et une échéance ont leurs propres causes. | Propager l’erreur de transport en conservant sa cause. Une lecture partielle suivie d’erreur ne doit pas être traitée comme un dernier enregistrement réussi. |
| Fin du pipeline | `nsh` collecte une seule fois chaque droit WAIT, ferme le contrôleur et retire la délégation fichier. En cas d’échec de la seconde admission, retrait avant l’attente du producteur. | Rendre le statut d’awk et archiver séparément celui de cat, y compris pour une sortie anticipée normale du lecteur. |

Le tampon du transport contient huit trames de 128 octets et une offre suspendue au plus. Le tampon des enregistrements appartient à l’application. Une ligne plus longue que la file doit pouvoir avancer. La révocation ne retire pas rétroactivement les octets déjà copiés en espace utilisateur. [Fermeture et disparition](../../novaOS/kernel/src/sched/process/ipc/mod.rs) · [FIFO et EOF](../../novaOS/kernel/src/sched/process/ipc/traffic.rs) · [Contrôleur et directions](../../novaOS/kernel/src/sched/process/ipc/pipe.rs) · [Collecte](../shell/executor.rs)

## Solution en espace utilisateur et limite vérifiée

L’adaptateur lecteur peut alimenter le même moteur AWK à partir de `process::pipe_read`, assembler les enregistrements à cheval sur deux trames et appeler IPC_CLOSE sur sa seule extrémité. Le shell conserve le contrôleur. Le vrai ELF cat conserve seul la lecture du fichier. Le vrai ELF awk reçoit seulement RECEIVE et les deux sorties console. Ce scénario de pipe exclut les opérandes fichier, les sources -f/-i, les redirections dynamiques et l’exécution de commandes externes.

Ce chemin nécessite que le lanceur ait installé RECEIVE dans le processus awk. Le lancement ordinaire ne crée pas cette liaison. Le contrôleur est MANAGE seul, les extrémités n’ont pas GRANT. Le shell ne peut donc pas accomplir cette installation avec ses opérations utilisateur. Les variantes de SPAWN actuelles choisissent ensemble le rôle **et** l’image immuable (`role as u64`). Passer la variante de wc ou de head exécuterait cet autre ELF. Alimenter awk via un fichier intermédiaire ne qualifierait ni la contre-pression, ni la fermeture du lecteur, ni la révocation du vrai flux. [Bibliothèque utilisateur](../libraries/process.rs) · [Contrôle avant et après admission](../../novaOS/kernel/src/sched/process/ipc/spawn.rs)

## Modification minimale soumise, non appliquée

Le [diff proposé](proposal/admission-lecteur.diff) ajoute un rôle et une valeur d’argument du SPAWN existant, puis restreint la préparation du processus à ce rôle précis. Son application suppose une décision sur ce contrat de lancement, même si le nombre de sélecteurs resterait 48. Réserver d’abord l’indice ordinaire 17 à la véritable image awk et une identité d’installation distincte. Vérifier les assertions d’indice au moment de construire le catalogue. Les indices 15 et 16 désignent déjà le témoin SDK et SAMPLE. Insérer simplement awk dans la boucle COMMANDS décalerait ces deux entrées.

Conserver le contrôle du propriétaire et de la racine MANAGE, le PID du producteur lié, l’unicité du lecteur, la validation sous la garde après l’admission, les deux droits WAIT et la destruction du processus non publié en cas d’échec. Le nouveau rôle emprunterait le chemin lecteur existant, ports OUTPUT|ERROR, canaux fichier nuls, extrémité RECEIVE en configuration 2. Aucune extension des sondes réservées à head, aucun droit SEND, GRANT, MANAGE ou clavier. Le consommateur valide le programme avant tout effet métier et rend explicitement les erreurs d’allocation et d’E/S. Le coût de l’interprétation nécessite de mesurer les ressources du processus indépendamment de la file du noyau.

Le chemin ordinaire appelle `principal_in`, qui prépare un bail `tmp` par `new_lease`. `launch` active les neuf appels de threads, et l’installation du terminal recopie normalement `QUALIFY` depuis le parent. La seule inscription du rôle accorderait donc plus de droits que prévu. Le diff conserve la décision d’admission du gestionnaire et refuse une politique avec travailleurs. Il utilise ensuite la création sous garde sans bail, désactive les appels de threads avant le premier quantum et installe strictement les ports autorisés. Le parent peut tracer l’essai sans transmettre sa capacité de qualification. La valeur ordinaire 17 est refusée tant que le contrat fichier séparé ne la couvre pas. Ces restrictions ne doivent pas être étendues aux anciennes commandes. [Bail temporaire](../../novaOS/kernel/src/sched/process/tmp.rs) · [Porte des threads](../../novaOS/kernel/src/sched/process/threads.rs)

Le diff se relit avec `git apply --check components/awk/proposal/admission-lecteur.diff` depuis la racine. Il suppose la réservation du nouvel indice à l’image correcte, la mise à jour du manifeste de supervision et la construction du véritable ELF. Avant tout lancement, vérifier aussi la compatibilité de l’allocateur du binaire mono-thread avec le refus de THREAD_TLS. Si le démarrage exige plusieurs threads ou un bail, la proposition doit revenir en arbitrage.

Une seconde décision serait nécessaire pour combiner fichiers et pipe dans un même lancement. La présente demande se limite à un producteur cat et un lecteur awk au premier plan. L’admission ordinaire sur fichiers nécessite aussi une identité de catalogue stable, un plan de délégation nominatif par le shell et une entrée documentaire ARC-026 sans préparation d’espace temporaire métier.

## Qualification à exiger avant intégration

Quatre profils QEMU TCG, AArch64 et x86_64, chacun sur P9/512 et P9/4096, deux vCPU et 256 Mio. Employer des images jetables. Rejouer les témoins historiques cat|wc et cat|head sur toutes les géométries, vérifier les 48 sélecteurs, les quatre ELF historiques, les numéros de catalogue et les droits réels. Les mesures reprises d’une livraison antérieure sont à identifier séparément.

1. Comparer sortie, stderr, statuts et octets réellement consommés pour flux vide, NUL, UTF-8 coupé, E/S multi-trames, dernière ligne sans LF et gros flux synthétique. Exercer un tour complet de file et la pression sur un producteur suspendu.
2. Observer `{print $1; exit}` avec cat encore bloqué puis avec cat déjà terminé. Vérifier CLOSED ou succès de cat suivant l’ordre imposé, succès du lecteur, collecte unique et sortie END. Exercer un échec de la sortie console.
3. Révoquer avant la première lecture, pendant RECEIVE et pendant SEND. Enregistrer l’ordre réel et distinguer REVOKED, CLOSED, EOF et délai. Contrôler le refus d’un lecteur supplémentaire et de canaux fichier sur le lecteur de pipe.
4. Comparer le profil ordinaire et le profil QUALIFY du parent. Sonder le refus du bail TMP, de THREAD_CREATE, de QUALIFY et du lancement par numéro ordinaire 17. Vérifier que le disque conserve son empreinte et que le gestionnaire ne laisse aucun moniteur ni bail en cas d’admission refusée.
5. Sur le chemin fichier séparé, tester une sélection autorisée, un voisin surprotégé, une délégation insuffisante et son retrait en cours de lecture. Relire le voisin hors invité. Comparer aide, version et manuel FR/EN, compteurs et SHA du disque avant/après consultation.
6. Archiver noyau, ELF, arguments, traces PID/droits, pics de pages, nombre d’octets, résultats différentiels et SHA-256. Après collecte, vérifier la restitution des ressources et l’absence de données résiduelles au pipeline suivant.

## Dépendances de la stabilisation du 16 novembre

Le dépôt distant a depuis publié `e5d7124a53e9b0d6a896b7001f717f655c371367` (arbre `e73ce9e2209449bda3e1e1475ac15053ac3e312e`). La comparaison avec la base du worktree ne montre aucun changement des trois fichiers visés par le diff. Elle modifie le service utilisateur de fichiers, la pagination du répertoire et les tests de lecture P9. Avant une intégration autorisée sur cette base, rejouer les lectures successives et la dernière lecture courte avec conservation des handles, vérifier les refus et retraits sur les quatre profils et rejouer les tests de stabilisation correspondants. Les preuves de cette branche portent sur sa propre base.

## Rejeux réalisés sur le noyau gelé

Base de développement `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`, puis commit d’étude `e79e2919803e59b1b50b490fb3231c6e62b11bb0`. Les sources système des deux arbres sont identiques. Chaque profil démarre le noyau de cette base avec deux vCPU, 256 Mio et son propre disque P9. Pour les deux architectures et les deux tailles de secteur, le banc head termine ses 41 commandes et relit les 11 fichiers hors invité. Le banc wc termine ses 40 observations. Les quatre journaux de fin de chaque banc, les résultats par commande, les UART, les ISO, les disques et leur géométrie figurent dans [l’archive des rejeux](lab/results/temoins-qemu.tar.xz). Son manifeste interne et la relecture des 143 membres permettent de vérifier les originaux.

Le registre a réussi ses neuf contrôles, dont les 48 sélecteurs et la régénération. Les ELF historiques sdk-example et threads-test ont aussi été réellement relancés sur les deux architectures, avec vérification des empreintes d’origine et de la restitution. [Rapports individuels et journaux](lab/results/abi)

Ces témoins vérifient notamment la pression, le drainage, le réveil après fermeture et le retrait forcé. Pour awk, seul le véritable pipeline Darwin a été exécuté. L’exécution du véritable lecteur NovaOS, le parcours de fichiers sous sa délégation, la mesure de sa mémoire en pages et la révocation de ses propres droits restent à qualifier. Les fichiers synthétiques du témoin head vont jusqu’à 65 536 octets. Ce nombre mesure le fichier exercé et ne constitue pas une limite du format P9. Les deux profils de géométrie utilisent des volumes de laboratoire de 2 Mio.

### Reproduction dans le worktree dédié

Exporter NOVAOS_TMPDIR et TMPDIR conformément au dépôt. Utiliser un nouveau dossier de rejeu et les originaux des ELF historiques identifiés par les JSON. Les commandes ci-dessous démarrent de vraies VM. Le dossier d’arbitrage contient en plus une copie de chaque ELF historique.

~~~sh
set -e
preuves="$PWD/novaOS/build/rejeu-awk-pipe"
mkdir -p "$preuves"
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
python3 -B novaOS/scripts/generate_abi.py --check
python3 -B novaOS/tests/abi_registre.py -v
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel
    for secteur in 512 4096; do
        python3 -B novaOS/tests/pipe_head.py "$arch" "$secteur" "$preuves/head-$arch-$secteur"
        python3 -B novaOS/tests/pipe_natif.py "$arch" "$preuves/wc-$arch-$secteur" --sector "$secteur" --cpus 2
    done
    for profil in sdk-example threads-test; do
        python3 -B novaOS/tests/abi_elf_archive.py "$arch" "$profil" "../preuves-cloture-0220/elf/$arch-$profil/original.elf"
    done
done
~~~
