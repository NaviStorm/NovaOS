# NovaOS v0.25.0 — Protection DMA précoce de la RAM

## Verdict de la MOE

Sur Q35 TCG, l'autorité du banc établit la traduction **avant la première instruction du firmware**. OVMF charge ensuite le même PE depuis un FAT en RAM, avec Secure Boot actif ou inactif. Le PE vérifie ses objets signés et remet un reçu de l'époque. L'autorité contrôle à nouveau la racine, les permissions, le contenu des tables et le stockage. Les douze cellules et les journaux sont publiés avec leurs empreintes. La qualification de la reprise EFI au sens littéral de « zéro écriture » reste ouverte. A8 reste fermé. [Preuves](validation/diagnostic-uefi-5j/SHA256SUMS) · [Reproduction](../boot/diagnostic-5j/README.md)

## Autorité et transfert

1. Le superviseur crée un Q35 neuf avec un vCPU suspendu au reset, une unité VT-d, des modèles PCI privés et un namespace synthétique. Il contrôle la ROM, la RAM, les canaux QMP/qtest et la libération du vCPU. Le réseau est un hub privé. Aucun périphérique hôte n'est attaché. Le support chargé par le firmware passe par PIO. La source de confiance avant chargement est cette autorité externe et son inventaire de construction.
2. Le superviseur découvre le remappeur par le FlatView et les fonctions par QMP. Il lit en PIO le catalogue fw_cfg et retrouve la DMAR dans les commandes de somme de contrôle du lieur ACPI. À ce stade, c'est la table **avant liaison**, avec champ de checksum à remplir par OVMF. Il rapproche ses portées de la base de registres et des RID réellement présents. Les registres RTADDR et GSTS doivent décrire une instance vierge avant programmation. Un pont, une attribution concurrente, une unité inattendue, une RMRR ou une portée inexpliquée entraînent le refus de ce profil.
3. Dix pages de flash en lecture seule portent la racine, le contexte et deux arborescences indépendantes à quatre niveaux. Le NIC et le stockage ont chacun un DID, une feuille en lecture seule et un cadre témoin distinct dans la même flash. Le registre de propriété des cadres provient du constructeur. Aucune traduction ne vise la RAM du diagnostic, ses piles, son code, ses données, ses clés publiques ou les tables du remappeur. Cette exclusion couvre aussi les futures allocations d'OVMF dans la RAM. Le contrôleur reste propriétaire des tables. Les baux réseau et stockage restent à leurs titulaires.
4. Le banc observe RTPS, TES et la racine effective, relit les cadres et vérifie l'égalité exacte entre les baux et les feuilles. Chaque démarrage reçoit une époque aléatoire de 128 bits. Le modèle transmet son identifiant en PIO au firmware, qui le présente dans les options de l'image chargée. Le PE exige le format exact, une plage mémoire lisible de type attendu, un mode UEFI connu et un catalogue vérifié, y compris les signatures de manifestes et de paquets. Il conserve les interruptions et rend son écho en RAM. Seul le superviseur du même démarrage peut rapprocher ce reçu de ses observations antérieures. L'époque est publique. Sa copie, à elle seule, n'authentifie aucune autorité sur une autre machine et n'accorde aucun droit matériel.
5. En l'absence de preuve, l'autorité retient la VM et ses ressources. Le refus pour opération en vol conserve la commande EDU pendante avant toute programmation du remappeur. En fin d'essai, QEMU détruit l'instance. Ce cycle ne délivre aucun reçu de drainage ou de restitution individuelle.

L'interpréteur des baux accepte plusieurs remappeurs disjoints et refuse les RID ou cadres partagés, les tables inconnues, le pass-through et les permissions contradictoires. Le laboratoire actif couvre le mode VT-d legacy, le segment zéro, les endpoints directs et les pages de 4 Kio. Le constructeur place les tables selon l'inventaire de la VM de calibration et contrôle son identité sur chaque instance avant exécution. Le PE reste identique. [Spécification Intel VT-d](https://www.intel.com/content/www/us/en/content-details/774206/intel-virtualization-technology-for-directed-i-o-architecture-specification.html)

## Qualification

| Profil | Observation et décision |
|---|---|
| Sans IOMMU, puis unité inactive sans établissement autorisé | Deux refus avant la première instruction du firmware. |
| Baux actifs avec Secure Boot actif, puis inactif | Deux échos valides et distincts. Même FAT et même PE. Les deux domaines restent identiques avant et après. Le PE distingue les deux états de confiance. |
| BME et tentative DMA | BME d'EDU relu actif **avant l'établissement**. Écriture réelle de quatre octets vers le canari, faute VT-d du RID correspondant et canari intact. Les commandes PCI finales montrent BME à zéro. Cette cellule ne démontre pas un BME maintenu jusqu'à la première instruction du PE. |
| Feuille avec droits excessifs, puis feuille visant la RAM protégée | Deux racines contradictoires injectées sur des VM neuves et arrêtées. Refus après détection des droits ou de la cible physique. |
| Table sans provenance, puis titulaire inconnu | Deux refus avant chargement. L'essai de propriété masque le bail côté autorité et conserve le matériel synthétique. |
| Transaction potentiellement en vol | Registre de commande EDU relu à 3, canari intact et GSTS initial conservé. Refus sans reprise ni recyclage. |
| Paquet interne altéré, dans les deux modes | Le PE renvoie 14 et laisse l'écho nul. |
| Certificat révoqué dans dbx | OVMF retourne EFI_ACCESS_DENIED à LoadImage. Aucun transfert au PE. |
| Stockage | Compteurs QMP du namespace à zéro pour les lectures, écritures et FLUSH. Son empreinte complète est conservée. Les résultats des scénarios avec entrée couvrent aussi les appels firmware effectués par le diagnostic. |
| Domaines historiques plus riches | Rejeu ciblé indépendant du témoin historique Q35. Les dix tables, six feuilles NVMe et vingt-deux feuilles NIC correspondent aux deux reçus d'origine, avec les mêmes octets avant et après. Durée de la première capture 18,65 ms. Cette observation sur invité suspendu n'atteste pas rétroactivement son propre chargement. |
| Deux remappeurs | Douze pages de tables validées par le modèle hôte synthétique. QEMU x86 refuse deux vIOMMU. |
| Régression hôte | Par ISA, 27 tests Rust du diagnostic, 6 contrôles historiques du décodeur et 10 contrôles Python 5J réussis. Darwin ARM64 natif et x86_64 sous Rosetta. Générateur ABI vérifié, 48 sélecteurs. Les campagnes QEMU AArch64 historiques conservent leur provenance. |

L'adaptateur EDU utilise le transfert documenté par le modèle, avec une adresse interne à la VM et un tampon synthétique. [Périphérique EDU de QEMU](https://www.qemu.org/docs/master/specs/edu.html)

## Consommations et délais

| Mesure du lot archivé | Secure Boot actif | Secure Boot inactif |
|---|---:|---:|
| Découverte, acquisition et validation avant libération, horloge hôte | 778,13 ms | 810,52 ms |
| Dont programmation et contrôle de la traduction, lecture des tables et instrumentation | 46,13 ms | 48,29 ms |
| Du marqueur firmware précédant StartImage à la première lecture TSC du PE | 113 000 ticks | 95 000 ticks |
| Arène temporaire EFI utilisée | 59 752 octets | 59 584 octets |
| Allocations vivantes au reçu terminal | 0 | 0 |

Les dix tables occupent **40 Kio**. Les deux cadres témoins ajoutent 8 Kio en flash. Le reçu invité occupe 80 octets et le transfert d'époque 32 octets. L'image PE chargée occupe 4 509 696 octets, dont l'arène statique commune de 4 Mio. Le PE non signé sur disque occupe 231 424 octets. Le plafond RSS Darwin observé pour la commande du banc est de 343 162 880 octets, avec une copie temporaire de 256 Mio pour localiser le reçu. Le temps TSC inclut la sortie du marqueur de mesure et StartImage. Il s'agit de deux observations en TCG. Aucune extrapolation de p99, de temps CPU natif ou de coût physique.

Par rapport aux 4 476 928 octets du terminal 5I, le nouveau PE chargé augmente de 32 768 octets. Son pic d'arène augmente de 3 048 octets au nominal actif. Le receveur 5J contrôle les options, le mode UEFI et la carte mémoire. Les chronométrages 5I sur débogueur ne couvrent pas ces mêmes opérations. [Mesures 5I](diagnostic-uefi-5i.md)

## Réserve APIC et contrat proposé à la MOE

La trace 5I conserve une écriture CPU 0 de 32 bits, valeur zéro, à l'adresse xAPIC EOI `0xfee000b0`. Le nom de région QEMU `apic-msi` ne suffit pas à identifier une émission MSI du NIC. Le modèle acquitte le vecteur en service de plus haute priorité, actualise l'état d'interruption et peut diffuser l'EOI à l'IOAPIC pour un vecteur déclenché par niveau. Le dossier 5I ne contient pas le vecteur correspondant à cette écriture précise.

En 5J, la trace d'interruptions montre le vecteur **0x20** interrompant du code dans la plage du PE, puis l'écriture zéro au registre APIC 0x0b. Le module LocalApicTimerDxe du firmware qualifié programme le vecteur 32 et son gestionnaire appelle SendApicEoi avant la notification de minuterie. L'acquittement permet la livraison normale des interruptions ultérieures. Le PE 5J n'ajoute aucun CLI à son entrée ou à son attente terminale. Les drapeaux de SMI et S3 du Q35 sont conservés. Le premier essai avec le firmware PIO historique s'est arrêté dans SaveSmiFeatures lors de l'enregistrement du script S3. La variante conserve le contrôle des huit octets négociés et enregistre l'écriture et la vérification de retour en PIO. L'exécution d'une reprise S3 reste à qualifier. [Manuel Intel, volume 3, APIC](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) · [Gestionnaire de minuterie EDK II](https://github.com/tianocore/edk2/blob/fbe0805b2091393406952e84724188f8c1941837/OvmfPkg/LocalApicTimerDxe/LocalApicTimerDxe.c)

**Proposition soumise à décision, sans changement du critère actuel.** Entre le premier RIP EFI et l'arrêt, permettre uniquement l'EOI local xAPIC du BSP, largeur 32 bits, valeur zéro, associé à une interruption de minuterie attribuée au vecteur et au gestionnaire du firmware qualifié. Exiger une trace complète sans perte, l'identité du vCPU, la correspondance avec une entrée d'interruption et la preuve que l'EOI n'acquitte aucune IRQ réseau ou stockage. Garder le refus pour toute autre écriture ou événement non attribué. Contrôler séparément les commandes PCI, la traduction, les tables, les IRQ périphériques, les compteurs et le contenu du namespace. Une interruption absente sur un essai ne valide pas cette exception.

## Limites et entrée en 5K

Le banc détient un modèle fermé depuis le reset. Une plateforme physique devrait fournir une autorité firmware ou supérieure, établie avant le chargement, un canal de transfert de confiance et une preuve de couverture de tous les agents DMA, y compris hors PCI, des alias, des réservations et des transactions déjà engagées. Une lecture tardive du bit TES, une table DMAR seule, un nonce copié ou un délai ne satisfont pas ce contrat. Le profil de laboratoire ne qualifie ni le MSI ni un firmware OEM.

Pour envisager 5K, conserver un bail dont l'époque, le domaine, le RID et les cadres sont corroborés, disposer d'un oracle indépendant pour la continuité du stockage, et définir les preuves de fin de transaction et d'invalidation par demandeur. Les cellules de refus donnent l'état initial de quarantaine. La restitution individuelle requiert son propre mandat. La décision sur l'exception APIC reste à prendre avant de qualifier la reprise EFI selon ce nouveau contrat.
