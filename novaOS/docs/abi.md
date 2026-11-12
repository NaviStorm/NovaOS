# ABI native NovaOS v1 — v0.7.0

Le contrat partagé se trouve dans `abi/native.rs`. Les deux architectures utilisent des mots de 64 bits et un ordre little-endian. Un sélecteur vaut `(1 << 16) | opération`. Un appelant peut reconnaître une version incompatible avant d’interpréter sa réponse. Les valeurs attribuées dans cette version sont fixes. Une rupture future exige une nouvelle version et une notice de migration.

La v0.8.0 ajoute les sélecteurs `0x10020` à `0x10029`. Les appels ci-dessous conservent leurs valeurs et leur convention de retour. [Extension IPC](ipc-abi.md)

Le port privé du pilote utilise `0x10040`. La préparation de v0.11.0 attribue `0x10041` à `HEAP_RESIZE`. Cette extension publique traite les bornes en octets du tas privé avec les conventions de registres ci-dessous. Le troisième argument réservé doit être nul. [Contrat, erreurs et qualification du tas](novafs-integration-v0.11.0.md#tas-privé-et-abi)

La v0.13.0 attribue `0x10042` à `SPAWN` et `0x10043` à `CONSOLE`. Ces appels utilisent les mêmes registres et conventions de retour sur les deux architectures. [Lancement, arguments et droits du terminal](cli-v0.13.0.md#contrat-de-lancement-et-de-console)

La v0.16.2 ajoute le sélecteur de présentation 65 à `CONSOLE`. Il exige les droits conjoints du propriétaire de la session interactive. [Opérations et bornes](vi-v0.16.2.md#contrat-terminal)


## Appel et retour

| Architecture | Instruction | Sélecteur | Arguments a, b, c | Retour statut, valeur |
| --- | --- | --- | --- | --- |
| x86_64 Ring 3 | `syscall` | RAX | RDI, RSI, RDX | RAX, RDX |
| AArch64 EL0t | `svc #0x4e` | x8 | x0, x1, x2 | x0, x1 |

Les arguments inutilisés sont ignorés. Un succès renvoie le statut zéro. En cas d’erreur, la valeur est zéro. Sur x86, RCX et R11 sont détruits par l’instruction. Les autres GPR hors résultats, RSP et les sélecteurs de données sont préservés. Les drapeaux arithmétiques et DF sont conservés, IF est imposé et IOPL reste nul. Sur ARM, x2 à x30, SP_EL0, TPIDR_EL0 et NZCV sont préservés. Le retour utilise EL0t et autorise les IRQ. Les routines de l’image indépendante dans `user/gate.rs` montrent comment déclarer les registres et les effets mémoire à Rust.

| Sélecteur | Appel | Arguments | Valeur en cas de succès |
| --- | --- | --- | --- |
| `0x10001` | `process_id` | Aucun | Numéro du processus courant |
| `0x10002` | `thread_id` | Aucun | Numéro de génération unique du thread courant |
| `0x10003` | `thread_yield` | Aucun | Zéro, à la reprise du thread |
| `0x10004` | `process_exit` | a = code de sortie u64 | Terminaison définitive |
| `0x10005` | `process_wait` | a = clé d’attente, b = adresse du résultat, c = 32 | 32 octets écrits |
| `0x10006` | `debug_write` | a = adresse des octets, b = longueur | Nombre d’octets acceptés |

`debug_write` accepte de 1 à 256 octets ASCII entre `0x20` et `0x7e`. Le noyau copie et filtre le message avant d’ajouter son préfixe PID et sa fin de ligne. Le quota est de seize messages acceptés pour la durée de vie du processus. Un message rejeté ne consomme aucune unité.

| Statut u64 | Signification |
| ---: | --- |
| 0 | Succès |
| 1 | Version du sélecteur inconnue |
| 2 | Numéro inconnu dans la version 1 |
| 3 | Longueur, alignement ou texte invalide |
| 4 | Plage utilisateur inaccessible ou permission matérielle insuffisante |
| 5 | Droit absent, incorrect ou déjà consommé |
| 6 | Quota de diagnostics atteint |

## Résultat d’attente et droits

`WaitInfo` est alignée sur 8 octets et mesure exactement 32 octets. Le noyau sérialise les champs individuellement. Aucun remplissage de pile, adresse noyau ou descripteur interne n’est copié.

| Offset | Type | Champ |
| ---: | --- | --- |
| 0 | u32 | Version, actuellement 1 |
| 4 | u32 | Taille, actuellement 32 |
| 8 | u64 | Identifiant du processus terminé |
| 16 | u64 | Nature de la terminaison, 0 pour une sortie, 1 pour une faute |
| 24 | u64 | Code de sortie, ou zéro en cas de faute |

Le lanceur de confiance peut transférer son droit unique de suivi à un nouveau processus. La référence interne contient le slot et la génération de la cible. Son type `WaitRight` autorise exclusivement l’attente. La clé transmise au programme est comparée à celle installée dans le processus appelant avant la résolution de l’objet. Connaître le PID ou la clé d’un autre processus ne confère aucun droit.

Une cible vivante fait bloquer l’appelant. L’inscription et la vérification du prédicat sont atomiques sur CPU0. Après collecte des pages et de la pile, le collecteur réveille le détenteur. Un droit conservé maintient le statut de terminaison disponible après la restitution des pages. Le noyau consomme le droit après une copie réussie. Une adresse devenue invalide rend l’erreur 4 et permet une nouvelle attente avec une autre destination. La fin du détenteur libère un droit inutilisé. Les droits sont accordés à la création vers un objet plus ancien. La v1 possède un emplacement d’attente par processus.

## Frontière de privilèges

Sur x86, STAR sélectionne le code noyau `0x28` et la base de retour `0x40`. Les sélecteurs de retour CPL3 sont `0x53` pour CS et `0x4b` pour SS. LSTAR contient l’adresse de la porte assembleur. FMASK masque notamment IF, TF, DF et AC. EFER.SCE est activé après les MSR. Dès l’entrée, le stub sauvegarde le RSP utilisateur dans un mot superviseur propre au BSP et charge RSP0 depuis le TSS avant son premier accès à la pile. NMI et erreur machine disposent d’une IST terminale pour couvrir cette fenêtre. [Manuel système Intel, SYSCALL et SYSRET](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf)

Le PC doit appartenir à la page RX privée et SP à la pile du processus, sommet inclus. Ces pages restent possédées pendant l’appel. Sur un cadre valide, le retour ordinaire emprunte `sysretq`. TF ou DF empruntent `iretq` sur un cadre intégral validé. Une adresse invalide termine le processus sur sa pile noyau. SYSENTER demeure désactivé. Le noyau active SMAP et SMEP si CPUID les annonce.

Sur ARM, le vecteur synchrone venant d’EL0 sauvegarde tous les GPR, ELR, SPSR, SP_EL0 et TLS sur SP_EL1 avant le dispatch. Il contrôle le mode enregistré, la classe SVC64 et l’immédiat. Le PC de retour doit aussi être aligné sur 4 octets, SP sur 16. Le retour `eret` reconstruit un SPSR EL0t. Si le processeur annonce FEAT_PAN, le noyau active PAN et configure SCTLR_EL1.SPAN pour sa réactivation à l’exception. [Modèle d’exceptions Arm](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Exception%20model.pdf) · [Registre PAN](https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/PAN--Privileged-Access-Never)

Les interruptions sont réautorisées après la sauvegarde du contexte. Une préemption matérielle, une cession ou une attente suspendent alors la pile noyau privée. Les deux portes masquent les IRQ avant de restaurer les registres et la pile utilisateur.

## Copies utilisateur

`copy_from_user` et `copy_to_user` bornent la longueur, vérifient l’addition de l’adresse et rejettent la moitié haute. Pour chaque page, elles vérifient la propriété du mapping, les entrées parentes, le descripteur matériel de feuille, les permissions et l’appartenance du cadre à la RAM gérée. Les primitives traduisent au plus deux segments pour une opération de 256 octets.

La validation complète et le transfert par le HHDM superviseur s’effectuent sous le même verrou mémoire, IRQ locales masquées. Aucune adresse traduite n’en sort. Une seconde page absente laisse la destination intacte. Une opération bloquante conserve uniquement l’adresse numérique, refait la traduction après le réveil et vérifie à nouveau les permissions. [Sondes QEMU et mesures physiques](mesures-v0.7.0.md)

## Migration et limites

Les binaires de démonstration v0.6.0 utilisant `int 0x81` ou `svc #0x51` doivent être recompilés. L’image livrée utilise les six sélecteurs natifs. Le système reste mono-CPU, avec un seul thread utilisateur créé par processus et un espace à adresses fixes. Le mot de transition x86, la synchronisation locale et les invalidations de TLB devront évoluer pour plusieurs CPU. Le quota du journal est par processus. Les validations portent sur QEMU TCG et l’isolation architecturale.
