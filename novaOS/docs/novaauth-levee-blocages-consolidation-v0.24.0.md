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

## Résultats mémoire du candidat 2622173

Les quatre premiers témoins de la campagne du 21 novembre 2026 réussissent sur les binaires du commit 262217378bf13f1c820b39e7f9966bcf3962fee8. Chaque VM B effectue six lancements ; les trois derniers suivent une réauthentification explicitement demandée. La synthèse complète des compteurs avant/après est conservée dans `preuves-levee-blocages-b08ba80/memoire-quatre-profils-2622173.json`, avec le chemin et le SHA-256 de chaque résultat natif.

| Profil QEMU | Cas à deux VM | Cadres du tas, cycles 1→6 | Octets actifs après nettoyage, cycles 2/3 | Cache chargé | Résultat mémoire |
|---|---|---|---|---|---|
| x86_64/P9-4096 | Perte du reçu d'admission | 80, 112, 112, 112, 112, 112 | 137 056 / 137 056 | 131 584 | Réussi |
| AArch64/P9-4096 | Perte du reçu terminal | 80, 112, 112, 112, 112, 112 | 132 960 / 132 960 | 127 488 | Réussi |
| AArch64/P9-512 | Deux consommations concurrentes | 80, 112, 112, 112, 112, 112 | 132 960 / 132 960 | 127 488 | Réussi |
| x86_64/P9-512 | Ancien ticket non consommé | 80, 112, 112, 112, 112, 112 | 137 056 / 137 056 | 131 584 | Réussi |

Les 32 cadres supplémentaires correspondent à une attribution HEAP_RESIZE de NovaFS, conservée dans son tas dlmalloc. Aucun octet demandé encore vivant ne subsiste dans cette dernière plage après les cycles 2 à 6. Les chargements suivants y servent chacun une allocation (32 512 octets sur x86_64/P9-4096, 30 464 sur x86_64/P9-512, 24 272 sur AArch64), sans nouvelle attribution physique. Cette observation prouve une réutilisation de la plage, pas que chaque octet de ses 128 Kio a été sollicité.

Après nettoyage, le vecteur d'espace occupe successivement 336, 360, 360, 336, 360, 360 octets sur les quatre profils ; le vecteur de nœuds reste à 32 octets. La totalité des variations d'allocations actives suit ce vecteur, tandis que les autres allocations hors cache/vecteurs restent à 5 080 octets. Le nombre de blocs actifs reste à 35 sur x86_64 et 34 sur AArch64. Le cache reste strictement sous son budget historique inchangé de 1 065 472 octets.

Distinction des mesures : les octets actifs sont les tailles demandées des allocations encore vivantes ; les cadres résidents du tas atteignent 112 ; les 32 derniers cadres sont retenus par l'allocateur et réutilisés, sans allocation cliente encore active dans cette plage ; le compteur des cadres de tas physiquement rendus reste à zéro. L'espace résident non demandé inclut les métadonnées et alignements dlmalloc : il n'est pas présenté comme un décompte exact d'octets libres ni comme de la mémoire rendue au noyau. Aucune croissance résiduelle inexpliquée n'est observée dans ces six cycles ; cela ne constitue pas une preuve sur un nombre illimité de cycles.

Les racines, handles, endpoints, nœuds, processus, threads et droits de collecte reviennent à leur référence après chaque lancement. Le pool IPC et les cadres libres globaux restent au plateau des cycles 2 à 6. La réserve instrumentée AArch64 de 301 cadres demeure ouverte et séparée. Ces quatre témoins mémoire réussis ne remplacent pas le reste de la campagne de consolidation, les recettes VFS distinctes ni la qualification de production.

## Expiration entre lancement réussi et lecture du statut

La campagne 2622173 s'arrête après six scénarios réussis sur le septième, création retardée x86_64/P9-4096. Le troisième enfant B (PID 20) est créé sous bail valide, sort à 0, et ses ressources sont restituées. La commande distincte `echo NXR_$?__FIN` du banc arrive ensuite à 75 312 ms pour une échéance de 75 292 ms : NSHLIVE1 retourne 5 et le shell refuse la saisie. Ce résultat n'est ni une fuite, ni un lancement après expiration, ni un échec de cut. Il reste enregistré comme échec du parcours de qualification, sans le convertir en succès.

Les mesures indépendantes 2 et 3 demandent désormais, comme les cycles 4 à 6, une nouvelle authentification explicite avant le lancement. Il n'y a ni prolongation de bail, ni renouvellement pendant une transaction, ni répétition de la commande précédente après perte de son statut. Les décisions natives de connexion et leurs échéances sont archivées. Le noyau, NovaAuth et les bornes restent inchangés ; seuls l'ordonnancement du banc et le rapport changent. Les résultats mémoire déjà acquis restent conservés ; ce protocole complémentaire doit réussir sur les quatre profils. La campagne globale reste incomplète.


## Bilan consolidé du 21 novembre 2026

Le candidat de code et de banc est `5abb9818313e4608c47d45991ce4a56c11f3ee35`.
Les binaires EXEC proviennent de `262217378bf13f1c820b39e7f9966bcf3962fee8` ;
les binaires VFS de `270fc58e0d954418726d6228cf0a1ba10bb62cfd`.
Les différences ultérieures affectent les sondes EXEC, le banc et les documents,
sans changement du chemin VFS qualifié. Les correspondances d'artefacts et leurs
copies exactes sont archivées. Les quatre ELF historiques ont été exécutés sur
un export QEMU neuf de `5abb981`, avec leurs octets historiques inchangés.

| Contrôle | Résultat observé | Portée |
|---|---|---|
| Mémoire, six lancements B | 4/4 profils réussis avec le protocole 2622173, puis 4/4 avec les baux explicites 5abb981 | Plateau exact 2/3, réutilisation des 32 cadres, cycles 4–6 sans croissance ; aucune restitution physique de ces cadres revendiquée |
| Admission acquittée, création retardée | 4/4 profils réussis sur le banc 5abb981 | B bloquée pendant RETRAIT_EN_COURS, issue A démontrée avant RETIREE |
| Récupération VFS 2-sur-2 | 4/4 profils réussis, cinq démarrages par profil | Lecture privée réelle avant retrait, refus après restauration, puis lecture légitime / refus privé / lecture légitime dans la nouvelle session |
| Ancien état et autorité hors ligne | Refus sur les quatre profils VFS | Aucun accès privé obtenu par le seul rejeu d'une tête historique |
| Contrôles ABI | 9/9 réussis | 48 sélecteurs conservés |
| ELF historiques | 4/4 réussis | AArch64 EL0 et x86_64 Ring 3, sdk-example et threads-test ; restitution vérifiée |
| Tests hôtes ciblés | 103/103 réussis | Banc, oracles de ressources et conversion d'échéances ; distincts des essais invités |
| Noyaux ordinaires | Identité binaire conservée avec la base vérifiée | Comparaison 97cee19, puis examen des différences QEMU/banc uniquement ; aucune activation ordinaire |

Les mesures détaillées sont dans
[la synthèse vérifiable](validation/novaauth-memoire-5abb981.json).
Les traces de chargement et de nettoyage conservent les mesures avant, pendant et
après chaque cycle. Le pic d'allocations actives, les pages résidentes et le cache
ne sont pas confondus. Les variations de 24 octets du vecteur Space sont expliquées
exactement ; aucun dépassement de la référence chaude n'est admis.

### Réserves bloquantes distinctes de la mémoire

La campagne globale de 32 cas reste **incomplète**. Le protocole 2622173 donne
six réussites puis un échec sur sept tentatives (expiration de bail entre la
commande réussie et la lecture de son statut). Le protocole 5abb981 donne quatre
réussites, un échec et vingt-sept cas non exécutés ; un rejeu diagnostique isolé
et une sonde instrumentée supplémentaires échouent aussi. Les succès de versions
ou de scénarios différents ne sont pas additionnés pour annoncer 32/32.

Sur x86_64/P9-4096, deux défis explicitement demandés expirent lors du cas
perte-admission, avant ticket. Une exécution isolée reproduit des expirations,
d'abord en A puis pendant la réauthentification de B. La charge ou la concurrence
hôte ne sont donc pas retenues comme cause démontrée. Les anciens résultats
négatifs restent archivés, avec les décisions natives de refus et le drainage.

Une sonde dans un export privé, **non intégrée au candidat**, mesure : création du
défi à 151 232 ms, échéance à 152 732 ms, réponse du signataire à 151 232 ms,
retour de la vérification fraîche de liaison à 152 736 ms. Le segment mesuré dure
1 504 ms ; le refus à quatre millisecondes au-delà de l'échéance est sûr. La
consultation fraîche de l'autorité après signature appartient à ce segment et
reste indispensable. La répartition précise entre transport, ordonnanceur et
autorité n'est pas établie. Aucun cache de décision ni allongement du défi n'a
été introduit pour obtenir un succès.

Ce parcours révèle également une limite du banc : le vérificateur atteint ses
seize messages DEBUG ; le marqueur NXBAIL suivant n'est plus disponible, et
l'oracle refuse de conclure à une connexion. Le quota reste inchangé. Le défaut
de visibilité du banc reste à corriger par un encodage borné des événements,
sans supprimer les refus ni affaiblir les contrôles. Le patch et le binaire de
sonde sont conservés séparément des binaires qualifiés.

La réserve instrumentée AArch64 de **301 cadres** reste ouverte. La stabilité
NovaFS sur six cycles ne résout ni cette réserve séparée, ni les expirations de
défi, et ne prouve pas une borne sur un nombre illimité de cycles. Aucun essai
UTM ou matériel physique n'a été exécuté dans cette campagne.

**Verdict : qualification mémoire ciblée réussie sur les quatre profils QEMU ;
consolidation non apte à fusionner en l'état.** Il reste à résoudre les deux
réserves d'authentification/visibilité, puis à rejouer les 32 cas bloquants sur le
candidat final. Aucune fusion dans main, aucun tag et aucune qualification de
production ne découlent de ce bilan.

### Conservation des preuves

Racine durable : `/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/`.
L'archive `qualification-memoire-novafs-5abb981-preuves.tar.gz` contient
**1 774 fichiers de preuve plus le manifeste**, y compris les résultats négatifs.
Les 1 775 membres ont été décompressés et comparés par SHA-256 aux originaux.

- Archive : `c4827a21be87ee01d6bd524e3034f778b45a1f2a40ee3432eda491f5a3f4d9b6`.
- `preuves-levee-blocages-b08ba80/SHA256SUMS` : `09d224b6367c68fca013da1dce4a50b2c7405d41c1af98efeffa7e396d4d8697`.
- Manifeste privé des 602 artefacts et fichiers de laboratoire : `ec41916f9008495ea301f8b3597090de9abd0c641294421c494d88566ce7b357`.

Les binaires exacts et secrets de laboratoire restent dans
`artefacts-prives-levee-blocages-b08ba80/`, hors Git et hors temporaires. Seuls les
sources et rapports sont publiables sur la branche dédiée. Les 1 800 preuves
initiales du candidat b08ba80 restent intactes ; leur archive et leur manifeste
ont été vérifiés séparément (`verification-preuves-initiales.json`).
