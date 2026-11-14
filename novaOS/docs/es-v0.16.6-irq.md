# VirtIO-blk, scrutation et réveil du pilote

Qualification du prototype `codex/es-groupees-0166` à partir de `4ae44124b42ad9cf971ee8c134cb2e4145aa17c5`. L'expérience change l'attente d'une commande bloc déjà soumise. Le scénario principal insère 4 Mio dans une source de 4 Mio, réalise aussi des éditions localisées et publie une destination de 8 Mio + 1 octet sous un nom libre. Le chronomètre de sauvegarde couvre la commande `:w`, le dialogue de consentement et la réponse de succès après F2. Le banc conserve aussi les instants de consentement et d'acquittement. [Audit précédent](es-v0.16.6-vcpu.md)

**Décision recommandée. Conserver le polling par défaut.** Sous UTM, avec le binaire hors sonde, 21 des 38 sauvegardes IRQ ont atteint ou dépassé une seconde, contre 9 sur 38 en polling. Le candidat préserve l’intégrité dans les essais, mais n’apporte pas la réduction CPU demandée sur la charge de référence.

## Chemin matériel et réveil

Le pilote de référence arme un ticket contenant le prochain index de l'anneau `used`, soumet une chaîne puis écrit dans le registre de notification. Il scrute d'abord l'index de complétion, puis le compteur d'IRQ. La boucle `wait` consulte le compteur architectural et utilise `spin_loop`. Elle s'arrête après quatre millions d'itérations au plus, avec une seconde borne exprimée en ticks bruts. Le pilote consomme donc du temps de vCPU pendant qu'une requête peut déjà être en traitement dans QEMU. La vérification ultérieure couvre la tête de chaîne, la longueur, le statut du périphérique et les défauts IOMMU. Les files de contrôle IOMMU et les procédures de retrait ont leurs propres attentes. [Transport et retrait](../kernel/src/hardware/block/mod.rs) · [Anneau et descripteurs](../kernel/src/hardware/block/queue.rs) · [Attente de référence](../kernel/src/hardware/block/io.rs)

| Invité | Route du disque | Mise en veille du vCPU lorsque la file prête est vide |
|---|---|---|
| AArch64, QEMU de développement | INTx PCI sur GICv2, route exclusive contrôlée à l'énumération | `WFI`, puis livraison de l'IRQ après restauration du masque |
| AArch64, UTM autonome | INTx PCI sur GICv3 sans ITS, route du disque sélectionné | Même ordonnanceur et même attente idle |
| x86_64, QEMU | MSI-X du disque vers le LAPIC, vecteur `0x51` | `sti; hlt; cli` sous le contrôle de la boucle idle |

Le prototype utilise un événement privé de l'ordonnanceur sur le BSP. Une seule chaîne peut être en vol dans le transport qualifié. Le propriétaire garde le mutex de session et les cadres DMA épinglés. Sous masque IRQ local, il revérifie ensemble l'index `used`, la consommation du ticket et l'incrément attendu du compteur d'IRQ. Si les trois concordent, il lit le descripteur avec la barrière mémoire. Sinon, il arme l'événement et se bloque sous le même masque. Une IRQ arrivée avant l'armement satisfait le premier contrôle. Une IRQ rendue pendante pendant cette section sera livrée après la commutation. Le gestionnaire fait passer le thread bloqué en file prête, sans prendre le mutex de session. Le propriétaire vérifie de nouveau la complétion au retour. Le marqueur de ticket est séparé des 16 bits de l'index, y compris lors du passage de 65 535 à zéro. Le mécanisme de notification des anneaux VirtIO permet des notifications supplémentaires et exige la vérification de l'état partagé. [Fixture d'attente](../tests/fixtures/io-0166-irq-attente.patch) · [Ordonnanceur](../kernel/src/sched/mod.rs) · [Spécification VirtIO 1.2, split virtqueues](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

Une interruption sans ticket concordant ne consomme pas la commande. Sur le GIC, la première assertion isolée sans bit de file dans l'ISR laisse la route installée disponible pour la vraie complétion. Huit assertions de ce type consécutives masquent la ligne pour borner une tempête. Une interruption de file remet ce compteur à zéro. En l'absence d'IRQ valide, le tick de l'ordonnanceur réveille le propriétaire après une seconde d'horloge invitée. Un repli en scrutation est alors permis sur la même commande, avec un budget total de deux secondes depuis le début de l'attente et quatre millions d'itérations de repli au plus. Il reste nécessaire d'avoir observé la bonne IRQ. Une expiration conserve l'erreur et le chemin de retrait/quarantaine. Ces bornes supposent un vCPU exécuté, un timer périodique livré et un ordonnanceur qui progresse. Suspendre la VM suspend aussi la progression du pilote.

Sur x86, la première ouverture du disque appelle la calibration TSC existante sur 64 ticks PIT à 250 Hz, soit environ 256 ms de scrutation une fois au démarrage. Le candidat garde la fréquence en mémoire pour les échéances suivantes. Ce coût précède le début de la mesure de sauvegarde. AArch64 lit la fréquence architecturale du compteur. Pendant un retrait, l'attente bornée de la fin de chaîne avant le reset reste celle du pilote historique. [Timer x86](../kernel/src/arch/x86_64/timer.rs) · [Retrait et quarantaine](../kernel/src/hardware/block/lease.rs)

Les droits sur la file, le prêt immuable, la révocation et la publication CoW passent par les mêmes entrées. L'IRQ rend possible la collecte d'une commande. L'acquittement durable reste conditionné au succès des deux FLUSH ordonnés du protocole F1/F2. Un arrêt forcé de VM avec relecture du QCOW2 exerce le modèle d'interruption du moteur. La perte d'alimentation de l'hôte reste hors du banc. [Protocole de sauvegarde](es-v0.16.6-dma.md) · [Garanties conditionnelles de stockage](../../novaFS/docs/novafs-publication-conditionnelle.md)

## Méthode de comparaison

Les bancs alternent l'ordre des deux modes sur des images jetables identiques au sein de chaque paire. Ils distinguent les ELF de latence des ELF de diagnostic. Une sonde invitée conserve jusqu'à 256 durées individuelles de commandes en ticks et leur type, soit 2 056 octets de stockage statique. Le journal est émis à la fin de la session, hors du chronométrage de sauvegarde. Le dépouillement confronte le nombre et la somme des durées de chaque type aux compteurs indépendants du pilote. Les compteurs de parcage, de réveil par IRQ, de réveil par échéance, de repli et de parasite sont réservés à la variante instrumentée. [Trace individuelle des commandes](../tests/fixtures/io-0166-irq-latences.patch) · [Compteurs de réveil](../tests/fixtures/io-0166-irq-sonde.patch)

Le banc QEMU direct utilise des disques bruts et contrôle les octets après l'exécution. Le banc UTM utilise l'application 5.0.8 et son moteur QEMU 10.0.12, le profil UEFI/GICv3, un vCPU TCG, 256 Mio et des QCOW2 jetables. Il redémarre la VM, ouvre la destination, exporte le disque arrêté puis vérifie les générations, la source et la destination par un lecteur indépendant. Sous Darwin, la sonde relie le PID QEMU à l'UUID de la VM et identifie le seul thread vCPU par le moniteur. Les compteurs par niveau de cœur mesurent son temps CPU sur Performance et Efficiency. Le temps actif de l'ordonnanceur invité inclut les éventuelles interruptions d'exécution de l'émulateur. Les attentes VirtIO mesurées du côté invité, le temps de service du backend et le CPU du vCPU peuvent se recouvrir. [Banc QEMU](../tests/irq_qemu.py) · [Banc UTM](../tests/irq_utm.py) · [Mesure Darwin](../tests/vcpu_darwin.py)

Les percentiles utilisent le rang supérieur `ceil(n × p)`, sans interpolation ni retrait de valeurs extrêmes. Les essais de diagnostic fournissent les distributions de commandes, d'IRQ et de réveils ainsi que le nombre d'échanges IPC, contrôles compris. Les essais de latence ordinaire mesurent l'effet du pilote sans la sonde invitée. Le temps mural commence avant le consentement automatisé. Le CPU du vCPU est mesuré aux bornes qui l'encadrent. Le débit utile divise la taille de la destination par la durée de publication retenue. Les quantités réellement lues et écrites par le backend dépendent du CoW et des métadonnées.

## Binaires et reproduction UTM

Le banc a utilisé un Mac M4 Max avec 128 Gio sous macOS 27.2 (26B123). `qemu-img` 11.1.3 prépare et inspecte les images. L'application exécute son propre moteur. Les secteurs physiques et logiques annoncés à l'invité ont la même taille. Le profil « stockage » réalise des écritures de 4 Mio suivies de `fsync` sur un fichier jetable du même hôte. « Repos » désigne l'absence de charge ajoutée par le banc. La politique ordinaire laisse macOS choisir les cœurs. Le témoin `-b` applique `taskpolicy` au seul moteur de la VM jetable. La répartition P/E est mesurée pour chaque sauvegarde.

| ELF UTM | SHA-256 | text | data | bss |
|---|---|---:|---:|---:|
| Polling | `27f221da5570d2b156c1220175dfc6a84baad25df2b68b53e8ea5a09074eb16b` | 1 934 055 | 12 720 | 16 800 |
| IRQ | `89fb1955d79672e767d13b0fc92c61c3d1a4487abcbc0578f6b7cb0dcdfa01a2` | 1 936 055 | 12 720 | 16 832 |
| Polling instrumenté | `0de61a17539855b61a480a6060035f0ad51cb83ff3788eaad234f42e72d61df6` | 1 946 375 | 12 720 | 19 737 |
| IRQ instrumentée | `e44994b85891532ddb602d770a069185643296969247c62004a98e0494baeb0a` | 1 951 443 | 12 720 | 19 840 |

Tailles en octets d'après `llvm-size`. Hors sonde, le candidat ajoute 2 000 octets à la colonne text et 32 à la BSS. La qualification de sauvegarde utilise les invités de 256 Mio. À 64 Mio, le banc a lancé les deux VM, mais l’amorçage n’a pas atteint le contrôle ABI. Le banc le classe comme non exécuté. Le canal de réveil et son armement sont statiques. Chaque relecture conserve le pic de cadres de la session entière. L'arène DMA multipage garde son budget existant.

Préparer quatre archives séparées suivant le [protocole système UTM](es-v0.16.6-dma.md) et la [construction des deux variantes CPU](es-v0.16.6-vcpu.md#reproduction). Sur les deux archives IRQ, appliquer en dernier `io-0166-irq-attente.patch`. Sur l'archive IRQ instrumentée, ajouter `io-0166-irq-sonde.patch`. Sur les deux archives instrumentées, ajouter `io-0166-irq-latences.patch` après `io-0166-vcpu.patch`. Utiliser `patch -p1 -F0` à la racine de chaque archive puis construire le noyau depuis son répertoire `novaOS`. Les variables ci-dessous désignent le profil privé de test, le générateur NovaFS, les quatre ELF construits et un nouveau dossier de sortie.

```sh
python3 -B novaOS/tests/irq_utm.py campagne "$PROFIL_UTM" "$GENERATEUR" \
  "$POLLING" "$IRQ" "$POLLING_SONDE" "$IRQ_SONDE" "$SORTIE"
python3 -B novaOS/tests/irq_utm.py rapport "$SORTIE" "$SORTIE/resultats-apparies.json"
```

## Intégrité et défauts injectés

- Les 20 cibles de régression NovaOS ont réussi sur chaque architecture. Elles incluent les 25 démarrages, les 24 groupes bloc, ELF/processus/IPC, CLI, `nsh`, `vi`, le montage avec progrès et les mesures mémoire. Les quatre témoins du chemin polling et les anciens appels bloc ont été rejoués. Les 11 scénarios GIC/DMA et les 23 scénarios DMA séquentiels AArch64 ont également réussi. [Commandes, empreintes et résultats par architecture](validation/es-v0.16.6-irq-regressions.json)
- NovaFS a réussi, sur ARM64 natif et x86_64 sous Rosetta, 196 tests standards et 199 instrumentés par architecture, avec six ignorés dans chaque suite. Le million d'opérations différentielles, les 2 304 injections historiques, les 486 injections de publication groupée et les échanges interarchitectures P9, F01 et LZ4 ont été rejoués. Les contrôles de concurrence, de révocation et de quarantaine ont donné le résultat attendu. Le dossier de qualification distingue les 113 commandes réussies du jeu définitif et les incidents des étapes de préparation. [Régressions et oracles de relecture](validation/es-v0.16.6-irq-regressions.json)
- Les quatre invités dédiés aux défauts de notification ont présenté six situations matérielles en 512 et 4 096 octets. Un SPI isolé sans bit de file a laissé le propriétaire endormi jusqu'à la vraie IRQ, puis la lecture a réussi. Huit assertions successives ont masqué la ligne et entraîné un timeout en 1 242,951 et 1 242,796 ms. Sur x86, avec MSI-X masqué et descripteur déjà rendu, le propriétaire a expiré en 1 127,592 et 1 157,015 ms. Chaque nouveau propriétaire a relu les bons octets après retrait. Le modèle des ordres possibles entre parcage et interruption emploie la vraie fonction de collecte et des doubles de la file et de l'ordonnanceur. Il a exercé sur les deux architectures les publications avant ou après parcage, les doublons, les limites des compteurs, les échéances et l'absence de notification. [Défauts injectés, délais et vérification des disques](validation/es-v0.16.6-irq-defauts.md) · [Modèle de l'attente](../tests/irq_attente_modele.rs)
- Sur les binaires définitifs, **6/6 coupures UTM** aux frontières pages, F1 et F2 ont passé en 512 et 4 096, puis **2/2 redémarrages supplémentaires** depuis des copies des exports F2/512 ont réussi. Les 36 coupures et les 20 défauts/retraits QEMU sont consignés avec leurs oracles. La destination reste absente au point pages et à F1, puis présente et complète à F2 dans ces essais. [Exports, générations et états après redémarrage](validation/es-v0.16.6-irq-regressions.json)

Un redémarrage UTM de préparation a échoué avant l'invite `nsh` après une coupure F2/512, sous charge des régressions et avec une ébauche qui omettait un contrôle d'échéance après réveil. Le client a rendu le code 26 (erreur d'E/S). Les compteurs de cette session indiquent jusqu'à 12,411 s pour un aller-retour IPC. Le journal ne permet pas d'attribuer l'erreur à un délai précis. La GPT, les générations 67/68, la source de 4 Mio et la destination de 8 Mio étaient intègres sur l'export immédiatement après la coupure. Une copie du même QCOW2, d'empreinte `a47355913690f81f8d1cf36e9df26fb76fe3a9d4e26e5975fd1380a547cb5591`, a ensuite atteint l'invite à charge réduite. L'essai F2/512 sur le binaire définitif a passé dès le premier redémarrage. [Incident et contre-essais](validation/es-v0.16.6-irq-regressions.json)

Les deux essais instrumentés de mise au point sous UTM ont pris respectivement 428,061 ms en polling et 760,784 ms en IRQ. Le candidat a compté 3 complétions immédiates et 187 parcages réveillés par IRQ. Les deux essais donnent 56 échanges, 190 commandes VirtIO, deux FLUSH et le même SHA-256 après redémarrage. Ils ont partagé le Mac avec d'autres travaux de qualification et sont exclus des percentiles appariés. [Traces de mise au point](validation/es-v0.16.6-irq-pilotes.json)

## QEMU direct

Les 136 sauvegardes et leurs relectures ont réussi. Les 48 sauvegardes instrumentées ont utilisé 43 à 57 échanges et deux FLUSH chacune. Les 24 essais IRQ instrumentés ont enregistré 4 254 parcages, tous réveillés par IRQ, et 54 complétions immédiates. Les pics de cadres et les maximums par processus sont identiques entre modes sur les profils ordinaires. [Détail QEMU par architecture, secteur et scénario](es-v0.16.6-irq-qemu.md) · [Mesures individuelles QEMU](validation/es-v0.16.6-irq-qemu.json)

| Commande au succès, par architecture, 22 mesures ordinaires par mode | Polling p50 / p95 / p99 (ms) | IRQ p50 / p95 / p99 (ms) |
|---|---:|---:|
| AArch64 | 129,613 / 134,853 / 135,509 | 129,166 / 134,330 / 135,210 |
| x86_64 | 209,389 / 217,191 / 217,216 | 213,238 / 218,425 / 218,874 |

Sur la grande édition, l'IRQ a raccourci 11 des 32 sauvegardes appariées hors sonde. La médiane des différences de CPU du thread TCG pendant la publication est de −3,24 / −3,84 ms sur AArch64 (512 / 4 096) et de −5,00 / +0,97 ms sur x86_64. La scrutation disparaît pendant le parcage. Le coût de la commutation et de la reprise s'ajoute sur des requêtes RAW qui prennent fréquemment quelques dizaines de microsecondes. Le gain en CPU et le gain en temps de réponse dépendent ainsi du scénario.

## Application UTM, 48 paires avec redémarrage

Les 96 mesures ont eu lieu du 13 novembre à 22 h 49 au 14 novembre à 1 h 07 CET, après les régressions et les mesures QEMU directes. Les 96 redémarrages et les 96 relectures indépendantes ont réussi. Les traces du thread TCG attribuent la totalité de son temps CPU à des cœurs Efficiency dans les 96 fenêtres. La campagne ne fournit donc aucune comparaison IRQ/polling sur cœur Performance sous UTM. La charge globale et les données du backend sont enregistrées par essai. [Relevés individuels, IPC, requêtes bloc, P/E, génération, SHA-256 et percentiles](validation/es-v0.16.6-irq-utm.json)

Le tableau donne le temps de la commande au succès, en ms. Δ CPU est le p50 des différences appariées de temps CPU du seul vCPU (IRQ moins polling). Les nombres de dépassements incluent toute sauvegarde d'au moins une seconde. « Combinée » désigne la destination de 8 388 609 octets et « locale » l'insertion de Ω en tête pour une destination de 4 194 306 octets.

| Cellule et profil | Paires | Polling p50 / p95 / p99 | IRQ p50 / p95 / p99 | ≥ 1 s polling / IRQ | Δ CPU apparié (ms) |
|---|---:|---:|---:|---:|---:|
| C1 · combinée 4 096, stockage | 20 | 929,4 / 1 165,6 / 1 951,2 | 1 059,2 / 1 275,0 / 1 574,2 | 5 / 11 | 149,8 |
| C2 · combinée 4 096, stockage, arrière-plan | 8 | 750,3 / 995,0 / 995,0 | 1 107,8 / 1 307,0 / 1 307,0 | 0 / 5 | 144,1 |
| C3 · combinée 4 096, repos | 6 | 1 004,9 / 1 135,7 / 1 135,7 | 1 123,9 / 1 229,0 / 1 229,0 | 4 / 5 | 132,9 |
| C4 · locale 512, repos | 4 | 522,5 / 562,7 / 562,7 | 628,1 / 689,9 / 689,9 | 0 / 0 | 65,8 |
| C5 · sonde, combinée 4 096, stockage, arrière-plan | 4 | 989,0 / 1 355,2 / 1 355,2 | 988,1 / 1 015,8 / 1 015,8 | 2 / 2 | -75,3 |
| C6 · sonde, combinée 512, repos | 4 | 919,5 / 1 013,2 / 1 013,2 | 1 014,8 / 1 198,6 / 1 198,6 | 2 / 3 | 21,7 |
| C7 · sonde, locale 512, repos | 2 | 576,1 / 580,9 / 580,9 | 602,4 / 791,3 / 791,3 | 0 / 0 | 28,0 |

Sur les 20 paires ordinaires sous charge de stockage (C1), l'IRQ a raccourci cinq sauvegardes et réduit le CPU de six. La différence appariée médiane est de **+187,0 ms de temps mural et +149,8 ms de CPU**. Le débit utile médian de la grande destination passe de 8,5 à 7,5 Mio/s. Le thread TCG exécute 3,043 milliards d'instructions hôtes au p50 en polling et 3,670 milliards en IRQ. Les p50 de CPU sur le vCPU sont respectivement 739,4 et 910,8 ms. Après consentement, les p50 de temps mural sont de 891,5 et 1 020,4 ms. Le candidat manque donc aussi l'objectif sur cette fenêtre plus étroite.

La comparaison par paires donne un temps CPU inférieur avec IRQ dans **7 des 38 paires hors sonde**. Sur les grandes sauvegardes instrumentées au repos (C6), le temps actif médian de l'ordonnanceur invité est de 752,7 ms en polling et de 753,8 ms en IRQ. Le temps idle médian passe de 120,2 à 148,8 ms. Le parcage effectivement observé libère le thread en attente, mais l'émulation comprend aussi la livraison de l'IRQ, la sélection du thread prêt et la reprise de la pile du pilote. La durée attribuable à chacune de ces étapes dans l'application UTM reste inconnue. Le plus petit p99 de C1 pour le candidat coexiste avec un p95 et une fréquence de dépassement supérieurs. En C2 et C3, p95 et p99 sont tous deux plus élevés avec IRQ. À ces effectifs le p99 est le maximum de chaque cellule. Il ne permet pas d'établir une équivalence statistique des queues de distribution.

### Lectures individuelles et extrêmes

Les vingt répétitions C1 sont présentées dans l'ordre d'exécution des paires. Les autres cellules et chaque requête instrumentée sont dans le fichier de résultats. Les durées sont en ms.

| Mode | Répétitions 1 à 10 | Répétitions 11 à 20 |
|---|---|---|
| Polling | 575,1 · 757,9 · 1 951,2 · 756,5 · 765,8 · 1 165,6 · 1 074,3 · 740,2 · 949,7 · 929,4 | 942,7 · 1 046,9 · 759,6 · 810,4 · 985,0 · 947,7 · 765,0 · 739,5 · 970,0 · 1 157,0 |
| IRQ | 733,3 · 855,7 · 978,2 · 1 155,4 · 958,5 · 910,7 · 898,7 · 1 574,2 · 1 136,8 · 901,4 | 1 275,0 · 1 072,4 · 1 059,2 · 1 076,4 · 1 230,8 · 1 166,9 · 1 161,8 · 942,2 · 1 150,9 · 928,0 |

- **Polling, C1-r03, 1 951,191 ms.** Le vCPU a comptabilisé 1 520,818 à 1 545,821 ms de CPU selon l'encadrement libproc. Le backend a enregistré 801,267 ms cumulées pour les lectures, 41,458 ms pour les écritures et 3,231 ms pour les deux FLUSH. L'IRQ de la même paire a pris 978,244 ms et le backend 10,030 ms en lecture. Les deux exécutions ont donc effectivement reçu des temps de service très différents malgré la charge prescrite identique.
- **IRQ, C1-r08, 1 574,229 ms.** La publication après consentement a pris 1 529,924 ms. Le vCPU a comptabilisé 890,669 à 914,366 ms de CPU. Les 143 lectures du backend ont cumulé 436,202 ms, les 45 écritures 23,129 ms et les deux FLUSH 3,117 ms. La différence entre le temps mural et le CPU comptabilisé vaut 659,863 à 683,560 ms. Les compteurs ne déterminent pas le temps exact de préemption par macOS.
- **IRQ, C2-r02, 1 107,751 ms.** Cet autre dépassement inclut 1 069,252 ms après consentement, tandis que les 143 lectures ont cumulé 9,665 ms et les deux FLUSH 2,969 ms côté backend. Les lectures lentes de l'extrême C1-r08 n'expliquent donc pas tous les dépassements.

Les 20 sauvegardes instrumentées ont utilisé **56 échanges** pour la grande édition et **41** pour l'édition locale, contrôles compris. Les dix essais IRQ comprennent 1 864 complétions matérielles, dont 20 immédiates et **1 844 parcages avec 1 844 réveils par IRQ**. Aucun réveil par délai, repli, timeout ou parasite en fonctionnement normal. Chaque commande de sauvegarde a provoqué deux FLUSH du backend et une progression de la génération vérifiée après redémarrage. Hors sonde, le pic de la session est de 9 735 cadres de 4 Kio pour la grande édition et de 1 175 pour la locale, identique en polling et IRQ. La sonde ajoute un cadre au pic pour chaque scénario. Le pilote garde les 80 cadres de son arène DMA.

En C5, le tableau suivant porte sur 572 lectures, 180 écritures et huit FLUSH **par mode**. Les latences individuelles sont mesurées au niveau du pilote, en µs, et incluent l'attente de la complétion.

| Requête | Polling p50 / p95 / p99 (µs) | IRQ p50 / p95 / p99 (µs) |
|---|---:|---:|
| lecture | 166,0 / 7 412,0 / 8 344,0 | 206,0 / 360,0 / 736,0 |
| ecriture | 425,0 / 1 015,0 / 6 136,0 | 462,0 / 1 107,0 / 11 442,0 |
| flush | 316,0 / 11 690,0 / 11 690,0 | 380,0 / 10 248,0 / 10 248,0 |

Pour cette cellule, les p50 des deux barrières F1 puis F2 sont de 3,1 puis 0,3 ms en polling, 3,4 puis 0,4 ms en IRQ. Le JSON conserve chaque durée en ticks et le détail des secteurs 512. Pendant la mise en veille du Mac, le journal Darwin signale une fermeture de capot à 22 h 59 min 38 s et plusieurs réveils de maintenance jusqu'à 23 h 09 min 32 s. Cela recoupe les sessions C5-r01 des deux modes. Les observations restent incluses et identifiées. La fenêtre de commande est chronométrée avec l'horloge monotone du Mac ; le temps de sommeil physique ne peut pas en être déduit. Une inhibition temporaire de veille automatique est ensuite attachée à la durée du processus de campagne. Les autres cellules, dont la cellule principale C1, conservent leur propre série. Un hôte sous tension, y compris lors d'un arrêt forcé de la VM, conserve les limites de durabilité déjà décrites pour QCOW2.

## Arbitrage

**Garder le pilote en polling et laisser le candidat IRQ désactivé dans les archives expérimentales.** Les tests démontrent un parcage borné, une gestion des notifications parasites et la préservation de l'intégrité dans le modèle exercé. Les temps de service RAW directs sont courts et les économies de CPU y varient selon la cellule. Sur UTM, 11 des 20 grandes éditions C1 dépassent le seuil avec IRQ, contre 5 avec polling. Les interruptions ne suffisent pas pour qualifier une sauvegarde durable systématique sous une seconde. Le polling conserve lui aussi des dépassements à traiter. L'ancien essai de veille hybride et ses limites de répétition figurent dans le [rapport multipage](es-v0.16.6-dma.md). La branche livrée conserve les interfaces et P9 de la référence.

La qualification suivante mesure la QoS effective, les séjours du vCPU sur P et E et les intervalles de préemption du polling. [Qualification des performances hôte](es-v0.16.6-qos.md)
