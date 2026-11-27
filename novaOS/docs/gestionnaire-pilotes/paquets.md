# Manifeste de pilote et installation

**Proposition v1, à arbitrer avant le premier installateur.** Le paquet relie deux objets signés. Le paquet ELF authentifie les octets exécutables pour une ISA. L’enveloppe de pilote authentifie le contrat matériel, les droits demandés, les dépendances et les empreintes des paquets ELF. Le vérificateur expérimental existant reconnaît `NovaOS/ELF/pkg01`, Ed25519 strict, SHA-256, une ISA par paquet, 1 à 8 membres et jusqu’à 4 Mio de charge cumulée. Il refuse les clés faibles. La politique fournit la clé autorisée au vérificateur. Le précédent qualifié sous QEMU concerne un ELF applicatif. [Vérificateur existant](../../user/novaauth/src/distribution.rs) · [Qualification du lanceur](../lancement-generique-courtier-qemu.md)

## Représentation proposée

Un manifeste TOML UTF-8, limité à 16 Kio, une signature détachée Ed25519 de 64 octets et les objets immuables désignés par leur empreinte. Le message signé est la concaténation des octets ASCII `NovaOS/driver-package/v1`, d’un octet nul, de la longueur du manifeste en u32 little-endian et de ses octets exacts. Le manifeste contient les condensats de chaque paquet, de l’ELF choisi et du profil de médiation. L’installateur vérifie la cohérence du triplet identité, version numérique et ISA dans `pkg01`, y sélectionne le membre signé puis contrôle son SHA-256. Le chargeur revalide la même copie scellée, les protections des segments et l’entrée. Aucun changement d’encodage ou de fins de ligne entre signature et vérification. [Chargeur ELF](../../kernel/src/memory/user/elf.rs) · [Proposition Ethernet initiale](../architecture-ethernet/pilotes.md)

L’exemple donne la forme d’un **candidat fictif de laboratoire** pour ARM. Les condensats nuls rendent cette entrée ininstallable. Le nom du profil et le nom de la fonction système requise sont des propositions. Une variante x86_64 requiert sa propre entrée `image` avec un autre paquet ELF signé. Le manifeste interne v0.20.0 de NovaOS demeure un document indépendant.

```toml
schema = "novaos.driver/1"
id = "org.novaos.lab.virtio-net"
version = [1, 0, 0]
security_epoch = 1
origin = "laboratoire"
publisher = "org.novaos.lab"
signing_key_id = "pilotes-lab-01"
elf_package_id = "b1c427ee4c874cbdace583ae2e23c6a2"
elf_package_revision = 1
class = "ethernet"
provides = ["ethernet/1.0"]
required_caps = ["commander-instance", "attendre-evenement"]
optional_caps = []

[compat]
architectures = ["aarch64"]
abi_major = 1
os_min = [0, 20, 0]
os_before = [0, 21, 0]
required_features = ["courtier-ethernet-arm-3e"]

[[image]]
isa = "aarch64"
member = "virtio-net-arm"
package_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
elf_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"

[[match]]
bus = "pci"
vendor = 0x1af4
device = 0x1041
class_base = 0x02
subclass = 0x00

[[dependency]]
id = "org.novaos.lab.courtier-virtio-net"
kind = "system-profile"
min = [1, 0, 0]
before = [2, 0, 0]
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
required = true

[hardware]
dma = ["descripteurs:r", "disponibles:r", "utilises:w", "tx:r", "rx:w"]
mmio = ["profil-virtio-pci-commun", "notification-validee"]
irq = ["intx-mediee"]
max_irq_routes = 1
dma_pages = 22
queues = 2
entries_per_queue = 16

[quotas]
private_pages = 32
ipc_roots_created = 1
ipc_handles = 6
ipc_boxes_created = 0
pending_operations = 8
```

| Champ ou règle | Validation et effet |
|---|---|
| Identité, versions et origine | ID ASCII unique dans le registre de l’éditeur, stable à travers les mises à jour. La première installation lie l’ID et son espace de noms à un éditeur autorisé. Trois entiers naturels bornés u32 pour la version, ordre lexicographique numérique. Intervalle OS semi-ouvert, version réellement annoncée, fonctions et profil de construction approuvés. La seule bannière du noyau ne suffit pas. Interdire deux contenus pour le même éditeur/ID/version |
| Plateformes, images et objets | Une image unique par ISA déclarée. ID de paquet ELF sur 16 octets non nuls, révision u32 positive, même ID/révision dans chaque paquet ELF. Le membre doit respecter la grammaire du vérificateur. Empreinte SHA-256 sur le fichier entier, 64 chiffres hexadécimaux minuscules, non tous nuls. Les membres et la signature sont vérifiés depuis une vue immuable |
| Correspondances de bus | Alternatives dans `match`, conjonction des champs d’une entrée. Pour PCI, vendeur/produit puis précision optionnelle de révision et sous-système. Pour USB, VID/PID ou classe autorisée, configuration, classe/sous-classe/protocole d’interface et liaison au parent. Pour plateforme, fournisseur d’inventaire et identifiant firmware autorisés. Un type de bus inconnu rend la variante inéligible |
| Dépendances et interfaces | Intervalles semi-ouverts et type déclaré pour profil système, contrôleur parent ou service. Le plan fige chaque version et son empreinte. Un profil privilégié requiert la signature distincte de l’autorité système. Les services annoncés portent majeure et mineure. Une dépendance facultative manquante retire la fonctionnalité correspondante |
| Autorité effective | Intersection du besoin signé, de la politique locale, du profil de médiation et du groupe prouvé. Tout droit obligatoire hors intersection refuse le plan. Les facultatifs absents sont visibles à la négociation. `r` et `w` sont vus du périphérique. MMIO et IRQ sont des opérations de profil typées. Compter à part les pages du contrôleur et les feuilles fournies au pilote |
| Bornes et évolutivité | Maximum proposé de 8 images, 32 correspondances, 16 dépendances directes, 32 droits et 64 entrées dans le plan résolu. Contrôler dimensions, débordements et cohérence des quotas avant allocations. Rejeter doublons, chemins, clés inconnues en v1, versions de schéma inconnues et parseurs divergents. Une nouvelle syntaxe nécessite une autre version négociée |

## Chaîne de confiance et mises à jour

Les clés autorisées proviennent de la politique de machine. `signing_key_id` aide à choisir parmi les clés déjà admises pour ce rôle, cet éditeur, cet espace de noms et cette origine. La clé n’est jamais importée depuis le paquet. Un administrateur peut autoriser l’installation d’un candidat vérifié, sans devenir signataire de distribution ni administrateur d’un contrôleur. Un profil local explicite peut admettre une clé de développement enrôlée volontairement, uniquement sur une cible de développement isolée. Les catalogues et racines de production excluent ces clés. Un binaire local non authentifié demeure inéligible au chargement automatique. Le passage à la distribution exige les signatures et la politique de distribution. [Séparation développement/distribution prévue](../reconciliation-v0300/arbitrages.md)

| Mutation | Point de décision et comportement en cas d’interruption |
|---|---|
| Installation | Vérifier hors ligne enveloppe, paquets `pkg01`, profil système, chaîne de clés et état de révocation. Préparer les objets immuables puis publier une tête de catalogue attendue, liée à la politique et à un ID de mutation. Un paquet installé est disponible pour la sélection. Son installation seule n’arme aucun périphérique |
| Mise à jour | Garder les deux révisions adressables, vérifier l’ensemble des dépendances et comparer les droits et quotas. Une augmentation exige une nouvelle décision de l’administrateur. Publier la candidate, geler les nouvelles admissions sur la révision remplacée, arrêter et réconcilier chaque ancienne instance puis admettre la candidate avec une nouvelle génération. L’ancien profil reste disponible pour le retrait jusqu’à son terme. Un échec est visible par groupe |
| Désinstallation | Fermer les nouvelles ouvertures, vérifier les dépendants, orchestrer les retraits. Un refus de retirer un dépendant bloque l’opération ou laisse la révision explicitement en retrait. Garder objets, profil d’arrêt et preuves tant qu’un bail, une quarantaine ou une opération de rollback les référence. Libérer uniquement après extinction des références |
| Rollback | Nouvelle mutation comparée à la tête actuelle. Revalider signature, empêchements, dépendances et plancher d’époque de sécurité. Réadmettre la révision précédente avec une incarnation fraîche uniquement si la clôture est prouvée. Un retour en dessous du plancher ou vers une clé révoquée est refusé. Une panne du nouveau pilote peut donc laisser le groupe fermé |
| Perte de réponse, coupure ou restauration | Interroger la décision par ID et ancienne tête. Après conflit ou reçu ambigu, conserver la tête authentifiée et geler les admissions concernées. F1, ancre et F2 de P9 ordonnent la persistance locale. Une restauration intégrale exige une source de fraîcheur indépendante pouvant détecter un recul du catalogue ou de la liste de révocation. À défaut, suspendre les admissions dynamiques et conserver le mode de démarrage approuvé |

La rotation transmet le rôle par une décision authentifiée de l’autorité de politique. Une révocation ferme les nouvelles admissions et classe les instances actives pour confinement. Un objet révoqué peut rester épinglé comme preuve. Sa présence n’autorise aucune relance. La gouvernance des clés de production, la fraîcheur hors ligne, l’amorçage du stockage qui héberge les paquets et le support durable des décisions appellent les arbitrages de la phase suivante. [Ordre de publication P9](../../../novaFS/docs/novafs-recovery.md) · [Limites de la publication conditionnelle](../../../novaFS/docs/novafs-publication-conditionnelle.md)
