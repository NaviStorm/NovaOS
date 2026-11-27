# NovaOS v0.25.0 — Phase 3E — cycle de vie matériel

Sur AArch64, le port restitue les **22 cadres NIC (88 Kio)** après une fermeture avec un seul descripteur RX consommé sur seize. Sur Q35, la MOA a demandé de conserver la garde de réadmission. Le test en invité confirme le refus du nouveau NIC après restitution complète des 26 cadres de l’ancienne instance, pendant que le même bail NVMe poursuit sa transaction.

Référence de livraison 3D `4c3bd8896c496636118a7f4751b6947fba72d1e3`. Noyau expérimental et sources de la sonde Q35 qualifiés en 3E `fd3ff113fcaf39bee804eebdcedc44557f41817d`. Les résultats portent sur les profils QEMU du laboratoire. [Phase 3D](phase3d.md) · [Critères de qualification et contrats des instances](contrats-phase3e.md)

## Preuve de restitution sur ARM

VirtIO 1.3 § 2.6.1 permet au pilote de restituer les ressources d’une virtqueue après vérification de son reset. Le périphérique doit alors cesser de traiter les demandes de cette file et d’émettre ses notifications. En PCI, sections 4.1.4.3.1–2, la négociation du bit 40 autorise l’écriture de 1 dans `queue_reset`. Le périphérique présente 1 pendant le reset et 0 dans `queue_reset` et `queue_enable` à sa fin. Le reset global du périphérique (§ 2.4) et le retrait des traductions (§ 5.13.6.6) ont chacun leurs propres effets. [Spécification OASIS VirtIO 1.3](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)

Le courtier vérifie que la capacité commune du NIC en BAR 4 couvre les deux octets de `queue_reset` à l’offset 58. Sur offre compatible, il négocie et relit le masque `0x10320000020`. La fermeture de la racine de soumission du pilote précède le masquage d’INTx. Le courtier vérifie la sélection et l’activation initiale de RX 0 et TX 1, émet les deux commandes et relit les registres de chaque file. Une valeur hors domaine invalide la tentative dès sa première lecture. Le reçu privé associe ces confirmations au PID, à l’incarnation et à la génération matérielle de l’acteur réservé par le superviseur. [Implémentation des resets](../../kernel/src/hardware/block/net_arm/reset.rs)

La restitution intégrale exige également le contrôle existant du reset global, de BME et des entrées utilisées, la purge de l’IRQ, les reçus de retrait des traductions et le détachement du NIC. L’oracle reconstruit depuis la trace les IOVA, les permissions et les rattachements des deux périphériques. Lors d’un décès, d’une révocation ou d’une réponse de publication incertaine, le client vérifie quatre tickets incertains et quatre annulés, puis le refus de leur acquittement. Les resets ne fournissent aucun reçu de transmission. [Politique de restitution](../../kernel/src/hardware/block/net_arm.rs) · [Vérifications du client](../../user/ethernet/client.rs)

## Matrice des résultats

| Cas | Observation | Cadres à la fin du profil |
|---|---|---|
| ARM, fermeture RX partielle et réouverture | Deux générations successives. Un RX rendu sur seize à chaque génération, une trame exacte par sens et refus de l’ancienne capacité. | 0 NIC, 0 contrôleur |
| ARM, décès, révocation, publication incertaine | Trois démarrages. Quatre trames au pair par sens dans chaque cas. Le pilote se termine avec le code prévu, les deux autres acteurs avec le code 0. | 0 NIC, 0 contrôleur |
| ARM, acquittement différé | Le reçu logiciel différé entraîne une relecture. Dans le modèle matériel de faute, TX présente 1 pendant 32 lectures MMIO avant les deux zéros de fin. | 0 NIC, 0 contrôleur |
| ARM, commande TX non transmise, ancienne génération ou option absente | Les trois variantes rendent les huit pages TX en vertu de la propriété des buffers. Sans offre du bit 40, la trace compte zéro accès au champ `queue_reset`. | 14 NIC + 9 contrôleur |
| ARM, acquittement MMIO absent ou invalide | Le modèle exécute le vrai reset puis présente durablement 1, ou présente 2. L’expiration de l’attente et la première valeur invalide aboutissent chacune à la quarantaine. | 14 NIC + 9 contrôleur |
| ARM, reçu IOMMU différé ou non livré | Les deux resets sont acquittés. Une ancienne génération et un numéro erroné sont refusés. La livraison du reçu exact permet la restitution et la réouverture. | 0 si collecté, sinon 22 NIC + 9 contrôleur |
| ARM, repli et profil court historique | Le disque invité reste lisible. Le choix explicite de DATA et l’épuisement des huit boîtes exercent le repli négocié. | DATA finit avec 0 cadre DMA. Le profil court historique conserve 14 NIC + 9 contrôleur. |
| Q35, réadmission pendant le bail NVMe | Après la collecte du reçu de l’ancien NIC, la génération 2 est refusée pendant un READ dont la CQE est ensuite collectée. | 0 NIC. Après fermeture du NVMe, 8 ancres communes. |

Le modèle ARM de faute modifie uniquement la lecture de l’acquittement TX après le vrai reset. Il utilise les mêmes sources QEMU UTM et la même correction RX que le témoin. Les lectures et écritures du BAR sont conservées pour le reset nominal, l’absence du bit 40, le retard et la valeur invalide. Pour le retour durablement occupé, la preuve réunit le marqueur du modèle, l’expiration dans l’invité et les mappings encore présents. [Delta du modèle](../../tests/ethernet_v1/modele_reset_arm.patch) · [Construction isolée](../../tests/ethernet_v1/modele_reset_arm.py)

## Ressources et continuité NovaFS

Sur le cycle ARM court, les trois acteurs ont 38 cadres DMA épinglés à l’ouverture, dont 22 pour le NIC, et 9 à 10 pages IPC lors du relevé. Après collecte de chaque groupe, le banc retrouve 61 837 cadres libres, zéro page IPC et zéro processus utilisateur. Le retrait complet du NIC laisse 16 cadres pour le stockage et son contrôleur. À la fermeture du disque, le profil nominal retrouve 61 875 cadres libres et zéro cadre DMA. Une preuve de reset insuffisante retient 56 Kio pour le NIC et 36 Kio pour le contrôleur partagé. Un reçu IOMMU non livré retient 88 Kio pour le NIC. Le registre des 48 sélecteurs, les quatre racines par acteur, les huit crédits par session et le plafond global de huit boîtes de 4 Kio sont conservés.

Q35 conserve le PID 3 du disque, la génération VT-d 1 et l’époque IPC 3. La tentative du candidat PID 10 maintient les 14 cadres attendus, huit ancres communes et six cadres NVMe. Le READ de CID 10 est soumis avant la tentative et sa CQE est collectée après le refus. La trace ne situe pas l’instant de fin de lecture interne au modèle. Les cinq WAIT de la QI et les fronts BME attestent une seule attribution matérielle de chaque périphérique.

La trace Q35 contient 22 READ, 7 WRITE et 3 FLUSH terminés, dont 31 commandes après le WAIT du NIC. Le lecteur indépendant remonte le namespace jetable, retrouve les ancres 3 et 4 et vérifie les 4 231 octets de `/sortie` ainsi que la page de garde. Les barrières F1 et F2 correspondent aux CID 32 et 34. NovaFS acquitte après F2. Le profil termine avec 24 566 cadres libres, zéro page IPC et zéro processus utilisateur. Sur ARM, le contenu du volume témoin reste identique. La trace IOMMU atteste les accès du disque entre les UNMAP du NIC pendant la lecture et le FLUSH de réconciliation.

La première sonde comparait l’époque IPC 3 à la génération matérielle 1. Son oracle rejetait la transaction malgré la présence de sa CQE. La sonde corrigée conserve les deux identités séparément. Les contrôles de réadmission du domaine Q35 gardent exactement les sources de phase 3D. La sonde appelle ces contrôles depuis le banc. [Sonde Q35](../../tests/ethernet_v1/sonde_garde_q35.patch) · [Oracle de continuité](../../tests/ethernet_v1/port_q35_stockage.py)

## Comparaison Ethernet ciblée

Médianes de trois démarrages par variante, avec ordre alterné. Chaque démarrage fournit 1 024 lots de huit trames de 1 514 octets. Le bilan conserve également les cellules de un et quatre des mêmes exécutions.

| Invité | TX 3D → 3E (Mbit/s) | p99 du lot 3D → 3E (ms) | IPC par trame 3D → 3E |
|---|---:|---:|---:|
| AArch64 | 27,59 → 25,80 (−6,47 %) | 3,846 → 4,036 (+4,94 %) | 14,933 → 14,936 |
| Q35 | 35,68 → 36,32 (+1,78 %) | 3,219 → 3,038 (−5,62 %) | 14,963 → 14,951 |

La baisse ARM se retrouve dans les trois paires. Une contre-épreuve de trois paires supplémentaires conserve le même ELF ARM 3E et change uniquement l’offre du bit 40. Avec le bit offert, le débit médian vaut 26,98 Mbit/s et le p99 3,816 ms. Avec le bit masqué, ils valent 27,06 Mbit/s et 3,912 ms. L’écart de débit entre les deux offres tombe à −0,31 %, avec deux paires sur trois où l’offre du bit est plus rapide. La négociation seule ne reproduit pas la baisse entre les images 3D et 3E. La cause précise de cet écart reste ouverte.

Le débit compte les bits utiles des trames TX, avec écho au pair. Le p99 mesure la latence aller-retour du lot. Les variantes partagent firmware, modèle, topologie et instrumentation au sein de chaque architecture. ARM 3E négocie le reset individuel. Q35 utilise le profil Ethernet simple pour cette mesure. Trois démarrages par variante bornent l’interprétation. La réserve de débit et de p99 Q35 ainsi que la réserve NovaFS sous `BUSY (24)` restent ouvertes.

## Reproduction et portée

Les 37 démarrages retenus comprennent 13 cas ARM de cycle de vie et de transport, cinq observations de registres, le refus Q35, les douze démarrages de comparaison avec 3D et les six démarrages complémentaires à ELF ARM identique. Les invités utilisent un vCPU sous TCG, 256 Mio, QEMU 10.0.12 Cortex-A72 avec GICv3 sans ITS et QEMU 11.1.3 Q35 avec OVMF expérimental. Les tests hôtes ciblés donnent 20 réussites sur Darwin ARM64 natif et 20 sur Darwin x86_64 sous Rosetta. Les neuf tests du registre ABI et la vérification de sa génération réussissent. Le manifeste interne NovaOS reste à v0.20.0. [Mesures et matrice par démarrage](../validation/ethernet-v1-phase3e/bilan.json) · [Inventaire des fichiers bruts](../validation/ethernet-v1-phase3e/inventaire.json) · [Journaux, captures courtes et traces MMIO](../validation/ethernet-v1-phase3e/preuves.tar.gz) · [Empreintes SHA-256](../validation/ethernet-v1-phase3e/SHA256SUMS) L’inventaire contient 508 fichiers et le paquet compact 244 pièces.

Les contrats préparatoires couvrent les groupes isolables, la propriété du contrôleur, la conservation des reçus, les retraits concurrents, l’épuisement des générations, la reprise après perte du superviseur et les quarantaines persistantes. Le laboratoire réconcilie ses reçus en mémoire pendant la vie du superviseur. La reprise physique et la réadmission positive au milieu du même bail NVMe restent ouvertes. [Contrats du futur gestionnaire](contrats-phase3e.md)

Depuis le worktree Ethernet, choisir une nouvelle destination. Les constructions et démarrages acquièrent le verrou partagé. La dernière commande reconstruit le bilan et le paquet vérifiable depuis les sorties brutes.

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
export PATH="$NOVAOS_TMPDIR/ovmf-net-outils-20261125/venv12/bin:$PATH"
banc=novaOS/tests/ethernet_v1
sortie=../archives-ethernet-phase3e-rejeu
for lot in reference modele images arm garde mmio ethernet option-arm hote
do
  python3 -B "$banc/rejouer_phase3e.py" "$sortie" "$lot" || exit 1
done
python3 -B "$banc/bilan_phase3e.py" "$sortie" "$sortie/bilan"
```
