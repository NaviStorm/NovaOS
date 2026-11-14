# NovaOS v0.17.0 — Développement SMP

## Conception

La table MP de Limine fournit les identifiants MADT (APIC) ou firmware ARM (affinité MPIDR). Le chargeur assure le démarrage matériel des secondaires et l’entrée longue 64 bits ou EL1. Le noyau attribue des indices logiques denses en conservant l’identité matérielle entière. La capacité de calcul peut rester inconnue. Le choix d’affinité porte sur les indices logiques. [Protocole MP](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#mp-multiprocessor)

Avant de rendre les cadres du chargeur, chaque processeur doit avoir quitté ses piles, vecteurs, descripteurs et traductions hérités. Une absence d’acquittement doit conserver ces cadres. Les timers locaux préemptent chaque processeur. Les lignes clavier et disque peuvent rester routées vers le processeur d’amorçage. L’horloge des échéances IPC avance une fois par période au niveau machine.

L’ordonnanceur actuel publie le sortant en file prête avant la sauvegarde assembleur de son pointeur de pile. En multicœur, il faut garder indivisibles la sélection, la sauvegarde de SP et le départ effectif de l’ancienne pile. Le courant, la pile idle, les piles d’exception et les points d’entrée depuis le mode utilisateur appartiennent au CPU. Un réveil doit contrôler la génération du TCB et le prédicat d’attente sous la même exclusion que le blocage. Le transfert d’un thread prêt respecte son masque d’affinité. Les prêts ordinaires ont priorité sur idle.

Les tables IPC et de capacités sont aujourd’hui partagées dans `Core`. Les appels qui réservent plusieurs objets puis les publient en plusieurs étapes sous masque IRQ demandent une exclusion interprocesseurs sur toute la transaction. La livraison DMA doit revalider la propriété et conserver le bail jusqu’à la fin du retrait matériel. Les tables de pages doivent survivre au dernier utilisateur et aux invalidations distantes acquittées. [Ordonnanceur publié](ordonnancement.md) · [Audit des dépendances mono-CPU](stockage-smp-v0.16.7.md#limites-de-larchitecture-actuelle)

## Intégration de la qualification v0.16.7

Le 14 novembre 2026, `main` a intégré les sondes, les tests et le rapport v0.16.7 au commit `3e14a50bb5bc7960d16c1fbcfc7f49a681d6e7c6`. Ses deux parents sont `64603c4` et `7aaf6dc`. Son arbre Git est identique à celui du candidat qualifié. Les constructions AArch64 et x86_64, les contrôles de progression du montage, les courses IPC, les 24 groupes bloc par cible et les montages sur exports propres ont été rejoués. Un double de test QoS omettait la bannière du terminal et faisait déjà échouer six cas sur la référence. La branche SMP corrige le double et les sept cas y passent. [Qualification intégrée](stockage-smp-v0.16.7.md)

## Implémentation sur la branche SMP

| Composant | Comportement de la branche |
| --- | --- |
| Amorçage | NovaOS demande la table MP à Limine, conserve l’APIC ID ou l’affinité MPIDR et affecte au BSP l’indice logique 0. Chaque secondaire vérifie son identité avant de signaler sa présence depuis sa pile NovaOS. Une absence d’acquittement arrête la reprise avant toute restitution des cadres du chargeur. Sans table MP, seul le BSP est exploité. |
| x86_64 | GDT, TSS, piles IST et points d’entrée système privés par CPU. Le PIT donne les échéances globales. Chaque AP possède un timer LAPIC périodique étalonné sur le BSP. Deux vecteurs IPI servent à l’invalidation TLB et au réveil d’un CPU disponible. Le changement CR3 et les invalidations précèdent le réemploi des cadres. |
| AArch64 | Pile d’exception, vecteurs et timer physique PPI 30 initialisés sur chaque PE. GICv2 utilise son interface locale. Avec GICv3, la HAL sélectionne le redistributeur par affinité et configure le SGI de réveil. La racine basse vide précède le changement d’espace utilisateur. Les traductions utilisateur sont non globales, avec un ASID par PE. Les invalidations diffusées dans le domaine de partage interne sont achevées avant restitution. |
| Ordonnancement | File FIFO commune bornée à 32 slots, sélection du premier thread dont l’affinité autorise le CPU. Chaque CPU a son thread courant et son thread idle. Les processus ordinaires peuvent tourner sur les CPU en ligne. Les threads noyau historiques restent sur le BSP sauf affinité explicite. Le pilote disque reste sur CPU0, destinataire des interruptions matérielles du disque. |
| IPC et capacités | La même exclusion interprocesseurs protège l’inscription d’un dormeur, la génération du TCB, la publication de son réveil, les capacités et la collecte. L’assembleur libère le verrou de commutation après sauvegarde de SP et installation de la pile suivante. Les mutex bloquants gardent leur ordre FIFO. L’horloge des échéances progresse une fois par tick machine. |
| DMA et mémoire | La dernière vérification du bail au moment de la publication et sa révocation par le superviseur utilisent le même verrou. Le pilote conserve les cadres jusqu’au retrait confirmé. La vérification du TLB permute deux cadres de contenus distincts après lecture du premier motif par chaque AP, puis exige la nouvelle valeur avant de restituer les pages. |
| Instruments | Les mesures d’occupation additionnent le repos de chaque vCPU, y compris les périodes qui chevauchent les bornes. Le budget disponible est le temps écoulé multiplié par le nombre de vCPU. L’ancien décodeur rejetait une sauvegarde terminée quand plusieurs CPU dormaient pendant le même intervalle. Les temps de sauvegarde du tableau proviennent de l’horloge hôte. |

Le lancement INIT/SIPI ou PSCI et la découverte initiale dans les tables du firmware relèvent du chargeur. Le noyau vérifie les entrées qu’il reçoit et initialise lui-même la traduction, les piles, les interruptions et l’ordonnancement. Le stationnement testé retire chaque secondaire du masque actif depuis idle, puis le maintient en HLT/WFI. Les indices d’affinité sont indépendants du type de cœur. Cette branche ne propose aucune classification ou politique énergétique. [Topologie](../kernel/src/smp.rs) · [Ordonnanceur](../kernel/src/sched/mod.rs) · [Exclusion interprocesseurs](../kernel/src/sched/gate.rs) · [LAPIC](../kernel/src/arch/x86_64/lapic.rs) · [Traductions ARM](../kernel/src/arch/aarch64/paging.rs)

## Qualification du 14 novembre 2026

Les invités utilisent QEMU 11.1.3 TCG, 256 Mio, Q35 avec CPU `max` ou `virt` avec Cortex-A72. La sonde teste deux processus de calcul par vCPU, attend que chacun ait progressé en Ring 3 ou EL0 sous préemption, puis vérifie ses codes de sortie. Un second rendez-vous impose la participation de tous les threads noyau épinglés avant leur libération. Chaque participant attend aussi plusieurs ticks locaux. La sonde vérifie ensuite la migration de CPU0 vers CPU1 et le stationnement des secondaires. [Banc QEMU](../tests/smp.py) · [Sondes invitées](../kernel/src/sched/smp_probe.rs) · [Relevés](validation/smp-v0.17.0.json)

| Profil QEMU | CPU actifs vérifiés | Processus utilisateur pour 4 vCPU | Secondaires vérifiant la traduction modifiée |
| --- | --- | ---: | --- |
| x86_64, xAPIC | 1, 2, 4 | 8 | CPU1, CPU2 et CPU3 |
| AArch64, GICv2 | 1, 2, 4 | 8 | CPU1, CPU2 et CPU3 |
| AArch64, GICv3 sans ITS | 1, 2, 4 | 8 | CPU1, CPU2 et CPU3 |

Neuf démarrages supplémentaires ont exercé les mêmes assertions avec les modèles QEMU `EPYC`, `Haswell-noTSX` et `cortex-a76`, chacun à 1, 2 et 4 vCPU. QEMU a signalé les fonctions CPUID que TCG ne présente pas sur les deux modèles x86. Les 18 résultats individuels précisent le modèle, les identités matérielles, les préemptions et les masques observés. Pour les reproduire, définir `NOVA_X86_CPU=EPYC` ou `NOVA_X86_CPU=Haswell-noTSX` sur le banc x86 et `NOVA_ARM_CPU=cortex-a76` sur le banc ARM en GICv2.

À quatre vCPU, les traces des montages ordinaires en secteurs de 512 octets identifient les services par PID et par CPU. Sur chaque architecture, les services bloc et NovaFS ainsi que le client passent sur les quatre CPU. Le pilote reste sur CPU0. Les deux montages ont effectué 2 198 lectures, rendu les ressources de session et conservé le SHA-256 de l’image. L’injection AArch64 au LBA 7 416, armée après 1 500 lectures, entraîne un refus sur la lecture 2 030, un retrait réussi et une image identique. Cette reproduction localise l’erreur injectée. La cause de l’incident historique reste ouverte.

Les 25 démarrages historiques par architecture tournent à un vCPU. Sur quatre vCPU, les contrôles ELF, processus, IPC, files pleines, décès, 24 groupes bloc par architecture, fichiers et espace de noms exercent les lecteurs et écrivains isolés. Les courses IPC couvrent 10 000 répétitions des cinq ordres et la collecte des requêtes réveillées. L’endurance IPC a totalisé 17 268 cycles en 300,305 s sur ARM et 16 323 cycles en 300,504 s sur x86. L’endurance allocateur exerce trois travailleurs de fragmentation épinglés sur les secondaires et des travailleurs de calcul sur le BSP. En 66,036 s et 62,867 s, respectivement, elle a compté 132 172 et 126 114 allocations et retrouvé exactement le bilan physique de départ à l’issue de chacune des neuf périodes.

Le premier essai classique AArch64/512 a expiré dans le moniteur QMP pendant l’attente de la barrière F1. Le rejeu complet du banc fichiers a franchi ce point et réussi sur les deux géométries. Les traces du premier essai ne permettent pas d’attribuer le délai au noyau, à QEMU ou à l’hôte.

### Mesures de mémoire

Le tas occupé avant lancement des processus de la sonde vaut 10 272, 43 040 et 108 576 octets sur ARM pour 1, 2 et 4 CPU. Les valeurs x86 sont 9 248, 74 784 et 205 856 octets. Le surcoût de tas mesuré par secondaire est donc de 32 Kio sur ARM et 64 Kio sur x86. S’y ajoute une pile idle de 64 Kio de cadres par secondaire, entre deux pages de garde non mappées. Ces deux postes représentent respectivement **96 Kio et 128 Kio par secondaire**, hors tables de pages et structures partagées. Les cadres libres au démarrage varient aussi avec les réservations du chargeur. Les piles et tables locales survivent au stationnement du CPU.

### Sauvegardes groupées

Le profil groupé est activé explicitement dans des exports indépendants. Le tableau mesure le délai hôte de la commande au succès sous QEMU, sur disque brut de secteurs de 4 096 octets. Une exécution sans sonde et une avec compteurs ont eu lieu par cellule à quatre vCPU sur `94f6898`. Le compteur inclut les opérations de contrôle suivant le contrat v0.16.6. Chaque mesure instrumentée a observé deux FLUSH et chaque destination a été vérifiée par un lecteur indépendant. La colonne mono provient de la qualification antérieure. Les charges hôtes diffèrent et les latences ne mesurent donc pas à elles seules le coût du SMP. [Référence mono](validation/stockage-v0.16.7/performances.json)

| Architecture | Édition | Mono historique, ms | SMP sans sonde, ms | SMP avec sonde, ms | Échanges et contrôles |
| --- | --- | ---: | ---: | ---: | ---: |
| ARM | Création 4 Mio | 75,131 | 184,700 | 193,800 | 36 |
| ARM | Insertion au milieu de 4 Mio | 92,074 | 194,436 | 158,237 | 43 |
| ARM | Ajouts de 4 Mio, sortie 8 Mio | 128,519 | 252,085 | 267,114 | 56 |
| x86 | Création 4 Mio | 132,502 | 223,757 | 255,286 | 36 |
| x86 | Insertion au milieu de 4 Mio | 140,882 | 172,420 | 179,499 | 43 |
| x86 | Ajouts de 4 Mio, sortie 8 Mio | 190,952 | 277,426 | 254,555 | 56 |

Un second essai compare le même ELF `bb65c3e` sans sonde en alternant **1, 4, 4, 1 vCPU**, sans changer la géométrie, la charge de travail ou le lecteur de contrôle. Pour les ajouts de 4 Mio dans 4 Mio, les temps individuels ont été **141,211 / 199,522 / 200,085 / 139,851 ms** sur ARM et **198,239 / 254,939 / 261,912 / 206,335 ms** sur x86. Les quatre essais à quatre vCPU sont plus lents que leurs témoins mono. Le traitement de cette sauvegarde n’exploite donc pas le parallélisme pour réduire son délai sur ces profils. Ces deux répétitions par configuration ne permettent pas de caractériser le p99 ou de répartir le surcoût entre TCG, verrou du noyau et changements de contexte.

### Reproduction

Depuis la racine du dépôt, ces commandes utilisent les images jetables du banc. La variante GICv3 exige à la fois le noyau `gicv3-lab` et la machine correspondante.

```sh
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" test smp-test
    NOVA_VCPUS=4 make -C novaOS ARCH="$arch" elf-test ipc-test ipc-races lifecycle-test block-test file-test namespace-test cli-test
    NOVA_VCPUS=4 make -C novaOS ARCH="$arch" endurance DURATION=60
    NOVA_VCPUS=4 make -C novaOS ARCH="$arch" ipc-endurance DURATION=300
    python3 -B novaFS/lab/p51-validation-suites.py "$arch"
done
(
    cd novaOS
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build --release --locked --offline \
        --target aarch64-unknown-none-softfloat --features gicv3-lab --target-dir build/target-gicv3-smp
    NOVA_ARCH=aarch64 NOVA_GIC=3 NOVA_KERNEL_ELF="$PWD/build/target-gicv3-smp/aarch64-unknown-none-softfloat/release/novaos" \
        python3 -B tests/smp.py
)
```

Pour la chaîne expérimentale, `reconstruire_es_0166.py` exige un répertoire de sortie neuf. Les profils `sans` et `sonde` isolent les mesures des compteurs. Le profil `coupe` arrête QEMU sur des points de publication identifiés. Le pilote reste en polling dans ces essais. Le stockage classique demeure la configuration distribuée. [Construction des profils](../tests/reconstruire_es_0166.py) · [Montage avec observation CPU/PID](../tests/stockage_smp_montage.py) · [Coupures et contrôle du contenu](../tests/coupure_cow_groupee.py) · [Propriété et révocation DMA](../tests/dma_propriete.py)

## Stockage, compatibilité et limites de qualification

- **Coupures groupées.** Sur chaque architecture, neuf interruptions ont été exercées en secteurs de 512 octets avec une source et un ajout de 4 Mio, et neuf en secteurs de 4 096 octets avec une source de 524 416 octets et une insertion d’un octet. Les 36 images ont retrouvé un état complet après remontage. Les essais arrêtent notamment une écriture, F1 et F2 en vol dans QEMU. L’oracle relit les octets de la source et la présence ou le contenu intégral de la destination avant et après le redémarrage. Pendant F2, une destination complète peut déjà être visible avant l’acquittement.
- **Concurrence.** Le témoin sur secteurs de 512 octets couvre les écritures concurrentes, la suppression d’une source ouverte, le remplacement du nom, la conservation des éditions locales et les frontières UTF-8. Les tests d’espace de noms démarrent deux clients sur chaque géométrie. Les quatre profils de stockage conservent les refus de droits et les erreurs de périphérique. Les trames de négociation ont passé cinq contrôles par architecture Darwin, complétés par les essais IPC des invités.
- **Propriété DMA.** La copie instrumentée du dernier noyau couvre 464 injections et 256 fins de session sur les quatre couples architecture/géométrie. Les images sont identiques après les essais. Un bail, une réservation ou une adresse invalidés avant livraison laissent le nombre de pages livrées à zéro. Les deux anciens portails refusent la lecture révoquée. Après restitution, le nombre de cadres de référence est zéro sur ARM et neuf sur x86 pour la machine VT-d. L’essai d’invalidation TLB distante porte sur les traductions CPU. Le banc de propriété contrôle séparément les traductions DMA.
- **NovaFS.** Sur Darwin ARM64 natif et x86_64 sous Rosetta, 196 tests standards et 199 instrumentés ont réussi par architecture, avec six tests marqués ignorés dans chaque suite. Le différentiel a exécuté un million d’opérations par architecture. Le rejeu des 2 304 injections d’ancre par architecture a donné 1 288 états anciens, 656 nouveaux et 360 refus. Les arbres Git de `novaFS/` et de `novaOS/abi/` sont identiques à ceux de `main`. Le SMP ne change ni le droit d’écriture des clients, ni P9, ni PC1.
- **Régressions des applications.** Outre le banc CLI classique, les tests `cli-p9-test`, `nsh-test`, `nsh-scripts-test`, `nsh-terminal-test` et `vi-test` ont été exécutés dans les deux géométries sur chaque architecture. Chaque cellule du banc `vi` a relu 16 fichiers et collecté 17 ELF. La validité visuelle de l’application Terminal UTM en mode SMP reste à vérifier.
- **Régressions du firmware.** Le moteur QEMU 10.0.12 embarqué dans UTM 5.0.8 a exécuté 11 essais UEFI/GICv3 et 23 scénarios DMA séquentiels sur états TPM et disques jetables. Ces deux bancs historiques lancent un seul vCPU. Le premier lancement a été bloqué côté hôte à la création du socket Unix du TPM simulé, avant le démarrage de l’invité. Le rejeu avec ce socket a exercé les deux suites complètes.
- **Dernière révision fonctionnelle.** Sur `bb65c3e`, la grande édition a été rejouée sans sonde en **207,094 ms sur ARM** et **250,999 ms sur x86**. Les deux mesures avec sonde ont donné respectivement 327,030 et 385,242 ms, avec 56 échanges et contrôles et deux FLUSH. Les résultats chronométrés concernent le profil groupé en polling, explicitement activé. La distribution utilise toujours les E/S classiques.

### Limites et décision proposée

Poursuivre sur `codex/smp-0170` avant une fusion. Le rendez-vous, les préemptions et les changements de traduction ont été observés jusqu’à quatre processeurs logiques. La table de topologie a seize entrées et le lanceur de qualification accepte 1, 2 ou 4 vCPU. Les nombres supérieurs, x2APIC, les topologies physiques Intel/AMD ou ARM hétérogènes et le remplacement à chaud d’un CPU restent hors qualification. Les interfaces matérielles des profils Q35 et virt font partie des prérequis. Les mesures par compteur x86 entre CPU supposent des TSC synchronisés.

Le verrou commun peut sérialiser les appels noyau longs. La file reste bornée et le pilote de disque conserve une file matérielle attachée au BSP. Les essais n’établissent aucune borne générale de latence sous contention ou inversion de priorité. La panique arrête le CPU fautif. Un arrêt coordonné de tous les processeurs et une politique de traitement d’un CPU défaillant en cours d’exécution restent à qualifier. Le stationnement d’essai garde les ressources privées du CPU et se fait une fois ses travailleurs collectés.

Les coupures concernent des processus QEMU sur un Mac resté alimenté. Le comportement sous perte électrique et les performances de ce noyau dans l’application UTM à quatre vCPU restent ouverts. Avant de proposer une diffusion, qualifier ces configurations, le confinement d’une panne de CPU et la contention avec des répétitions suffisantes. L’activation générale des E/S groupées demande une décision distincte.

La [qualification de phase 2](smp-phase2-v0.17.0.md) reprend la contention, l’arrêt concerté, les états flottants et les essais dans l’application UTM. Les limites ci-dessus décrivent l’état de la phase 1.

La phase 3 mesure les allers-retours IPC à charge constante et proportionnelle, les affinités et la concurrence avec NovaFS. Elle documente également la correction de la fenêtre MMIO x86. [Mesures et qualification de phase 3](ipc-smp-phase3-v0.17.0.md)
