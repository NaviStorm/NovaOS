# VirtIO-net Ethernet — Qualification RX privée du 25 novembre 2026

Référence de départ `7f66f9a635527d22569311e6928d340363fe8062`. Construction des modèles et des témoins hôtes au commit `8404f1598381806eff50e7d7211ed3f86c55fd20`, premier échange Ethernet au commit `1cc06eeed23a0917e6e65d0daddaed1d35084cab`, campagne finale au commit `964424d0aab49fe0259c7e15eaccb61a6e2fb249`. Le profil de qualification active explicitement `net-ethernet-lab`. Les conclusions portent sur les binaires identifiés ci-dessous et le réseau de laboratoire isolé.

## Défaut RX et témoin de non-régression

Dans `hw/net/virtio-net.c`, en mode moderne à en-tête de 12 octets sans négociation de `MRG_RXBUF`, le modèle reproduit les dix premiers octets et laisse inchangés les deux octets de `num_buffers`. Sur un tampon initialisé à zéro, la valeur lue est 0. Sur un tampon rempli de 0x5a, elle est 23130. La trame et sa longueur peuvent être correctes dans les deux cas. Le diff approuvé étend l’écriture du compteur existant au format moderne et conserve le traitement historique du format legacy. L'invité continue à exiger exactement 1 avec un seul tampon et à vérifier l'intégrité de chaque trame. Le test hôte exerce douze cas par version en amont et après correction, notamment les en-têtes scindés, les tampons trop courts et les chemins legacy et fusionné du modèle. NovaOS utilise uniquement le chemin sans fusion.

| Binaire de laboratoire | SHA-256 de l’exécutable utilisé |
| --- | --- |
| AArch64, QEMU 10.0.12 branche de distribution UTM | `539a6c4cefa699a730ddbabf8855aff3d77f2e79d0610fa864da3b98975c66d5` |
| x86_64, QEMU 11.1.3 | `dea251a3cbbaf2d9bb1110b5ebea82ea44ee818875f7bbe7e7419f8e542c90ef` |
| Diff commun | `64bba00421bea605521de0514a5639730fa61fee2e1de2ee55480608bec832ef` |

La construction AArch64 à partir de l’amont 10.0.12 seul échoue au lancement de la machine du banc, qui exige le périphérique TPM CRB fourni par la distribution UTM. Les archives initiales, les fichiers C avant/après, les journaux de construction et les empreintes des trois exécutables sont conservés. Le rapport de défaut et les références normatives sont disponibles pour une revue en amont. [Analyse et signalement préparatoire](virtio-net-revue-format-rx.md) · [Sources de la distribution UTM](https://github.com/utmapp/qemu/releases/tag/v10.0.12-utm)

## Confinement sur les binaires corrigés

- Q35. Le banc compare le firmware standard et la variante expérimentale, contrôle Secure Boot, la révocation dbx, le refus de l’ELF altéré, le cas sans VT-d et le refus sans EVENT_IDX. Sur le parcours Ethernet, le SID 56 est traduit dans le domaine NIC 3 avant activation du Bus Master. L’inspection des tables de la machine couvre les 22 pages actives du bail et leurs permissions. L'interruption provient du NIC sur GSI 23. Après drainage et invalidation, les 27 pages libérables sont restituées. Les trois pages permanentes appartiennent à la racine VT-d vide, à la file d’invalidation et à son témoin de complétion.
- AArch64. Le démarrage `boot-bypass=false` et l’endpoint encore détaché couvrent la phase où le firmware active le Bus Master. Les traductions réseau (SID 56, domaine 3) et disque (SID 48, domaine 1) sont distinctes. Les deux profils observent 22 pages réseau au pic dans le domaine 3 et 217 traductions de ce domaine dans la campagne finale. La ligne 38 du GIC est déclenchée par le NIC. Après drainage, le disque demeure lisible, le Bus Master du NIC est désarmé et le compteur DMA revient à zéro.
- Les deux invités négocient EVENT_IDX et conservent la répartition des accès aux descripteurs, à l’anneau disponible, à l’anneau utilisé et aux buffers. Le pair utilise une paire de sockets locale privée, sans pont, TAP ni réseau hôte externe. Le pilote utilisateur isolé reçoit une seule racine IPC. Sa terminaison nominale restitue cette racine et les trois pages IPC temporaires.

## Mesures sur les trois exécutions finales

Chaque exécution a transféré 16 trames de 60 ou 1 514 octets dans **chaque sens**, soit **12 592 octets Ethernet par sens**. Les motifs sont distincts selon le sens et la séquence. Le pair et l’invité vérifient les octets. Les 16 identifiants de complétion RX et TX sont uniques. La dix-septième soumission TX est refusée avec le statut natif 7 pendant que les 16 entrées sont occupées.

| Profil | IRQ | Rafale utile par sens | Notification → dernière collecte invitée | Cadres DMA au pic → après arrêt |
| --- | --- | ---: | ---: | ---: |
| Q35 x86_64 | GSI 23 | 1,487 Mbit/s | 67,752 ms | 30 → 3 |
| virt AArch64, P9-512 | INTID 38 | 1,145 Mbit/s | 87,989 ms | 38 → 0 |
| virt AArch64, P9-4096 | INTID 38 | 1,073 Mbit/s | 93,924 ms | 38 → 0 |

La durée est mesurée sur l’horloge de l’invité et inclut les réveils et la remise au pilote. Le débit divise les octets utiles par cette durée. Le protocole distinct de débit soutenu et de RTT demeure à exécuter.

La création et l’exécution du pilote ajoutent 29 cadres résidents (116 Kio) sur les trois parcours entre le point précédant sa création et le point files pleines. Le nombre de cadres, de tables de pages, de pages IPC et de processus revient à sa valeur de départ après la sortie du pilote. Les 152 Kio DMA AArch64 incluent 9 cadres de contrôleur, 7 du disque et 22 du NIC. Sur Q35, le pic DMA vaut 120 Kio. Ces totaux incluent leurs buffers. Les 92 Kio prévisionnels ne sont pas à additionner. L’écart résident entre démarrage et fin du scénario est de +3 cadres Q35, +12 cadres P9-512 (heap et cache retenu augmentent tous deux de 12) et −1 cadre P9-4096. Les métriques détaillées figurent dans `bilan.json`. L’attribution de la totalité du code résident et de toutes les ressources partagées au seul sous-système réseau demeure ouverte.

## Stockage et perte de réponse

Sur AArch64, le pair attend le signal de fin des mesures de stockage avant sa dernière réponse. L’invité effectue 64 lectures de 4 Kio et 16 FLUSH pendant que l’échange Ethernet reste en vol. L’empreinte du volume synthétique relu est contrôlée.

| Profil | Lecture | FLUSH p50 | FLUSH p95 | FLUSH p99 |
| --- | ---: | ---: | ---: | ---: |
| P9-512 | 47,81 Mio/s | 75,000 µs | 407,008 µs | 407,008 µs |
| P9-4096 | 50,96 Mio/s | 75,496 µs | 164,000 µs | 164,000 µs |

Avec 16 FLUSH, p95 et p99 correspondent au maximum observé. La réserve historique P9-4096 (p95 de 146 à 174 µs) reste ouverte. Le BUSY (24) reproduit avec l’ancien gestionnaire reste un défaut distinct. Les mesures d’écriture en concurrence et le parcours stockage simultané sur Q35 restent à qualifier.

Le témoin négatif conserve le même ELF que la rafale nominale correspondante et supprime volontairement une réponse du pair. Sur chaque architecture, le pair reçoit 16 trames et en renvoie 15. Le processus finit avec le code 8, les racines reviennent à zéro, le Bus Master est désarmé et les ressources DMA incertaines restent en quarantaine. Les journaux conservent le refus de la recette nominale. Une interruption durable de la publication d’une complétion matérielle après DMA reste un autre cas à qualifier.

Les premiers essais via le hub et la socket de faible capacité ont montré une séquence manquante sur AArch64. Le témoin final utilise une liaison directe et des buffers de socket bornés. Les deux changements ont été essayés ensemble. Sur Q35, la fenêtre MMIO temporaire initialement choisie ne satisfaisait pas le contrat de restitution de la route. Le parcours corrigé réutilise le créneau validé libre avant ouverture. Les essais intermédiaires et leurs échecs restent archivés.

## Compatibilité et travaux ouverts

Les neuf contrôles du registre ABI ont réussi. La comparaison des noyaux ordinaires avec les deux ELF de référence `positions-ordinaire` constate, sur chaque architecture, la même entrée et quatre segments PT_LOAD identiques (octets, adresses, tailles, permissions et alignement). Les empreintes des ELF entiers diffèrent. Les comparaisons et empreintes sont publiées dans `noyaux-ordinaires.json`.

Le lot reste ouvert pour le bouclage 16 bits et l’endurance de 100 000 trames réelles par sens, la saturation RX au-delà des 16 buffers, la progression après réarmements répétés, le redémarrage du pilote dans le même invité, la panne avec complétion matérielle indéterminée, les écritures de stockage en concurrence et le protocole soutenu de débit et de RTT. La qualification est spécifique aux machines QEMU isolées et aux exécutables identifiés. La variante OVMF et le correctif du modèle sont privés au laboratoire.

## Preuves

Le manifeste extérieur couvre le bilan, la comparaison ELF, les deux inventaires et l’archive de traces. `CONTENU-SHA256` vérifie chaque fichier après extraction. `ARTEFACTS-LOCAUX-SHA256` couvre les sources et binaires volumineux conservés dans `OS-worktrees/archives-virtio-net/rx-corrige-20261125`. Les images disques et magasins de clés du banc restent locaux. Dans le dossier de preuves, exécuter `shasum -a 256 -c SHA256SUMS`. [Manifeste](validation/virtio-net/rx-corrige-20261125/SHA256SUMS) · [Mesures et statuts](validation/virtio-net/rx-corrige-20261125/bilan.json) · [Objectifs de qualification initiaux](virtio-net-qemu.md)
