# NovaOS v0.18.0 — Phase 2 matérielle

## Périmètre qualifié

Le 14 novembre 2026, le gestionnaire de périphériques et un pilote utilisateur distinct ont piloté la carte PCI EDU de QEMU sur AArch64 et x86_64, avec 1, 2 et 4 vCPU. Le pilote lit les vrais registres du modèle, reçoit son interruption INTx et perd son accès matériel lorsque le noyau retire son bail. L'essai utilise 256 Mio de RAM, TCG, un CD UEFI et une VM neuve par profil. Le code qualifié est celui du commit `608d81acc79e0132e7f34c4a1bc60dba4aae4238`. La phase 1 de référence est `9370b63a2843860b11855d28535a5a5228b4e515`. [Preuves série et QMP](validation/materiel-v0.18.0-phase2.json) · [Découverte de phase 1](materiel-v0.18.0.md)

| Niveau | Résultat |
|---|---|
| Opérationnel sous QEMU 11.1.3 | Inventaire PCI, vérification des BAR du profil fermé, page MMIO privée, notification INTx de la carte, acquittement, retrait SMP et restitution. |
| Modèle vérifié sur Darwin | Validation des ressources, contrat de domaine DMA physique, ordre de retrait et quarantaine après erreur injectée. |
| Matériel physique | Aucune exécution sur Dynabook. La porte d'entrée DMA physique refuse l'attribution. Un backend VT-d, AMD-Vi ou SMMU et son retrait matériel restent à qualifier. |

Le profil exige le feature Cargo `materiel-lab` et le scénario de démarrage `materiel-pilote-qemu`. Il utilise l'ABI IPC v1 existante. Les options par défaut restent le stockage classique et le polling VirtIO. Les changements portent sur la branche matérielle, sans nouvelle distribution ni modification de NovaFS v0.9.13 P9.

## Attribution PCIe et MMIO

L'inventaire générique décode les BAR mémoire et I/O ainsi que les fenêtres des ponts de type 1. En présence d'une taille inconnue, il publie l'observation sans autoriser le mapping. Pour EDU, l'admission exige le BDF `0000:00:06.0`, l'identité `1234:11e8`, la topologie fermée, le type et la taille connus de chaque ressource active, leur alignement et l'absence de chevauchement. Les bases et tailles sont recoupées dans le banc avec QMP. Aucun BAR n'est déplacé ou sondé par écriture de masque. [Géométries, ponts et limites](materiel-phase2-pci-dma.md) · [Spécification du périphérique EDU](https://www.qemu.org/docs/master/specs/edu.html)

La carte a une BAR de 1 Mio. Le noyau valide l'ouverture PCI et exclut toute intersection de cette BAR avec la carte mémoire Limine. Il place uniquement sa première page de 4 Kio à l'adresse virtuelle `0x700000` du pilote. Les pages voisines doivent être libres. La PTE est utilisateur, en lecture seule, non exécutable et de type Device sur Arm ou non cachée sur x86. Cette page contient aussi des registres DMA. Le noyau vérifie leur état initial inactif et n'autorise aucune écriture utilisateur directe. Le tampon interne du moteur DMA reste hors de la page attribuée. Les contrôles de pointeurs pour l'ABI excluent la page MMIO. [Attribution et retrait des PTE](../kernel/src/memory/user.rs) · [Bornes de la BAR](../kernel/src/memory/user/mmio_lab.rs)

La réservation est exclusive et liée à la racine de tables du processus et à une époque monotone. Le retrait ferme les handles, supprime la PTE et utilise l'invalidation des traductions SMP avant de rendre les cadres de tables. Une anomalie de vérification conserve la réservation. La destruction d'un espace dont le retrait est incertain met sa racine en quarantaine. La BAR elle-même appartient au périphérique. Elle n'entre jamais dans le pool de cadres RAM.

## Du gestionnaire au pilote utilisateur

1. Le BSP inventorie la carte avant le démarrage des CPU secondaires. Il reprend le seul registre de commande PCI sous un ticket privé. Sur Q35, le firmware avait laissé `PCI_COMMAND=0x0007`. L'adoption le ramène à `0x0002`. Sur Arm la transition est `0x0000 → 0x0002`. L'intervalle avant la reprise par NovaOS reste hors de la qualification.
2. Le gestionnaire lit l'offre en page privée RO. Il compare identité, emplacement, image et droits au catalogue. Le thread du pilote reste bloqué jusqu'à la décision d'attribution. Le noyau valide lui-même les ressources et l'exclusivité. Gestionnaire et pilote ont des PID et des tables distincts. [Gestionnaire](../user/edu_manager.rs) · [Supervision](../kernel/src/sched/process/edu.rs)
3. Le pilote reçoit trois handles locaux. L'un peut uniquement envoyer une commande typée au courtier, les deux autres peuvent uniquement recevoir une réponse ou une notification IRQ. Aucun ne permet la duplication. La commande transporte l'époque et un argument borné. Les écritures de vivacité, de masque, de déclenchement et d'acquittement sont les seules opérations proposées. Le pilote vérifie lui-même en lecture volatile l'identité `0x010000ed`, l'inversion de son mot de vivacité et l'état de la cause IRQ. [Pilote utilisateur](../user/edu.rs) · [Trames privées](../user/edu_protocol.rs)
4. Une sortie normale, une faute du pilote ou la mort du titulaire ferme le bail. Le superviseur traite aussi le pilote qui boucle sans appel système. La préemption l'arrête sur son propre CPU. Une nouvelle incarnation reçoit un nouveau PID et une nouvelle époque. La collecte de chaque couple gestionnaire/pilote précède le scénario suivant. Tous les relancements du banc sont explicites.
5. La fermeture finale vérifie le repos de la carte et du contrôleur, restaure le routage, retire les fenêtres superviseur puis coupe le décodage mémoire, I/O et le bus mastering. Elle vérifie l'identité, la BAR et le mot de commande par relecture. Le ticket reste détenu en cas d'incertitude. La valeur finale observée par QMP est zéro sur les six profils. [Ticket PCI](../kernel/src/hardware/lab_irq/pci.rs)

## IRQ et concurrence

Sur Q35, INTA du slot 6 utilise l'IOAPIC GSI 22 et le vecteur LAPIC `0x52`. Sur `virt`/GICv2, il s'agit de l'INTID 37. Le noyau refuse une route déjà utilisée et conserve la configuration à restaurer. Le pilote n'accède jamais aux registres du contrôleur d'interruptions. Chaque occurrence livrée est liée à l'époque et à un numéro croissant. Le handler masque la route, vérifie la cause de la carte et signale le seul endpoint de réception. Un numéro périmé ou déjà acquitté est refusé. Le démasquage est explicite. Plusieurs déclenchements avant acquittement peuvent se fondre en une seule cause de niveau. [Courtier IRQ](../kernel/src/hardware/lab_irq.rs) · [Route GICv2](../kernel/src/hardware/lab_irq/arm.rs) · [Route IOAPIC](../kernel/src/hardware/lab_irq/x86.rs)

Le retrait ferme les admissions, masque la route, désasserte la cause attendue et vérifie l'absence de transaction IRQ en vol avant de libérer le bail. Le banc force les deux ordres entre acquittement déjà en file et retrait. Il vérifie aussi le réveil d'un TCB effectivement bloqué sur son endpoint, la mort avec cause non acquittée et une nouvelle interruption après la destruction du destinataire. Les six traces permettent de retrouver les PID, l'ordre des événements, l'adresse de chaque faute et le bilan de restitution. [Oracle par scénario](../tests/materiel_pilote.py)

## Matrice du pilote

| Invité QEMU | vCPU | CPU du retrait → CPU du pilote | Scénarios | BAR actives comparées à QMP | PCI_COMMAND final |
|---|---:|---|---:|---:|---|
| AArch64 EL0, virt/GICv2 | 1 | 0 → 0 | 11/11 | 3 | 0 |
| AArch64 EL0, virt/GICv2 | 2 | 0 → 1 | 11/11 | 3 | 0 |
| AArch64 EL0, virt/GICv2 | 4 | 0 → 3 | 11/11 | 3 | 0 |
| x86_64 Ring 3, Q35 | 1 | 0 → 0 | 11/11 | 4 | 0 |
| x86_64 Ring 3, Q35 | 2 | 0 → 1 | 11/11 | 4 | 0 |
| x86_64 Ring 3, Q35 | 4 | 0 → 3 | 11/11 | 4 | 0 |

| Scénario | Propriété contrôlée |
|---|---|
| normal | Lecture de l'identifiant et du complément de vivacité. Deux notifications réellement acquittées, répétitions masquées et faux acquittements contrôlés. Refus du DMA physique avant démarrage. Après collecte, l'interruption tardive produit une entrée dans le handler, zéro notification et un drainage confirmé. |
| refus | Le pilote refuse le démarrage sans attribution. Le gestionnaire récupère la sortie 112 et termine normalement. |
| ecriture, hors-page | Faute de protection sur l'écriture à `0x700004`. Faute de page absente sur la lecture à `0x701000`. |
| retrait | La lecture en cours sur le CPU du pilote finit en faute à `0x700000` après invalidation. |
| pendante | Une cause livrée reste non acquittée lors du retrait. L'ancien handle et la lecture de la page sont ensuite refusés. |
| muet | Le superviseur arrête le pilote qui boucle et restitue ses ressources. |
| acquit-avant, retrait-avant | Le banc confirme que l'acquittement est déjà en file. Un seul des deux ordres l'exécute. Dans les deux cas le bail finit retiré. |
| mort-irq | Une faute utilisateur détruit le pilote pendant que son IRQ est non acquittée. La cause et la route sont drainées avant la réattribution. |
| irq-attente | Le superviseur observe le TCB bloqué sur son endpoint avant de faire lever l'IRQ. Le pilote se réveille, acquitte et termine avec le code zéro. |

Dans chaque VM, l'oracle attribue 22 issues à des PID distincts et reconnaît les sept fautes matérielles attendues. Le bilan final observe zéro page IPC. Après collecte et normalisation du tas, le nombre de blocs alloués, les octets occupés, les pages engagées et les cadres libres retrouvent leurs valeurs initiales. La garde interprocesseurs sérialise les opérations concurrentes. Le test des deux ordres vérifie leurs résultats déterministes, et l'essai à plusieurs CPU vérifie le retrait réel d'une traduction utilisée à distance. [Captures, empreintes d'ELF, ISO et traces](validation/materiel-v0.18.0-phase2.json)

## Régressions et reproduction

| Contrôle exécuté | AArch64 | x86_64 |
|---|---|---|
| Modèles Rust sur macOS | 58/58 natifs | 58/58 sous Rosetta |
| Inventaire QEMU et fenêtres de pont | avec et sans ACPI | PCI autorisé et refusé |
| IPC classique QEMU | profil 256 Mio réussi | profil 256 Mio réussi |
| Bloc classique et sécurité DMA | 4 contrôles × 2 géométries réussis | 4 contrôles × 2 géométries réussis |
| SMP et FPU/SIMD ordinaires | 4 vCPU, 2 profils réussis | 4 vCPU, 2 profils réussis |

Les quatre contrôles bloc par géométrie (512 et 4096 octets) couvrent le motif de données et son retrait, les permissions DMA et l'IRQ pendante, les baux et la relance, ainsi que la panne en vol et la quarantaine. Les neuf suites de modèles incluent 18 tests PCI, 4 tests du contrat DMA physique et 2 tests de géométrie MMIO. Les profils SMP ordinaires vérifient notamment les huit clients, la préemption, la migration et la cohérence des traductions distantes. Le profil FPU/SIMD vérifie deux cycles de migration des huit clients. [Suites de modèles](../tests/materiel_modeles.py) · [Contrôles bloc](../tests/bloc.py) · [Régression SMP](../tests/smp.py) · [Régression FPU](../tests/smp_phase2.py)

Depuis la racine, avec les prérequis du banc de phase 1 installés sur le Mac de qualification, exécuter les commandes suivantes. La cible du pilote construit son ELF dans un répertoire Cargo distinct. Le JSON du rapport conserve les séries et les réponses QMP de façon portable.

```sh
make -C novaOS materiel-modeles-test
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" materiel-pilote-test
    make -C novaOS ARCH="$arch" materiel-test
    (
        cd novaOS || exit 1
        export NOVA_ARCH="$arch"
        python3 -B tests/ipc.py test
        python3 -B tests/bloc.py --only qualify
        PYTHONPATH=tests python3 -B -c 'import os,smp; smp.run(os.environ["NOVA_ARCH"], 4)'
        . scripts/target.sh
        RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
            --release --locked --target "$rust_target" --features user-fpu --target-dir build/materiel-fp
        NOVA_KERNEL_ELF="$PWD/build/materiel-fp/$rust_target/release/novaos" PYTHONPATH=tests \
            python3 -B -c 'import os,smp_phase2; smp_phase2.run(os.environ["NOVA_ARCH"], "smp-fp", 4)'
    )
done
```

## Limites et suite proposée

Le courtier qualifié admet un seul appareil de laboratoire avec une route INTx exclusive, une topologie fixe et des registres dont les lectures sont sûres. Les tailles arbitraires, le routage derrière un pont, le partage INTx, MSI/MSI-X, un périphérique à lectures destructrices et le hotplug nécessitent des politiques distinctes. La présence d'une capacité MSI est vérifiée, son activation est refusée pour EDU. Le contrôle de l'EOI et de la TLB porte sur les deux machines QEMU décrites. Il ne qualifie ni un GICv3 physique ni UTM. La reprise de propriété de la carte ne prouve aucune protection DMA durant l'exécution du firmware.

Aucun fichier du moteur NovaFS, de son format P9 ou de l'ABI publique n'est modifié. Les longues campagnes de stockage, d'endurance IPC et les suites complètes de `vi` et `nsh` n'ont pas été réexécutées. Les preuves de leur qualification v0.17.0 restent attachées à cette version. [Bilan v0.17.0](fusion-smp-v0.17.0.md)

Pour la phase suivante, qualifier dans QEMU le rattachement d'un groupe entier à **un seul backend IOMMU concret**, puis vérifier par observation indépendante le retrait et l'invalidation avant le réemploi d'un cadre. Le choix du backend et toute extension à un périphérique physique relèvent du prochain arbitrage MOA/MOE.
