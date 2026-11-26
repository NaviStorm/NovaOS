# Paquets de pilotes et cycle de vie

## Installation et sélection

L’autorité système autorise une famille de profils matériels. L’autorité de distribution des pilotes signe le manifeste et les empreintes des ELF de chaque ISA. L’administrateur choisit les paquets admis sur la machine. Une signature de distribution ne vaut ni autorisation de bus, ni qualification de la plateforme, ni droit réseau pour une application.

1. **Installer.** Écrire une révision candidate dans un objet immuable, vérifier taille, hachage, signature, chaîne de confiance, révocations, cible, version du schéma et registre de dépendances. Signer une enveloppe avec séparation de domaine `NovaOS/driver-package/v1` couvrant la longueur et les octets exacts du manifeste UTF-8 et les empreintes des objets nommés. Interdire clés dupliquées, chemins sortants, champs obligatoires inconnus et variantes d’analyse ambiguës. Les clés privées restent hors du système de chargement. L’exemple ci-dessous décrit la charge utile de cette enveloppe.
2. **Résoudre.** Le résolveur construit un plan immuable avec les versions et empreintes précises de l’ELF, de l’adaptateur, de la politique et des interfaces requises. Il rejette cycle, dépendance requise absente, majeure incompatible, version mineure insuffisante, profil révoqué et quotas hors bornes. Les options absentes diminuent les fonctions annoncées. Aucun téléchargement implicite au branchement.
3. **Publier le catalogue local.** Valider le plan complet avant de publier atomiquement la nouvelle génération du catalogue. En cas d’interruption, retrouver par reçu soit l’ancienne génération complète soit la nouvelle. Les décisions active et candidate sont distinctes. Une mise à jour d’index n’arrête aucune autre carte. La primitive durable du catalogue et la protection de son plancher de sécurité restent à arbitrer.
4. **Découvrir.** À l’événement PCI, USB ou virtuel, relire l’inventaire passif et sa génération. Constituer des candidats par bus et identifiants. Pour PCI, comparer vendeur, produit, classe, révision et sous-système selon le masque signé. Pour USB, vérifier aussi configuration, interface et identité de rattachement. Pour un port logiciel, utiliser l’identité de son fournisseur autorisé. L’absence de matériel ou une suppression concurrente invalide le plan.
5. **Choisir.** Éliminer les candidats inéligibles selon ISA, profils de sécurité/IRQ, révocations et bornes. Appliquer ensuite le choix administratif épinglé à l’empreinte, puis le rang de politique locale, puis la précision du match (identité exacte avec sous-système et révision, identité exacte, famille explicitement permise). À précision égale, comparer la version sémantique uniquement au sein du même paquet/éditeur. Deux candidats distincts encore premiers donnent `CONFLIT` reproductible, carte fermée et identifiants de candidats dans le diagnostic autorisé. Une épingle indisponible exige une décision de politique. L’ordre des fichiers et l’heure du branchement sont sans effet.
6. **Admettre.** Réserver atomiquement les ressources CPU, DMA, IPC et le budget de panne. Revalider la génération du catalogue, la présence et l’exclusivité du groupe. Vérifier les **mêmes octets** scellés à la création du processus, ELF64 Rust `no_std` de l’ISA, segments bornés, W^X, point d’entrée exécutable, pile NX et profil d’appels restreint. Une étiquette « Rust » ne prouve aucune propriété de sûreté. Installer les seules feuilles requises et initialiser les fenêtres à zéro. Toute préparation partielle a un propriétaire et une voie de retrait.
7. **Ouvrir.** Démarrer le pilote sans accès actif au matériel. Négocier les versions sur le port privé, préparer les pages et l’IRQ avec le groupe fermé, puis autoriser le superviseur à publier. Le service Ethernet annonce le nouveau port après succès. Un paquet couvrant cent références reste un objet installé. Deux cartes identiques obtiennent deux PID/incarnations, domaines ou médiations de bus, baux et crédits indépendants.

## Manifeste v1 proposé

Exemple de forme pour le seul profil VirtIO de laboratoire. Les deux empreintes nulles sont des valeurs fictives explicitement refusées à l’installation. La variante de profil exprime les minima et maxima **acceptés par ce paquet**. Le matériel détecté peut encore imposer des limites plus étroites.

```toml
schema = 1
id = "org.novaos.labo.virtio-net"
version = "1.0.0"
security_epoch = 1
issuer = "distribution-laboratoire"
abi = 1
ethernet = { major = 1, min_minor = 0, max_minor = 0 }
broker = { profile = "virtio-split-directionnel", major = 1, min_minor = 0 }
required_caps = ["commander-sa-fonction", "attendre-son-evenement"]
optional_features = []
required_hw_features = ["VERSION_1", "ACCESS_PLATFORM", "MAC", "EVENT_IDX"]
dma_roles = ["tx-r", "rx-w", "descripteur-r", "disponible-r", "utilise-w"]
dependencies = [{ id = "adaptateur.virtio-split", major = 1, min_minor = 0 }]

[[images]]
isa = "x86_64"
object = "elf/x86_64/pilote"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"

[[images]]
isa = "aarch64"
object = "elf/aarch64/pilote"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"

[[matches]]
bus = "pci"
vendor = 0x1af4
device = 0x1041
class_base = 0x02
subclass = 0x00

[resources]
rx_queues = [1, 1]
tx_queues = [1, 1]
rx_depth = [16, 16]
tx_depth = [16, 16]
slot_bytes = [2048, 2048]
dma_device_pages = [22, 22]
cpu_window_slots_per_direction = [1, 8]
driver_created_roots = [0, 0]
driver_leaf_handles = [4, 4]
max_pending_waits = 1
```

`matches` est une liste de variantes alternatives. À l’intérieur d’une variante, les contraintes sont cumulatives. Un ID PCI supplémentaire se déclare dans une autre entrée signée. Une variante qui change les rôles DMA ou la procédure de retrait référence un autre profil approuvé. Les bornes sont inclusives, vérifiées sans débordement. Le profil complémentaire signé décrit alignements, multiples admissibles, réserves d’anneau, largeur d’adresse, limites de segment, dépendances entre files, géométrie IOMMU et bornes de mémoire CPU/piles/IPC. Une ressource obligatoire absente dans le paquet **et** le profil lié rend le plan inadmissible. Résoudre la dépendance vers une empreinte autorisée avant toute ouverture. La demande d’un droit dans un manifeste n’en crée aucun.

## Gestion de l’instance

| Transition | Décision et résultat après panne ou demande concurrente |
|---|---|
| Découverte → sélection → réservation | Un verrou par groupe et un reçu par opération empêchent deux propriétaires. Une nouvelle génération d’inventaire invalide une réservation obsolète. Un refus rend les ressources restées purement logicielles |
| Préparée → active | Le superviseur connaît déjà les allocations et l’état de chaque publication. Une panne avant confirmation de l’ouverture entre dans son protocole de retrait. La visibilité du port et la génération active sont publiées ensemble |
| Active → fermeture | Débranchement, révocation du signataire, échéance administrative, arrêt ou décès ferment les admissions. Retirer les feuilles clientes, réveiller les attentes et arrêter la source. Les reçus restent consultables par une autorité de gestion bornée. [Preuves de restitution](dma.md) |
| Fermeture → retirée ou quarantaine | Un reçu complet permet la restitution. Une incertitude épingle les ressources concernées et interdit la réattribution du groupe. Un remplacement ne peut utiliser le bail de la génération précédente |
| Remplacement | Vérifier et préparer hors ligne la révision candidate tout en gardant l’ancienne installée. Fermer l’ancien port, retirer son bail et constater la restitution. Démarrer ensuite la candidate avec une nouvelle génération. Publier le port après vérification du lien administratif et du contrat. Une interruption entre les deux laisse le port explicitement indisponible |
| Retour à la version antérieure | Reprendre l’objet signé épinglé seulement s’il reste compatible avec le matériel, les dépendances, les listes de révocation et le plancher de sécurité courant. Refaire l’admission et les preuves d’ouverture avec un bail neuf. Un refus ou une quarantaine conserve la carte fermée |
| Redémarrage système | L’intention durable de version est relue. Les tickets, feuilles et baux du démarrage antérieur sont périmés. Avant recyclage de cadres potentiellement exposés, la plateforme doit fournir une preuve de remise à zéro et de confinement adaptée. Le redémarrage d’un processus seul ne la fournit jamais |

Le retrait d’un paquet bloque les lancements futurs et programme le retrait de ses instances actives. Garder les objets référencés pendant la fermeture ou la réconciliation. Un pilote compatible s’installe sans relier le noyau dès que le chargeur de pilotes, son profil matériel et sa porte de médiation ont été livrés et qualifiés. La qualification du lancement ELF générique de la base porte sur d’autres autorités et capacités. [Chargeur et révocation d’une image scellée](../lancement-generique-retrait.md)
