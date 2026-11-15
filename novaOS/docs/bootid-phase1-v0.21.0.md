# BootID et identité durable · qualification de v0.21.0

Le profil x86_64 de développement dispose de RDRAND. Le profil AArch64 ordinaire utilise Cortex-A72 sans FEAT_RNG. Sur ce dernier, EDK II et Limine ne remettent aucun DTB au noyau dans le démarrage observé. Le profil AArch64 `-cpu max` permet une qualification séparée de RNDR. La disponibilité de l’époque durable sur Cortex-A72 reste soumise à arbitrage. Les manifestes NovaOS, nova-system et NovaFS identifient toujours respectivement v0.20.0, v0.2.0 et v0.9.13 P9.

## Origine et portée du BootID

Au premier démarrage du noyau, le BSP détecte RDRAND par CPUID ou FEAT_RNG par ID_AA64ISAR0_EL1. Il lit deux mots de 64 bits. Chaque mot exige une indication de succès de l’instruction et admet au maximum dix essais. Deux mots identiques sont rejetés comme signe élémentaire d’une source bloquée. Le résultat complet, ou l’indisponibilité définitive pour ce boot, est publié une seule fois avant les CPU secondaires. Le BootID est opaque, lisible par une instance dans son propre triplet et dépourvu d’autorité. [Source et tests unitaires](../kernel/src/boot_id.rs)

La qualification dépend de la chaîne d’entropie de l’hôte. Les binaires Homebrew QEMU 11.1.3 utilisés ont pour SHA-256 `a7ace1986bb2af7e706402c3bd36b8450a463409e17a707a4c5a078cc521aba1` (AArch64) et `a462b64050825e6adfc88ea14b3896205056b2f0b0090938b536ce69381739ab` (x86_64). Sur la voie ordinaire, le code QEMU appelle son générateur cryptographique et le fournisseur GnuTLS demande `GNUTLS_RND_RANDOM`. La voie déterministe de QEMU et la relecture d’un enregistrement sont exclues du profil. Une différence entre échantillons illustre la fraîcheur observée. La justification de la source vient de cette chaîne, du profil de lancement et des contrôles de succès de l’instruction. [Sélection du générateur QEMU](https://github.com/qemu/qemu/blob/master/util/guest-random.c) · [Fournisseur cryptographique QEMU](https://github.com/qemu/qemu/blob/master/crypto/random-gnutls.c) · [Niveaux de GnuTLS](https://www.gnutls.org/manual/html_node/Random-number-generation.html)

La machine virtuelle Arm peut placer une graine dans son arbre de périphériques. Cela exige que le chargeur transmette l’arbre, que le noyau sache en établir la provenance et que la graine soit renouvelée à chaque démarrage pertinent. La seule présence de l’option de QEMU ne satisfait pas ces conditions pour le profil Cortex-A72 testé. La fonctionnalité d’entropie du protocole Limine amont admet elle-même un générateur initialisé par le temps en l’absence de source forte. Le noyau ne l’utilise pas. [Machine Arm virt](https://www.qemu.org/docs/master/system/arm/virt.html) · [Contrat d’entropie de Limine](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#entropy-feature)

## Incarnation complète et contrôle d’accès

La trame contient 16 octets opaques de BootID, puis deux entiers de 64 bits little-endian, InstanceID et génération. Le premier entier est attribué par le registre commun, le second avance à chaque réemploi du slot. Une instance conserve la même trame de sa création à sa destruction. Chaque processus enfant autorisé référence la même entrée. Ses threads utilisent l’appartenance du processus. Le SDK public et les indices du catalogue v0.20.0 conservent leur contrat.

| Porte interne de phase 1 | Arguments actuels | Autorité vérifiée |
|---|---|---|
| `INFO` | `(2, sortie, 32)` | Identité propre. Les sélecteurs volatils 0 et 1 restent disponibles. |
| `WORKER` | `(entrée, argument, 32)` | Principal admis et autorisation du manifeste, puis égalité de la trame complète. |
| `OFFER_STOP` | `(entrée, 0, 32)` | Principal applicatif admissible et identité propre. |
| `OFFERED` | `(droit, sortie, 32)` | Droit local d’attente et offre consentie par le principal cible. |
| `STOP` | `(droit, entrée, 32)` | Même droit et même offre, puis identité complète de la cible. |

Les portes vérifient la mémoire utilisateur lors de la copie. Sans BootID, une demande d’identité durable autorisée reçoit `REFUSED`. Le numéro local, un PID, le chemin de l’application et les octets d’un triplet observé ne permettent aucune délégation. Le rendez-vous privé du gestionnaire utilise une liaison vivante au noyau, conservée exclusivement en mémoire pendant ce démarrage. [Trame](../abi/instances.rs) · [Contrôles du lanceur](../kernel/src/sched/process/ipc/spawn.rs)

## Résultats du 15 novembre 2026

| Profil QEMU 11.1.3 | Démarrages | Résultat |
|---|---:|---|
| AArch64 `max`, RNDR | 3 | Trois époques distinctes. Deux triplets du démarrage précédent refusés par la porte de création d’enfant. |
| x86_64 `max`, RDRAND | 3 | Même épreuve réussie, avec le même InstanceID et la même génération aux trois démarrages. |
| AArch64 Cortex-A72 ordinaire puis Cortex-A72 avec graine DTB désactivée | 2 | Identité durable et création du travailleur refusées. PROCESS_ID, IPC_CREATE et IPC_NOTIFY réussissent dans l’application. |
| x86_64 avec RDRAND et RDSEED masqués | 1 | Même refus explicite et mêmes appels v1 réussis. |

Sur chaque architecture le premier invité atteint son arrêt normal, puis QMP réinitialise la même VM. Le second invité reste actif après avoir collecté son enfant. Le banc tue alors QEMU par SIGKILL et lance un nouveau processus. Chaque fois, le principal détient le droit de créer un enfant, présente le triplet ancien et reçoit un refus. La même opération réussit avec sa nouvelle identité. Les sept journaux série couvrent les neuf démarrages puisque deux boots successifs écrivent dans le même journal lors du reset. [Valeurs individuelles, profils et SHA-256](validation/bootid-v0.21.0-redemarrages.json) · [Journaux intégraux](validation/bootid-v0.21.0-journaux.tar.gz)

Les 7 tests unitaires passent par architecture hôte, ARM64 natif et x86_64 sous Rosetta. Les six profils SMP de l’application passent sur QEMU `max` (24 fins de famille). Chaque valeur est le nombre de cadres libres retrouvé à l’identique avant et après la sonde. Le nombre de TCB retrouve respectivement 2, 3 et 5.

| Architecture | 1 vCPU | 2 vCPU | 4 vCPU |
|---|---:|---:|---:|
| aarch64 | 62520 | 62486 | 62437 |
| x86_64 | 63071 | 63014 | 62932 |

Sur les profils de développement ordinaires, les régressions cycle de vie des processus, ELF, courses IPC et CLI sur secteurs de 512 et 4 096 octets réussissent sur les deux architectures. Le test des capacités vérifie le refus du triplet altéré avec le bon droit, le refus du bon triplet avec un mauvais droit, et la révocation des descendants après arrêt. Les campagnes longues NovaFS, stockage et DMA restent couvertes par leurs dossiers historiques. [Qualification précédente](instances-phase1-v0.21.0.md)

Commandes depuis `novaOS/`, avec l’environnement temporaire du projet défini :

```sh
for arch in aarch64 x86_64; do
    make ARCH="$arch" instances-modele-test instances-reboot-test
    NOVA_ARM_CPU=max make ARCH="$arch" instances-test
    make ARCH="$arch" lifecycle-test elf-test ipc-races cli-test
done
```

## Limites et décision

Le BootID assure une unicité pratique sous l’hypothèse d’un générateur qualifié. Il ne permet pas de détecter qu’une machine a repris une copie de sa mémoire ni de garantir la qualité cryptographique d’un hyperviseur mal configuré. La qualification QEMU en TCG ne vaut pas qualification d’UTM, d’un processeur physique ou d’une perte d’alimentation.

Pour Cortex-A72, deux décisions sont possibles. La première consiste à conserver le profil historique pour les fonctions volatiles et à exiger explicitement FEAT_RNG pour les opérations durables. La seconde consiste à qualifier une source d’entropie transmise par le firmware ou un périphérique isolé, renouvelée à chaque démarrage normal et forcé. Dans les deux cas, une époque absente ou ambiguë interdit de reprendre une ancienne racine temporaire. Le contrat prévoit des vues neuves, une vérification d’appartenance et la quarantaine. [Contrat de phase 2](contrat-tmp-prive-v0.21.0.md)
