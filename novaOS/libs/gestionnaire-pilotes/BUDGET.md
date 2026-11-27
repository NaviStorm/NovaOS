# Budget mémoire de la phase 4C

Les mesures proviennent du banc Darwin en mode release. Le bilan d’intégration exprime séparément les octets de Rust, les demandes de tas et les cadres de **4 096 octets** à réserver. Les cadres de pilote et de DMA sont des compteurs du modèle. Les pages réellement attribuées par un allocateur NovaOS, la fragmentation et les tables de traduction demandent un banc invité. [Mesures brutes](qualification.txt) · [Budget d’origine 4A](../../docs/gestionnaire-pilotes/budgets.md) · [Calculs du catalogue 4B.1](../catalogue-pilotes/PHASE4B1.md)

## Mémoire mesurée

Le scénario résident utilise 64 paquets, 128 nœuds PCI, 32 groupes connus et quatre paires DATA de contrôle. Le banc teste séparément les deux plans exportés, une quarantaine complète et la restitution page par page. Les compteurs d’allocateur portent sur les octets demandés à Darwin.

| Poste | ARM64 natif | x86_64 Rosetta | Interprétation |
|---|---:|---:|---|
| Index de 64 paquets | 5 632 o | 5 632 o | 88 o/entrée, sous la cible de 128 o |
| Documents authentifiés possédés | 82 806 o | 82 806 o | 64 versions du vecteur signé |
| Politique et têtes catalogue/politique | 299 + 40 + 120 o | idem | Catalogue et politique totalisent 88 897 o |
| Tête gestionnaire | 2 416 o | 2 416 o | Inclut les 2 104 o de la tête d’inventaire et les compteurs de liaison |
| Nœuds PCI | 16 384 o | 16 384 o | 128 × 128 o. Inventaire complet avec sa tête, 18 488 o |
| Remplacement d’un instantané PCI | +16 384 o | +16 384 o | Ancien et nouveau se recouvrent. Delta après publication, 0 o |
| Registre de 32 groupes | 12 800 o | 12 800 o | 400 o/groupe. Une instance engagée ajoute ici 30 o de rôles autorisés |
| Journal et liaisons | 11 264 + 736 o | idem | 64 places de 176 o réservées au démarrage et quatre liaisons de 184 o |
| Emplacements des plans | 576 puis 1 152 o | idem | Chaque emplacement comprend la valeur `Plan`. Delta réellement retenu par plan, emplacement inclus, 854 o sur ARM et 853 o sur x86 |
| Pic supplémentaire d’une sélection | 9 134 o | 9 133 o | Même pic lors de la sélection du deuxième plan |
| Tas initial du gestionnaire avec inventaire | 27 648 o | 27 648 o | Somme des nœuds PCI et du journal préalloué |
| Pile du thread de mesure | 143 360 o réservés | 131 072 o réservés | 128 Kio demandés à pthread. Profondeur échantillonnée au passage dans l’allocateur pendant les cycles, 10 456 et 30 143 o respectivement |

En régime de repos après échauffement, la somme identifiée est **133 649 octets** sur les deux ISA. Elle additionne 88 897 o de catalogue/politique, 2 416 o de tête, 16 384 o de nœuds, 12 800 o de registre, 736 o de liens, 11 264 o de journal et 1 152 o de capacité conservée pour les plans. Les têtes d’inventaire et de catalogue sont déjà imputées. Le pic de tas additionnel observé pendant chaque série nominale vaut 9 134 o sur ARM ou 9 133 o sur x86. Les échantillons de pile donnent une borne inférieure de profondeur touchée. L’espace de saisie du fournisseur, les binaires d’objets prêtés par l’appelant, le runtime des tests et les arrondis de malloc ont des propriétaires distincts.

Le banc injecte un défaut à chaque allocation successivement atteinte sur les chemins de création du registre (1), de remplacement de 128 nœuds PCI (1), de création de groupe (1), de sélection du vecteur à un candidat (89), de réservation (1), de liaison (1) et de reprise (1). Il compare le tas vivant après refus et les états qui pourraient être publiés. Les 26 tests historiques couvrent en plus les allocations du parseur et de l’admission. [Instrumentation et assertions](tests/memoire.rs)

## Code et constantes des objets nus

| Cible | Composant | Code `.text*` | Constantes `.rodata*` | `.data*` / `.bss*` |
|---|---|---:|---:|---:|
| AArch64 | Gestionnaire | 47 284 o | 1 562 o | 0 / 0 |
| AArch64 | Catalogue | 82 212 o | 2 736 o | 0 / 0 |
| x86_64 | Gestionnaire | 50 255 o | 2 340 o | 0 / 0 |
| x86_64 | Catalogue | 93 172 o | 2 702 o | 0 / 0 |

La somme brute code + constantes représente 133 794 o sur ARM et 148 469 o sur x86, soit 33 et 37 cadres par arrondi du total. Ce sont deux objets Rust mesurés par `llvm-size -A` avec les implémentations cryptographiques logicielles. L’édition de liens finale peut éliminer des sections, instancier des génériques, ajouter les bibliothèques cryptographiques, l’allocateur, des alignements et des segments. Les acteurs qui embarquent leur propre copie du texte doivent chacun la compter. [Compilation et mesure](qualifier.py)

## Réservation de conception pour 4D

Les nombres suivants additionnent des enveloppes arrondies **par poste**, à 64 paquets, 128 nœuds, 32 groupes et deux plans. Les maxima de tous ces postes peuvent correspondre à des états différents. Un seul appel de catalogue peut utiliser la zone de travail à un instant donné. Le gestionnaire conserve une référence au catalogue et inclut la tête de l’inventaire.

| Poste résident | Calcul en octets | Cadres |
|---|---:|---:|
| Catalogue maximal, documents, politique et deux têtes | 1 285 792 | 314 |
| Tête du gestionnaire | 2 416 | 1 |
| Inventaire et noms de plateforme | 128 × (128 + 2 × 96) | 10 |
| Métadonnées des groupes | 32 × 400 | 4 |
| Rôles du bail d’après les listes signées | 32 × 12 640 | 99 |
| Deux plans exportés | 1 152 + 2 × (28 599 − 344) | 15 |
| Liaisons privées | 32 × 184 | 2 |
| Journal des reçus | 64 × 176 | 3 |
| **Sous-total résident** | | **448** |

Les 12 640 octets par bail majorent la capacité réservée pour quatre listes de 32 noms. Classe, IRQ et MMIO demandent jusqu’à 98 octets par nom avec son en-tête et DMA jusqu’à 101 avec sa direction. La table des groupes comprend le bail, les masques par page, les numéros de génération et les gardes d’attachement. Les plafonds initiaux 4A de 64 o/fonction, 256 o/groupe et 128 o/reçu sont dépassés. Le nœud possède son adresse et l’identité de correspondance, le groupe ses trois ensembles de preuves et le journal un exemplaire de la demande et de sa réponse pour le rejeu exact. Les tailles mesurées figurent dans le décompte. [Implémentation du registre](src/manager.rs) · [Journal de contrôle](src/ipc.rs)

| Provision complémentaire | Cadres |
|---|---:|
| Entrée manifeste de 16 Kio | 4 |
| Parseur, 256 Kio de demandes cumulées | 64 |
| Résolveur, marge temporaire 24 Kio | 6 |
| Candidat et meilleur plan pendant la recherche, 2 × 28 Kio | 14 |
| Instantané remplaçant, jusqu’à 40 960 o | 10 |
| Recouvrement de l’ancien registre pendant croissance, 31 × 400 o | 4 |
| Ancien tableau de liaisons, 31 × 184 o | 2 |
| Ancien emplacement de plan pendant croissance | 1 |
| Réponse de reprise des 32 liaisons | 1 |
| Pile de travail proposée, 192 Kio | 48 |
| **Sous-total complémentaire** | **154** |

**L’enveloppe commune proposée pour la sélection et le contrôle hors ligne atteint 602 cadres, soit 2 408 Kio.** Elle couvre les deux tableaux et la pile de travail. Son addition prudente laisse une marge entre opérations exclusives. Le décodage du manifeste conserve son plafond propre de 262 144 octets, indépendant des 16 Kio de lecture. Le remplacement d’un inventaire suppose un fournisseur qui livre ses nœuds par valeur. Une copie entière maintenue chez ce fournisseur appartient aussi au bilan d’intégration.

**Chargement des objets et changement de politique.** Au moment d’admettre des ELF, ajouter jusqu’à 1 025 cadres par paquet `pkg01` de 4 195 212 o, soit 2 050 cadres pour deux ISA résidentes. Une seconde copie fait à nouveau payer ces cadres. Une ressource externe de 4 Mio coûte jusqu’à 1 024 cadres si elle est simultanément résidente. Pour l’admission, prévoir aussi l’enveloppe signée (5 cadres), les métadonnées des membres ELF (1) et l’ancien tableau d’index (2). L’autorité qui change la politique doit chiffrer la coexistence des anciennes et nouvelles politiques et du trousseau entrant. Le banc 4C sélectionne dans un catalogue déjà construit. [Budgets détaillés de 4B.1](../catalogue-pilotes/PHASE4B1.md)

## Propriétaires, quarantaine et marge de clôture

- **Un groupe en quarantaine complète.** Le registre retient 33 cadres privés, dont un de clôture, et 22 cadres DMA simulés, soit **55 cadres ou 220 Kio**. Il conserve aussi un processus, un endpoint, six nœuds IPC, une attente et un crédit de reçu. Les trois conditions de l’oracle sont nécessaires pour restituer une page exposée. Après confirmation d’une seule page, 21 restent imputées au bail.
- **Repos de la série d’endurance.** Les quatre paires provisionnées entre cinq acteurs représentent 80 cadres CPU simulés, cinq processus, huit endpoints et seize nœuds. Les compteurs DMA, boîte, attente et reçu reviennent à zéro. Une clôture libère les ressources d’instance aussi longtemps que l’oracle a produit les attestations nécessaires.
- **Projection à deux groupes.** Sept acteurs de contrôle, dont deux services préexistants, et deux pilotes occupent neuf emplacements de processus. Le modèle affecte 16 cadres par acteur de contrôle et 55 par groupe actif. Les huit boîtes sont imputées dans le pool IPC historique, plafonné à 128 cadres, une seule fois. La quarantaine d’un de ces groupes conserve sa charge dans le total.
- **Exemple consolidé pour 4D.** En remplaçant les 16 cadres forfaitaires du gestionnaire par les 602 cadres communs, compter 96 cadres pour les six autres acteurs, 110 pour les deux groupes, 128 pour le pool IPC et 16 pour un texte pilote de 64 Kio effectivement partagé. Le sous-total est **952 cadres (3 808 Kio)** avant texte du gestionnaire/catalogue et avant staging. À titre de repère, l’ajout des deux objets nus arrondis donne 985 cadres sur ARM ou 989 sur x86. L’admission réelle devra utiliser les segments liés, les surcoûts de pages et d’allocateur, les processus et les tables de traduction du profil, les ancres partagées et les cadres de stockage déjà engagés. Une seconde version de pilote et tout texte réellement dupliqué s’imputent séparément.

Le groupement de plusieurs acteurs dans un seul processus Darwin permet d’observer l’état et le tas avec précision. L’affectation finale des tables, de la pile et des segments aux futurs acteurs NovaOS reste à arrêter avant un lancement privilégié.
