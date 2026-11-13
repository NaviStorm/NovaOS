# NovaOS v0.16.6 — Transferts groupés

**État au 13 novembre 2026.** Prototype sur la branche `codex/es-groupees-0166`, à partir de `3193bc9108bdc771f4627f57c84e093375cc9ea4`. Le système publié reste NovaOS v0.16.5, NovaFS v0.9.13 P9. La décision MOA couvre l'expérimentation. L'intégration attend la revue MOE/MOA. [Résultats du prototype](#sauvegarde-dans-les-invités-novaos) · [Mesures vérifiées](validation/es-v0.16.6.json)

[Qualification élargie : éditions arbitraires, coupures F1/F2 et application UTM](es-v0.16.6-qualification.md).

## Audit préalable

Référence mesurée sur les deux architectures, QEMU TCG, une VM neuve, un fichier dense de 4 Mio, une insertion en tête et une sauvegarde sous un nom libre. L'histogramme compte un SEND suivi d'un RECEIVE réussi par le processus NovaFS, y compris le service de ses clients et le relais bloc. Le relais exécute à son tour chaque demande sur le pilote. Son trafic supplémentaire ne double pas le compteur publié. [Traces v0.16.5](validation/es-v0.16.5.json)

| Composante de la session | Calcul sur secteurs de 4 Kio | Tours |
|---|---|---:|
| Pièces de lecture bloc | 8 031 × 43 | 345 333 |
| Pièces d'écriture bloc | 1 511 × 43 | 64 973 |
| BEGIN et END | 2 × (8 031 + 1 511) | 19 084 |
| FLUSH | 137 | 137 |
| INFO et CLOSE bloc | 1 + 1 | 2 |
| Fenêtres de lecture du courtier | 1 024 × 47 | 48 128 |
| Autres échanges et effets des bornes du compteur | solde | 128 |
| Total | | 477 785 |

Chaque aller-retour bloc copie une trame de 128 octets du service au relais, du relais au pilote et à travers le portail, puis renvoie la réponse. La mémoire DMA reste privée au pilote matériel. Le montage contrôle les deux générations. Le premier G lit le contenu par fenêtres de 4 Kio. Lors de la sauvegarde, chaque lot de suffixe de 64 Kio repasse par le moteur pour vérifier la révision et le texte, préparer une transaction CoW, écrire les pages et attendre F1 et F2. Les 137 FLUSH et 1 511 écritures couvrent toute la session. Le profil de 4 Mio relève 229 767 allocations cumulées du service et 1 280 Kio de pic des extensions du tas, ainsi que 52 allocations de vi. Le critère de livraison est désormais une sauvegarde durable de 4 Mio en moins d'une seconde sur chaque architecture.

## Contrat proposé avant implémentation

Une boîte de transfert contient un seul lot de 1 à 4 096 octets par canal explicitement configuré par le lanceur. PUT et TAKE exigent la capacité locale désignée, son droit SEND ou RECEIVE, un tag non nul et une plage utilisateur entièrement lisible ou inscriptible. Un TAKE vérifie l'autre partie, le tag et la longueur exacte. Après consommation, le lot devient indisponible. La révocation d'un handle interdit sa réutilisation. La fermeture de l'endpoint libère le tampon. Aucun pointeur utilisateur ne persiste. L'allocation est faillible et plafonnée à une page de contenu par canal. Une opération de 16 ou 64 Kio se décompose en pages. Ce choix correspond au cadre DMA existant et à l'unité du moteur. Un lot ne confère aucun droit sur un fichier, un LBA ou une page physique.

Le plan de contrôle conserve les trames natives. Le service bloc transmet la demande au pilote, qui vérifie le bail et soumet une chaîne VirtIO limitée à la partition. L'accès aux octets passe par une paire supplémentaire réservée au moteur et au pilote. Une seule soumission matérielle porte sur la page, y compris sur secteurs logiques de 512 octets. Une erreur ou une complétion partielle invalide le lot entier. Une erreur d'écriture après soumission ne permet pas de recommencer automatiquement. FLUSH, son résultat et les deux barrières P9 conservent leur contrat. Pour la lecture versionnée du courtier, le service publie une page sur le canal de réponse après le contrôle du fichier et de sa révision. Tous les anciens sélecteurs et profils de trames conservent leur interprétation.

## Critères MOA et arbitrage du chemin de sauvegarde

La livraison exige simultanément ≤ 64 allers-retours IPC et < 1 000 ms pour la sauvegarde durable de 4 Mio. Le fichier d'essai est dense, avec une insertion au début et publication sous un nom libre. La fenêtre IPC commence à l'entrée de la publication dans vi, avant BEGIN. Elle finit au retour réussi de SAVE dans vi, après le renommage, la synchronisation et le retrait des délégations. Elle inclut la préparation de l'autorité et le contrôle de la source. La frappe de la commande, le montage, le premier parcours G et le rendu du succès sont hors de ce compteur. Le temps mural du banc inclut la frappe synthétique de la commande et le rendu du succès. La confirmation est envoyée dès réception de l'invite.

| Échanges livrés pendant une sauvegarde | AArch64 | x86_64 |
|---|---:|---:|
| vi ↔ nsh | 36 | 36 |
| nsh ↔ service NovaFS | 122 | 122 |
| NovaFS ↔ service bloc | 2 818 | 2 818 |
| Service bloc ↔ pilote | 2 818 | 2 818 |
| Total des paires requête/réponse | 5 794 | 5 794 |
| Dons synchrones de capacités acquittés, en sus | 2 | 2 |
| Total avec ces opérations de contrôle | **5 796** | **5 796** |

Le compteur enregistre chaque livraison réussie dans le noyau, avec les deux PID et le type du message. Les deux sens de chaque liaison sont équilibrés et aucun événement ne manque. Les 2 684 dépôts PUT et les 2 684 retraits TAKE réussis transportent le contenu des requêtes déjà comptées. Ce sont des appels locaux supplémentaires au noyau. Le portail DMA et les interruptions sont comptabilisés à part. Dans la même fenêtre le périphérique termine 1 240 lectures, 1 444 écritures et 134 FLUSH. Leur somme égale exactement les 2 818 demandes sur **chacune** des deux liaisons bloc. Sur la session complète, l'histogramme historique du seul processus NovaFS compte 7 224 tours contre 477 785 en v0.16.5, soit −98,49 %. Les deux périmètres de mesure sont distincts.

### Proposition à l’issue de la première mesure

Le dialogue actuel est séquentiel par page de 4 Kio et par barrière. Même avec des pages de données infiniment grandes et des calculs gratuits, les 134 FLUSH observés exigent déjà 268 allers-retours sur les deux liaisons bloc. La limite des écritures de 64 Kio par transaction impose 64 lots pour 4 Mio alignés, avant la publication du nom. Agrandir seulement la boîte IPC ne suffit donc pas.

La proposition est de conserver les services isolés et d'ajouter un travail de copie versionnée vers candidate privée. Le courtier soumettrait en une requête une description bornée des intervalles source et édités, le handle de destination et la révision attendue. L'autorité resterait limitée à cette opération. Le moteur garderait la décision sur les blocs CoW, le catalogue et les références. Le pilote garderait la soumission DMA. Un protocole de flux asynchrone pour le service bloc et le pilote devrait permettre de les faire travailler sur des anneaux bornés, avec crédits, numéros de génération, accès mémoire directionnel, achèvement strictement ordonné et révocation avant réemploi. Une commande composée peut porter plusieurs opérations dépendantes et exiger F1 avant la soumission d'une ancre, puis F2 avant de rapporter le succès. Toute erreur de complétion, de barrière ou de révocation interrompt la publication. Les lecteurs de P9 continueraient à voir seulement des générations valides selon leur contrat existant.

Les 64 lots P9 actuels impliquent des dépendances entre générations. Une réponse de complétion attendue pour chacun d'eux demeure un aller-retour logique même lorsqu'elle est lue dans un anneau. Il faut qualifier soit une préparation groupée à budget borné pour candidate neuve avec une seule décision de publication, soit un plan validé de dépendances pour plusieurs générations, exécuté sans aller-retour entre processus à chaque lot. La première piste doit qualifier la réservation de blocs hors de la génération publiée, les erreurs d'allocation et la découverte après coupure. La seconde doit prouver que le moteur peut planifier les générations suivantes avec un état intermédiaire privé et abandonner tous les descendants d'une écriture incertaine. Aucune des deux n'est démontrée par l'essai de cette version.

Budget de conception à vérifier end-to-end sur images jetables, en comptant aussi les signalisations, les complétions et les dons. Viser au plus 8 échanges vi/courtier, 8 courtier/moteur, 20 moteur/bloc et 20 bloc/pilote, avec 8 d'enveloppe pour contrôle et retrait. Les crédits ou les attentes synchrones qui dépassent cette enveloppe invalideraient le dessin. La taille physique des E/S, le coût des contrôles de chaque empreinte, le pic de mémoire et les délais de F1/F2 doivent être mesurés indépendamment. L'objectif de 64 est une condition d'essai, pas un résultat prédit à partir de ce budget.

La MOA a autorisé le **prototype isolé** de préparation groupée et d'une file de commandes bloc sur mémoire explicitement déléguée, avant d'intégrer un nouveau type de délégation ou de revoir le contrat de publication. La voie synchrone, le modèle de privilèges et le format sur disque servent de témoin. Pas de permission implicite de monter un autre volume, de programmer le DMA ou de remplacer une destination existante.

### Mise en œuvre examinée

- Le pilote possède une seule page utilisateur pour les E/S groupées, allouée lorsque le lanceur fournit la capacité. Deux boîtes superviseur de 4 Kio chacune desservent le profil CLI. Le plafond global est de huit boîtes. Les mappings DMA, les baux, les quotas du tas et le nombre de cadres DMA du profil sont conservés. Le noyau copie par mots volatils de 64 bits dans les pages DMA alignées et valide le nombre d'octets de la complétion VirtIO. Un lot incomplet ferme la session d'E/S. [Portail bloc](../kernel/src/hardware/block/lease.rs) · [Capacités IPC](../kernel/src/sched/process/ipc/bulk.rs)
- Dans le service, le client ordinaire négocie READ_PAGE au montage. Une lecture versionnée vérifie capacité, handle, identité de session, révision et étendue. Une vue déléguée conserve ses restrictions. Une écriture groupée dépose directement les octets immuables préparés par le moteur. L'interface historique de 96 octets et les fragments fichier de 88 octets restent accessibles. [Bibliothèque cliente](../user/storage/fileapi.rs) · [Adaptateur de bloc](../user/novafs/device.rs)
- Le moteur évite une recherche d'extent pour chaque page ajoutée au-delà de la fin validée. La page de bord conserve son contrôle. La préparation CoW peut relire une métadonnée déjà vérifiée pour la même famille, le même volume et la référence complète. Une page modifiée dans cette transaction a toujours priorité. Un défaut de cache impose la validation complète. Le service compile le SHA-256 portable pour la vitesse. Les empreintes P9 et leur domaine restent identiques. Le courtier aligne les fins de lots intermédiaires sur les pages de la destination pour éviter de réécrire une page de bord à chaque lot. [Écriture du moteur](../../novaFS/src/fs/data.rs) · [Préparation et publication](../../novaFS/src/engine.rs) · [Courtier](../../components/shell/vi_source.rs)

### Profilage des étapes

Séries exploratoires de 4 Mio sur QEMU TCG et secteurs de 4 096 octets. Ces essais cumulatifs associent parfois plusieurs changements. Une seule observation par étape ne permet pas d'attribuer sa variation au seul changement de code. Les mesures finales répétées et la mesure distincte du compteur sont données ci-dessous.

| Version ou étape | AArch64, ms | x86_64, ms |
|---|---:|---:|
| v0.16.5, référence publiée | 37 150 | 33 420 |
| Transport bloc par page | 2 681 | 2 098 |
| Avec lecture versionnée par page | 2 060 | 2 148 |
| Avec traitement des extents en append et copies DMA alignées | 1 239 | 1 338 |
| Avec optimisation de compilation SHA-256 | 1 000 | 922 |
| Avec cache propre en préparation, deux VM et régressions NovaFS simultanées | 4 674 | 3 377 |

La sonde finale mesure les phases successives de COPY_RANGE, cumulées sur le suffixe. Lecture des octets, validation de texte et de révision, puis préparation sans écriture, puis publication du lot jusqu'à l'acquittement de F2. La publication finale du nom, le préfixe et les contrôles supplémentaires sont inclus dans le chronomètre de bout en bout. Les coûts physiques ci-dessous sont inclus dans les phases, ils ne doivent pas leur être ajoutés.

| Mesure instrumentée, ms | AArch64 | x86_64 |
|---|---:|---:|
| Lecture et validation du suffixe | 327,33 | 288,77 |
| Préparation logique CoW du suffixe | 70,01 | 93,15 |
| Publication et persistance des lots du suffixe | 356,43 | 268,29 |
| Dans le pilote, lecture / écriture / FLUSH sur toute la sauvegarde | 54,34 / 64,75 / 11,62 | 39,31 / 32,22 / 8,38 |
| Appel de publication vu de vi | 801,34 | 694,71 |

Le service acquitte après F2 du renommage final. Une candidate temporaire peut survivre à une interruption antérieure au renommage. L'essai prouve le comportement au niveau du périphérique virtuel avec FLUSH honoré et cache writeback, puis la relecture indépendante. La perte d'alimentation du SSD hôte n'a pas été exercée. Les limites P9 face à une ancre déchirée s'appliquent. [Audit des ancres](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

### Reproduction

Construire le lecteur hôte puis le noyau depuis la racine. Utiliser une VM à la fois pour comparer les durées sans instrumentation. Le banc crée ses propres images, vérifie les deux générations puis compare source et destination octet par octet.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel
    for secteur in 4096 512; do
        python3 -B novaOS/tests/vi_performance.py "$arch" "novaOS/build/save-$arch-$secteur.json" 4194304 --secteur "$secteur" --retour-fin
    done
done
```

Pour compter les échanges, appliquer la fixture dans **une copie de travail isolée**. La sonde compte les livraisons en CPL3/EL0 et vérifie leur symétrie, la totalité des opérations du périphérique et l'absence d'événements non classés. Sur le profil testé, les PID 1 à 4 sont ceux du relais, du moteur, du pilote et de nsh. Le PID de vi est extrait de la trace. Un statut 1 avec les deux booléens de conformité rendus indique un échec du critère. [Sonde](../tests/compteur_sauvegarde.rs) · [Patch isolé](../tests/fixtures/io-0166-sauvegarde.patch) · [Vérificateur](../tests/vi_ipc_sauvegarde.py)

```sh
patch -p1 -F0 < novaOS/tests/fixtures/io-0166-sauvegarde.patch
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel
    python3 -B novaOS/tests/vi_performance.py "$arch" "novaOS/build/save-$arch.json" 4194304 --retour-fin
    python3 -B novaOS/tests/vi_ipc_sauvegarde.py "novaOS/build/cli-$arch-4096-vi-perf-4194304-index.serial" "novaOS/build/save-$arch.json"
done
```

## Sauvegarde dans les invités NovaOS

Profil mesuré le 13 novembre 2026 sur macOS 27.2 (26B123), QEMU direct 11.1.3 TCG, un vCPU, 256 Mio, disque VirtIO brut, cache writeback et FLUSH activé. Chaque essai démarre une VM sur une nouvelle image. Dans le vrai vi, le banc ouvre une source dense de 4 194 304 octets, parcourt son index, ajoute un `X` en tête et confirme une sauvegarde sous un nom libre. La destination mesure 4 194 305 octets. Les temps couvrent `:w` jusqu’à l’affichage du succès, réponse immédiate à la demande de confirmation comprise. Ils excluent l’ouverture, le parcours préalable et la vérification ultérieure au lecteur hôte. Les trois répétitions de chaque cellule utilisent des invités sans sondes. Des campagnes de qualification ont également utilisé le Mac. [Protocole de mesure](../tests/vi_performance.py)

| Architecture | Secteur | v0.16.5 de référence | Étape avec transferts de 4 Kio, médiane | Prototype, trois sauvegardes durables (ms) | Allers-retours |
|---|---:|---:|---:|---|---:|
| AArch64 | 4 096 | 37,15 s | 803,75 ms | 242,557 · 240,252 · 271,577 | 59 |
| AArch64 | 512 | — | 788,84 ms¹ | 250,119 · 247,060 · 250,846 | 59 |
| x86_64 | 4 096 | 33,42 s | 712,04 ms | 364,539 · 253,720 · 259,244 | 59 |
| x86_64 | 512 | — | 718,28 ms¹ | 270,528 · 257,673 · 255,734 | 59 |

¹ Une mesure de l’étape intermédiaire sur secteurs de 512 octets. Les références v0.16.5 proviennent du rapport publié. Les anciens comptes de 477 785 et le nouveau compte de 4 314 utilisent l’histogramme historique sur toute la session. Le tableau suivant porte uniquement sur la sauvegarde. [Référence v0.16.5](es-v0.16.5.md) · [Résultats par géométrie et par architecture](validation/es-v0.16.6.json)

| Activité mesurée pendant la sauvegarde | Étape pages de 4 Kio | Prototype |
|---|---:|---:|
| vi ↔ nsh | 36 | 5 |
| nsh ↔ NovaFS | 122 | 8 |
| NovaFS ↔ service bloc | 2 818 | 22 |
| Service bloc ↔ pilote | 2 818 | 22 |
| Dons synchrones acquittés | 2 | 2 |
| **Total interprocessus** | **5 796** | **59** |
| Total en comptant aussi 2 CREATE et 2 CLOSE locaux | 5 800 | **63** |
| Lectures / écritures / FLUSH physiques VirtIO | 1 240 / 1 444 / 134 | 1 039 / 1 048 / 2 |

Les 22 commandes bloc comprennent neuf lots de lecture, neuf d’écriture, deux lots avec barrière et deux lectures individuelles. Le pilote a vérifié 2 085 positions, copié 2 085 pages et soumis 2 087 commandes par son portail groupé. Vingt réservations et vingt fins de lot donnent **6 297 appels locaux** à cette interface. Les 25 PUT et 25 TAKE transportent les listes de blocs, les pages isolées et le préfixe. Ces appels franchissent la frontière utilisateur/noyau et contribuent au temps mesuré. Les 59 échanges comptent chaque paire de livraisons effectives entre processus, avec le contrôle de symétrie pour chaque liaison. [Compteur du noyau](../tests/compteur_sauvegarde.rs) · [Vérificateur de cohérence](../tests/vi_ipc_sauvegarde.py)

Sur secteurs de 4 Kio, la préparation logique et les transferts de données précédant F1 prennent 225,55 ms sur AArch64 et 232,54 ms sur x86_64 dans les exécutions instrumentées. L’écriture des métadonnées avec F1 prend 2,73 / 2,49 ms. L’écriture de l’ancre avec F2 prend 0,36 / 0,31 ms. La préparation calcule les empreintes pendant le transfert des pages privées. L’acquittement à vi exige le retour réussi de F2 puis la fin du dialogue de délégation.

### Contrat du prototype

- L’activation dans l’invité exige l’application explicite d’une fixture et le profil CLI de qualification 42. Le lanceur lie des endpoints nommés aux deux processus concernés. Le chemin normal conserve les opérations publiées. Le banc Darwin est un exemple Cargo avec feature explicite. Les anciens clients, le lecteur P9 et le noyau de `main` conservent leurs contrats. [Fixture de l’invité](../tests/fixtures/io-0166-prototype.patch) · [Banc Darwin](../../novaFS/lab/es-groupees/banc.rs)
- vi transmet son préfixe au courtier sur le canal de session. nsh vérifie l’insertion par comparaison du préfixe original, obtient les délégations de lecture de la source et de création sous le nom choisi, puis lit la confirmation. Le moteur contrôle la paire de cibles, l’identité, la révision, la taille, le parent, la disponibilité du nom et les quotas. Un écrivain exclusif conserve le moteur jusqu’à la décision. Chaque page source doit correspondre à son empreinte P9 et à son remplissage terminal. [Préparation](../../novaFS/lab/es-groupees/preparation.rs)
- Le moteur réserve dans une copie privée de l’allocateur qui protège les deux générations montables. Les données préparées peuvent atteindre le disque avant F1. La nouvelle génération ne devient visible qu’avec son manifeste et son ancre. Le moteur exécute la publication P9 habituelle en une transaction. Les deux barrières incluent les écritures dont elles attestent la persistance. Une erreur de complétion ferme le transport et interdit une nouvelle tentative automatique.
- Une réservation porte une direction, un tag croissant, une liste figée de 1 à 128 LBA et une étendue privée du moteur. Le noyau conserve sa propre copie des LBA, revalide les droits locaux, la direction, la position et le LBA au moment de la soumission matérielle. Chaque accès utilisateur passe par une copie contrôlée. Seules les pages privées du transport sont visibles en DMA. La fin du lot exige toutes les complétions et, si demandée, la complétion de sa barrière. Les limites, la géométrie et les droits du bail VirtIO restent applicables.
- Un retrait bloque les soumissions suivantes. Une commande déjà admise garde son tampon privé. Une tentative de retrait pendant son vol rend `WOULD_BLOCK`. Après sa complétion, le retrait peut être acquitté. Une complétion incertaine interdit de certifier le retrait. Le mécanisme historique de mise en quarantaine protège la réattribution du matériel. Un ancien handle, un tiers ou un tag périmé ne permet pas de reprendre le lot. Le banc invité exerce aussi la fermeture du pair et le renouvellement de l’endpoint.

### Intégrité, révocation et concurrence

| Essai | Résultat observé |
|---|---|
| Simulation P9, 39 frontières × 3 injections, pour chaque secteur et architecture hôte | 105 anciens états intégraux, 9 nouveaux intégraux et 3 refus de montage sur ancre déchirée, soit 117 par cellule et 468 au total. Après acquittement, le nouvel état est relu. |
| Deux écritures en concurrence vers le même nom | Un seul gagnant, source et destination vérifiées après remontage. Révision périmée et nom déjà occupé refusés. |
| Deux commandes en attente et retrait en cours de transaction sur Darwin | Première page déjà admise terminée, pages suivantes intactes, requête en attente et ancien descripteur refusés. Le nom final reste absent après retrait avant publication. |
| QEMU, arrêt après un lot de données, après F1 et après F2, secteurs 512 et 4 096 sur les deux architectures | Après les deux premiers arrêts, source intacte et nom absent. Après F2 avant retour à vi, source intacte et nouvelle destination complète. Vérification hôte, nouveau démarrage, montage normal, seconde vérification. |
| QEMU, retrait après publication d’un descripteur OUT dans la virtqueue | Le retrait immédiat rend WOULD_BLOCK et ferme l’admission. Le retrait après complétion invalide le pair. Le compteur de soumissions reste fixe. Relecture exacte de la source et nom absent à froid. La sonde vérifie aussi le refus d’un LBA voisin au même index. |
| Compatibilité | Anciennes images clientes de v0.16.5 exécutées sur le transport intermédiaire sur les deux architectures. Lecteur NovaFS de la référence compilé séparément, commande verify sur chaque image du banc Darwin. Le banc invité compare intégralement source et destination après arrêt. |

Les 16 scénarios QEMU d’arrêt et de retrait utilisent une source de 524 416 octets pour franchir une frontière de lot. La série de douze mesures durables utilise la source de 4 Mio. Le défaut connu des ancres P9 impose toujours un refus explicite en cas de déchirure ambiguë. Le succès d’un FLUSH dans cette campagne porte sur le backend virtuel retenu. Une perte d’alimentation du Mac, le QCOW2 de l’application UTM et une charge multiprocesseur restent hors qualification pour le nouveau contrat. [Injection dans la VM](../tests/coupure_cow_groupee.py) · [Audit des ancres](../../novaFS/docs/novafs-p9-ancres-dechirees.md)

### Mémoire et gros fichiers

| Pic résident observé (Kio), secteurs 4 096 | AArch64 v0.16.5 → prototype | x86_64 v0.16.5 → prototype |
|---|---:|---:|
| vi | 444 → 444 | 448 → 448 |
| nsh | 460 → 460 | 472 → 476 |
| Service NovaFS | 1 716 → 2 184 | 1 736 → 2 208 |
| Pic de cadres au compteur du noyau (nombre) | 827 → 962 | 836 → 973 |

L’étape intermédiaire mesure respectivement 1 660 et 1 684 Kio pour NovaFS. Le lot réutilise jusqu’à 512 Kio de pages de données et la file de métadonnées retient au plus 64 pages. Le pilote passe de 16 à 36 Kio dans le profil instrumentable, avec 16 Kio de pile utilisateur. Les maxima résidents sont échantillonnés. Les budgets système et les plafonds de l’allocateur P9 s’appliquent. Les comptages d’allocations du moteur hôte sont fournis dans les résultats avec leur propre périmètre.

La voie de lecture paginée a été exercée sur fichiers denses. Le premier écran sur 16 / 100 / 1 024 Mio arrive en 38,60 / 47,85 / 98,29 ms sur AArch64 et 25,80 / 30,40 / 82,60 ms sur x86_64 après montage. Pour 1 Gio, le montage complet prend 208,00 / 182,46 s. Une navigation de 40 lignes coûte 22,13 / 24,20 ms. Le premier parcours jusqu’à la fin lit et indexe l’intégralité en 685,27 / 572,29 s. Le seuil de sauvegarde de 4 Mio ne s’extrapole pas à ces tailles.

### Régressions et limites de l’arbitrage

Les deux invités ont exécuté les 25 démarrages, 24 groupes bloc, trois contrôles ELF/processus/IPC, quatre essais CLI et les suites fichiers, espaces de noms, nsh, scripts, terminal, vi, écran, volumes et mémoire sur l’étape intermédiaire. Les essais du prototype ont ensuite rejoué le contrat IPC et la suite vi sur les deux géométries, avec 17 ELF collectés, 16 fichiers relus par cellule, conflit concurrent, retrait, remontage et persistance. Sur le moteur UTM 10.0.12, les régressions AArch64 couvrent 11 profils DMA/GICv3 et 23 profils DMA séquentielle. Elles qualifient la protection historique.

NovaFS a exécuté 196 tests standards et 197 instrumentés par architecture Darwin, avec six tests ignorés dans chacune de ces deux suites, un million d’opérations différentielles et les 2 304 injections historiques par architecture. Les contrôles complémentaires comprennent snapshots, clones, chiffrement, compression, migration et quatre relectures P9 croisées. Le banc distinct à cinq processus Darwin ramène les échanges de 2 777 à 45, retrait inclus, et à 46 avec le déclencheur externe. [Résultats détaillés](validation/es-v0.16.6.json)

**Arbitrage proposé.** Les deux seuils sont atteints pour une insertion ASCII en tête d’un fichier dense ASCII/LF autonome, publiée sous un nom libre. Le prototype n’a pas qualifié la sauvegarde groupée d’éditions arbitraires, de texte multioctet, de fichiers creux ou de pools chiffrés. Avec les quatre contrôles locaux, la marge est d’une unité. Une lecture individuelle supplémentaire coûte deux échanges sur les liaisons bloc et porterait le total conservateur à 65. Une généralisation demande un plan d’édition borné pour les préfixes et suffixes multiples, des tests de fragmentation, de conflits et d’UTF-8, puis une mesure sur le stockage UTM retenu. La prochaine décision MOE/MOA concerne cette qualification élargie. Le format P9, PC1, le remplacement d’un fichier existant et le jalon stable v0.30.0 conservent leur statut.

### Reproduire le prototype isolé

Depuis la branche expérimentale, créer des copies indépendantes avec `git worktree add --detach <dossier> HEAD`. Dans chaque copie, construire le lecteur hôte puis appliquer la fixture de base. Le noyau ordinaire ne dépend d’aucune de ces fixtures.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
patch -p1 -F0 < novaOS/tests/fixtures/io-0166-prototype.patch
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel ipc-test vi-test
    for secteur in 4096 512; do
        for essai in 1 2 3; do
            python3 -B novaOS/tests/vi_performance.py "$arch" \
                "novaOS/build/mesure-$arch-$secteur-$essai.json" 4194304 --secteur "$secteur" --retour-fin
        done
    done
done
```

Pour les compteurs, appliquer en plus `io-0166-prototype-sonde.patch`, reconstruire, exécuter le même scénario et passer sa trace série et son JSON au vérificateur. Pour les coupures, utiliser dans une autre copie la base et `io-0166-prototype-coupure.patch`, puis reconstruire.

```sh
python3 -B novaOS/tests/vi_ipc_sauvegarde.py novaOS/build/cli-aarch64-4096-vi-perf-4194304-index.serial novaOS/build/mesure-aarch64-4096-1.json
python3 -B novaOS/tests/coupure_cow_groupee.py aarch64 4096 novaOS/build/coupures-aarch64-4096.json
```

Pour le banc à cinq processus, construire le binaire `novafs` dans une copie du commit `3193bc9` et exporter son chemin absolu dans `NOVAFS_LECTEUR`. Exécuter ensuite dans la branche expérimentale, sans appliquer de patch invité.

```sh
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    NOVAFS_TARGET="$cible" novaFS/lab/run run --offline --features prototype-es-groupees --example es-groupees
done
```
