# Propriété des cadres DMA et révocation avant livraison

Les corrections DMA de la branche `codex/es-groupees-0166` sont présentes dans le noyau brut et dans la composition multipage. Le candidat qualifié est `13df1dce48677f6fd92256b99b0ab0948bde803a`. Le matériel DMA du pilote ne contient que des cadres superviseur. Sur le profil qualifié, le BSP traite les interruptions, exécute les admissions et peut réordonner les threads. [Défauts de l’audit initial](es-v0.16.6-integration-dma.md) · [Arène SG](../tests/fixtures/io-0166-virtio-multipage.patch)

## Ouverture et retrait

L’ancien constructeur allouait les pages avant `Domain::start`. Une erreur remontée par `?` pouvait laisser les pages épinglées sans référence. Une fermeture consommait le transport avant de savoir si son retrait réussirait. Les deux défauts concernaient la base à une page et la variante multipage.

Une seule réservation matérielle peut désormais exister. Elle couvre la construction, le service actif et le retrait. Le constructeur enregistre le domaine avant sa première publication, puis chaque slot avant de soumettre sa traduction. L’arène SG enregistre le cadre avant MAP, y compris pour une extension partielle. Une sortie précoce du propriétaire local emprunte le même chemin que la fermeture explicite. [Propriétaire et étapes de retrait](../kernel/src/hardware/block/ownership.rs)

| État | Condition avant progression | En cas d’erreur |
|---|---|---|
| Allocation privée | La réservation est unique. Le lot n’a publié aucune adresse | Le constructeur rend tout préfixe privé, y compris les tables VT-d encore privées |
| Chaîne soumise | Le disque a rendu la chaîne attendue | Les pages et la file restent possédées. Le reset du disque attend la restitution de la chaîne |
| Arrêt | Reset du disque acquitté et bus mastering relu désactivé | Le moteur garde le périphérique et le lot |
| Retrait IRQ | Route masquée, purge acquittée | Les adresses du gestionnaire restent valides jusqu’à la purge |
| Retrait des traductions | Chaque slot retiré est enregistré. Les deux banques SG gardent leur curseur de progression | La reprise repart du premier slot restant. Un échec après MAP retient aussi le cadre concerné |
| Fin du domaine | Sur ARM, détachement du SID, reset VirtIO-IOMMU et bus mastering désactivé. Sur x86, invalidations avec drainage et SID rendu absent | Une faute incertaine conserve l’inventaire en quarantaine |
| Restitution | La dernière barrière précède la restitution. Les pages rendues sont effacées côté allocateur | Les neuf cadres machine x86 restent dans le domaine stationné avec traduction et remappage activés |

Une nouvelle ouverture échoue tant que le retrait précédent détient la réservation. Un nouvel appel interne à `retire` reprend l’étape interrompue. Une fermeture réussie peut être répétée. Une défaillance matérielle permanente ou une réponse incohérente peut retenir l’inventaire jusqu’au redémarrage. La borne du profil SG atteint 144 cadres de 4 Kio par machine, socle VT-d compris. Le banc de fautes ci-dessous alloue jusqu’à quatre pages supplémentaires dans une banque. [Allocateur et effacement](../kernel/src/memory/mod.rs) · [Gestion du bail](../kernel/src/hardware/block/lease.rs)

Les buffers de requête et de réponse VirtIO-IOMMU demeurent réservés lorsqu’une commande a dépassé son délai. Le collecteur traite son retour avant de préparer une autre commande. Un UNMAP tardif est associé à son slot. Un refus explicite d’UNMAP déclenche, après arrêt du disque et purge, le détachement global du SID. La réussite du DETACH est requise avant la réutilisation, même si UNMAP a répondu NOENT. Le profil ARM vérifie le mode de refus des endpoints détachés. VT-d conserve son tail et son cookie de complétion pendant l’attente. [Pilote ARM](../kernel/src/hardware/block/viommu.rs) · [Pilote x86](../kernel/src/hardware/block/vtd.rs) · [VirtIO 1.2, §5.13.6.4 et §5.13.6.6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)

## Soumission et livraison

La génération d’un bail est activée à l’ouverture réussie. La destruction du pilote ou le retrait la désactive. Une tentative de rattachement avec l’ancienne génération échoue même si le PID titulaire a déjà été effacé. La soumission des pièces historiques, des pages et des lots revalide le bail sous masque local jusqu’à la publication du descripteur.

Les réponses de 128 octets et les pages relues repassent par le contrôle du bail sous le même masque que la copie vers l’espace utilisateur. Le lot mono-page vérifie les deux extrémités IPC, le retrait et le bail matériel sous la même section BSP que la copie intégrale et le crédit de la réservation. L’ancienne séquence avançait le compteur de pages dès la complétion disque. Un refus ultérieur sur le bail ou sur l’adresse de destination laissait le compteur avancé malgré une copie refusée. La nouvelle séquence ferme la réservation après un tel refus. Une fin de transfert matériel incertaine conserve l’état de doute. Pour la chaîne SG, le noyau vérifie de nouveau le bail matériel, les deux extrémités IPC et la réservation à la livraison. Le masque couvre la copie et la confirmation. Le profil borne cette section à 64 pages, soit 256 Kio. La complétion d’un OUT déjà soumis peut modifier le disque pendant une demande de retrait. Le refus final interdit l’acquittement de la transaction et tout renvoi automatique. L’émission déjà effectuée reste susceptible d’avoir modifié le disque. Les chemins de persistance conservent les deux FLUSH. [Portails](../kernel/src/sched/process/syscall.rs) · [Lot mono-page](../tests/fixtures/io-0166-prototype.patch) · [Lot SG et contrôle de livraison](../tests/fixtures/io-0166-virtio-multipage.patch)

Dans les deux variantes, la lecture de l’entrée utilisée suit une barrière après vérification de l’index. Le chemin de polling l’exécute dans `io::wait` et la variante IRQ dans `lire_utilise`. Sur AArch64 la barrière inclut `dsb sy`. Les deux variantes conservent le ticket sur un SPI isolé sans bit de file et bornent à huit la série parasite avant masque de la ligne. [Attente et barrières](../kernel/src/hardware/block/io.rs) · [Traitement des IRQ](../kernel/src/hardware/block/mod.rs) · [Variante IRQ](../tests/fixtures/io-0166-irq-attente.patch)

## Résultats dans QEMU

Le 14 novembre 2026, les huit cellules ci-dessous ont exercé le pilote VirtIO-blk et le domaine IOMMU sous QEMU direct 11.1.3. Chaque cellule démarre trois invités sur une image raw de 1 Mio portant deux pages de motifs déterministes. Le banc hôte vérifie chaque octet de l’image après chacun des trois arrêts. Les empreintes SHA-256 des quatre ELF instrumentés et des 24 journaux série figurent avec les résultats individuels. [Résultats et empreintes](validation/es-v0.16.6-correctifs-dma.json) · [Topologie QEMU](../scripts/bloc_qemu.py)

| Invité | Secteur | Attente | Fautes d’ownership déclenchées | Cycles avec décès du titulaire | Cadres privés de session | Cadres DMA après retrait |
|---|---:|---|---:|---:|---:|---:|
| AArch64 | 512 | Polling | 120 | 64 | 16 | 0 |
| AArch64 | 4 096 | Polling | 120 | 64 | 16 | 0 |
| AArch64 | 512 | IRQ | 120 | 64 | 16 | 0 |
| AArch64 | 4 096 | IRQ | 120 | 64 | 16 | 0 |
| x86_64 | 512 | Polling | 112 | 64 | 7 | 9 |
| x86_64 | 4 096 | Polling | 112 | 64 | 7 | 9 |
| x86_64 | 512 | IRQ | 112 | 64 | 7 | 9 |
| x86_64 | 4 096 | IRQ | 112 | 64 | 7 | 9 |

Les 928 injections couvrent l’allocation de chaque position du socle froid de 16 cadres, l’installation de la file et du domaine, l’entrée en service, la collecte, le reset, la route et la purge IRQ, le retrait des traductions et le détachement. Les extensions partielles de l’arène simulent un refus d’allocation et une erreur de MAP. Le banc répète les fermetures et exerce la sortie anticipée du propriétaire Rust. Chaque scénario exige le nombre exact de cadres prévus à sa frontière et après reprise. Les neuf pages VT-d persistent une seule fois par machine, dont la traduction et le remappage d’interruptions restent activés. Le test ARM déclenche également un SPI parasite puis une vraie lecture et vérifie que la ligne reste active. [Injections dans les pilotes](../tests/dma_propriete.py) · [Assertions d’ownership](../tests/dma_propriete_invite.rs)

Chaque cellule vérifie quatre voies de retour. Pour le portail historique et PAGE, la lecture avec bail actif restitue le motif complet. Le retrait entre la fin de lecture et la copie refuse ensuite la trame de 128 octets ou maintient intacts les 4 096 octets sentinelles. Le pilote historique et le pilote PAGE utilisent chacun leur propre démarrage pour le refus.

Pour les lots mono-page et SG, deux vrais processus d’essai appellent les sélecteurs dans l’invité. Les 64 essais de livraison couvrent 16 succès et 48 refus. Chacun effectue exactement une soumission matérielle. Le témoin positif compte une page livrée sur le lot et deux sur SG. Les trois fautes par voie portent sur le retrait du bail, le retrait de la réservation pendant la commande et une destination incomplète. Sur le lot, la dernière partie de la destination dépasse la borne du tas. Sur SG, la seconde page est retirée après le DMA. Chaque refus conserve la première page et les octets encore accessibles, laisse le crédit à zéro et refuse une deuxième tentative sans soumission. Pour le lot, le lecteur essaie aussi de relivrer la page par le sélecteur de transfert puis de finaliser la réservation. Les deux appels sont refusés. [Témoins positifs et négatifs par voie](../tests/dma_livraison_invite.rs)

Ce banc cible les retours de lecture et le cycle du pilote. Il n’a pas chronométré les écritures ni simulé une perte d’alimentation physique. Dans ces essais, les injections déclenchent des points de retrait reproductibles sous le BSP unique. Les témoins positifs vérifient l’utilisation de la capacité de leur propre processus. Les témoins négatifs observent directement le tampon privé du destinataire et les compteurs après la tentative.

## Reproduction

Partir de deux exports jetables du commit indiqué. Appliquer successivement `io-0166-prototype`, `io-0166-plages`, `io-0166-virtio-multipage`, `io-0166-invite` avec `patch -p1 -F0`. Ajouter `io-0166-irq-attente` à l’export IRQ. Pour chaque export, depuis le dépôt contenant les tests, donner à `ARCHIVE` son chemin absolu.

```sh
python3 -B novaOS/tests/dma_propriete.py "$ARCHIVE" instrumenter
for arch in aarch64 x86_64; do
    make -C "$ARCHIVE/novaOS" ARCH="$arch" kernel
    python3 -B novaOS/tests/dma_propriete.py "$ARCHIVE" "$arch"
done
```

L’instrumentation compile le vrai gestionnaire de session, les vrais pilotes de domaine et l’arène. Les injections logicielles se produisent aux points indiqués du code invité. QEMU fournit des périphériques fonctionnels. Les tests distinguent les délais injectés avant l’observation d’un acquittement, les libérations réelles de l’allocateur et les vraies interruptions du GIC. Les essais matériels de panne permanente, de coupure électrique du Mac et de concurrence multiprocesseur sont hors de cette preuve. [Injection et lecture indépendante des images](../tests/dma_propriete.py) · [Assertions exécutées dans l’invité](../tests/dma_propriete_invite.rs)
