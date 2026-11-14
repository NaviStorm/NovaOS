# NovaOS v0.17.0 — Développement SMP

## Conception

La table MP de Limine fournit les identifiants MADT (APIC) ou firmware ARM (affinité MPIDR). Le chargeur assure le démarrage matériel des secondaires et l’entrée longue 64 bits ou EL1. Le noyau attribue des indices logiques denses en conservant l’identité matérielle entière. La capacité de calcul peut rester inconnue. Le choix d’affinité porte sur les indices logiques. [Protocole MP](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#mp-multiprocessor)

Avant de rendre les cadres du chargeur, chaque processeur doit avoir quitté ses piles, vecteurs, descripteurs et traductions hérités. Une absence d’acquittement doit conserver ces cadres. Les timers locaux préemptent chaque processeur. Les lignes clavier et disque peuvent rester routées vers le processeur d’amorçage. L’horloge des échéances IPC avance une fois par période au niveau machine.

L’ordonnanceur actuel publie le sortant en file prête avant la sauvegarde assembleur de son pointeur de pile. En multicœur, il faut garder indivisibles la sélection, la sauvegarde de SP et le départ effectif de l’ancienne pile. Le courant, la pile idle, les piles d’exception et les points d’entrée depuis le mode utilisateur appartiennent au CPU. Un réveil doit contrôler la génération du TCB et le prédicat d’attente sous la même exclusion que le blocage. Le transfert d’un thread prêt respecte son masque d’affinité. Les prêts ordinaires ont priorité sur idle.

Les tables IPC et de capacités sont aujourd’hui partagées dans `Core`. Les appels qui réservent plusieurs objets puis les publient en plusieurs étapes sous masque IRQ demandent une exclusion interprocesseurs sur toute la transaction. La livraison DMA doit revalider la propriété et conserver le bail jusqu’à la fin du retrait matériel. Les tables de pages doivent survivre au dernier utilisateur et aux invalidations distantes acquittées. [Ordonnanceur publié](ordonnancement.md) · [Audit des dépendances mono-CPU](stockage-smp-v0.16.7.md#limites-de-larchitecture-actuelle)
