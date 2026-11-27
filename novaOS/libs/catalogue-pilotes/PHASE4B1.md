# NovaOS v0.25.0 — Catalogue de pilotes hors ligne, phase 4B.1

**L’index tient dans 5 632 octets pour 64 paquets, soit 88 octets par entrée sur les deux architectures.** Avec les manifestes de référence qu’il possède, le catalogue occupe 88 512 octets de tas, contre 159 488 en 4B. Le prix du chargement à la demande est mesurable. Avec 64 versions toutes éligibles, la sélection passe de 19 à 648 µs en médiane sur ARM64 et de 65 à 1 461 µs sous Rosetta.

Références vérifiées le 27 novembre 2026. Architecture 4A `05b5788476735a97450ca490c0b86ad07c3ff8d7`, source Rust 4B `621321850cffdd92fd54682ce04b42bb273e5752`, livraison témoin `0eea6bc694533afc4ab037d08b6c40cf28e1103b`. Implémentation 4B.1 `c31b33b885bf115c6654b37c3a535d8e792f939c`, révision qualifiée avec le banc définitif `319eb8793140a5a90da1136983e9af98019decea`. [Preuve comparative](qualification-4b1.txt) · [Qualification 4B d’origine](qualification.txt)

## Index et propriété

Une entrée ordonnée par identité et version conserve le SHA-256 du manifeste exact, la révision du pilote, l’indice du signataire dans la politique immuable, sa priorité, une référence propriétaire au document et la position de l’identité décodée. Si cette identité est entièrement échappée et que sa chaîne décodée ne figure nulle part dans le document, l’entrée l’ajoute en suffixe privé, à concurrence de 96 octets. Le suffixe reste extérieur aux octets signés. Un critère PCI unique et exact permet d’écarter rapidement un fournisseur ou un dispositif incompatible. Tous les autres critères passent par la sélection universelle. Une assertion à la compilation impose une entrée de 128 octets au plus. [Implémentation](src/index.rs)

L’admission conserve l’ordre des contrôles 4B, dont la signature Ed25519 séparée par domaine, SHA-256, la politique et les paquets ELF `pkg01` de chaque ISA. Elle prépare la copie privée et réserve la place dans l’index avant l’unique insertion. Les entrées existantes demeurent consultables après un refus. Le catalogue emprunte la politique immuable. Les paquets ELF et les signatures viennent de l’appelant et sont lus pendant l’admission. Le catalogue garde les octets du manifeste admis et leurs empreintes. Le plan possède ses données et ses liaisons. [Admission](src/admission.rs) · [Sélection](src/selection.rs)

Le sélecteur décode un candidat à la fois, déplace ses chaînes utiles dans le plan et libère le reste. Il réutilise les octets du manifeste pour les clés et les chaînes simples. Les dépendances d’un paquet n’ont qu’une représentation résidente dans ses octets signés. Les ressources de l’autorité et les liaisons effectivement retenues dans le plan ont leur propre comptabilité. La déduplication entre paquets est limitée à l’identité d’une révision déjà admise et au trousseau partagé. Une arène de décodage conservée au repos ajouterait son maximum au résident du catalogue. Le prototype rend les espaces temporaires à l’allocateur à chaque opération.

## Politique d’allocation

Le document UTF-8 reste limité à 16 384 octets. Le lexer et la grammaire proviennent de la même version de `toml_parser` qu’en 4B. Le décodeur conserve les positions des événements sur 16 bits et emprunte les fragments sans échappement. Les lignes vides ou commentées successives sont validées et condensées. Les tables, clés pointées, tableaux, échappements, dates et champs de schéma restent contrôlés. La profondeur des conteneurs est bornée à 32. Les champs inconnus des tables reconnues, les doublons et les valeurs mal typées sont refusés. Un bus futur reste inéligible. [Décodage du document](src/document.rs) · [Schéma typé](src/decode.rs)

**Le plafond par décodage est de 262 144 octets de demandes de tas cumulées**, sortie typée comprise. Chaque réservation débite sa taille entière avant `try_reserve_exact`, y compris lors d’un agrandissement. Une libération ne recrédite pas l’opération. Une demande trop grande produit `Limit` et l’épuisement de l’allocateur produit un refus. L’adaptateur ELF conserve son erreur publique `Elf` en cas d’échec interne. Le budget du catalogue peut être abaissé à la construction. Le reçu de politique utilise le plafond commun. L’allocateur hôte mesure séparément les octets simultanément vivants. Les métadonnées de malloc, l’arrondi aux pages, la pile et les données déjà fournies par l’appelant sont hors de ce compteur. [Comptabilité](src/bounded.rs) · [Épreuves d’allocation](tests/allocations.rs)

Le seuil cumulé minimal mesuré vaut 21 150 octets pour le vecteur nominal, 21 176 pour l’identité échappée et 130 445 pour le document maximal dense. À un octet sous chaque seuil, l’admission est refusée. Les mêmes manifestes passent au seuil exact. Le plafond par défaut laisse de la place aux autres répartitions autorisées du schéma. La référence de 16 Kio de lecture reste distincte de cette enveloppe. Un document de longueur admissible peut être refusé pour son coût structurel.

**Correction de schéma.** Le désérialiseur Serde de 4B remplissait un tableau Rust de trois composantes puis ignorait une quatrième composante de version. Le nouveau décodeur contrôle la longueur avant conversion. Le test différentiel vérifie ce refus et la compatibilité des triplets valides. [Schéma v1](SCHEMA.md) · [Comparaison des décodeurs](src/decode/tests.rs)

## Mesures de mémoire

Octets demandés à l’allocateur, en release. Les deux hôtes donnent les mêmes résultats pour le catalogue. Dans la colonne 4B, le total inclut les manifestes typés et leurs allocations. Dans les colonnes 4B.1, la somme de l’index et des documents donne le total résident. Le pic d’admission comprend les entrées déjà admises dans le catalogue en construction.

| Paquets de référence | Total 4B | Index 4B.1 | Documents 4B.1 | Total 4B.1 | Pic admission 4B | Pic admission 4B.1 |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 2 492 | 88 | 1 295 | 1 383 | 31 072 | 8 856 |
| 16 | 39 872 | 1 408 | 20 720 | 22 128 | 68 452 | 29 601 |
| 32 | 79 744 | 2 816 | 41 440 | 44 256 | 108 324 | 51 729 |
| 64 | 159 488 | 5 632 | 82 880 | 88 512 | 192 752 | 95 985 |

L’entrée 4B mesure 528 octets avant les allocations de ses champs, soit 33 792 octets de tableau à 64 entrées. Les deux paquets ELF fournis au banc occupent 275 et 272 octets. Les variantes générées pour 64 candidats appartiennent au banc. La politique avec un signataire et un profil retient 683 → 299 octets de tas et mesure 120 octets par valeur. Le descripteur du catalogue mesure 32 → 40 octets. À 64 paquets nominaux, catalogue et politique, structures comprises, occupent donc **160 323 → 88 971 octets**, avant plan et entrées externes.

| Décodage ou sélection | 4B | 4B.1 |
|---|---:|---:|
| Pic parseur, manifeste original de 1 293 octets | 31 024 | 8 856 |
| Modèle typé vivant à la sortie, même manifeste | 1 962 | 754 |
| Pic parseur, manifeste complété à 16 Kio par des espaces | 134 536 | 8 856 |
| Pic parseur et pic d’admission d’un document maximal dense | 228 488 | 56 424 |
| Modèle typé à la sortie, maximal dense | 16 897 | 16 241 |
| Résident pour 64 maximaux d’exactement 16 Kio | 1 115 200 | 1 054 208 |
| Pic d’admission du 64e maximal | 1 326 239 | 1 094 160 |
| Pic additionnel de sélection, un candidat nominal éligible | 1 527 | 8 856 |
| Pic additionnel de sélection, un maximal dense | 1 527 | 56 424 |
| Plan nominal, valeur + tas, ARM64 / x86_64 | 344 + 280 / 279 | 344 + 280 / 279 |
| Plan maximal dense, valeur + tas, ARM64 / x86_64 | 344 + 355 / 354 | 344 + 355 / 354 |

Le maximal dense utilise 16 381 octets avant ses trois espaces finaux, 32 interfaces fournies, 32 droits, 32 règles de correspondance et 16 dépendances. L’identité du pilote et les noms longs exploitent les 96 octets autorisés. Les 15 services optionnels absents expliquent la petite taille du plan maximal de ce vecteur. Un autre scénario matérialise 63 liaisons dans le même plan. La conservation de tous les octets signés rend les commentaires et espaces facturables au résident, même lorsqu’ils ne donnent aucun champ typé. [Générateur du maximal](examples/support/maximal.rs) · [Chaîne signée](tests/plans.rs)

## Mesures de temps

Médianes des trois p50 et des trois p99, en **microsecondes**. Chaque cellule donne p50 / p99. Même machine Darwin ARM64, même rustc 1.99.1, mêmes profils release et vecteurs pour les deux révisions. x86_64 est exécuté par Rosetta. Trois processus indépendants par architecture et révision, 501 appels par série pour Ed25519 et l’admission, 1 001 pour la résolution et la sélection. Les objets, la politique et les cibles sont déjà disponibles en mémoire. [Trois séries détaillées](qualification-4b1.txt)

| Opération | ARM64 4B | ARM64 4B.1 | x86_64 4B | x86_64 4B.1 |
|---|---:|---:|---:|---:|
| Signature Ed25519 de l’enveloppe | 31,167 / 43,416 | 33,625 / 41,167 | 53,875 / 69,250 | 54,958 / 69,209 |
| Admission complète, deux ELF | 107,875 / 148,709 | 114,333 / 137,500 | 187,625 / 208,917 | 193,917 / 230,875 |
| Résolution du profil nominal | 0,125 / 0,209 | 0,083 / 0,166 | 0,209 / 0,334 | 0,167 / 0,250 |
| Sélection, 1 paquet | 0,292 / 0,417 | 10,333 / 13,125 | 1,042 / 1,334 | 22,333 / 32,250 |
| Sélection, 16 paquets, 1 éligible | 0,458 / 0,625 | 10,125 / 13,125 | 1,333 / 1,500 | 22,334 / 28,625 |
| Sélection, 32 paquets, 1 éligible | 0,666 / 0,750 | 10,167 / 12,917 | 1,667 / 2,042 | 22,416 / 28,416 |
| Sélection, 64 paquets, 1 éligible | 0,917 / 1,250 | 10,250 / 13,209 | 2,333 / 2,916 | 22,458 / 30,459 |
| Sélection, 64 révisions toutes éligibles | 19,250 / 29,417 | 647,875 / 737,875 | 65,417 / 83,333 | 1 460,583 / 1 658,708 |
| Sélection, 1 maximal dense | 4,541 / 6,416 | 66,792 / 78,917 | 6,792 / 9,542 | 128,959 / 152,792 |

Quand 63 entrées échouent au filtre PCI, le travail dominant de 4B.1 reste le décodage du seul candidat. Quand les 64 versions sont éligibles, chacune est décodée. Le publicateur reste hors ligne. Le budget d’activation QEMU de phase 4A couvre davantage d’opérations et devra être éprouvé dans son propre profil.

## Code et constantes

Octets du seul objet relogeable du catalogue, mesurés par `llvm-size`. Le total peut inclure des tables de déroulement. Les dépendances finales et le chargeur doivent être facturés dans le futur ELF intégré.

| Cible nue | Code 4B → 4B.1 | Constantes 4B → 4B.1 | Total Berkeley 4B → 4B.1 | .data / .bss |
|---|---:|---:|---:|---:|
| AArch64 softfloat | 180 668 → 82 212 | 6 953 → 2 736 | 198 509 → 93 948 | 0 / 0 |
| x86_64 | 254 887 → 93 172 | 11 703 → 2 702 | 266 590 → 95 874 | 0 / 0 |

L’exemple Mach-O lié présente des segments `__TEXT / __DATA` de 819 200 / 16 384 → 507 904 / 16 384 octets en ARM64 et 909 312 / 24 576 → 557 056 / 20 480 en x86_64. Ces tailles de segments hôtes incluent le runtime.

## Matrice de non-régression

| Vérification | ARM64 natif et x86_64 sous Rosetta |
|---|---|
| 18 tests historiques | Tous réussis sur chaque architecture, y compris authenticité ELF, révocation, époques, séparation des origines, règles USB/plateforme, déterminisme, quotas et refus |
| Huit tests supplémentaires | Tous réussis sur chaque architecture. Grammaire différentielle, profondeur et budget de graphe, quatre épreuves de bornes et d’allocation, deux épreuves de plans |
| Manifeste et listes | Entrées exactes de 16 Kio à blancs, à lignes vides et à champs denses, dépassement d’un octet, noms de 96/97 octets, tableaux aux limites et une entrée excédentaire, champs répétés, échappements, tables pointées et imbriquées, commentaire invalide |
| Allocateur réellement défaillant | Retour nul injecté successivement à chacune des 88 allocations d’une admission, 176 d’une sélection à deux candidats et 14 d’un ajout de ressource. Chaque refus laisse le solde de tas initial. Après chaque échec d’admission, l’entrée et le plan précédents restent identiques |
| Répétition et budget | 64 refus à chaque budget réduit, 16 réadmissions idempotentes de chaque variante au seuil exact, refus structurel dense au plafond par défaut |
| Égalité inter-révisions | Comparaison intégrale des champs du plan imprimés par le même banc. 20 plans et 8 décisions de refus identiques, dont deux plans à 63 liaisons, sur chacune des deux architectures. SHA-256 du transcript canonique `100ecdb22da36efa64d5bac64f0bc2337c64629047c4d5803a8ebcf7ac2356e5` |
| Pile hôte | Chaîne signée de 63 ressources, refus au-delà de 64 références parcourues, maximal dense et manifeste avec 28 tableaux imbriqués exercés dans un thread de 128 Kio demandés. Pile réservée constatée par pthread, 143 360 octets sur ARM64 et 131 072 sur x86_64. Le maximum de pile effectivement touché reste à mesurer sous NovaOS |
| Compilation et isolation | Deux bibliothèques nues release compilées. Clippy hôte ARM64 sur les cibles du composant avec avertissements traités en erreurs. Les deux exemples Darwin sélectionnent pour les deux ISA. Scénarios QEMU, matériels et de persistance non exécutés, hors périmètre du catalogue hors ligne |

Le compteur d’allocations et les appels pthread sont propres aux tests hôtes. La bibliothèque reste `no_std + alloc`. Le diff versionné se limite à `novaOS/libs/catalogue-pilotes`. Les sources NovaFS, NovaAuth, noyau, IPC, garde Q35 et manifeste v0.20.0 sont identiques à la livraison 4B. T03 conserve sa réserve de fraîcheur et de persistance.

## Budget proposé pour l’intégration

Pour **un catalogue, une politique et une opération en cours**, compter séparément les postes ci-dessous. Les plafonds portent sur les octets logiques demandés à l’allocateur avant ses propres métadonnées.

| Poste | Réservation proposée et justification |
|---|---|
| Résident de l’index | 5 632 octets nécessaires à 64 entrées. Maintenir la ligne de 8 Kio de 4A pour son arrondi et sa marge |
| Documents signés propriétaires | Jusqu’à 64 × (16 384 + 96) = 1 054 720 octets, dont le suffixe éventuel de l’ID. Avec l’index exact, 1 060 352 octets. Les SHA-256 sont déjà comptés dans l’index |
| Politique et dépendances authentifiées | Jusqu’à 225 280 octets de tas. 64 signataires de 128 octets avec deux identités de 96 octets, 64 empreintes bloquées de 32 octets, 64 ressources de 128 octets avec un nom de 96 octets et au plus 16 dépendances de 88 + 96 octets. Les capacités excédentaires du trousseau fourni sont éliminées avant rétention |
| Structures de tête | 40 octets pour le catalogue et 120 pour la politique. Total résident maximal calculé de **1 285 792 octets** pour les trois lignes précédentes et ces deux têtes |
| Lecture et décodage | 16 Kio d’entrée et jusqu’à 256 Kio de demandes cumulées, modèle de sortie inclus. Espaces libérés entre candidats. Compter un espace par consultation simultanée si l’intégration autorise la concurrence |
| Plans et résolution | Prévoir **28 Kio par plan vivant**. Majorant de 28 599 octets avec 63 liaisons de 104 + 96 octets, 32 droits, 32 IRQ, 32 MMIO, 32 rôles DMA, les noms et la valeur Plan de 344 octets. Prévoir momentanément l’ancien meilleur plan et le candidat. Allouer en plus 24 Kio de marge temporaire pour le parcours et le recouvrement pendant la croissance de ses vecteurs. La ligne de 2 Kio de 4A couvre les références compactes envisagées. Les liaisons attestées de ce prototype figent deux SHA-256, le nom, la version et le genre |
| Admission et publication | Enveloppe signée au plus 17 Kio, métadonnées empruntées de huit membres ELF sous une page, place pour ancien et nouveau tableau lors d’un agrandissement de l’index. Pour la construction de la politique, compter également l’ancien vecteur lors d’un agrandissement et le trousseau entrant pendant sa normalisation |
| Objets du banc ou du futur lecteur | Pour deux ISA, jusqu’à deux objets `pkg01` de 4 195 212 octets chacun, empruntés pendant l’admission. Un artefact de ressource peut atteindre 4 Mio. Comptabiliser toute copie de staging et les pages de cache réellement détenues au même instant. Les plans simultanément conservés par les appelants restent à leur charge |

La réserve de tables communes de 4A peut garder sa ligne d’index. Les documents, la politique et l’espace de décodage justifient des réservations propres avant intégration. Toute garantie en cadres physiques exige le coût de l’allocateur de l’acteur, ses arrondis, la pile de la cible, le code final lié, les copies du lecteur et le nombre maximal de requêtes et de plans vivants. [Budgets 4A](../../docs/gestionnaire-pilotes/budgets.md)

## Prérequis de 4C et reproduction

Avant le raccord d’un lanceur, arrêter la limite de concurrence, le nombre de plans exportés et le budget en cadres du futur acteur. Vérifier le refus d’admission sur son allocateur et sa pile, ainsi que la durée d’une sélection froide avec de nombreux candidats éligibles. Définir le propriétaire et la durée de vie des objets authentifiés, puis la revalidation de l’inventaire et de la politique entre sélection et activation. Le débat sur la fraîcheur de l’autorité durable reste ouvert. [Prérequis d’autorité de phase 4B](AUTORITE.md)

Depuis le répertoire du composant, avec `NOVAOS_TMPDIR` configuré pour le laboratoire et un répertoire de compilation privé distinct des autres worktrees, exécuter la commande suivante. Elle exporte le témoin depuis Git dans un répertoire temporaire, lui applique uniquement le banc de mesure, compile avec les paquets Cargo verrouillés en mode hors ligne et compare les sorties des deux versions.

```sh
CARGO_TARGET_DIR="$NOVAOS_TMPDIR/catalogue-4b1-reproduction" python3 -B comparer.py
```
