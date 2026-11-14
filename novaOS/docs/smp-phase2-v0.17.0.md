# NovaOS v0.17.0 — Stabilisation SMP, contention et contexte flottant

Qualification du 14 novembre 2026 sur la branche `codex/smp-0170`. La référence initiale est `8a42b728`. Le dernier noyau testé est `da411f14a2bce3864548a69c71c6f91eb06fdddb`. Les mesures intermédiaires identifient leur binaire. Les versions distribuées du noyau, du moteur NovaFS et du format P9 conservent leurs identifiants. [Phase 1](smp-v0.17.0.md) · [Mesures individuelles, empreintes ELF et matrices](validation/smp-phase2-v0.17.0.json)

## Contention et politique retenue

Sur la première exécution instrumentée à quatre vCPU, la plus longue détention du verrou sur CPU0 couvrait la création d’un processus de la sonde. Elle a duré 7,13 ms sur ARM et 17,82 ms sur x86. La préparation de l’espace utilisateur, notamment l’allocation et le remplissage de ses pages, se faisait alors sous le verrou commun. Pour les créations indépendantes, elle se fait désormais avant l’acquisition de la garde de publication. Les slots, la génération, le PID, l’affinité et le thread prêt sont publiés ensemble. Un échec rend l’espace encore privé. La création atomique d’un groupe de services IPC conserve son exclusion de bout en bout. Le chemin d’attente lit désormais le propriétaire avant de retenter une écriture atomique. Il traite toujours les demandes de traduction et d’arrêt. [Publication](../kernel/src/sched/process.rs) · [Garde](../kernel/src/sched/gate.rs)

Après ce changement, les pointes à quatre vCPU apparaissent encore dans la publication et dans `ipc::traffic::invoke`. Sur 20 maxima locaux par architecture, huit sur ARM et neuf sur x86 se situent dans la transaction IPC. Huit et neuf autres se situent dans `process.rs`. Le chronométrage porte sur la garde extérieure. Il ne permet pas d’imputer tout son coût à une seule instruction interne. Le changement de pile clôt la détention du sortant juste avant la libération assembleur et commence celle du revenant à sa réacquisition.

Le tableau donne les médianes de cinq démarrages par cellule, avec le même ELF pour les trois topologies d’une architecture. Chaque démarrage distribue 16 millions d’itérations entières entre huit travailleurs, puis exécute 1 000 allers-retours de 128 octets entre deux processus. Le délai par tour inclut les deux acteurs et les interventions du contrôleur. Le délai du calcul inclut l’observation de fin à la résolution du timer. Les attentes et détentions sont des sommes sur les CPU pour la fenêtre entière. Elles peuvent dépasser sa durée murale.

| QEMU TCG, 256 Mio | vCPU | Calcul, ms | IPC, µs/tour | Acquisitions contestées | Attente cumulée, ms | Détention cumulée, ms | Commutations | Migrations |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| AArch64 GICv2 | 1 | 37,390 | 97,924 | 0 | 0 | 100,334 | 4 096 | 0 |
| AArch64 GICv2 | 2 | 20,166 | 224,980 | 17 467 | 182,137 | 234,845 | 3 724 | 288 |
| AArch64 GICv2 | 4 | 15,719 | 528,086 | 31 981 | 1 132,753 | 519,069 | 3 604 | 1 222 |
| x86_64 xAPIC | 1 | 44,129 | 61,211 | 0 | 0 | 81,644 | 4 108 | 0 |
| x86_64 xAPIC | 2 | 24,405 | 104,999 | 19 098 | 111,436 | 134,574 | 3 747 | 107 |
| x86_64 xAPIC | 4 | 25,189 | 193,741 | 21 447 | 681,130 | 286,088 | 2 599 | 970 |

Le débit médian du calcul à quatre vCPU atteint environ 1 018 millions d’itérations/s sur ARM et 635 millions sur x86. L’IPC devient plus lent à mesure que davantage de CPU se disputent la file et le verrou. Les compteurs distinguent les ticks interrompant le mode utilisateur, le noyau actif et idle. À quatre CPU, les médianes respectives valent 91/261/216 sur ARM et 89/166/137 sur x86. Ce sont des échantillons à 250 Hz par CPU. Le temps bloqué sur IPC mesure l’intervalle inscription–réveil, pas le délai jusqu’au prochain passage en exécution. Les temps de stockage du tableau suivant proviennent des acquittements du pilote. Ces observations se recouvrent et ne s’additionnent pas en un bilan comptable du temps CPU.

Une variante éphémère mémorisait sous la garde les destinataires déjà avertis. À quatre vCPU, le nombre médian de diffusions passait de 1 786 à 1 712 sur ARM et de 1 645 à 1 534 sur x86. L’aller-retour médian passait respectivement de 528 à 655 µs et de 194 à 188 µs. Les deux campagnes partageaient le Mac avec d’autres validations. Le code publié conserve la politique de réveil initiale. L’affinité des pilotes, la FIFO et la génération du dormeur restent les critères de sélection. Tous les acteurs épinglés ont progressé. La saturation et les files pleines sont exercées par les tests IPC. Le verrou commun ne fournit aucune borne générale d’équité ou d’inversion de priorité sous charge arbitraire. Aucune subdivision de ce verrou n’est proposée pour intégration.

Les maxima et les valeurs extrêmes figurent dans le JSON. Par exemple, le calcul ARM mono a pris 101,710 ms lors d’un des cinq essais. Le tas avant la sonde compte 10 272 / 43 040 / 108 576 octets sur ARM, 9 248 / 74 784 / 205 856 sur x86 à 1 / 2 / 4 vCPU. Les différences correspondent à 32 et 64 Kio de tas par secondaire. Avec la pile idle de 64 Kio, le supplément est de 96 et 128 Kio par secondaire, hors tables et instruments statiques. Les images et les mesures individuelles identifient les deux populations expérimentales.

## Arrêt et conservation du diagnostic

Le premier CPU qui demande l’arrêt devient coordinateur par opération atomique. Il masque ses interruptions, publie la demande et, si le contrôleur est prêt, envoie une notification aux autres CPU. Les secondaires la consultent à l’entrée timer/IPI et dans les attentes des verrous du noyau et de l’allocateur. Chaque répondant inscrit son SP et son bit avant la veille permanente, interruptions masquées. Le coordinateur interroge le compteur matériel pendant au plus une seconde pour les CPU en ligne. Le budget x86 est étalonné avant lancement des secondaires. La sortie série fatale évite le verrou de journalisation et conserve les adresses de pile côté superviseur. Les piles, tables, cadres et éventuels baux encore détenus restent réservés jusqu’au redémarrage. La panne d’un périphérique série peut limiter la trace. Les acquittements et les SP restent en mémoire. [Coordinateur et boîtes d’acquittement](../kernel/src/smp.rs)

| Situation | Injection et observation |
| --- | --- |
| Fin normale | À quatre vCPU, CPU0 reçoit le masque `0xf`. L’arrêt système après la fermeture des services GPT vérifie aussi le retrait DMA. |
| Panique noyau | Au-delà d’un vCPU, CPU1 déclenche une véritable panique. CPU0 et les autres secondaires acquittent sur leurs piles. Une panique en mono reste locale. |
| Secondaire absent au démarrage | Le lanceur de test omet le dernier AP attendu. Le délai d’amorçage expire avant le transfert et la restitution des pages Limine. À quatre vCPU, le masque absent est `0x8`. |
| CPU muet en fonctionnement | Un travailleur épinglé sur CPU1 masque ses IRQ et reste en calcul. À quatre vCPU, le coordinateur termine avec `ack=0xd` et `muets=0x2`. Aucun autre CPU ne reprend son travail. |

Les trois topologies ont été exercées sur QEMU x86_64 et AArch64 avec GICv2 et GICv3 sans ITS. Chaque acquittement de la matrice finale vérifie aussi un SP non nul et distinct. Le délai dépend du compteur architectural et de l’exécution du coordinateur par l’hyperviseur. La suspension de toute la VM ou la perte du coordinateur empêchent une garantie de temps réel côté hôte. [Sondes d’arrêt et de calcul](../tests/smp_phase2.py)

## Calcul flottant et SIMD

Le profil de compilation `user-fpu` active la sauvegarde systématique au changement de thread utilisateur. Les binaires distribués conservent leur profil softfloat et le refus matériel historique. Chaque slot dispose d’une zone superviseur alignée à 64 octets. Au premier départ d’un nouveau TCB, l’initialisation écrase l’état de son prédécesseur. Les IRQ sont masquées pendant la sauvegarde et la restauration avant libération du verrou de commutation. Le noyau et ses gestionnaires restent compilés en softfloat. Le retour vers un thread purement noyau réinterdit l’usage matériel. L’activation n’ajoute aucun appel système ni droit IPC.

| Profil | État pris en charge | Banc exécuté |
| --- | --- | --- |
| AArch64 Cortex-A72 | V0–V31 entiers, FPCR et FPSR, 528 octets utiles par slot. FPEN autorise EL0 uniquement pour le contexte utilisateur. SVE/SME restent interdits. Le démarrage refuse FPMR, les capacités FP/SIMD incompatibles et un MIDR différent. | EL0 sous QEMU GICv2/GICv3 à 1/2/4 vCPU, puis UTM GICv3 à quatre vCPU. |
| x86_64 avec AVX | CPUID, OSXSAVE, XCR0 = 7. XSAVE64/XRSTOR64 standard, x87, SSE/SSE2 et parties hautes YMM. L’étendue CPUID doit correspondre aux 832 octets attendus. | Ring 3, QEMU `max,vendor=GenuineIntel` à 1/2/4 vCPU. |
| x86_64 sans AVX | FXSAVE64/FXRSTOR64, FCW initial 0x037f et MXCSR 0x1f80. Le noyau vérifie la compatibilité du masque MXCSR entre CPU. | Ring 3, QEMU `qemu64,vendor=GenuineIntel,-avx,-xsave` à 1/2/4 vCPU. |

Le profil FP réserve 18 Kio de BSS sur ARM (32 × 576 octets après alignement) et 26 Kio sur x86 (32 × 832), indépendamment du nombre de CPU actifs. Le profil par défaut n’alloue pas ces zones. Le modèle AMD sans garantie CPUID de restauration des pointeurs x87 est refusé dans le profil FP. Le chemin AMD admis par détection reste à tester matériellement. AVX-512, les registres étendus ARM, le hotplug et les topologies hétérogènes restent hors qualification. La détection des profils n’établit aucune propriété pour l’ensemble des processeurs physiques. [Manuel Intel, état étendu](https://cdrdv2-public.intel.com/843827/253665-sdm-vol-1-dec-24.pdf) · [Manuel système AMD](https://www.amd.com/content/dam/amd/en/documents/processor-tech-docs/programmer-references/24593.pdf) · [Contexte ARM](../kernel/src/arch/aarch64/fpu.rs) · [Contexte x86](../kernel/src/arch/x86_64/fpu.rs)

Les programmes natifs vérifient l’état initial, un calcul simple et double précision, le signe de zéro, une comparaison NaN non ordonnée et des opérations vectorielles. Chaque client utilise un motif propre dans chaque registre testé, y compris les deux moitiés des registres V et les portions hautes des YMM quand AVX est annoncé. Les contrôles d’arrondi, le statut ARM, la pile x87 et l’état MXCSR sont relus après préemptions. À quatre vCPU, chaque cycle lance huit utilisateurs, constate leur progrès avant et après déplacement vers un autre CPU, vérifie les codes de sortie puis recommence sur des slots réutilisés. La suite processus exerce séparément pénurie, décès et restitution. Ces contrôles ne constituent pas une suite exhaustive de conformité arithmétique IEEE 754. [Programme ARM](../user/fp_aarch64.S) · [Programme x86](../user/fp_x86_64.S)

## Stockage et application UTM

Sur QEMU 11.1.3 en TCG, le scénario insère au total 4 Mio à plusieurs positions dans une source dense de 4 Mio. Le lecteur indépendant contrôle la destination de 8 Mio. Les temps sont mesurés entre commande et confirmation de durabilité, F1 et F2 comprises, en secteurs de 4 096 octets. Le compteur de 56 inclut les contrôles selon le contrat de la version 0.16.6. Le relevé étendu qui ajoute les opérations locales de copie du prêt vaut 64. Les E/S classiques et le polling VirtIO restent les comportements par défaut.

| QEMU | vCPU | Sauvegardes individuelles, ms | Échanges et contrôles | FLUSH |
| --- | ---: | --- | ---: | ---: |
| AArch64 | 1 | 171,893 / 161,738 / 177,942 | 56 | 2 |
| AArch64 | 2 | 202,609 / 189,665 / 206,429 | 56 | 2 |
| AArch64 | 4 | 303,132 / 303,354 / 301,408 | 56 | 2 |
| x86_64 | 1 | 212,597 / 221,275 / 221,215 | 56 | 2 |
| x86_64 | 2 | 278,347 / 254,776 / 236,820 | 56 | 2 |
| x86_64 | 4 | 278,499 / 324,871 / 331,966 | 56 | 2 |

Le premier couple à quatre CPU sur le candidat final a donné 289,702 ms sur ARM et 330,594 ms sur x86, avec deux FLUSH et 56 échanges chacun. Les compteurs du pilote permettent de distinguer le temps entre notification et observation de fin DMA du temps des FLUSH. Les sommes incluent le délai d’ordonnancement de la VM. Les timestamps de la commande, de la préparation et de la confirmation restent distincts.

L’application UTM 5.0.8 a démarré les disques QCOW2 jetables par UEFI privé avec QEMU 10.0.12, machine virt-10.0, Cortex-A72, 256 Mio, GICv3 sans ITS, TPM CRB et quatre vCPU. Dans la trace de reprise, le SID 56 a été attaché au domaine du disque système avant activation BME. Le firmware a rendu ses droits, avec zéro mappage actif. NovaFS a remonté le disque, `vi` a rouvert la destination, le pilote a confirmé son retrait, puis chacun des quatre CPU a acquitté. Les essais ont eu lieu dans l’application importatrice, avec export à froid et relecture indépendante. [Banc stockage UTM](../tests/es_groupees_utm.py) · [Banc processeurs UTM](../tests/smp_utm.py)

Sur Darwin, les quatre valeurs `thread_id` du moniteur UTM correspondent au **PID du moteur**. QEMU direct présente la même particularité. Dans QEMU direct, la lecture de libproc identifie quatre threads TCG nommés et confirme une progression de leur temps CPU sur chaque architecture. Dans l’application UTM, les threads concernés sont anonymes pour libproc. Un échantillonnage séparé avec `/usr/bin/sample` sur le PID associé à l’UUID de la VM identifie quatre threads Darwin distincts dont la pile contient `mttcg_cpu_thread_fn`, avec `ForceMulticore=true`. Ce relevé établit la topologie du moteur. Le nombre de cœurs physiques simultanément actifs et leur classe P/E restent indéterminés. Les mesures de sauvegarde ont été réalisées sans cet échantillonnage. Un premier essai avait échoué dans l’observateur QoS conçu pour un seul vCPU. [Implémentation amont de l’identifiant QEMU](https://github.com/qemu/qemu/blob/master/util/oslib-posix.c) · [Banc UTM et lecture des piles](../tests/smp_utm.py)

| Application UTM, candidat final | Commande au succès, ms | Échanges avec contrôles | FLUSH | Durée cumulée des FLUSH, ms | Pic de cadres de session |
| --- | ---: | ---: | ---: | ---: | ---: |
| Insertion localisée, classique | 33 277,822 | 84 336 | 134 | 64,645 | 5 846 |
| Même insertion, groupée | 178,952 | 43 | 2 | 1,290 | 2 967 |
| Ajouts totalisant 4 Mio dans 4 Mio, groupés | 232,042 | 56 | 2 | 1,486 | 9 732 |

Un premier témoin classique sur le profil à quatre vCPU avec `ForceMulticore=false` avait pris **221 644,603 ms**. L’essai groupé de grande édition sur ce profil avait pris 243,025 ms. Les mesures seules ne permettent pas d’attribuer l’écart à cette option, au stockage QCOW2 ou à la charge de l’hôte. Les mesures finales utilisent `ForceMulticore=true`. Les maxima intermédiaires sont conservés. Ces effectifs ne suffisent pas à établir un p99 de production.

## Régressions et reproduction

Les régressions ont accompagné les révisions de la phase. Les campagnes SMP, arrêts et FPU, la relecture UTM et les deux confirmations QEMU de la grande sauvegarde ciblent le candidat final. Sous QEMU, les 25 démarrages historiques par architecture couvrent les chemins mono, dont le refus FPU du profil distribué. Les campagnes multicœurs exercent l’ordonnanceur, les descripteurs ELF, les processus, les portails IPC et leur saturation, les capacités, le retrait DMA, les 24 groupes bloc, les fichiers, les espaces de noms, les commandes de nsh et vi. Le rapport JSON donne les codes de retour par cible et les données d’endurance. Le premier passage x86 du banc fichiers a expiré dans QMP en attendant un point d’arrêt F1. Le rejeu intégral a atteint F1 et F2, puis achevé les deux géométries. Les valeurs de mémoire et de génération proviennent du lecteur indépendant.

À quatre vCPU, l’endurance de l’ordonnanceur a duré 60,700 s sur ARM et 60,589 s sur x86. Le nombre de cadres libres a retrouvé sa valeur initiale sur les deux invités. L’endurance IPC a effectué 29 151 cycles en 300,954 s sur ARM et 25 623 cycles en 300,373 s sur x86. Le journal JSON conserve les durées et les compteurs de chaque campagne.

Les 36 images de coupure groupée, neuf par architecture et géométrie, ont toutes passé la relecture octet par octet et le contrôle de génération. Le banc interrompt notamment une écriture et les barrières en vol. En géométrie 512, la source et l’ajout mesurent chacun 4 Mio. En 4 096, la variante historique insère un octet dans 524 416 octets. Une première tentative d’insertion de 4 Mio sur cette petite fixture a été refusée sur les deux architectures avant le point de coupure. Elle ne qualifie aucune frontière. Les coupures de QEMU laissent le Mac alimenté.

Le banc de propriété DMA a exercé le pilote groupé en polling sur une archive instrumentée du candidat final. Pour chacune des topologies à un et quatre vCPU, les géométries 512 et 4 096 donnent 240 injections sur ARM et 224 sur x86. Les huit combinaisons totalisent 928 injections et 512 cycles de session. Toutes les images sont restées identiques. Le banc vérifie la quarantaine, la restitution après confirmation et le contenu des tampons du client après révocation des deux portails de lecture. À la fermeture, il retrouve le niveau de départ de la machine, soit zéro cadre DMA sur ARM et neuf cadres permanents sur x86. [Banc de propriété et de révocation DMA](../tests/dma_propriete.py)

Les bancs firmware historiques ont également passé 11 scénarios DXE et 23 scénarios de politique DMA séquentielle avec le moteur QEMU 10.0.12 livré par UTM. Chaque VM de ces deux bancs utilise un seul vCPU. Leurs témoins de quarantaine, de refus d’un support et de retrait en suspens complètent les trois essais à quatre vCPU exécutés dans l’application. Le JSON distingue les deux bancs et conserve les empreintes du noyau et de la pflash. [Banc DXE](../tests/prototype_uefi.py) · [Banc de politique DMA](../tests/dma_sequentiel.py)

Darwin ARM64 natif et Darwin x86_64 sous Rosetta ont chacun passé 196 tests NovaFS standards, 199 instrumentés et un million d’opérations différentielles. Six cas sont déclarés ignorés par chacune des deux suites de base. Le banc complémentaire a vérifié les 2 304 injections d’ancre par architecture et leur répartition entre 1 288 états anciens, 656 nouveaux et 360 refus. Les arbres `novaFS/` et `novaOS/abi/` sont identiques à la référence de phase 1. Le profil groupé demeure une construction expérimentale explicite. Ni les capacités des clients ni le protocole disque n’ont changé.

Depuis `novaOS/`, les commandes suivantes reproduisent la matrice d’arrêt et les calculs FP. Le nom de modèle x86 est important pour respecter le profil CPUID qualifié.

```sh
for arch in aarch64 x86_64; do
    make ARCH="$arch" kernel
    NOVA_ARCH="$arch" python3 -B tests/smp_phase2.py arret
done
for target in aarch64-unknown-none-softfloat x86_64-unknown-none; do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
        --release --locked --offline --target "$target" --features user-fpu --target-dir build/fp
    NOVA_ARCH="${target%%-*}" NOVA_X86_CPU=max,vendor=GenuineIntel \
        NOVA_KERNEL_ELF="$PWD/build/fp/$target/release/novaos" python3 -B tests/smp_phase2.py fp
done
NOVA_ARCH=x86_64 NOVA_X86_CPU=qemu64,vendor=GenuineIntel,-avx,-xsave \
    NOVA_KERNEL_ELF="$PWD/build/fp/x86_64-unknown-none/release/novaos" python3 -B tests/smp_phase2.py fp
```

Pour reproduire une cellule du profil de contention, construire le noyau ordinaire avec `make ARCH=aarch64 kernel`. Depuis `novaOS/`, exécuter par exemple `NOVA_ARCH=aarch64 PYTHONPATH=tests python3 -B -c 'import smp; smp.run("aarch64", 4)'`. Le journal série contient les compteurs et la fréquence de leur horloge. Durant les cinq répétitions, les topologies ont été présentées dans les ordres 1/2/4, 4/2/1, 2/4/1, 1/4/2 puis 4/1/2. L’architecture exécutée en premier a alterné entre répétitions. Les trois répétitions de stockage ont utilisé les trois premiers ordres.

Pour GICv3, compiler `--features gicv3-lab,user-fpu` dans une autre cible et utiliser `NOVA_GIC=3`. Le lanceur `tests/smp_utm.py` reçoit le profil système UTM, l’ELF GICv3, le générateur d’images P9, un répertoire neuf et le scénario `smp-test`, `smp-fp` ou `smp-arret`. L’option `--echantillon-tcg` relève les piles du seul moteur correspondant à la VM jetable. Les mesures de stockage utilisent `tests/integration_main_performances.py mesurer` avec les exports `sonde`, `utm-sonde` et `compteurs`. Les options, les fixtures et la méthode de construction sont celles de la [phase 1](smp-v0.17.0.md#reproduction).

## Décision proposée

Poursuivre sur la branche SMP. Le stockage groupé a tenu ses seuils dans les cellules exécutées, et les contextes flottants ont conservé leurs données sous préemption et migration. Le calcul parallèle progresse davantage sur ARM que sur x86. L’IPC entre deux acteurs se dégrade à quatre CPU sur les deux cibles. Avant une fusion, mesurer le coût des transactions IPC sous charge isolée, puis vérifier une réduction du surcoût à quatre vCPU qui préserve la saturation, l’équité observée et la révocation. La co-exécution exacte sur les cœurs physiques du Mac reste à mesurer pour attribuer la variabilité à l’hôte. Maintenir le profil flottant expérimental jusqu’à la qualification des autres modèles matériels retenus pour v0.30.0.
