# VirtIO-net sous QEMU

## État du confinement DMA

Le prototype OVMF Q35 et son témoin Secure Boot ont été exercés jusqu’à l’activation de VT-d avec une racine vide. Sur AArch64, le contrat de propriété du contrôleur partagé entre disque et NIC est soumis à arbitrage. Le NIC reste sans bail sur les deux architectures. [Résultats Q35 et portabilité](virtio-net-ovmf-q35.md) · [Contrat AArch64 pour revue](virtio-net-contrat-iommu-aarch64.md)

## Premier lot et attribution matérielle

Base de développement `5c7f1bfa313a3859ccc0830660438e76d598acb1`. Le premier lot porte sur Ethernet non tagué dans deux machines de laboratoire, Q35 x86_64 et virt AArch64. Le gestionnaire matériel peut déjà recevoir un inventaire PCI en lecture seule. Il reconnaît la fonction Ethernet moderne de type VirtIO 1 (1af4:1041, classe 0200). Une correspondance de l'inventaire ne permet aucun transfert.

Le courtier superviseur existant distingue lecture, écriture et IRQ et lie les ressources à une incarnation. Le portail du disque a une géométrie, des commandes, une route IRQ et un domaine DMA propres au stockage. Les attributions EDU et NVMe sont elles aussi limitées à des appareils et topologies précises. L'attribution de la carte Ethernet nécessite une décision explicite. [Courtier matériel](../kernel/src/hardware/driver_resources.rs) · [Attributions de laboratoire](../kernel/src/hardware/pci_inventory/lab_resources.rs) · [Transport bloc](../kernel/src/hardware/block/mod.rs)

**Contrat proposé pour arbitrage.** Le superviseur identifie la fonction PCI et son groupe isolable, refuse les topologies ambiguës et réserve une instance de pilote utilisateur. Il garde les BAR, la configuration PCI, les adresses DMA et la publication finale des descripteurs. Un bail privé permet seulement de poster un tampon RX, de soumettre une trame TX, de prendre les complétions et d'attendre la notification de cette instance. Chaque opération vérifie le titulaire, la génération du bail, la file, le slot et la longueur. Aucune délégation à une application. La future pile recevra une interface de trames séparée après décision sur ses droits.

Prévoir un domaine de traduction dédié au seul NIC, sans traduction vers la mémoire des applications ou vers le disque. Mettre le matériel à zéro, négocier VERSION_1, ACCESS_PLATFORM et MAC, valider FEATURES_OK, lire la MAC sous génération stable, préparer les deux files, poster les tampons RX, activer DRIVER_OK. Écarter les offloads, les files multiples et les tampons RX fusionnés. Une absence de traduction sûre ou un refus de fonctionnalités arrête l'initialisation. [Spécification VirtIO, périphérique réseau](https://github.com/oasis-tcs/virtio-spec/blob/master/device-types/net/description.tex)

Le pilote gère deux files séparées, RX 0 et TX 1, plafonnées à seize entrées chacune. Prévoir un tampon de 2 Kio par entrée et un descripteur par trame. Publier avec barrière mémoire avant notification. À l'IRQ, identifier la source, lire les indices avec acquisition et drainer les deux files dans leur borne. Revérifier après réarmement pour couvrir une complétion concurrente. Une file pleine exerce une contre-pression. Une complétion dupliquée, inconnue ou hors bornes arrête la file. À la fermeture, bloquer les soumissions, masquer et purger l'IRQ, constater la fin du DMA, invalider les traductions et détacher le domaine avant restitution. Garder les pages en quarantaine si une étape reste incertaine.

## Objectifs proposés avant les essais de données

| Épreuve | Mesure et seuil initial de laboratoire |
|---|---|
| Intégrité | Pair Ethernet synthétique sur socket Unix privé, comparaison octet par octet des trames de 60 à 1 514 octets sans FCS, identifiants de séquence dans les deux sens. Aucun TAP ni routage hôte. |
| Saturation | Bloquer les complétions, remplir 16 slots, constater le refus de la 17e demande, reprendre sans écrasement. Compter séparément les pertes RX faute de tampon. |
| Endurance et IRQ | Au moins 100 000 trames par sens pour franchir le bouclage des indices 16 bits. Prouver la progression sans polling de secours, y compris après réarmement. |
| Mémoire invitée | Mesurer cadres avant attachement, au repos, au pic de saturation et après retrait. Budget de données de 64 Kio par carte, anneaux, traductions et mémoire du processus comptés séparément. Zéro tampon perdu et retour aux compteurs de référence après retrait confirmé. |
| Débit et latence | Un vCPU TCG, 256 Mio, échauffement 5 s puis trois fenêtres de 10 s. Cible initiale de 1 Mbit/s utile par sens sur trames de 1 514 octets et RTT p95 inférieur à 50 ms sur trames de 64 octets, mesuré par le pair sur une seule horloge monotone. Publier les mesures même si un objectif est manqué. |

## Reproduction de l'inventaire

Depuis la racine du worktree dédié, exporter les temporaires selon AGENTS.md. Construire chaque noyau, puis exécuter le banc depuis le worktree. Le script conserve ISO, ELF, trace série et inventaire QMP par architecture dans un nouveau répertoire durable. Il prend le verrou commun avant les VM. Le hub d'inventaire a un seul port et ne donne accès à aucune interface hôte.

```sh
make -C novaOS ARCH=x86_64 kernel
make -C novaOS ARCH=aarch64 kernel
python3 -B novaOS/tests/virtio_net_qemu.py ../archives-virtio-net/inventaire-01
```

## Jalons proposés

Conserver v0.25.0 comme proposition de jalon réseau. Soumettre successivement le lot Ethernet VirtIO et son isolation DMA, le service de trames et ses droits clients, puis la pile IPv4/IPv6 et les transports avec leurs mandats propres. Le premier critère de passage est un échange réel avec le pair isolé sur les deux invités, avec restitution prouvée. La qualification sur matériel physique exigera des preuves propres au matériel et à son IOMMU. [Feuille de route](reconciliation-v0300/proposition.md)

## Résultats du 25 novembre 2026

| Contrôle exécuté | x86_64 | AArch64 |
|---|---|---|
| Inventaire invité et QMP, commit `0c96925babdbfa367d3f3be2383feb1aea576771` | 8 fonctions concordantes, NIC au 00:07.0, 1,962 s | 5 fonctions concordantes, même BDF, 5,698 s |
| Inventaire et témoin logiciel invité, commit `2bef4d84c073122342f8af777d1c2492808ffa16` | Ring 3, succès en 1,789 s | EL0, succès en 5,694 s |
| Files logicielles par invité, sans DMA | 100 000 réservations/complétions RX et 100 000 TX, saturation, bouclage, restitution et refus du doublon | Mêmes contrôles réussis |
| Protocole d'initialisation, format de trame et erreurs sur dispositif simulé, huit tests Darwin par cible | 8/8 sous Rosetta | 8/8 natifs |
| Taille des structures Rust mesurée sur Darwin | `Queue` 6 octets, `Link` 18 octets | `Queue` 6 octets, `Link` 18 octets |
| Échange Ethernet avec un pair, IRQ réelle du NIC, mémoire DMA au repos/en charge, débit et latence | Non exécutés, bail soumis à arbitrage | Même état |

Les durées du tableau englobent le démarrage et la collecte. QEMU 11.1.3 utilisait TCG, un vCPU, 256 Mio et un hub privé sans autre port. Dans les deux invités, le processus d'inventaire est sorti avec le code 0. Le scénario vérifie également le refus de son accès au bootstrap IPC et aux portails bloc. L'état logiciel des files est restitué après chaque lot de seize slots. Aucun transfert réseau n'intervient dans le témoin.

Sur Q35, les deux traces relèvent `master=true` dans l'inventaire PCI du NIC. Sur virt AArch64 elles relèvent `master=false`. Ces captures décrivent le bit au moment de l'inventaire. Elles n'établissent ni l'origine de sa valeur ni la protection DMA depuis le firmware. Cette condition devra être vérifiée avant l'attribution. Les 92 Kio du contrat sont un budget partiel proposé. La mémoire réelle du futur pilote actif et de son domaine reste à mesurer. [Contrat intégral soumis à arbitrage](virtio-net-bail-dma.md)

Le verrou commun a été détenu de 10:39:30,234 à 10:39:37,938 UTC pour l'inventaire (attente 0,351 ms) et de 10:46:00,591 à 10:46:08,118 UTC pour les témoins de files (attente 0,301 ms). Les deux campagnes se sont terminées avec libération du verrou. Le contrôle `make -C novaOS abi-check` a réussi.

Pour reproduire la deuxième campagne, depuis `novaOS/`, construire les deux cibles comme suit puis exécuter le banc. Le noyau ordinaire ne sélectionne pas la sonde.

```sh
for NOVA_ARCH in x86_64 aarch64; do
    export NOVA_ARCH
    . scripts/target.sh
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
        --release --locked --offline --target "$rust_target" --features virtio-net-probe || exit 1
done
python3 -B tests/virtio_net_qemu.py --files ../../archives-virtio-net/files-reproduction
```

Les séries brutes, relevés QMP, résultats Darwin et horaires sont publiés dans [l'archive de traces](validation/virtio-net/traces.tar.gz). Les ISO et ELF exacts sont conservés dans `OS-worktrees/archives-virtio-net/{inventaire-01,files-01}/`. Leurs octets ont été relus et comparés aux empreintes du banc. Vérifier l'archive publiée avec `shasum -a 256 -c SHA256SUMS` depuis le répertoire de validation. Vérifier les pièces locales depuis la racine de l'archive durable avec le manifeste des artefacts. [Empreintes publiées](validation/virtio-net/SHA256SUMS) · [Manifeste des artefacts](validation/virtio-net/artefacts.sha256)
