# NovaOS v0.25.0 · 5Z · Démarrage UEFI du noyau x86_64

Le 28 novembre 2026, le même disque USB virtuel a démarré le noyau NovaOS sur trois machines Q35 sous OVMF standard, Secure Boot inactif. Le noyau exécute le scénario existant `smp-test`, ses tâches Ring 3 et ses échanges IPC, puis stationne les processeurs. Le premier essai physique 5Y reste une qualification du diagnostic EFI STANDARD. **A8 reste fermé pour le noyau.** [Registre](validation/qualifications-demarrage.md)

## Candidat

Image brute GPT de 71 303 168 octets (68 Mio), SHA-256 `c0e0d01c03d7cd6c8214c5bf1cbadd46cc9855aa010f6dace3204e264a5e2c83`. La partition FAT32 NOVAOS porte le premier EFI au chemin amovible, Limine, sa configuration, le plan d’intégrité et le noyau. NOVALOGS conserve ses 24 emplacements FAT32 vides. Deux constructions dans des répertoires distincts sont identiques par `cmp`. Le générateur accepte aussi une capacité exacte en secteurs de 512 octets. Un changement de géométrie change l’empreinte du disque. [Image comprimée](validation/diagnostic-uefi-5z/novaos-live-x86_64.img.gz) · [Empreintes](validation/diagnostic-uefi-5z/SHA256SUMS) · [Reproduction](../boot/live-5z/README.md)

| Composant du démarrage | SHA-256 |
|---|---|
| Premier EFI | `f3f9db92f05d620142bc92594a5d6d0ff29fbfc0076081995a3c194f4eb9bc15` |
| Limine limité au support USB, configuration liée | `31ac49e2c374ede7424e3544d9bc9c45eaca6254e0dc5e5824bff4b4d5cb8dc7` |
| ELF du noyau | `b71244d46542d969a7c33a6b066ca6181c3369deb908a17c04a0125560204e43` |

Le premier EFI vérifie le chemin USB déclaré par UEFI et l’état Secure Boot. Il lit uniquement sur la partition de sa propre image. Il compare le plan de trois composants à sa copie intégrée, puis valide longueur et SHA-256 de chaque fichier. Il réutilise la vérification Ed25519 des trois manifestes de laboratoire et de leur paquet ELF `pkg01`. L’intégrité de l’ELF **du noyau** relève du plan intégré et du BLAKE2b de Limine. Avec Secure Boot inactif, l’admission du premier PE dépend de la vérification externe du support et de sa conservation jusqu’à l’emploi. Le banc présente ce fichier en lecture seule.

`LoadImage` reçoit les octets de Limine déjà vérifiés. Le premier EFI contrôle le parent et le handle de partition avant `StartImage`. La variante de Limine 10.8.5 résout le disque USB parent par son chemin UEFI avant le premier `ReadBlocks`, limite son index à ce disque et à la poignée de partition amorcée, et impose cette partition au démarrage. Les chemins de récupération par PXE, par contenu du secteur ou par recherche de configuration sur une autre unité sont retirés pour ce chargeur. La configuration liée à l’exécutable impose `boot():/NOVAOS.ELF`, son empreinte BLAKE2b et le scénario `smp-test`. Le noyau et les binaires Limine historiques du dépôt restent identifiables à la base 5Y. [Correctif appliqué à l’amont](../boot/live-5z/limine-usb.patch) · [Publication amont v10.8.5](https://github.com/Limine-Bootloader/Limine/releases/tag/v10.8.5)

## Témoins indépendants

| Étape | Observation sur chacun des trois démarrages instrumentés |
|---|---|
| EFI depuis USB | Trace OVMF du chemin xHCI, point d’arrêt TCG à la première instruction du PE, signature de la table système en mémoire et message de validation du premier EFI. |
| Sortie des services de démarrage | Point d’arrêt dans la fonction `ExitBootServices` tirée de la table active, capture de la clé de carte, du RIP de retour et du registre RAX égal à zéro au retour. Limine effectue cette transition. |
| Entrée du noyau | RIP à `nova_start`, adresse `0xffffffff8007ef90`. Les 32 premiers octets lus dans l’invité correspondent au segment ELF livré. L’observateur lit alors la carte transmise par Limine. |
| Mémoire initialisée | RIP à `novaos::sched::start`, après l’installation de l’espace noyau et du tas. Depuis le CR3 actif, l’observateur parcourt quatre niveaux de tables via la RAM physique QEMU. Les 1 192 pages de l’image sont présentes, distinctes, superviseur et réservées au noyau dans la carte Limine. |
| Ordonnanceur en exécution | RIP à `novaos::main_thread` dans la première tâche. La série enregistre ensuite processus Ring 3, préemptions, échanges IPC, rendez-vous de tous les vCPU et arrêt normal. |

Les 1 192 pages se répartissent en 228 pages de code en lecture/exécution, 945 de constantes et une de requêtes en lecture seule NX, 18 de données en lecture/écriture NX. CR0.WP et EFER.NXE sont actifs. Les six cadres de tables parcourus sont disjoints des cadres de l’image. La carte reçue contient également des plages réservées au chargeur. Le noyau existant conserve le relais et les informations SMP avant sa récupération conditionnelle de mémoire. Ce contrôle porte sur les traductions CPU des pages de l’image. L’isolation DMA d’une machine physique demeure indéterminée. [Initialisation du noyau](../kernel/src/main.rs) · [Gestion mémoire](../kernel/src/memory/mod.rs) · [UEFI 2.11, services de démarrage](https://uefi.org/specs/UEFI/2.11/07_Services_Boot_Services.html)

## Stockage, réseau et refus

| RAM / vCPU | Carte Limine | Rendez-vous | SATA du reset à l’entrée EFI | SATA de l’entrée EFI à l’arrêt |
|---|---:|---|---|---|
| 256 Mio / 1 | 37 entrées | `0x1` | 6 lectures, 4 608 octets | 0 lecture, 0 écriture, 0 FLUSH |
| 384 Mio / 2 | 39 entrées | `0x3` | 6 lectures, 4 608 octets | 0 lecture, 0 écriture, 0 FLUSH |
| 512 Mio / 4 | 39 entrées | `0xf` | 6 lectures, 4 608 octets | 0 lecture, 0 écriture, 0 FLUSH |

Un disque interne SATA émulé de 4 Mio et une e1000e sur réseau isolé sont présents pendant chaque essai. Les compteurs bloc QMP, le contenu complet du disque témoin, les traces de commandes SATA, de contrôle et de descripteurs réseau ainsi que les registres PCI ont été contrôlés. Les deux écritures PCI xHCI observées réécrivent la valeur de commande héritée `0x7`. Le scénario noyau se termine avant le lancement du service de stockage. L’activation des pilotes internes, l’utilisation de NovaFS et une session interactive restent hors de ce parcours. [Observations brutes](validation/diagnostic-uefi-5z/observations.tar.gz)

Le chargeur Limine amont, essayé avant restriction du chemin disque, avait émis deux lectures SATA supplémentaires (36 864 octets) entre l’entrée du premier EFI et la sortie des services de démarrage. À 192 Mio un essai exploratoire avec le chargeur amont s’est arrêté dans son allocateur avant `nova_start`. Les traces sont conservées avec leur provenance. La plus petite configuration qualifiée du candidat est 256 Mio.

Quatre démarrages de supports altérés ont produit les motifs `LIMINE`, `CONFIG`, `NOYAU` et `MANIFESTE`, avec CPU arrêté en HLT avant le lancement de Limine. Une substitution de l’instruction d’entrée du premier PE est refusée par le contrôle du disque sur l’hôte. Chaque architecture hôte a passé 18 tests (3 de 5Z, 9 de 5N, 6 de 5O) sous Darwin ARM64 natif et x86_64 sous Rosetta. Le registre ABI contient 48 appels et sa génération est conforme. Les archives des profils antérieurs et la version officielle v0.20.0 restent les références historiques.

Deux éditions de liens brutes avaient des tables de symboles de tailles différentes selon le répertoire de construction. L’emballage conserve les neuf symboles utiles à la qualification et retire les autres symboles hors chargement. À chaque construction, il compare le point d’entrée et tous les segments `PT_LOAD` du noyau original et du noyau emballé, octets, adresses, longueurs mémoire, alignements et permissions compris.

## Conditions d’un second essai MSI

1. Mandat A8 distinct, limité à un démarrage du noyau, à la machine et au support désignés. Maintenir l’état Secure Boot inactif prévu pour cet essai. Vérifier la capacité, la GPT secondaire à sa dernière LBA, les deux partitions et le SHA-256 intégral obtenu après préparation ultérieure. Toute autre géométrie exige sa propre empreinte et un rejeu QEMU de cette image exacte.
2. Valider un moyen de constater localement l’entrée noyau, l’initialisation mémoire, le verdict SMP et l’arrêt sur le MSI. Les témoins GDB/QMP et la série émulée proviennent du laboratoire. Le verdict visible du premier EFI 5Y couvrait seulement le diagnostic.
3. Faire accepter explicitement les lectures et le repli éventuels du firmware avant l’entrée EFI, le niveau de confiance STANDARD, le comportement en cas de refus et l’absence de preuve physique sur les transactions DMA héritées. Si une preuve d’isolation physique est exigée, résoudre d’abord l’attribution VT-d ambiguë de 5Y. L’état CPU W^X observé sous QEMU ne répond pas à cette exigence.
4. Conserver le même binaire x86_64, le seul scénario autorisé, la garde réseau/stockage et la journalisation en RAM. Prévoir sauvegarde et récupération Windows, relevé externe du verdict, critères d’abandon si le chemin USB ou le témoin manque, extinction locale et retour volontaire à Windows. Une écriture native dans NOVALOGS requiert sa propre qualification.

La phase 5Z a utilisé exclusivement des fichiers ordinaires et des périphériques émulés. Le prochain essai physique reste soumis à la décision de la MOE.
