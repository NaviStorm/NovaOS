# NovaOS v0.17.0 — Réservations du stockage groupé

Le bilan de publication et le rejeu sur le candidat final figurent dans la [qualification de fusion v0.17.0](fusion-smp-v0.17.0.md).

**Décision proposée.** Lever le blocage technique portant sur les réservations du stockage expérimental et soumettre le candidat SMP à un nouvel arbitrage MOA/MOE. Le défaut initial passe de **0/4 à 4/4** contrôles de fermeture. Les quatre campagnes de 1 000 abandons retrouvent leurs bilans mémoire, et les 36 essais de perte d’acquittement conservent leurs cadres jusqu’au retrait confirmé. La branche de finalisation est la seule branche visée par cette livraison.

## Périmètre et propriété

Ce rapport concerne le chemin groupé expérimental, la branche `codex/finalisation-smp-0170` et le défaut établi au commit `cd5e45f86540db09a218ac262609c4d68c00c275`. La distribution conserve le stockage classique et le polling. Le moteur NovaFS v0.9.13, les deux barrières de publication P9 et l’ABI publique ont leur propre contrat. [Qualification ayant suspendu la fusion](qualification-stockage-v0.17.0.md) · [Configuration de la distribution](integration-v0.16.6.md)

| Ressource | Propriétaire et durée |
|---|---|
| Source, destination et droit d’écriture | Le processus de l’éditeur remet le plan au service NovaFS via les droits de session et le nom choisi par `nsh`. Le dialogue `OUI` valide la destination. Seul le retour de publication autorise le message de sauvegarde réussie. |
| Zone virtuelle de préparation CoW | Le processus NovaFS possède les pages utilisateur. Pour un lot, le noyau valide la plage et conserve son adresse, sa direction, son nombre de pages, le PID du moteur et un tag monotone sur le canal. La réservation de 256 pages représente 1 Mio d’adresses virtuelles. |
| Table des LBA et messages du relais | La liste de LBA est figée dans une boîte superviseur et transmise au pilote sur une voie dédiée. Le relais transporte une demande identifiée. La restitution d’un message IPC ne donne aucune autorité matérielle au relais. |
| Cadres privés de l’arène, virtqueue et domaine IOMMU | Le pilote détient le bail du matériel, lié à son PID et à l’époque de la session. Le noyau possède les cadres privés et les traductions DMA. Il recopie les données entre l’espace utilisateur et l’arène pour le sens et la plage du transfert. L’arène peut servir à plusieurs lots successifs de la même session. |
| Drainage ou quarantaine | L’objet de retrait matériel conserve la file encore en vol, le périphérique, le domaine, les traductions et les cadres. Une incertitude ou une erreur de retrait conserve leur propriétaire jusqu’à la reprise du retrait. |

## Défaut et frontières d’acquittement

Le moteur publiait sa réservation avant l’échange avec le relais. Quand cet échange expirait, le retour d’erreur sortait de `Block::lot` et abandonnait le plan. L’attente IPC était terminée, mais la réservation portée par l’autre canal restait ouverte. Le noyau la voyait encore lorsque `vi` avait reçu le statut 26, avec zéro page transférée. Les quatre échecs historiques portaient tous sur cette fenêtre. Le premier incident spontané AArch64/512, plus ancien, demeure inexpliqué. [Séries et identifiants du diagnostic initial](validation/fusion-smp-v0.17.0/reponses-stockage.json)

Pour une sauvegarde, l’acquit du pilote se rapporte à la commande de bloc. F1 ordonne la persistance des données, du manifeste et des références avant la publication de l’ancre. F2 porte sur l’ancre publiée. Une erreur d’écriture de l’ancre ou de F2 donne une publication incertaine (22). Les erreurs de préparation et de F1 remontent en erreur d’E/S (26). Le choix du nom et l’expiration IPC ne changent aucun de ces critères. [Ordre de publication](../../novaFS/src/engine.rs) · [Tests des frontières](../tests/stockage_phases.py)

## Machine à états du lot

Avant le correctif, le chemin de succès passait de la réservation à la publication IPC, puis à la soumission VirtIO, à la livraison des données et à la clôture par le pilote. Une erreur sur le chemin IPC réveillait le thread demandeur. La réservation sur le canal de données ne participait pas à cette transition. Le défaut a aussi une fenêtre avant la réservation elle-même, car le moteur dépose la table des LBA sur un second canal avant de réserver les adresses.

| État ou événement | Transition et propriété après correction |
|---|---|
| Table déposée, lot créé | Le moteur dépose les LBA pour le pilote. Une deuxième copie accompagne le tag, la direction et la plage virtuelle dans la réservation du superviseur. La réservation mémorise le PID et le thread demandeur. Le noyau rattache cette incarnation du canal aux deux handles privés d’envoi et de réponse du moteur. |
| Commande préparée puis publiée au relais | L’envoi peut expirer sur une file pleine. Après un envoi réussi, la réception peut expirer alors que le relais conserve le message. Dans les deux cas, la fin d’attente invalide le lot sous la garde SMP avant le réveil. Une erreur immédiate de ces appels suit la même règle. La perte d’un des deux droits de contrôle scelle la voie concernée pour les tags futurs. |
| Abandon avant admission matérielle | Le noyau efface la liste autorisée et retire la réservation. Le dernier tag reste consommé sur cette incarnation. Le chemin d’erreur du moteur demande la fermeture des deux capacités du lot, y compris si seule la table a été déposée ou si la réponse applicative est mal formée. Pour un lot en vol ou douteux, `IPC_CLOSE` peut rendre `WOULD_BLOCK` ou `REFUSED`. Ces retours maintiennent l’admission fermée et le handle pourra subsister jusqu’au retrait acquitté. |
| Soumission au pilote et visibilité du matériel | Le pilote contrôle les identités des deux extrémités, le bail, le tag, la direction et la progression. La dernière admission et la publication dans la virtqueue sont protégées par la même garde SMP que l’abandon. Un refus de construction avant la publication de `avail.idx` clôt le lot sans transfert. |
| Retrait demandé après soumission | Le retrait interdit les prochains descripteurs. Une commande en vol conserve ses cadres DMA privés. Le retour d’un appel IPC ne peut ni créditer la commande ni autoriser leur réemploi. |
| Complétion certaine | La virtqueue, la longueur et le statut sont contrôlés. La livraison en lecture revalide les droits du moteur et le bail. Si le lot a été retiré, la réservation est effacée sans crédit ni livraison. Sur succès, le pilote clôt le lot seulement après sa progression complète et le FLUSH requis. |
| Complétion incertaine, drainage ou quarantaine | Lorsque la commande constate la perte de son acquittement, le pilote déclenche aussitôt le drainage de cette époque, hors garde de l’ordonnanceur. Le propriétaire du transport conserve la file et les cadres associés. Un échec de drainage, de reset, de purge d’IRQ ou de retrait des traductions conserve le propriétaire en quarantaine. La reprise continue depuis la dernière étape acquittée. L’époque de session, l’incarnation de l’endpoint et l’identité de la capacité paire désignent exactement la voie de données et celle des LBA. Le début du retrait scelle sur cette voie le dernier tag à sa valeur maximale. Les anciens tags restent consommés et aucune génération suivante n’est admissible. Après l’acquittement matériel, le noyau détache les deux voies même si les processus qui possèdent les handles sont encore vivants. |
| Ressources restituées | La liste du lot est effacée et l’admission fermée. L’arène de session reste utilisable par son propriétaire pour les prochains lots autorisés. À la fermeture de la session, le retrait matériel complet permet de rendre aussi les cadres physiques au gestionnaire de pages. |

Le premier correctif (`1547bc6`) a relié les attentes au lot. La revue a mis en évidence qu’un retrait matériel réessayé avec les processus vivants devait aussi notifier le canal logique. Le complément (`f0e5020`) raccorde cette notification et la révocation des contrôles à l’incarnation de la voie. La révision qualifiée (`6534f23`) déclenche le premier retrait à la perte d’acquittement, y compris quand le shell garde la session ouverte. Le mutex du transport et la vérification de l’époque empêchent de retirer une session renouvelée. Le correctif se compose après les fixtures groupées. Le profil brut ignore ces fixtures. [Correctif du lot et du transport](../tests/fixtures/stockage-0170-retrait.patch) · [Compositions de profils](../tests/reconstruire_es_0166.py) · [Garde SMP](../kernel/src/sched/gate.rs)

## Rejouer la cellule de performance

Le scénario de référence insère 4 Mio en plusieurs endroits d’une source de 4 Mio, vers un nom libre, sur secteurs de 4 096 octets. La durée couvre la commande de sauvegarde, la confirmation de la destination et le retour durable à `vi`. La vérification et le hachage des disques interviennent ensuite. Le premier compteur additionne les échanges entre services et les dons acquittés. Le plafond de 64 inclut aussi les opérations de contrôle requises par le contrat. Les copies locales de données sont relevées séparément. [Calcul des compteurs](../tests/es_groupees_verifier_mesures.py)

QEMU direct utilise TCG, quatre vCPU et 256 Mio. Le profil UTM emploie quatre Cortex-A72 émulés sur `virt-10.0`, GICv3 sans ITS, TCG multithread, UEFI et QCOW2 clairsemé. Le profil de production conserve sa propre configuration.

Reconstruire les profils `sans`, `utm`, `sonde` et `utm-sonde` sur la révision du correctif avec `novaOS/tests/reconstruire_es_0166.py`. Le profil `sans` construit le générateur d’images `novaFS/target/aarch64-apple-darwin/release/examples/es-groupees`. Les profils mesurés fournissent leur propre lecteur hôte. Renseigner le chemin absolu du paquet UTM modèle déjà qualifié dans `modele`. Chaque sortie doit être neuve.

```sh
export NOVA_VCPUS=4 NOVA_QUALIF_VCPU=4
revision=$(git rev-parse HEAD)
sortie=$(mktemp -d)
python3 -B novaOS/tests/reconstruire_es_0166.py "$revision" "$sortie/exports" sans utm sonde utm-sonde
generateur="$sortie/exports/sans/novaFS/target/aarch64-apple-darwin/release/examples/es-groupees"
profil="$sortie/exports/sonde"
for arch in aarch64 x86_64; do
    python3 -B novaOS/tests/integration_main_performances.py mesurer "qemu-$arch" groupe compteurs \
        4096 ajout-4m-multiple "$profil" "$generateur" \
        "$profil/novaFS/target/aarch64-apple-darwin/release/novafs" "$sortie/qemu-$arch" --conserver-images
done
profil="$sortie/exports/utm-sonde"
python3 -B novaOS/tests/integration_main_performances.py mesurer utm-aarch64 groupe compteurs \
    4096 ajout-4m-multiple "$profil" "$generateur" \
    "$profil/novaFS/target/aarch64-apple-darwin/release/novafs" "$sortie/utm" \
    --profil-utm "$modele" --conserver-images
```

Pour les cellules sans instrumentation, reprendre les mêmes commandes avec le relevé `ordinaire`, le profil `sans` pour QEMU et le profil `utm` pour l’application. Pour le témoin antérieur, utiliser le constructeur provenant de son propre commit.

`inventaire.json` conserve le SHA Git et les empreintes des supports et journaux du passage. En cas de refus, conserver aussi le diagnostic propre à l’application ou à l’injecteur. Le rejeu UTM importe un paquet fictif identifié par un UUID neuf et exporte sa copie après le redémarrage. [Banc QEMU et UTM](../tests/integration_main_performances.py)

## Interprétation des mesures de mémoire

- Le nombre de pages autorisées par les lots décrit les plages virtuelles que le pilote est encore autorisé à faire copier. Les pages privées du moteur peuvent rester allouées pour la session après la fermeture de la réservation.
- Les cadres DMA épinglés sont des cadres physiques détenus par le noyau. Les arènes d’une session ouverte peuvent déjà être chaudes. Pour comparer deux points à l’intérieur d’une session, employer la même géométrie et le même état d’arène. Au repos après destruction des services, comparer avec le bilan de démarrage de la campagne.
- Le bail matériel de session et l’admission d’une requête sont deux objets distincts. La fin de vie d’un lot doit interdire sa soumission. La fermeture de session doit également rendre l’époque du bail inactive. La quarantaine conserve les cadres associés tant que le périphérique ou le domaine ne peuvent pas attester le retrait.
- La génération, le PID et les handles d’une nouvelle incarnation avancent. Les décomptes de réservations, d’attentes, de handles, de cadres épinglés et de cadres libres sont comparés à leur état de référence, en tenant compte des ressources explicitement retenues.

Le retrait matériel passe par la fin des commandes soumises, l’arrêt du périphérique et le contrôle du bit de maîtrise du bus, la suppression de la route et la purge des interruptions, le retrait acquitté des traductions et l’arrêt du domaine. La libération de l’arène et des cadres du transport suit ces étapes. Les tests de perte d’acquittement exercent la reprise de ce retrait. [Propriétaire du transport](../kernel/src/hardware/block/ownership.rs) · [Arène du profil groupé](../tests/fixtures/io-0166-virtio-multipage.patch) · [Tests de propriété](../tests/dma_propriete_invite.rs)

## Rejouer les fermetures et l’endurance

Une VM d’endurance utilise trois acteurs natifs à capacités distinctes. Le moteur passe les appels de réservation et d’IPC et le pilote réalise des lectures VirtIO sur un disque fictif en lecture seule. L’horloge du banc rend déterministe l’ordre des expirations. Le profil de fermeture utilise la vraie chaîne `vi → VFS → NovaFS → relais → VirtIO` sur un autre disque. Le collecteur fige l’image initiale, conserve le disque arrêté, redémarre sa copie et vérifie la source ainsi que l’absence de destination avec le lecteur P9 hôte.

```sh
revision=$(git rev-parse HEAD)
sortie=$(mktemp -d)
python3 -B novaOS/tests/reconstruire_es_0166.py "$revision" "$sortie/export" diagnostic-stockage \
    --sonde stockage-0170-reponses.patch --sonde stockage-0170-endurance.patch
export NOVA_VCPUS=4 NOVA_QUALIF_VCPU=4
banc="$sortie/export/diagnostic-stockage/novaOS/tests/reservations_endurance.py"
for arch in aarch64 x86_64; do
    for secteur in 512 4096; do
        for essai in vi endurance; do
            python3 -B "$banc" "$arch" "$secteur" "$essai" "$sortie/$arch-$secteur-$essai.json"
        done
    done
done
```

Chaque appel peut être exécuté individuellement depuis le dépôt. Un nouveau répertoire de preuves est attribué à chaque tentative, y compris quand l’oracle signale un échec. Les empreintes des ELF, la configuration de démarrage, les séries et les rapports permettent de retrouver une tentative précise. [Banc invité](../tests/reservations_endurance_invite.rs) · [Vérification hôte](../tests/reservations_endurance.py) · [Scénarios de réponses tardives](../tests/reponses_stockage.py)

## Couverture de concurrence et modèle de panne

| Frontière du mandat | Observation qui tranche |
|---|---|
| Expiration avant publication | La réservation existe, la demande n’a pas encore été envoyée et la réception privée expire. L’oracle exige zéro réservation et zéro page autorisée avant tout `IPC_CLOSE`. |
| Expiration pendant la publication | La file de huit messages est pleine. Le moteur est bloqué dans `IPC_SEND` et l’échéance survient pendant la tentative d’envoi. Le relais draine ensuite les huit messages antérieurs. La publication élémentaire dans la file et l’expiration sont sérialisées par la garde SMP. |
| Expiration après publication | Le relais reçoit la trame et la retient avant de la transmettre. La réception du moteur expire sur l’autre canal. Dans la session de `vi`, le statut noyau 8 produit le statut public 26. Le nombre de soumissions matérielles est mesuré avant et après le refus. |
| Annulation et soumission concomitantes | L’admission finale et le kick se trouvent dans la même section critique. Les essais ordonnent le retrait avant le kick puis après le kick et avant la consommation de `used`. Le second cas doit drainer la commande déjà visible et refuser tout crédit de livraison. |
| Réponse tardive ou ancienne | Un envoi tardif expire sur la réception abandonnée. Un acquittement de l’ancien tag, livré sur un canal vivant, est rejeté par le service. Les deux oracles vérifient l’absence du message de sauvegarde et l’état du disque après remontage. |
| Mort du relais, du pilote et du demandeur | Les arrêts du relais et du pilote sont exercés pendant une sauvegarde de `vi`. Si le relais meurt après que le moteur a bloqué dans la réception, la disparition de l’endpoint réveille le moteur avec `CLOSED` (11). Si le relais a déjà détruit l’endpoint, le moteur ne retrouve plus le handle dans sa table et reçoit `DENIED` (5) à l’entrée de la réception. Le statut public exigé est 26 dans les deux ordres. Le banc d’endurance termine alternativement le processus moteur détenteur de la réservation et le pilote par leur propre appel de sortie. Il vérifie la disparition du lot avant d’arrêter les acteurs restants. |
| Révocation du bail ou d’un droit temporaire | Les chemins historiques, PAGE, groupé monopage et multipage vérifient les droits, l’absence de livraison et la conservation des octets témoins après révocation. Une expiration sur un canal indépendant laisse le lot légitime intact. |
| Acquittement matériel retardé ou perdu | Le banc conserve les deux processus durant la première tentative de retrait, observe la quarantaine et son nombre de cadres, puis autorise le drainage. Il exige la disparition des deux voies de données et le retour des cadres au bilan initial après retrait confirmé. |
| Réutilisation des identifiants | Le slot peut être repris. Le PID, le handle, le tag de l’opération et le cookie de réveil sont vérifiés dans leurs contextes respectifs. Les tests IPC vérifient aussi qu’un réveil de l’ancienne incarnation ne réveille pas la suivante. |
| Plusieurs annulations, saturation et redémarrage | Deux appels `IPC_CANCEL`, ou un appel `IPC_CANCEL` et un `IPC_REVOKE`, ciblent la même attente. Une réservation déjà occupée refuse le second lot. Les files IPC sont saturées à leur capacité réelle. Les trois acteurs sont recréés par séries, avec de nouvelles identités et une nouvelle époque du bail. |

Cette matrice distingue les ordres imposés pour rendre les courses reproductibles, la préemption observée dans les invités, et les transitions impossibles à séparer sous la garde SMP. Les pertes d’acquittement, suspensions et coupures sont injectées dans des VM jetables. La persistance est vérifiée dans le modèle de la VM et de son support hôte. Une coupure d’alimentation physique du Mac reste hors campagne.

## Endurance et bilan mémoire sur le correctif

Sur chacune des quatre configurations, les 1 000 abandons se répartissent en 168 expirations avant envoi, 168 expirations d’un envoi bloqué, 336 expirations après réception par le relais, 320 doubles annulations et 8 courses entre annulation et révocation. Chaque profil alterne ces abandons avec 136 lectures VirtIO confirmées et contrôlées. Huit séries recréent chacune le moteur, le pilote et le relais. Quatre morts du moteur et quatre morts du pilote sur réservation active viennent en plus des 1 000 abandons. Le banc impose et vérifie une migration du moteur, et relève ses préemptions réelles.

| Invité / secteur | Cadres libres à froid | Repos avant → après les 1 000 abandons | Cadres privés de la machine au repos | Préemptions du moteur | Restitution logique maximale | Retour maximal de l’appel de fermeture |
|---|---:|---:|---:|---:|---:|---:|
| AArch64 / 512 | 62 419 | 62 416 → 62 416 | 0 | 180 | 6,101 ms | 184,518 ms |
| AArch64 / 4 096 | 62 419 | 62 416 → 62 416 | 0 | 78 | 1,429 ms | 114,185 ms |
| x86_64 / 512 | 62 925 | 62 916 → 62 916 | 9 | 112 | 0,097 ms | 203,028 ms |
| x86_64 / 4 096 | 62 925 | 62 916 → 62 916 | 9 | 71 | 0,911 ms | 156,500 ms |

Le maximum de restitution couvre la transition de la réservation sous la garde, à l’horloge de l’invité TCG. Le maximum de fermeture couvre un appel distinct effectué ensuite avec les commutations entre acteurs. Les compteurs du premier démarrage incluent trois cadres de tables de pagination ARM ou neuf cadres permanents VT-d x86. À chaud, les invités ARM ont conservé 62 061 cadres libres et les invités x86 en ont conservé 62 570 pendant un service actif. Les deux côtés de chaque série montrent trois endpoints, huit handles et dix-sept cadres DMA machine compris. Après destruction des acteurs, chaque profil compte zéro endpoint, handle, attente, réservation, page autorisée, bail actif et quarantaine. La génération IPC passe de 16 à 3 544. Les huit époques de pilote sont distinctes. Les cadres utilisés par les services ont retrouvé le même bilan à chaque série. QMP compte zéro écriture sur les quatre disques fictifs de lectures. [Empreintes des ELF, évolutions des générations et observations individuelles](validation/fusion-smp-v0.17.0/reservations-endurance.json)

## Fermeture de la réservation après le refus de vi

**4/4 contrôles de fermeture** sur le correctif `6534f232c558cf6a1f15e1a340e25d56416b7310`. Dans les quatre invités, la sonde du relais mesure une réservation de 256 pages, tag 4 370. La réception expire à 506 ticks avec le statut 8. Quand `vi` reçoit le statut 26, la sonde noyau compte zéro réservation et zéro page autorisée. Le compteur VirtIO reste à 6 822 soumissions, depuis l’observation du lot retenu jusqu’à la fermeture de la session.

| Invité / secteur | Réservations relais → refus | Cadres libres chauds avant → après | Cadres DMA de session en fin d’essai | Générations relues |
|---|---:|---:|---:|---|
| AArch64 / 512 | 1 → 0 | 62 416 → 62 416 | 0 | 67, 68 |
| AArch64 / 4 096 | 1 → 0 | 62 416 → 62 416 | 0 | 67, 68 |
| x86_64 / 512 | 1 → 0 | 62 916 → 62 916 | 0 | 67, 68 |
| x86_64 / 4 096 | 1 → 0 | 62 916 → 62 916 | 0 | 67, 68 |

Au refus, l’époque 2 du pilote était encore la session légitime de `nsh`. Les sondes y comptaient 32 cadres DMA machine comprise et aucune commande en vol. L’abandon a retiré toute autorité propre au lot. Puis l’arrêt des services a rendu le bail de session inactif, avec zéro quarantaine. Sur x86, les neuf cadres du domaine racine VT-d restent attribués à la machine. À froid, le témoin positif avait créé la destination `echauffement` avec confirmation effective. L’essai négatif suivant a retrouvé exactement les 4 Mio de la source et aucune destination `essai-reponse`. Les deux ancres ont les mêmes générations avant le consentement, après l’arrêt et après le remontage indépendant. Aucun succès n’a été annoncé pour la sauvegarde refusée. [Archives et inventaires des huit essais](validation/fusion-smp-v0.17.0/reservations-endurance.json)

## Retrait matériel avec les deux acteurs vivants

**36/36 essais** sur douze combinaisons architecture × nombre de vCPU (1, 2, 4) × taille de secteur (512, 4 096). Chacune exerce la lecture monopage, le scatter-gather de deux pages et le FLUSH. Le banc laisse le matériel voir la commande, masque l’acquit destiné au pilote et fait perdre la première confirmation du drainage. La commande demande d’elle-même le retrait du bail. Le service et le pilote conservent leurs handles pendant la quarantaine et la reprise.

| Compteur physique global | Avant | Pendant la quarantaine mono / SG / FLUSH | Après retrait confirmé |
|---|---:|---:|---:|
| AArch64 | 0 | 16 / 18 / 16 | 0 |
| x86_64, cadres machine inclus | 9 | 16 / 18 / 16 | 9 |

Aucune réutilisation des cadres retenus n’est observée avant la reprise. L’ancien bail refuse la commande, le compteur reste à un kick et les deux voies IPC sont détachées après l’acquittement. Une nouvelle demande de retrait laisse le bilan identique. L’oracle contrôle les octets accessibles aux deux acteurs, la restitution des cadres et l’empreinte du disque sur chaque géométrie. Les témoins de livraison normale, de retrait après kick, d’invalidation du tampon et de refus sur les portails historiques restent exercés. [Test invité de perte d’acquittement](../tests/dma_retrait_session_invite.rs) · [Infrastructure et oracles](../tests/dma_propriete.py)

Une perte permanente de l’acquittement conserve la quarantaine et empêche l’ouverture du prochain bail sur le périphérique. La campagne teste la persistance de cette quarantaine tant que le défaut est injecté et le réemploi des ressources après une reprise effective. Sa couverture s’arrête aux périphériques et versions émulés indiqués.

## Publication P9 et régressions

Les 32/32 essais de préparation, écriture et publication CoW donnent les mêmes états sur les deux architectures et les deux géométries. Dans ces cellules le volume initial présente G65/G66, puis le montage ou la lecture du témoin atteint G67 avant la confirmation. Les générations 67/68 du banc de fermeture ci-dessus tiennent à son échauffement supplémentaire dans la même VM.

| Point d’injection | Statut public | FLUSH de sauvegarde acquittés | État effectivement relu |
|---|---:|---|---|
| Avant écriture, pendant écriture, après le lot ou expiration de F1 | 26 | Aucun | G67, source exacte et destination absente |
| Écriture de l’ancre en erreur | 22 | F1 | G67, destination absente |
| Après écriture de l’ancre avant F2, ou expiration de F2 | 22 | F1 | G68, destination entière ; aucune confirmation de sauvegarde |
| Témoin durable | 0 | F1 et F2 | G68, destination entière, sauvegarde confirmée |

Les 24/24 scénarios de réponse couvrent l’ancien tag, la réponse tardive, l’annulation, la mort du relais, la mort du pilote et la révocation. Les 36/36 coupures QEMU couvrent les écritures de pages et les deux barrières, y compris des commandes en vol. Chaque résultat vérifie le montage et relit les octets. Les six groupes de défauts du chemin classique passent sur les quatre configurations (24/24). Les tests historiques du client fichiers ajoutent 40 frontières de coupure ou fermeture et 16 erreurs (56/56).

Pour NovaFS, chaque architecture exécute 196 tests standards et 199 instrumentés, 2 304 injections sur les ancres et un million d’opérations différentielles. Les six cas ignorés par les deux profils du lanceur sont comptés séparément. ARM64 s’exécute nativement sur Darwin et x86_64 sous Rosetta. Les arbres Git de l’ABI native (`a70d5f23ae91211dd7a54c429e723f5157600e58`) et de NovaFS (`3379ccd023e821e912130c7413113fe7546070cc`) sont identiques à ceux de `origin/main` pour cette qualification.

Les 108/108 groupes système qualifiés se répartissent en 17, 19 et 21 groupes AArch64 et 15, 17 et 19 groupes x86_64 pour un, deux et quatre vCPU. Sont couverts les capacités, le cycle de vie des processus, les courses IPC, la migration, l’ordonnanceur, l’arrêt coordonné, la FPU/SIMD, `nsh` et `vi`. Les ELF des profils bruts des deux architectures ont été reconstruits au même chemin et comparés octet par octet. Trente groupes spécialisés GICv3 et FPU ont été rejoués sur les ELF finaux dont les empreintes différaient. Les épreuves à quatre vCPU incluent 300 secondes d’IPC et 60 secondes d’ordonnancement par architecture. Les sept contrôles hôte de conservation des preuves ont aussi réussi. [Régressions individuelles, empreintes et incidents de construction](validation/reservations-dma-v0.17.0/regressions.json)

Deux passages intermédiaires de mort du relais ont révélé que le banc exigeait systématiquement le statut interne 11. Le témoin final vérifie les deux ordres 5 et 11 décrits ci-dessus, ainsi que le tag, le décès effectif du relais, le refus public 26, le retrait et l’absence de destination. Les deux images ont été relues après remontage sur copie. Sur AArch64/512, la source de 4 Mio et les deux générations G66/G67 ont été retrouvées intégralement. Cet incident instrumenté de mort du relais est distinct du refus spontané d’origine dont le disque a été perdu. [Résultats et conservation du refus intermédiaire](validation/reservations-dma-v0.17.0/regressions.json)

## Performance de la sauvegarde groupée

Un seul scénario est chronométré. Il insère 4 Mio répartis dans une source dense de 4 Mio, publie une destination de 8 Mio sous un nom libre puis relit les octets. Le temps du tableau commence à la commande d’écriture de `vi` et se termine à l’annonce de succès après F2. Les valeurs sont individuelles, en millisecondes. Les colonnes « avant » utilisent `cd5e45f`, les colonnes « final » utilisent `6534f23`.

| Environnement, secteurs 4 096 | Hors sondes avant | Hors sondes final | Instrumenté avant | Instrumenté final pendant les régressions | Instrumenté final après les régressions |
|---|---:|---:|---:|---:|---:|
| QEMU AArch64 / raw | 171,311 | 172,539 | 220,283 | 386,899 | 165,876 |
| QEMU x86_64 / raw | 231,321 | 229,368 | 290,233 | 616,004 | 231,672 |
| UTM AArch64 / QCOW2 | 755,383 | 875,461 | 1 385,665 et 212,497 | 569,801 | 810,102 |

Toutes les mesures finales sont inférieures à une seconde. Hors sondes, l’écart entre les deux passages vaut +1,228 ms sur QEMU ARM, −1,953 ms sur QEMU x86 et +120,078 ms sur UTM. Le témoin UTM antérieur comprend aussi une mesure à 1 385,665 ms pendant d’autres régressions. Le même noyau témoin a mesuré 212,497 ms lors d’une autre exécution. La charge de l’hôte et l’ordonnancement varient entre passages. Ces cellules de non-régression permettent de vérifier le seuil observé et d’identifier ces écarts. Leur effectif est insuffisant pour attribuer la variation UTM au seul correctif ou garantir une borne de latence sur tous les hôtes.

Tous les passages instrumentés comptent 47 échanges entre services et dons acquittés, **56 contrôles compris**, 190 soumissions VirtIO, 190 interruptions, 80 cadres privés d’arène et **deux FLUSH**. Le calcul couvre uniquement la sauvegarde. Les deux durées physiques F1/F2, relevées après les régressions, sont de 1,422/0,420 ms sur QEMU ARM, 1,283/0,399 ms sur QEMU x86 et 4,891/0,339 ms sur UTM. Les octets de destination après relecture donnent tous `c82aa6dfd3dcce1bb85d1fe60dec9073ea80ae37a0465c152d7c7d41fd5d6a25`.

À instrumentation égale, les pics observés de cadres de session sont identiques avant et après. Sur ARM QEMU, le pic vaut 9 751 cadres. Sur x86 QEMU il vaut 9 753 hors sondes et 9 754 avec sondes. Sur UTM il vaut 9 732. Les compteurs détaillés par PID, les charges hôte, les 19 mesures, le passage intermédiaire `f0e5020` et les empreintes des inventaires sont disponibles dans le relevé. Le premier témoin UTM a un inventaire externe uniquement. [Mesures individuelles et empreintes des preuves](validation/reservations-dma-v0.17.0/performances.json)

## Références Git et conservation

La branche de travail part du diagnostic `cd5e45f86540db09a218ac262609c4d68c00c275`. La révision SMP antérieure était `e08b9796e59523b917b9d020520d5df97c9a4380`. La référence de `main` pour cette livraison est `3e14a50bb5bc7960d16c1fbcfc7f49a681d6e7c6`. Les principaux commits de la branche de finalisation sont les suivants.

| Objet | SHA |
|---|---|
| Liaison entre l’abandon IPC et la réservation | `1547bc6d629dd66fae2c303b1d14f460521a4f1c` |
| Identité et retrait des deux voies de session | `f0e5020db3a1a3b95207480624174ae90414269f` |
| Drainage automatique à la perte de l’acquittement, code produit qualifié | `6534f232c558cf6a1f15e1a340e25d56416b7310` |
| Classification des deux ordres lors de la mort du relais | `b3e86275bb2e7150ea6b0ac7309ffbbfdbd31c09` |
| Résultats de fermeture et d’endurance | `4a30f72679a5073ba1a38a18e1660d91b5fe715d` |
| Sondes du retrait automatique en présence des deux acteurs | `eb3767ff5ae05ff60a44a42cd8f4c18bab74c69c` |
| Résultats des régressions ciblées | `6790229e5ff799d4c792510d6f317af346043150` |

Les archives volumineuses résident dans `novaOS/build/qualification-reservations-0170` du dépôt de travail principal. Les manifestes versionnés identifient les sous-répertoires, les images synthétiques, les séries, les configurations de VM et les empreintes. Les essais intermédiaires gardent leurs inventaires propres. Les profils ne changent ni les anciennes API ni le format P9. Le comportement de production reste le chemin classique en polling, et l’activation du profil groupé demande toujours une construction explicite. L’origine du premier refus spontané AArch64/512 et les coupures physiques du Mac demeurent hors des garanties de cette livraison.
