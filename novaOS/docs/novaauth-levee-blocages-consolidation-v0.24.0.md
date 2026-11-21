# NovaAuth v0.24.0 — Levée des blocages de consolidation

Référence de diagnostic : candidat local b08ba80f6edbd49f06fa7ad28a1810fd2157cd55. Reprise des corrections sur codex/novaauth-autorite-0240, sans publication de main ni activation ordinaire.

## Diagnostic original

Les huit échecs sont détaillés, avec événements natifs, échéances et décisions durables, dans [la matrice de diagnostic](validation/novaauth-huit-echecs-b08ba80.json). Les 1 800 preuves originales sont conservées et leur archive a été relue. Deux échecs relèvent du banc attendant une création malgré une annulation durable, quatre montrent une expiration TLS initiale avec refus sûr, un montre un défi expiré avant ticket, et un reste indéterminé avant admission. La charge hôte n'est pas une cause établie pour chacun.

## Corrections ciblées du banc

Après perte du reçu d'admission, exiger l'issue terminale native puis le reçu durable : enfant identifié et collecté, ou annulation irréversible sans enfant. La mutation n'est jamais répétée ; une consultation retrouve son issue. Continuer ensuite jusqu'au retrait définitif et au lancement B. Les créations retardées et les réponses terminales perdues restent des témoins distincts. Une expiration de délai n'est jamais une preuve d'annulation.

Après refus explicite d'un défi expiré, autoriser une seule nouvelle commande d'authentification dans la même instance. Aucun renouvellement implicite de session, aucun réemploi du défi ni augmentation des délais TLS (10 s) ou de défi (1,5 s).

Le refus après redémarrage doit aussi démontrer la restitution des services avant arrêt de la VM. Les mesures QEMU du noyau relèvent le shell, le registre et le signataire avant, pendant et après l'opération. Elles n'attribuent aucun droit nouveau.

## Référence de ressources

Exiger le retour des racines, handles, endpoints, nœuds, processus, threads et droits de collecte aux valeurs initiales. Les pages propres des trois acteurs doivent revenir à leur référence. Distinguer les attentes propres, bornées par les threads, des attentes globales variant avec les services indépendants. Le pool IPC peut acquérir un premier cadre. Après l’arbitrage mémoire ci-dessous, les deuxième et troisième lancements B doivent démontrer un plateau exact, prolongé par trois cycles supplémentaires, y compris pour les cadres libres. Toute différence inexpliquée reste bloquante. La réserve historique instrumentée AArch64 de 301 cadres reste séparée et ouverte.

## Qualification requise

Rejeu ciblé puis campagne sur AArch64/x86_64 et P9/512/P9/4096 ; deux VM, ordre durable, B bloquée pendant RETRAIT_EN_COURS, retrait définitif après issue démontrée, anciens tickets refusés avant expiration. Rejouer la récupération 2-sur-2, les refus VFS et les accès légitimes, les neuf contrôles ABI et les quatre ELF historiques. Conserver les résultats négatifs et les compteurs manquants comme tels. Ce document ne constate pas encore la réussite de cette qualification.

## Arbitrage mémoire complémentaire

La MOA/MOE autorise trois lancements B, le plateau exact entre deuxième et troisième, puis plusieurs cycles supplémentaires. Le banc en prévoit six : les trois derniers commencent chacun par une réauthentification explicitement demandée, entre transactions, sans prolongation du bail précédent. Les mesures avant/pendant/après chaque lancement et les anciennes traces restent conservées. Le contrôle inclut désormais les pages réellement attribuées/rendues par HEAP_RESIZE dans NovaFS, les octets et blocs vivants, le cache sous son budget historique de 1 065 472 octets et la réutilisation sans attribution physique à partir du troisième chargement. Les octets résidents non demandés comprennent aussi les métadonnées dlmalloc : ils ne sont jamais présentés comme physiquement libérés. Toute différence inexpliquée reste bloquante.

Sur le diagnostic x86_64/P9-4096, les allocations vivantes supplémentaires correspondent exactement à l'augmentation du cache (+81 920 puis +8 192 octets). Les fins de chargement passent de 149 368 à 145 224 octets vivants, tandis que NovaFS conserve 32 cadres supplémentaires. Le troisième lancement retrouve les cadres libres du deuxième. Ces observations motivent le nouvel oracle ; elles ne remplacent pas sa qualification multiarchitecture ni les cycles supplémentaires.

## Corrections des traces VFS

Le parcours enrichi émettait dix-sept messages DEBUG depuis le lanceur, pour une limite inchangée de seize : le marqueur de fin était refusé. Le dernier statut et le marqueur de fin sont maintenant regroupés dans un même message inférieur à 256 octets. Le parseur confondait aussi le préfixe « lecture L » avec « lecture L avant P » ; il exige désormais que l'intitulé soit immédiatement suivi de son résultat hexadécimal. Les assertions de lecture, refus et ordre L/P/L restent intégrales.

## Drainage après refus de reprise du profil EXEC

La trace instrumentée du 21 novembre montre END acquitté, le retour de l’opération TLS et l’entrée dans Wire::drop, puis le retrait du superviseur. Contrairement au lanceur VFS, exec_admission_launcher renvoie REFUSED au refus du registre ; service_manager termine et service::retired retire les services de sa famille avec Exit(125). Attendre un marqueur utilisateur du signataire retiré était une erreur de protocole du banc. Une observation diagnostique de 45 secondes ne la résout pas ; NOVA_TICKS continue d’avancer (4 529 puis 5 779 sur cinq secondes), les deux CPU sont dans sched::idle.

Le contrôle corrigé conserve le refus de l’ancienne référence et l’absence d’admission. Il exige en complément les traces noyau du retrait contraint et de la restitution après libération de chaque espace/pile : racines, handles, attentes, pages et threads à zéro pour chaque service retiré, puis l’attribution de tous les endpoints, nœuds et espaces restants aux seuls processus persistants identifiés avant le refus. Le premier rejeu de cette mesure confirme la restitution des services 17/22/23, mais révèle une assertion erronée exigeant zéro processus global : quatre processus de base restent légitimement présents (9 endpoints et 16 nœuds). Le contrôle vérifie désormais leur identité exacte et leur propriété des ressources, sans assimiler leur présence à une fuite. Ces sondes sont ajoutées uniquement par l’export QEMU. Le profil VFS conserve son contrôle distinct de fermeture TLS normale. Les résultats précédents restent des échecs, avec leurs traces ; la réussite du nouveau contrôle reste à qualifier.

L’examen a également trouvé une incohérence indépendante : canal::guard fournit des millisecondes, transmises sans conversion à des attentes IPC en ticks de 4 ms. Elle ne démontre pas la cause des huit échecs initiaux ni celle de ce retrait contraint. La conversion privée du signataire utilise désormais les ticks restants arrondis vers le bas, sans renouveler l’échéance ; les contrôles absolus avant/après les échanges restent présents. Les tests natifs couvrent bornes, expiration et dépassements. Sa qualification QEMU reste requise séparément, sans augmenter le délai TLS.

## Réutilisation des dernières pages retenues

La sonde QEMU relève désormais les allocations qui recouvrent la dernière plage physique attribuée au tas NovaFS, avant et après chaque chargement. Aucune adresse n’est publiée. À partir du troisième lancement, l’oracle exige simultanément zéro nouvelle attribution physique et une augmentation des allocations et octets servis dans cette plage ; le seul nombre global d’appels malloc ne suffit plus. Les compteurs sont encodés dans NXMEM4 (254 octets, dix-neuf compteurs de 48 bits) ; tout dépassement produit un diagnostic d’erreur et invalide la mesure. Les octets demandés encore vivants dans cette dernière plage sont également comptés, avant/après, afin de distinguer la rétention de pages d’une allocation encore active. Les anciens journaux NXMEM restent conservés comme preuves des essais antérieurs, sans être assimilés à cette mesure renforcée.

## Point final après nettoyage du tmp

Les compteurs NXMEM4 encadrent le chargement de l’ELF, avant sa création effective. Ils ne sont pas présentés comme une mesure après terminaison. Le témoin x86_64/P9-4096 montre 24 octets actifs de différence entre les chargements 2 et 3, sans croissance des cadres ; cet écart doit être observé au même point du cycle de vie avant de conclure au plateau des allocations actives.

Le message existant « TMP retrait confirme » est enrichi par NXFIN4 après la purge P9, sans message DEBUG supplémentaire. Il relève le tas, les allocations actives, la dernière plage réutilisée et le cache. L’oracle conserve toutes les mesures de chargement et exige aussi un plateau exact après nettoyage entre les cycles 2 et 3, puis jusqu’au sixième, pour les octets/blocs actifs, pages attribuées/rendues, charge du cache et allocations actives de la dernière plage. Un écart reste bloquant.

La campagne du candidat 97cee19 a réussi les témoins perte-admission x86_64/P9-4096 et perte-issue AArch64/P9-4096. Elle a été arrêtée explicitement au point sûr avant l’essai suivant (code hôte 77, aucune VM démarrée pour cet appel) pour compléter ce point de mesure. Ces deux succès ne constituent pas la qualification globale du candidat suivant.

## Attribution des variations de l'état d'espace P9

Le contrôle c171ff5 rejette le quatrième cycle x86_64/P9-4096 : le tas conserve 112 cadres, 35 allocations et le même cache, mais les octets actifs passent de 137 056 à 137 032, puis reviennent à 137 056. Cette baisse ne constitue pas une fuite ; son attribution était manquante. Une sonde sans allocation, ajoutée seulement à l'export QEMU, mesure les capacités des deux vecteurs persistants du moteur. Le vecteur Space contient des éléments de 24 octets ; les deux premiers cycles mesurent 336 puis 360 octets, le vecteur de nœuds reste à 32 octets, et le reste hors cache/vecteurs reste exactement à 5 080 octets. Le troisième cycle est identique au deuxième.

NXFIN5 conserve les dix compteurs précédents et ajoute ces deux capacités. Le plateau des cycles 2 et 3 reste strict sur tous les compteurs non cumulatifs. Sur les cycles supplémentaires, seule une baisse de Space, multiple de 24 octets, exactement égale à la baisse des octets actifs est admissible ; aucun dépassement de la référence du deuxième cycle n'est accepté. Les cadres, blocs, cache, nœuds et allocations de la dernière région restent exactement stables. Les tests rejettent une hausse même attribuée, une baisse inexpliquée ou une différence 2/3. Cela applique l'absence de croissance cumulative demandée, sans nouveau budget ni normalisation arbitraire des octets. Les anciennes assertions et traces NXFIN4 sont conservées dans le commit c171ff5 et ses preuves. La qualification de cette attribution sur les cycles supplémentaires et les quatre profils reste à exécuter.
