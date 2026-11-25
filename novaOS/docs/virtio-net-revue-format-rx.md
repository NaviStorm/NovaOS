# VirtIO-net — revue du champ RX `num_buffers`

**Décision soumise à la MOA/MOE, 25 novembre 2026.** Conserver le format Ethernet moderne de NovaOS et sa vérification de `num_buffers`. Le banc reproduit une écriture manquante dans le chemin RX logiciel de QEMU v10.0.12 et v11.1.3. Le diff soumis se limite à la branche de réception. Son application, la compilation d’un QEMU modifié et son utilisation restent soumises à décision. Le bail de données est fermé sur les deux invités. [Diff proposé](validation/virtio-net/ethernet-20261125/proposition-num-buffers.diff) · [Mesures et négociations invitées](virtio-net-event-idx-20261125.md)

## Contrat applicable à la trame

Les invités relisent la même sélection `0x320000020` après `FEATURES_OK`. Les bits 5, 29, 32 et 33 désignent respectivement MAC, EVENT_IDX, VERSION_1 et ACCESS_PLATFORM. Le bit 15 MRG_RXBUF est offert dans `0x10330bf8024` et reste à zéro dans la sélection. Les extensions de hachage, de tunnel et de segmentation restent désactivées. Les deux fonctions de configuration QEMU choisissent donc un en-tête invité de 12 octets. EVENT_IDX régit la notification des anneaux. Il ne change pas le format des paquets. [Négociations relues](validation/virtio-net/ethernet-20261125/campagnes.json) · [Configuration v10.0.12](https://github.com/qemu/qemu/blob/v10.0.12/hw/net/virtio-net.c#L638-L656) · [Configuration v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/net/virtio-net.c#L670-L687)

| Offset | Taille | Champ moderne du profil |
|---:|---:|---|
| 0 | 1 | flags |
| 1 | 1 | gso_type |
| 2 | 2 | hdr_len |
| 4 | 2 | gso_size |
| 6 | 2 | csum_start |
| 8 | 2 | csum_offset |
| 10 | 2 | num_buffers, entier little-endian |
| 12 | variable | Début de la trame Ethernet |

VirtIO 1.3 §5.1.6 définit la structure moderne. Les §5.1.6.3.2 et §5.1.6.4.1 imposent au périphérique un seul tampon et la valeur 1 quand MRG_RXBUF est absent. La variante legacy du §5.1.6.1 omet le champ dans ce cas et place la trame à l’offset 10. Elle ne s’applique pas à la négociation VERSION_1 ci-dessus. En TX, la règle est différente et la valeur fournie par le pilote est zéro (§5.1.6.2.1). Le contrôle RX existant de NovaOS vérifie `[1, 0]` aux offsets 10–11 et les bornes de `used.len`. Remplir le champ côté pilote avant réception rendrait impossible d’attribuer la valeur observée au NIC. [Spécification officielle OASIS VirtIO 1.3, §5.1.6](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html) · [Décodeur et encodeur NovaOS](../user/virtio_net/mod.rs)

## Reproduction indépendante du bail

Sur Darwin ARM64, le banc extrait et compile sans changement `receive_header`, `virtio_net_receive_rcu` et les définitions des structures des deux tags après vérification de leur SHA-256. La file, le filtre d’acceptation et les accesseurs mémoire sont doublés pour isoler les écritures de trames de 60 et 1 514 octets. L’oracle vérifie chaque octet des trames et la longueur de complétion. La file et son appel de notification sont simulés. Les observations de négociation PCI dans l’invité sont recueillies séparément. [Banc reproductible](../tests/virtio_net_rx_modele.py) · [Traces des copies](validation/virtio-net/ethernet-20261125/rx-hote.json) · [Empreintes amont](validation/virtio-net/ethernet-20261125/sources-qemu.json)

| Essai hôte | QEMU v10.0.12 | QEMU v11.1.3 | Vérification |
|---|---|---|---|
| Moderne sans fusion, tampon initialement nul | champ 0, longueur 72 | champ 0, longueur 72 | Échec de conformité sur les deux sources |
| Même essai, tampon initialement rempli de 0x5A | champ 0x5A5A | champ 0x5A5A | Les deux octets gardent leur valeur initiale |
| Moderne avec fusion, un tampon suffisant | champ 1, longueur 72 | champ 1, longueur 72 | Témoin positif, écriture à l’offset 10 |
| Moderne, trame maximale du profil (1 514 octets) | sans fusion 0, avec fusion 1 | sans fusion 0, avec fusion 1 | Longueur 1 526 et contenu exact dans les quatre cas |
| Legacy sans fusion | longueur 70, contenu exact | longueur 70, contenu exact | La trame commence à l’offset 10 |

Dans le premier cas, `receive_header` copie dix octets. L’affectation `cpu_to_le16(1)` touche `extra_hdr` local. La trame est copiée à l’offset 12. La copie terminale de deux octets vers le descripteur dépend de `mhdr_cnt`, qui reste nul. Le banc de conformité sort avec le code 2 sur les sources intactes. La lecture des exécutables installés retrouve les tailles de copie dans leur code hôte ARM64. Aucune réception DMA depuis NovaOS n’a été exécutée. [Source RX v10.0.12](https://github.com/qemu/qemu/blob/v10.0.12/hw/net/virtio-net.c#L1908-L2058) · [Source RX v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/net/virtio-net.c#L1919-L2067) · [Extraits binaires](validation/virtio-net/ethernet-20261125/copies-rx-binaires.json)

## Comparaison des solutions

| Approche | Compatibilité et coût | Décision proposée |
|---|---|---|
| Adapter NovaOS | NovaOS suit déjà le format VERSION_1. Accepter 0, ignorer le champ ou le préremplir à 1 masquerait la copie manquante. Passer à un en-tête de dix octets décalerait la trame de deux octets. | Conserver le décodeur, l’encodeur et le profil moderne. Un périphérique conforme peut utiliser ce format sans modification. |
| Négocier MRG_RXBUF, bit 15 standard | Le témoin hôte produit 1 à 60 et 1 514 octets. Les 1 526 octets de l’en-tête et de la plus grande trame du profil tiennent dans un slot de 2 048 octets. La spécification oblige à remplir intégralement chaque tampon sauf le dernier lors d’une réception fusionnée. Une garantie attestée de taille maximale à l’entrée pourrait ainsi borner le cas nominal à un seul slot. Pour des entrées dépassant cette borne, la fonctionnalité permet plusieurs entrées RX. Les anneaux de 16, les limites de trame et de copies, l’identification de chaque complétion et le drainage d’une réception partielle exigeraient une revue. Le succès sur un tampon ne qualifie pas ces obligations. | Option d’interopérabilité à arbitrer séparément si ce mode devient un besoin du pilote. Aucune négociation ajoutée dans NovaOS. |
| Corriger le chemin logiciel QEMU | En mode moderne, la destination du compteur existe déjà dans la même plage RX inscriptible. Utiliser la copie terminale existante conserve le format legacy et le comportement fusionné. Aucun changement de permission ni de binaire NovaOS proposé. | Favoriser une version amont standard dont cette propriété aura été vérifiée. Pour débloquer le laboratoire, examiner le diff ci-joint. Un QEMU local corrigé resterait facultatif et limité à la qualification. |

## Correctif soumis et vérifications avant emploi

Dans `virtio_net_receive_rcu`, la proposition collecte la destination des deux octets si le format moderne est actif **ou** si le format legacy a négocié la fusion. Elle supprime l’affectation locale sans publication. Le code terminal existant encode le nombre réel `i` et l’écrit dans la destination collectée avant `virtqueue_fill` et la notification. La branche existante de non-fusion exige déjà que la trame tienne dans la première entrée et abandonne une réception incomplète. En legacy sans fusion, le paquet continue à commencer à l’offset 10. Les deux contextes du diff ont été vérifiés à blanc sur chaque tag avec une tolérance de contexte nulle. Aucun exécutable corrigé n’a été construit ou utilisé. [Texte intégral du correctif](validation/virtio-net/ethernet-20261125/proposition-num-buffers.diff)

Si son utilisation est approuvée, le premier contrôle portera sur les deux sources corrigées avec tampons initiaux distincts, les tailles limites, le vecteur scindé à l’offset 10, les modes legacy avec et sans fusion, les réceptions fusionnées à plusieurs entrées et les complétions refusées ou partielles. Avant emploi dans un invité, identifier source, correctif, outils, configuration et SHA du nouvel exécutable. Requalifier sur chaque binaire la protection depuis le reset, la continuité firmware/noyau, les domaines disque et NIC, chaque permission d’anneau et de tampon, les invalidations et la restitution. Sur Q35, inclure le témoin Secure Boot de l’image altérée. Une fois ces préconditions et les oracles RX satisfaits, le mandat principal prévoit une IRQ produite par le NIC et des trames comparées au pair isolé dans les deux sens, puis le trafic simultané avec P9. L’étude actuelle laisse ouvertes les qualifications matérielles et de production.
