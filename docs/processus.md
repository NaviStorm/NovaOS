# Processus et privilèges dans NovaOS v0.6.0

La v0.7.0 remplace les deux portes de sortie expérimentales décrites pour la v0.6.0. [Convention actuelle, numéros et migration](abi.md)

## Démarrer la démonstration

Depuis la racine du dépôt sur macOS ARM64, lancer la VM choisie. Le démarrage normal affiche la bannière et écrit sur le port série les résultats des deux processus. Les PID 1 et 2 doivent sortir avec les codes 41 et 82. L’ordre de sortie dépend des interruptions.

```sh
make ARCH=x86_64 run   # Fenêtre VGA, clavier PS/2 AZERTY, série COM1 dans le terminal
make ARCH=aarch64 run  # Fenêtre ramfb, série PL011 dans le terminal, puis veille
```

Le programme de `user/` est compilé séparément avec le compilateur Rust stable de la cible et son éditeur de liens. Le script de liaison du programme place son entrée à `0x400000` et limite le code à une page. Le résultat est une image plate incorporée au noyau. À la création, chaque instance reçoit sa propre copie. Les seules données fournies par le lanceur sont des scalaires. Le programme ne lie aucun symbole du noyau.

## Cycle de vie

Le PID comprend un numéro monotone et un emplacement interne réutilisable. Le TCB référence le PID, tandis que le processus possède l’espace d’adressage et le compte des threads attachés. La table fixe est protégée par le masque IRQ de CPU0. La création réserve la racine, les pages et la pile superviseur avant de rendre le nouveau thread exécutable. Une réservation incomplète restitue ses cadres sans publier de PID.

À la sortie, le noyau enregistre le code ou le diagnostic de faute et commute sur un autre thread. La sortie d’un processus passe la main à idle pour permettre la collecte, même si d’autres processus restent continuellement prêts. Le collecteur rend la pile superviseur et détache le TCB. Au dernier détachement, il rend toutes les feuilles privées, leurs tables intermédiaires et la racine. Un parent qui conserve son droit de suivi peut encore récolter le statut. Abandonner ce droit permet la collecte automatique du dernier enregistrement.

Le droit de suivi `Child` est une valeur noyau à propriétaire unique. Récolter le statut consomme cette valeur. Ce modèle sépare l’identification publique d’un processus des opérations qu’un détenteur est autorisé à effectuer. Le programme utilisateur dispose seulement d’une sortie scalaire. La distribution de capacités à des services utilisateur reste à définir.

## Carte mémoire d’une instance

| Adresse virtuelle | Contenu | Accès utilisateur |
| --- | --- | --- |
| `0x400000` | Image du programme | Lecture et exécution |
| `0x401000` | Paramètres et compteurs privés | Lecture et écriture, NX |
| `0x600000` | Témoin privé optionnel | Lecture seule, NX. Adresse absente chez les autres instances sans témoin. |
| `0x7ff000` | Garde inférieure | Page absente |
| `0x800000` | Pile utilisateur de 4 Kio | Lecture et écriture, NX |
| `0x801000` | Sommet initial de pile et garde supérieure | Page absente |

Chaque cadre provient du gestionnaire physique et est remis à zéro avant utilisation. Le noyau relit les descripteurs et vérifie les gardes avant de publier le thread. Les alias HHDM restent superviseur et NX. Pour les feuilles RX et RO, l’alias devient aussi non inscriptible. La collecte rétablit l’alias RW avant le recyclage. Le contrôleur accède à la page de données par son cadre vérifié et un indice borné, avec le processus suspendu.

## Chemins matériels

**x86_64.** Chaque PML4 possède ses branches basses. U/S est positionné aux quatre niveaux pour les pages utilisateur. Les entrées hautes empruntées au noyau sont vérifiées superviseur. Les piles permanentes et les TCB maintiennent ancrée la branche de l’arène où évoluent les mappings noyau. Une commutation d’espace recharge CR3. PCID est désactivé et les feuilles NovaOS sont non globales. SMEP et UMIP sont activés lorsqu’ils sont présents. [Manuel système Intel](https://www.intel.com/content/www/us/en/content-details/843836/intel-64-and-ia-32-architectures-software-developer-s-manual-combined-volumes-3a-3b-3c-and-3d-system-programming-guide.html)

La GDT fournit un segment de code CPL3 et un segment de données plat. Le TSS porte le RSP0 de la pile superviseur du thread sélectionné. Le noyau construit le cadre initial `iretq` avec IF actif et IOPL zéro, initialise les sélecteurs utilisateur et efface les registres généraux. Sur IRQ, le stub conserve les GPR et les sélecteurs de données, et le cadre matériel conserve RIP, CS, RFLAGS, RSP et SS. Un RSP utilisateur non canonique est rejeté avant le retour d’interruption. Une faute de page entre par IST, puis un défaut utilisateur confinable quitte cette pile commune avant la terminaison. La porte DPL3 `int 0x81` accepte uniquement le code de sortie dans RAX. Les chemins rapides hérités du firmware sont désactivés.

**AArch64.** TTBR1 conserve les mappings EL1. TTBR0 désigne la racine privée du processus courant ou la racine vide des threads noyau. Les feuilles utilisateur sont non globales, portent AP pour les droits EL0 et PXN pour interdire leur exécution à EL1. UXN protège les données et les piles. Les changements de TTBR0 réutilisent l’ASID zéro avec barrières et invalidation complète des traductions avant le retour à EL0. La publication de code nettoie les données au point d’unification et invalide les alias d’instructions. [Modèle d’exceptions Arm](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Exception%20model.pdf)

Le premier `eret` reçoit un SPSR EL0t, un PC validé et une pile SP_EL0 privée. Les registres entiers et TLS sont initialisés à zéro. Les vecteurs d’exception issus d’EL0 utilisent la pile SP_EL1 du thread. Le chemin IRQ conserve x0 à x30, ELR, SPSR, SP_EL0 et TPIDR_EL0. Le programme utilise `svc #0x51` avec son statut dans x0. Le noyau vérifie le niveau d’origine et l’immédiat avant d’accepter la sortie.

## Vérifier sur QEMU

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
```

Dans `proc-test`, deux travailleurs gardent des motifs différents dans leurs registres et sur leurs piles. Chacun fait progresser un compteur à la même adresse virtuelle. L’un relit aussi un témoin privé et, sur ARM, chaque travailleur contrôle son TLS. Le banc exige au moins huit commutations **décidées par une IRQ timer interrompant directement le mode utilisateur** pour chacun. Les travailleurs ne cèdent jamais volontairement le processeur. Ils signalent un motif incorrect par le code 255.

Les processus fautifs sont lancés pendant ce calcul. Le banc vérifie le niveau matériel d’origine, le PC, l’adresse, le type d’accès et le syndrome, puis exige la sortie des deux survivants. Sur x86, le refus au retour d’IRQ d’un RSP non canonique est un contrôle logiciel effectué avant `iretq`. Les journaux complets sont conservés sous `build/serie-<architecture>-proc-test.log`.

Le scénario `proc-boot-fault` injecte aussi une instruction privilégiée dans le premier enfant du lanceur normal. Le second doit sortir avec le code 82. Le banc vérifie ensuite la saisie AZERTY et le défilement sur x86, ou la bannière et WFI sur ARM. Le lanceur borne son attente et rend la main à l’interface même si un enfant tarde à terminer.

## Mesures QEMU du 10 novembre 2026

Rust stable 1.99.1, QEMU 11.1.3, TCG, 256 Mio et un CPU par VM. Les programmes plats occupaient 403 octets sur x86_64 et 672 octets sur AArch64.

| Mesure du scénario `proc-test` | Q35 x86_64 | virt AArch64 |
| --- | ---: | ---: |
| PID distincts ayant atteint le mode utilisateur | 78 | 78 |
| Fautes confinées | 11 | 11 |
| Préemptions des travailleurs 1 / 2 | 25 / 24 | 24 / 23 |
| Itérations privées observées des travailleurs 1 / 2 | 2 928 818 / 7 169 649 | 2 686 086 / 4 849 274 |
| Cadres libres avant → après | 53 881 → 53 881 | 56 463 → 56 463 |
| Créations refusées avec restitution intégrale | 5/5 | 5/5 |
| Créations et collectes successives après les fautes | 64/64 | 64/64 |
| Fils collecté sans attente parentale sous charge | 1/1 | 1/1 |

Durant les cinq créations forcées à échouer, il restait respectivement 0, 2, 5, 9 et 20 cadres. Le banc a retrouvé chaque budget après le retour d’erreur, y compris après allocation partielle de la pile superviseur. Les deux TCB noyau sont restés les seuls présents. Pendant la phase suivante, le parent a abandonné son droit de suivi sur un fils qui est sorti avec le code 23. La collecte de cet orphelin a été constatée alors que les deux travailleurs de calcul étaient encore prêts, sans appel parental de collecte ni nouvelle création. Au terme du scénario, le Buddy a retrouvé exactement son état initial et la table des processus était vide.

| Violation volontaire | Diagnostic x86_64 | Diagnostic AArch64 |
| --- | --- | --- |
| Lecture du témoin appartenant exclusivement au voisin | `#PF` · `0x4` | `ABORT` · `0x92000006` |
| Lecture de la moitié haute du noyau | `#PF` · `0x5` | `ABORT` · `0x9200000f` |
| Écriture sur le témoin RO de son propre espace | `#PF` · `0x7` | `ABORT` · `0x9200004f` |
| Écriture sur le code RX | `#PF` · `0x7` | `ABORT` · `0x9200004f` |
| Branchement sur les données NX | `#PF` · `0x15` | `ABORT` · `0x8200000f` |
| Écriture dans la garde basse | `#PF` · `0x6` | `ABORT` · `0x92000046` |
| Écriture dans la garde haute | `#PF` · `0x6` | `ABORT` · `0x92000047` |
| Entrée rapide désactivée ou service inconnu | `#UD` · `0x0` | `SVC` · `0x56000052` |
| Pile utilisateur invalide | `#SS` · `0x0` | `ABORT` · `0x92000044` |
| Instruction privilégiée | `#GP` · `0x0` | `UNDEFINED` · `0x2000000` |
| Instruction FPU/SIMD | `#NM` · `0x0` | `FP` · `0x1fe00000` |

## Résultats des suites complètes

Les deux commandes `make test` ont terminé avec un code de sortie nul. Les captures intégrales ont validé le framebuffer sur les deux architectures, ainsi que l’édition et le défilement AZERTY sur Q35. Les autres démarrages ont exercé les protections superviseur, la récupération après pénurie, les sentinelles du Buddy, le mutex FIFO, les registres et les gardes de pile.

| Mesure | x86_64 | AArch64 |
| --- | ---: | ---: |
| Scénarios QEMU réussis | 22/22 | 21/21 |
| Préemptions de threads noyau pendant les 15 162 ticks de `sched-stress` | 15 162 | 15 162 |
| Durée hôte de cette VM, amorçage compris | 62,0 s | 95,8 s |
| Durée hôte de l’endurance courte, en secondes après amorçage | 15,501 | 16,727 |
| Threads collectés pendant cette endurance | 7 539 | 5 014 |

Les mesures de deux heures archivées concernent la v0.5.1. La régression d’endurance de cette livraison a utilisé les durées indiquées ci-dessus.

## Limites

La version cible un CPU QEMU TCG et un thread utilisateur par processus, avec une pile superviseur de 64 Kio par thread. La table possède 32 emplacements de processus et l’ordonnanceur 32 TCB partagés avec idle et les threads noyau. L’ajout de plusieurs threads dans un processus demandera notamment la terminaison coordonnée du groupe. Les adresses utilisateur sont fixes et les pages appartiennent exclusivement à leur espace.

Les fautes synchrones utilisateur confinables terminent le processus. Une corruption interne, une faute superviseur ou certaines erreurs matérielles restent terminales. FP/SIMD est interdit. Le statut de sortie et les mots de contrôle de la démonstration constituent une convention locale à cette image. L’isolation a été testée sur les machines virtuelles configurées, au niveau architectural. Le matériel physique, les canaux auxiliaires et les exécutions multicœurs restent hors des mesures.
