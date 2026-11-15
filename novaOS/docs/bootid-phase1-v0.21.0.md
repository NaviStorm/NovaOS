# BootID et identité durable · qualification de v0.21.0

Le BootID est qualifié sur le CD de développement QEMU Cortex-A72, AArch64 `max` et x86_64 `max`. La [qualification finale sur Cortex-A72](#qualification-finale-sur-cortex-a72) complète les preuves historiques du premier candidat. Les manifestes indiquent NovaOS v0.20.0, nova-system v0.2.0 et NovaFS v0.9.13 P9.

## Sources CPU du premier candidat

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

## Résultats historiques du candidat `10ccf26`

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

## Qualification finale sur Cortex-A72

Le CD de développement démarre le Cortex-A72 ordinaire de QEMU 11.1.3 avec un BootID de 128 bits sans RNDR. Le défaut du premier candidat venait du passage de relais. La machine `virt` renouvelait bien sa graine, mais le noyau ne recevait aucun DTB par sa requête Limine et connaissait seulement les instructions d’entropie des CPU. La lecture de l’arbre par le chargeur, pendant les services de démarrage EDK II, rend cette source accessible sur ce profil. [Relais UEFI](../boot/relais-bootid/main.rs) · [Lecture par le noyau](../kernel/src/boot_id/efi.rs)

### Source et chaîne de confiance

1. Sur `virt,gic-version=2` avec 256 Mio et le modèle `cortex-a72`, QEMU place 32 octets dans `/chosen/rng-seed`. La voie ordinaire utilise le générateur cryptographique de l’hôte. Le gestionnaire de reset de QEMU renouvelle cette propriété avant de redonner la main au firmware. Le mode déterministe, l’enregistrement/rejeu, un DTB imposé et un hyperviseur ou un chargeur non maîtrisé sortent du profil qualifié. [Machine virt](https://www.qemu.org/docs/master/system/arm/virt.html) · [Reset dans QEMU 11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/arm/boot.c#L586-L591) · [Renouvellement de rng-seed](https://github.com/qemu/qemu/blob/v11.1.3/system/device_tree.c#L613-L628) · [Choix du générateur](https://github.com/qemu/qemu/blob/v11.1.3/util/guest-random.c#L42-L62)
2. Le premier étage du CD utilise le protocole `FdtClient` d’EDK II. Il vérifie `model` et `compatible`, tous deux égaux à `linux,dummy-virt`, le nœud choisi et la longueur exacte de la graine. Il écarte une graine uniforme. Le BootID est constitué des 16 premiers octets de SHA-256 sur le préfixe ASCII `NovaOS/BootID/v1/QEMU-ArmVirt/chosen/rng-seed`, un octet nul et les 32 octets de la graine. Seule l’identité publique dérivée passe dans la table NovaOS. Le relais lance le Limine d’origine sur le même support, y compris en l’absence de graine. [Protocole EDK II du firmware](https://github.com/tianocore/edk2/blob/edk2-stable202408/EmbeddedPkg/Include/Protocol/FdtClient.h) · [Construction du CD](../scripts/iso.sh)
3. Le noyau AArch64 essaie d’abord RNDR si l’instruction est disponible. En repli, il lit la table de configuration EFI transmise par Limine en révision de base 3. Le BSP vérifie le type de firmware, l’en-tête et le CRC de la table système, l’unicité du GUID et le format du message. Chaque lecture physique doit appartenir à la mémoire de données runtime EFI réservée et correctement couverte dans la carte Limine. Une source absente ou invalide laisse les opérations durables en état `REFUSED`. Sur x86_64, la voie qualifiée reste RDRAND. [Sélection et publication unique](../kernel/src/boot_id.rs) · [Contrat Limine](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#base-revision-3)

Le GUID `e8fd92d6-fb62-4aa3-a667-0374baa501d5` identifie la table privée. Ses 32 octets contiennent `NVBOOT01`, deux u32 little-endian de valeurs 1 et 32, puis l’identité opaque non nulle. Le GUID, le CRC et les chaînes du DTB permettent de vérifier le format et la cohérence. La provenance dépend de l’intégrité de l’hôte, de QEMU, du firmware, du relais et de Limine avant le noyau. Le profil de laboratoire fixe ces composants. Secure Boot et l’attestation matérielle restent hors qualification.

### Résultats des deux redémarrages

QEMU 11.1.3 en TCG, macOS 27.2 ARM64, EDK II `edk2-stable202408-prebuilt.qemu.org`, Limine 10.8.5. Pour chaque transition, le même principal présente au noyau deux trames de 32 octets avec le même droit de création issu du manifeste. Les deux trames contiennent le numéro local `8000000000000002` et la génération `1`. L’argument de travail vaut 3. Le principal collecte un vrai processus enfant puis relit son identité. [Programme utilisateur](../user/instance_sample.rs) · [Banc QEMU](../tests/instances_reboot.py)

| Parcours Cortex-A72 | BootID présenté comme ancien | BootID courant | Ancienne identité | Identité courante |
|---|---|---|---|---|
| Arrêt normal du noyau, puis reset QMP dans le même QEMU | `e82a4fb1c84b5e2c408e4565f988190d` | `fa77ccafa2197f01ed25adaa0295f150` | `DENIED` | `OK`, enfant collecté avec 41, identité stable |
| Mort de QEMU par SIGKILL pendant que le principal est actif, puis nouvelle VM | `fa77ccafa2197f01ed25adaa0295f150` | `bd31f51e200607b4e49dc5e9d5d7f6db` | `DENIED` | `OK`, enfant collecté avec 41, identité stable |
| Démarrage séparé avec `dtb-randomness=off` | Sans objet | Source indisponible | `INFO` durable et `WORKER` renvoient `REFUSED` | PID et IPC historiques utilisables |

Sur les trois démarrages positifs, le banc a arrêté le vCPU avant EFI, lu le DTB physique à `0x40000000` et retrouvé, octet par octet, le BootID exposé par le noyau. Le journal indique `UEFI-QEMU-RNG-SEED` aux trois démarrages. Lors du témoin négatif, le banc a vérifié l’absence de la propriété dans le DTB. Les graines brutes ne figurent pas dans les archives. Les profils AArch64 `max`/RNDR et x86_64 `max`/RDRAND ont également réussi le reset, la relance forcée et la relecture de l’identité. [Mesures, empreintes et profils](validation/bootid-a72-v0.21.0-redemarrages.json) · [Journaux série archivés](validation/bootid-a72-v0.21.0-journaux.tar.gz)

### Révocation, collisions et limites de persistance

Les six essais invités, sur 1, 2 et 4 vCPU pour Cortex-A72 et x86_64, couvrent 24 fins de famille. Ils vérifient le retrait des droits dérivés, la survie de la racine extérieure et le retour aux nombres de cadres et de TCB du début de la sonde. Sur ARM64 natif et x86_64 sous Rosetta, les 10 tests de modèle et les 2 tests du relais réussissent sur chaque architecture. Le modèle rejoue délibérément la même époque dans un registre restauré et reproduit le même triplet. Le recyclage normal d’un slot, lui, incrémente la génération et refuse l’ancienne instance. [Journaux des modèles](validation/bootid-a72-v0.21.0-modeles.tar.gz) · [Révocation, relais et vérification de l’image EFI](validation/bootid-a72-v0.21.0-revocation.tar.gz)

| Cadres libres restitués à la valeur d’entrée | 1 vCPU | 2 vCPU | 4 vCPU |
|---|---:|---:|---:|
| Cortex-A72 | 62 519 | 62 485 | 62 436 |
| x86_64 | 63 071 | 63 014 | 62 932 |
| TCB après collecte, pour les deux architectures | 2 | 3 | 5 |

L’unicité est probabiliste. Pour `n` tirages indépendants uniformes de 128 bits, la probabilité d’au moins une collision est au plus `n(n−1)/2^129`. Une restauration du disque seul suivie d’un vrai redémarrage obtient une nouvelle époque si la source extérieure reste fraîche. Le noyau n’écrit aucun compteur d’époque dans la NVRAM ou NovaFS. Un compteur restauré avec le disque pourrait reprendre sa valeur antérieure. Après une coupure électrique réelle, une nouvelle époque dépend de la disponibilité et de la fraîcheur de la source au redémarrage. Le banc exerce la mort du processus QEMU. La coupure électrique du Mac, un Cortex-A72 physique, l’installateur UTM et son démarrage autonome restent à qualifier séparément.

Un simple rechargement du noyau ou du relais dans la même machine, sans reset de la plateforme, ne renouvelle pas la graine du DTB. Une reprise d’instantané incluant la RAM peut restaurer simultanément le BootID et les capacités vivantes. Un hôte qui rejoue une source plausible échappe aux contrôles internes. Une garantie contre ce modèle requiert une autorité extérieure au périmètre restauré. Pour les futurs temporaires, le contrat exige la création exclusive d’une racine neuve et le refus ou la quarantaine si une racine entre en collision ou si sa provenance est ambiguë. L’accès aux anciens temporaires par le futur service reste à tester lors de son implémentation. Le présent essai qualifie la porte de création de processus. [Contrat du futur tmp](contrat-tmp-prive-v0.21.0.md)

### Reproduire les contrôles ciblés

Depuis `novaOS/`, avec les temporaires hôte configurés selon `AGENTS.md`, Rust stable, QEMU, xorriso et `fdtget` (DTC 1.8.1 pour cette campagne).

```sh
rustup target add --toolchain stable aarch64-unknown-none-softfloat aarch64-unknown-uefi x86_64-unknown-none
rustup target add --toolchain stable aarch64-apple-darwin x86_64-apple-darwin
for arch in aarch64 x86_64; do
    make ARCH="$arch" instances-modele-test instances-reboot-test instances-test
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo test \
        --manifest-path boot/relais-bootid/Cargo.toml --locked \
        --target "$arch-apple-darwin" --target-dir build/relais-bootid
done
NOVA_ARM_CPU=max make ARCH=aarch64 instances-reboot-test
```

Les deux cibles hôte `aarch64-apple-darwin` et `x86_64-apple-darwin` sont nécessaires aux modèles et aux tests du relais. Les campagnes longues NovaFS, les tests d’installation et l’ensemble des régressions CLI n’ont pas été rejoués pour ce changement de démarrage. Les preuves de la qualification initiale sont conservées ci-dessus. La validation MOA/MOE de la fusion et celle de la phase du temporaire privé restent à obtenir.
