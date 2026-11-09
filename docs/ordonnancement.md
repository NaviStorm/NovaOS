# Threads noyau de NovaOS v0.5.0

Le premier thread reprend l’affichage et, sur x86_64, la saisie au prompt. Un second thread, idle, attend la prochaine IRQ lorsqu’il est seul à pouvoir tourner. Les threads de validation sont lancés par les arguments Limine des ISO temporaires de test.

## Politique commune

Le noyau réserve sur le tas 32 emplacements de descripteurs (TCB), dont celui d’idle. Chaque descripteur porte un identifiant de génération, l’état, la pile, le pointeur de contexte sauvegardé, la classe de priorité, l’affinité et l’identité opaque de l’espace mémoire. La file FIFO des prêts a une capacité fixe. Un identifiant ancien ne désigne jamais le nouveau thread qui réutilise son emplacement.

Un service appelle `sched::spawn(f)` avec une fonction Rust `fn()` et reçoit un `Result<Id, Error>`. `spawn_on(f, 1)` explicite l’affinité CPU0. `live(id)` permet de vérifier si cette génération est encore en vie. `sleep(0)` cède le reste du quantum et `sleep(n)` attend au minimum `n` ticks pour toute valeur positive. La fonction du thread termine en retournant normalement, ce qui libère ses gardes et ses objets Rust avant la collecte de la pile.

| Transition | Déclencheur |
| --- | --- |
| READY → RUNNING | Le thread est en tête de file, ou la file est vide pour idle. |
| RUNNING → READY | Le quantum expire ou le thread appelle `yield_now()`. Il rejoint la fin de file. |
| RUNNING → BLOCKED | `sleep(n)` inscrit une échéance, ou un événement enregistre un dormeur. |
| BLOCKED → READY | Un tick atteint l’échéance ou le producteur signale l’événement. |
| RUNNING → TERMINATED | La fonction du thread retourne après la destruction normale de ses variables locales. |

Le choix du prochain thread et l’enregistrement de la transition s’effectuent IRQ masquées. L’assembleur reçoit uniquement un emplacement stable où déposer SP et le SP à reprendre. Aucun emprunt Rust de la table des threads ne traverse la commutation. Une pile terminée est collectée par idle ou avant une création ultérieure, depuis un autre thread. Une création peut échouer si la table ou la mémoire est épuisée. Une réservation de pile incomplète restitue ses pages.

Le quantum est un tick, nominalement 4 ms. Le réveil rend un thread prêt à partir de son échéance. Son exécution dépend ensuite de la file FIFO. Le temps est compté en IRQ livrées. Une longue période IRQ masquées peut retarder ou regrouper des échéances matérielles. Cette horloge ne fournit pas de garantie temps réel.

## Mécanismes de la HAL

- **x86_64 / Q35.** Le canal 0 du PIT est programmé en mode périodique et passe par IRQ0 du PIC maître, vecteur `0x20`. Le stub empile les 15 registres généraux autres que RSP, établit l’alignement SysV, puis efface DF avant d’appeler Rust. Le cadre matériel et `iretq` conservent RIP, RSP et RFLAGS. La commutation interne préserve les registres non volatils de l’ABI et change RSP. Le PIC est acquitté avant la sélection, y compris si le retour d’interruption attendra une nouvelle tranche de CPU. [Manuel système Intel](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- **AArch64 / virt.** Le noyau utilise EL1h et SP_EL1 pour les threads. La source physique non sécurisée CNTP, réglée à partir de CNTFRQ_EL0, arrive sur la PPI 30 du GICv2. Les registres du distributeur et de l’interface CPU sont mappés en mémoire Device. Le vecteur IRQ courant sauvegarde x0 à x30, ELR_EL1 et SPSR_EL1 avant l’appel Rust. Le pilote recharge TVAL et écrit l’EOI avant la sélection. Le changement de pile conserve x19 à x30 selon AAPCS64. Le retour `eret` rétablit le PC et PSTATE. [Machine ARM virt de QEMU](https://www.qemu.org/docs/master/system/arm/virt.html)
- **Contexte commun.** Les deux cibles compilent sans zone rouge sous SP. Les threads de cette version appartiennent à l’espace superviseur actif et partagent les traductions du noyau. Une commutation de thread garde cet espace. Les registres de contrôle, les bases TLS et les registres de débogage restent réservés au noyau.
- **FPU/SIMD.** Les cibles Rust utilisent les registres entiers. NovaOS impose CR0.TS sur x86_64 et CPACR_EL1.FPEN=0 sur AArch64. Une instruction interdite arrête la VM avec un diagnostic série. Les sondes exécutent réellement une instruction x87 et une instruction Advanced SIMD, respectivement.

## Piles, sections critiques et attentes

Chaque thread reçoit 64 Kio RW/NX encadrés par deux pages de garde de 4 Kio non mappées. Le gestionnaire de pages v0.3.0 fournit les cadres. Le témoin en bas de pile et les bornes de SP sont vérifiés à l’ordonnancement, avec une marge de 512 octets. Sur x86_64, les fautes de page et les doubles fautes disposent de piles IST distinctes de 16 Kio dans le TSS. Sur ARM, le vecteur synchrone établit sa pile d’urgence avant le premier accès mémoire. Une sonde positionne réellement SP en limite et provoque une écriture dans la garde.

Une garde de section critique sauvegarde le masque d’interruption du CPU puis le restaure à sa sortie. Le tas conserve son verrou atomique et le journal série protège l’écriture d’un message. Le traitement normal du tick parcourt au plus 32 emplacements et emploie uniquement des structures préallouées. Création de piles et collecte restent en contexte de thread. Les appels de sommeil ou de cession en section non préemptible déclenchent un diagnostic, et `spawn` y renvoie une erreur.

Le pilote clavier contrôle le tampon et arme son attente sous le même masque IRQ. Un événement réveille un dormeur qui vérifie à nouveau son prédicat. Le mutex utilise ce mécanisme pour bloquer ses concurrents. Le propriétaire reste préemptible, et lui seul détient la garde de la donnée. La libération publie l’état disponible avant la notification. Idle vérifie la file sous masque avant `sti; hlt` sur x86_64 ou `wfi` sur ARM. Une IRQ en attente provoque le réveil.

## Reproduire la validation

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
```

| Scénario QEMU | Vérifications effectuées par le noyau et le banc hôte |
| --- | --- |
| `sched-test` | IRQ physiquement pendante sous masque puis livrée. Trois threads prêts pendant au moins 100 ticks sans cession volontaire. Les deux travailleurs vérifient des motifs distincts dans leurs registres et les drapeaux au passage des ticks. Sommeil, réveil, cession, exclusion mutuelle avec un propriétaire qui dort, saturation des TCB, réutilisation des identifiants et restitution de 48 piles successives. |
| `sched-guard` | L’écriture sous SP atteint la garde de la pile du thread. Le banc vérifie l’adresse fautive, le PC, le syndrome matériel et l’arrêt sur la pile d’urgence. |
| `sched-fpu` | Un thread tente une instruction FPU/SIMD. Le banc attend le diagnostic #NM ou EC=7 puis vérifie la veille terminale. |
| `sched-stress` | Deux travailleurs liés au CPU, sans yield, vérifient leurs contextes pendant au moins 15 000 ticks, soit 60 s nominales. Un contrôleur crée périodiquement un thread court, qui dort puis termine. Le test vérifie les progrès de chaque travailleur, au moins 14 000 préemptions et la terminaison de chaque thread court. |

Les compteurs `switches` des journaux PREEMPT et STRESS comptent exclusivement les changements entre deux threads ordinaires **décidés par un tick matériel**. Les cessions, blocages, terminaisons et transitions vers idle en sont exclus. Chaque résultat de travailleur A ou B couvre deux passages de tick observés dans l’assembleur de test. Le banc hôte possède un délai maximal indépendant du compteur du noyau.

## Résultats de la v0.5.0

Validation exécutée avec Rust stable 1.99.1, QEMU 11.1.3, un CPU émulé et 256 Mio de RAM par VM.

| Mesure sur la révision livrée | x86_64 | AArch64 |
| --- | ---: | ---: |
| Démarrages réussis de la suite complète | 17/17 | 16/16 |
| Préemptions pendant la phase à trois threads, zéro cession volontaire | 102 en 102 ticks | 102 en 102 ticks |
| Incréments protégés / blocages sur mutex | 40 / 39 | 40 / 39 |
| Cycles de création et restitution de pile, solde de cadres identique | 48 | 48 |
| Ticks / préemptions de l’essai prolongé | 15 162 / 15 162 | 15 162 / 15 162 |
| Vérifications de contexte achevées par travailleur A / B | 3 781 / 3 781 | 3 781 / 3 781 |
| Threads courts créés et terminés pendant cet essai | 60 | 60 |
| Durée hôte de la VM de stabilité, amorçage compris | 62,1 s | 81,0 s |
| Écriture dans la garde de pile | #PF, code `0x2` | Data Abort, ESR `0x96000047` |
| Instruction flottante ou vectorielle interdite | #NM | EC `0x07` |

Les deux suites ont également validé le masquage d’une IRQ en attente suivi de sa livraison, la saturation des emplacements et le rejet d’une ancienne génération. Les scénarios de pagination et de tas ont conservé leurs résultats attendus. Les captures complètes ont confirmé la bannière sur les deux cibles et l’édition AZERTY sur x86_64.

## Limites et risques résiduels

La validation cible QEMU TCG mono-CPU et les plateformes Q35 et virt/GICv2 configurées dans le dépôt. Le masque d’affinité accepté vaut 1. Les classes disponibles sont normale et idle. Tous les threads sont de confiance et partagent le même espace noyau. Le passage en Ring 3 ou EL0, les appels système, le SMP, les priorités multiples de travail et le changement de racine par processus restent hors de cette version.

Un mutex est non récursif. Les appelants doivent respecter un ordre cohérent pour plusieurs verrous. La politique ne garantit ni héritage de priorité ni délai maximal d’attente. Les pages de garde détectent un accès qui les touche ; un saut arbitraire de SP peut les franchir. Une corruption de pile ou une instruction interdite entraîne un arrêt global pour inspection. Les performances sur matériel physique, les fautes imbriquées et une exécution de plusieurs heures n’ont pas été validées.
