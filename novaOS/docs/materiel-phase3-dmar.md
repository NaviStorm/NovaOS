# Phase 3 — Prise de contrôle du profil PCI Q35 et de la DMAR

Le BSP reprend la carte EDU sur un profil QEMU exclusivement sélectionné au démarrage. L'inventaire PCI de phase 2, la politique ACPI de phase 3 et l'initialisation des registres VT-d sont trois contrôles successifs. Le backend VT-d reçoit un ticket privé uniquement après validation des deux premiers. [Découverte](../kernel/src/hardware/discovery/vtd.rs) · [Ressources PCI du profil](materiel-phase2-pci-dma.md)

## Ce que décrit le firmware

Le lecteur ACPI vérifie la provenance des pages, les sommes de contrôle, les tailles, les chevauchements et l'unicité de DMAR. La reconnaissance du laboratoire s'applique à un cliché de la table déjà vérifié. Elle exige un seul segment zéro, une fenêtre MCFG de bus 0 à 255 en `0xe0000000`, une DMAR de révision 1 et une seule unité DRHD de 4 Kio en `0xfed90000`. La largeur physique annoncée dans la DMAR est 48 bits. Le profil refuse les portées ambiguës ou étrangères, les structures supplémentaires, les réservations RMRR, les bits de commande inconnus et les fenêtres de registres qui recouvrent la carte mémoire Limine. Le noyau doit encore vérifier les capacités du registre matériel avant de programmer une racine. [Politique de reconnaissance](../kernel/src/hardware/acpi/q35_dmar.rs) · [Intel VT-d, chapitres 8 et 10](https://cdrdv2-public.intel.com/774206/vt-directed-io-spec%20.pdf)

Dans une VM QEMU 11.1.3 Q35 fraîche, avec `aw-bits=48,intremap=off`, le fichier de firmware `etc/acpi/tables` contient une DMAR de 112 octets. Son DRHD de 64 octets contient une portée IOAPIC et cinq portées PCI explicites sur le bus racine. Le générateur QEMU donne à EDU le SID `0x0030`. Le lecteur vérifie exactement une occurrence de chacun des cinq endpoints, dans tout ordre. Il reconnaît aussi la forme à portée de segment globale (`INCLUDE_PCI_ALL`) à condition que l'IOAPIC soit seul dans les portées, que le segment n'ait qu'un DRHD et que le recensement du bus reste identique. Cette seconde représentation relève des tests synthétiques. [Options Intel IOMMU de QEMU](https://www.qemu.org/docs/master/system/invocation.html)

## Maîtrise des émetteurs après ExitBootServices

Le diagnostic x86 utilise CF8/CFC sur le bus racine fermé, avant le lancement des AP. Cela permet d'identifier la carte et de couper ses droits même avec une DMAR absente ou invalide. Il requiert les cinq fonctions du Q35 de référence, sans pont PCI-PCI supplémentaire. Aucune donnée fournie par le pilote ne détermine l'adresse PCI ou l'adresse de la page de l'IOMMU.

| Fonction | Vérification avant admission |
|---|---|
| EDU `00:06.0` | Ticket de phase 2 sur la BAR0 connue. PCI_COMMAND est relu avec MSE seul parmi les trois bits de décodage. L'identifiant, la cause IRQ, l'état du moteur et les registres source, destination, longueur et commande DMA doivent être ceux d'une carte neuve. Une carte en cours d'opération est refusée. |
| AHCI Q35 `00:1f.2` | Vérification de la BAR5 du profil, puis des commandes actives et des tags NCQ sur chacun des six ports du modèle. Le BSP arrête le moteur de commandes puis le moteur de réception FIS, contrôle séparément CR/FR et revérifie CI/SACT. L'échéance est bornée. Il retire son mapping superviseur, puis ferme BME. Cette procédure n'émet ni commande au disque ni réinitialisation de contrôleur. |
| Pont hôte, LPC, SMBus | La topologie fermée identifie les fonctions du chipset émulé. Le BSP ferme BME avec des écritures PCI de 16 bits qui préservent PCI_STATUS, puis relit identité et commande. Ces trois fonctions n'ont pas de moteur autonome de DMA PCI utilisable dans cette configuration du banc. Cette règle ne s'applique pas à un chipset physique ou à un bus secondaire. |

Le ticket de la carte EDU est créé avant la première écriture PCI. Il conserve le profil autorisé et l'état hérité. Le médiateur vérifie l'identité, la BAR et les bits de commande à chaque ouverture ou fermeture BME. Il exige séparément la preuve de fin du moteur et la confirmation VT-d avant le réemploi d'un cadre. Les fenêtres de reprise temporaires sont UC et superviseur. Le profil conserve les pages du chargeur pendant toute la VM. En cas d'incertitude sur l'activité héritée, le profil refuse de poursuivre. Ces contrôles commencent après la remise du firmware. [Reprise de la carte](../kernel/src/hardware/lab_irq/pci.rs) · [Arrêt des moteurs AHCI, §10.1.2](https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/serial-ata-ahci-spec-rev1-3-1.pdf)

## Témoins de qualification

Les tests synthétiques passent par le lecteur ACPI complet. Ils couvrent DMAR absente, DRHD tronqué, deux unités, base inattendue, largeur incompatible, taille de fenêtre, portées absentes, dupliquées ou contradictoires, segment non admis et ajout d'un RMRR. La table peut rester lisible pour l'inventaire générique tout en n'accordant aucun ticket. Les deux encodages de portée Q35 sont contrôlés. [Tests du lecteur](../kernel/src/hardware/acpi/tests.rs)

Le banc négatif démarre un noyau de phase 3 avec une DMAR tronquée, une seconde DMAR ajoutée à une VM avec IOMMU, une deuxième carte EDU non autorisée et une carte précédemment programmée. Dans ce quatrième cas, QTest prépare le registre de longueur de l'EDU à 1 octet avant le démarrage du firmware. Le banc vérifie que BME et RUN sont à zéro et que la longueur a été enregistrée. Aucun transfert DMA n'est lancé. L'oracle combine journal invité, inventaire QMP et relecture indépendante du mot PCI_COMMAND de la première carte. Dans le cas de la seconde carte, le refus intervient avant la prise de possession de la topologie non reconnue. La valeur laissée par son firmware peut encore contenir BME. Cette VM ne remet aucun domaine au pilote. [Banc négatif](../tests/vtd_dmar.py)

| VM Q35 x86_64 TCG, 1 vCPU | Observation invitée | Relecture QMP après décision |
|---|---|---|
| DMAR unique, DRHD de longueur 4 | Erreur `Length`, demande Ring 3 refusée, ticket fermé | EDU `PCI_COMMAND=0`, BME des quatre pairs à zéro |
| Deux DMAR et un remappeur présent | Erreur `Format`, demande Ring 3 refusée, ticket fermé | EDU `PCI_COMMAND=0`, BME des quatre pairs à zéro, RTADDR nul et TE inactif |
| Deux cartes EDU et un remappeur présent | Topologie inattendue, arrêt sur le BSP avant le handoff | La carte originale conserve la commande firmware `0x0007`, RTADDR nul et TE inactif |
| EDU avec longueur DMA préprogrammée et RUN inactif | Reprise refusée après interdiction du bus mastering, avant admission VT-d et remise des pages du chargeur | Carte en lecture de registres superviseur avec MSE seul (`0x0002`), RTADDR nul et TE inactif |

Les quatre essais ont réussi dans QEMU 11.1.3. La série, le relevé QMP, les empreintes des ELF et des images et les octets des tables injectées sont conservés. La valeur `0x0007` du témoin à deux cartes EDU délimite précisément la garantie du système sur un matériel qu'il n'a pas pris en charge. [Preuves des quatre refus](validation/materiel-phase3-dmar.json)

Depuis `novaOS/`, avec le noyau x86_64 construit avec `vtd-lab`, fournir son chemin et un répertoire de preuves neuf.

```sh
python3 -B tests/vtd_dmar.py --elf build/vtd-phase3/cargo/x86_64-unknown-none/release/novaos \
    --preuves build/essai-vtd-dmar
```
