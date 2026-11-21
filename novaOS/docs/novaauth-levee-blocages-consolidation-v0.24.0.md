# NovaAuth v0.24.0 — Levée des blocages de consolidation

Référence de diagnostic : candidat local b08ba80f6edbd49f06fa7ad28a1810fd2157cd55. Reprise des corrections sur codex/novaauth-autorite-0240, sans publication de main ni activation ordinaire.

## Diagnostic original

Les huit échecs sont détaillés, avec événements natifs, échéances et décisions durables, dans [la matrice de diagnostic](validation/novaauth-huit-echecs-b08ba80.json). Les 1 800 preuves originales sont conservées et leur archive a été relue. Deux échecs relèvent du banc attendant une création malgré une annulation durable, quatre montrent une expiration TLS initiale avec refus sûr, un montre un défi expiré avant ticket, et un reste indéterminé avant admission. La charge hôte n'est pas une cause établie pour chacun.

## Corrections ciblées du banc

Après perte du reçu d'admission, exiger l'issue terminale native puis le reçu durable : enfant identifié et collecté, ou annulation irréversible sans enfant. La mutation n'est jamais répétée ; une consultation retrouve son issue. Continuer ensuite jusqu'au retrait définitif et au lancement B. Les créations retardées et les réponses terminales perdues restent des témoins distincts. Une expiration de délai n'est jamais une preuve d'annulation.

Après refus explicite d'un défi expiré, autoriser une seule nouvelle commande d'authentification dans la même instance. Aucun renouvellement implicite de session, aucun réemploi du défi ni augmentation des délais TLS (10 s) ou de défi (1,5 s).

Le refus après redémarrage attend aussi la fin du premier dialogue TLS avant arrêt de la VM. Les mesures QEMU du noyau relèvent le shell, le registre et le signataire avant, pendant et après l'opération. Elles n'attribuent aucun droit nouveau.

## Référence de ressources

Exiger le retour des racines, handles, endpoints, nœuds, processus, threads et droits de collecte aux valeurs initiales. Les pages propres des trois acteurs doivent revenir à leur référence. Distinguer les attentes propres, bornées par les threads, des attentes globales variant avec les services indépendants. Le pool IPC peut acquérir un premier cadre. Après l’arbitrage mémoire ci-dessous, les deuxième et troisième lancements B doivent démontrer un plateau exact, prolongé par trois cycles supplémentaires, y compris pour les cadres libres. Toute différence inexpliquée reste bloquante. La réserve historique instrumentée AArch64 de 301 cadres reste séparée et ouverte.

## Qualification requise

Rejeu ciblé puis campagne sur AArch64/x86_64 et P9/512/P9/4096 ; deux VM, ordre durable, B bloquée pendant RETRAIT_EN_COURS, retrait définitif après issue démontrée, anciens tickets refusés avant expiration. Rejouer la récupération 2-sur-2, les refus VFS et les accès légitimes, les neuf contrôles ABI et les quatre ELF historiques. Conserver les résultats négatifs et les compteurs manquants comme tels. Ce document ne constate pas encore la réussite de cette qualification.

## Arbitrage mémoire complémentaire

La MOA/MOE autorise trois lancements B, le plateau exact entre deuxième et troisième, puis plusieurs cycles supplémentaires. Le banc en prévoit six : les trois derniers commencent chacun par une réauthentification explicitement demandée, entre transactions, sans prolongation du bail précédent. Les mesures avant/pendant/après chaque lancement et les anciennes traces restent conservées. Le contrôle inclut désormais les pages réellement attribuées/rendues par HEAP_RESIZE dans NovaFS, les octets et blocs vivants, le cache sous son budget historique de 1 065 472 octets et la réutilisation sans attribution physique à partir du troisième chargement. Les octets résidents non demandés comprennent aussi les métadonnées dlmalloc : ils ne sont jamais présentés comme physiquement libérés. Toute différence inexpliquée reste bloquante.

Sur le diagnostic x86_64/P9-4096, les allocations vivantes supplémentaires correspondent exactement à l'augmentation du cache (+81 920 puis +8 192 octets). Les fins de chargement passent de 149 368 à 145 224 octets vivants, tandis que NovaFS conserve 32 cadres supplémentaires. Le troisième lancement retrouve les cadres libres du deuxième. Ces observations motivent le nouvel oracle ; elles ne remplacent pas sa qualification multiarchitecture ni les cycles supplémentaires.

## Corrections des traces VFS

Le parcours enrichi émettait dix-sept messages DEBUG depuis le lanceur, pour une limite inchangée de seize : le marqueur de fin était refusé. Le dernier statut et le marqueur de fin sont maintenant regroupés dans un même message inférieur à 256 octets. Le parseur confondait aussi le préfixe « lecture L » avec « lecture L avant P » ; il exige désormais que l'intitulé soit immédiatement suivi de son résultat hexadécimal. Les assertions de lecture, refus et ordre L/P/L restent intégrales.
