# NovaOS v0.25.0 — Gestionnaire de pilotes, phase 4C

**Qualification hors ligne du 27 novembre 2026.** Les six contrats A3 sont exercés sur des inventaires synthétiques. La révision Rust qualifiée est `55161f98a99439cd921fb8acdd88e61303bc5198`. Sur chaque architecture, les 26 tests du catalogue 4B.1 et les 28 tests du gestionnaire passent. Sur les 1 536 cycles chronométrés, chaque restitution nominale ramène le tas et les réservations à leur valeur de départ. [Résultats par exécution](qualification.txt) · [Budget en cadres de 4 Kio](BUDGET.md)

## Modèle et frontières de confiance

La bibliothèque indépendante `novaos-gestionnaire-sim` utilise Rust `no_std + alloc`. Le banc fournit les descriptions de bus, le profil de confiance, la topologie des acteurs et les observations de retrait. La bibliothèque utilise le catalogue compact 4B.1 par dépendance locale. Sa signature Ed25519, ses empreintes SHA-256, son contrôle ELF `pkg01`, ses deux canaux développement/distribution, ses planchers, ses quotas et son parseur borné restent ceux de la livraison `e9af41f00949e6daade0dd7cb267bcf3262c36e5`. [Architecture 4A](../../docs/gestionnaire-pilotes/README.md) · [Qualification du catalogue](../catalogue-pilotes/PHASE4B1.md)

- **Inventaire.** Un instantané contient jusqu’à 128 fonctions ordonnées par emplacement logique. Les adresses décrivent PCI/PCIe avec segment et BDF, USB avec parent, port, configuration et interface, ACPI, Device Tree ou fournisseur virtuel. Les identités de correspondance sont celles du catalogue. Un attachement contient le démarrage, le contrôleur, l’emplacement, l’apparition et la révision de topologie qui a établi son identité. Une notification de révision en avance suspend les admissions jusqu’à la relecture. Le remplacement vérifie les adresses, les identités, les liens orphelins et les cycles avant publication. [Inventaire exécutable](src/inventory.rs)
- **Groupes.** Le superviseur synthétique calcule la fermeture transitive des alias DMA et les dépendances de parenté. Il relève aussi tous les pairs des périmètres de reset et d’IRQ déclarés. L’autorité doit approuver le groupe entier. Une dépendance doit être active et un pair engagé dans un périmètre incompatible, y compris en quarantaine, bloque la réservation. Une fonction USB médiée exige un parent actif et un plan de classe signé lié à son profil. La feuille logique contrôle le sujet, son incarnation, la révision et les droits du plan enfant. [Groupes et médiation](src/manager.rs) · [Épreuves de découverte](tests/inventaire.rs)
- **Sélection.** Une opération exclusive consulte le catalogue. Deux plans peuvent rester exportés en même temps pour préparer un second groupe pendant que le premier attend sa réservation, ou pour conserver un plan de classe médiée. Le troisième reçoit un refus de capacité. Toute réservation recoupe les révisions de catalogue, de politique et d’inventaire. Le plan canonique `NPL1` inclut les empreintes des objets, les dépendances résolues, les droits, les quotas et la cible. Le ticket privé du banc associe son SHA-256 au bail complet. [Sérialisation du plan](src/plan_wire.rs) · [Test du ticket et de la lecture paginée](tests/groupes.rs)
- **Cycle de vie.** Le registre contient jusqu’à 32 groupes et huit contrôleurs simultanément engagés. La table de 14 états et 32 transitions inclut l’entrée initiale. Chaque mutation confronte la révision attendue à celle du groupe. Affectation, initialisation, publication, suspension, annulation et restitution ont des gardes supplémentaires. Le numéro de génération et l’incarnation admise croissent à l’échelle du banc. À épuisement, l’admission échoue avant effet. [Automate](src/lifecycle.rs) · [Table 4A et comparaison exhaustive](tests/cycle.rs)
- **Réconciliation.** Un reçu de contrôle constate l’issue d’une demande. Pour restituer une page simulée exposée, l’oracle doit attester l’arrêt de la source, le retrait de la traduction de cette page et le retour de sa propriété. Les trois observations peuvent arriver séparément. Leur numéro doit progresser et leur bail doit correspondre. Sans preuve, les ressources restent imputées à l’ancienne génération. La garde Q35 bloque réservation et réadmission sur le contrôleur configuré tant que le bail NVMe injecté est actif. [Épreuves de restitution](tests/groupes.rs) · [Garde Q35](tests/cycle.rs)

## Contrat DATA du prototype

Tous les entiers sont little-endian. L’enveloppe de 32 octets contient quatre u16 aux offsets 0, 2, 4 et 6 pour la majeure, l’opération, la taille et les drapeaux. Suivent l’époque, le numéro de demande et la révision attendue, trois u64 aux offsets 8, 16 et 24. Le couple époque/numéro identifie une demande sur un canal privé. Chaque époque commence par HELLO, y compris après reprise. Le profil commun expose la majeure 1 et la mineure 1. [Codec et bornes](src/ipc.rs)

| Relation provisionnée | Opérations, par numéro de fil | Portée |
|---|---|---|
| Tous les pairs | 1 HELLO, 10 RECU, 20 ACQUITTER | Négociation, inspection et consommation du reçu de cette autorité |
| Découverte | 2 ENUMERER, 3 CHANGEMENT, 4 RELIRE | Enregistrements résumés par curseur et révision. Le banc fournit la nouvelle description détaillée à `refresh` |
| Catalogue | 5 RESOUDRE, 6 LIRE_PLAN | Politique de sélection conservée côté autorité. Curseur zéro pour le digest du manifeste, puis lecture depuis 1 par fragments de 23 octets liés à la révision |
| Lanceur | 7 PREPARER, 8 LANCER, 9 ANNULER | Préparation et consommation unique du ticket local associé au bail |
| Superviseur | 11 RESERVER, 12 ATTRIBUER, 13 SUSPENDRE, 14 REVOQUER, 15 RECONCILIER, 16 ETAT | Bail exclusif et autorité de clôture survivante |
| Service de classe | 17 PREPARER_PORT, 18 PUBLIER, 19 FERMER | Visibilité de la génération du service |

HELLO occupe 32 octets et les autres requêtes 64. Leur corps encode la génération, deux arguments u64, un argument u32 et quatre octets réservés nuls. Une réponse occupe 88 octets avec le drapeau 1, un statut, une valeur, la génération et 32 octets dépendant de l’opération. La borne DATA reste de 128 octets. Les clés de requête peuvent être rejouées tant que le reçu ou la réponse de lecture est conservé. Une clé réutilisée avec un autre contenu donne un conflit. Après acquittement, un plancher de rejeu empêche une seconde exécution. Une consultation sans reçu donne INCERTAIN. Si une garde refuse une mutation avant effet faute de preuve, le reçu indique REFUSE avec le détail 0x80. Son acquittement rend le crédit au superviseur et laisse les ressources en quarantaine. Le test à une seule place rejoue ce refus, l’acquitte, apporte la preuve tardive et termine la restitution. [Distribution par rôle](src/control.rs) · [Requête de référence](vecteurs/reserver.data) · [Réponse](vecteurs/confirme.data) · [Générateur indépendant](vecteurs/generer.py)

L’autorité de banc possède le registre et les recettes de provisionnement. Les opérations DATA s’exercent uniquement avec l’objet de capacité privé du pair. Les entrées typées de politique, de disponibilité et d’observation représentent des autorités de confiance distinctes. À la reprise simulée du gestionnaire, les canaux changent d’époque et le registre survivant conserve les intentions et leurs reçus. La fonction `restart` exige l’ancienne révision de gestion et refuse un deuxième demandeur concurrent. La perte du registre ou du superviseur reste ouverte.

## Deux groupes simultanés

Le banc crée neuf paires privées pour deux groupes et un seul service de découverte. Chaque paire a deux endpoints, quatre entrées de table au total et son propre numéro d’époque. Les deux services de classe sont des acteurs distincts, chacun déjà à quatre racines. [Montage et vérification des totaux](tests/groupes.rs)

| Acteur | Création et réception | Racines / handles résultants |
|---|---|---:|
| Gestionnaire | Crée une paire par service, reçoit les feuilles des sept autres paires | 4 / 18 |
| Découverte | Crée sa paire | 2 / 2 |
| Catalogue, superviseur et lanceur | Chacun crée deux paires, une par groupe | 4 / 4 chacun |
| Deux services de classe préexistants | Chacun conserve ses quatre racines et reçoit deux feuilles | 4 / 6 chacun |
| Deux pilotes simulés | Quota de l’instance dans chaque plan signé | 1 / 6 chacun |

Avec les huit racines préexistantes des services, les deux baux et les neuf paires, le total atteint **28 endpoints, 56 nœuds et 8 boîtes globales**. Les paires DATA ajoutent zéro boîte. Le décès d’un pilote invalide son groupe et laisse la feuille de classe du pair indépendant utilisable. Un troisième groupe exige un nouveau calcul par créateur et par table. Les 32 places du registre peuvent contenir des groupes simplement découverts.

Avant admission, le registre réserve la mémoire privée et les quotas du plan, un cadre de clôture, une attente et une place de reçu de clôture. Un reçu ou une attente qui occupe sa place réservée compte une seule fois. Seul le superviseur du bail peut utiliser le crédit garanti pour une clôture à la bonne révision. L’essai sature les 64 places avec 63 reçus ordinaires et la réserve du groupe. Une demande étrangère ou périmée laisse au superviseur la possibilité d’engager le retrait. La file privée admet sept préparations ordinaires et protège sa huitième place. [Comptabilité](src/resources.rs) · [Tests de saturation](tests/controle.rs)

## Matrice de qualification

| Scénario 4A | Résultat observé hors ligne | Vérification ultérieure sous NovaOS |
|---|---|---|
| T04 — sélection | Deux ordres d’inventaire et d’admission donnent le même SHA-256 du plan. Épingle absente, éditeurs concurrents, dépendances absentes ou cycliques et cible inconnue exercés dans les suites 4B.1/4C | Autorité de fraîcheur et remise de l’objet scellé au lanceur |
| T05 — découverte | PCI/PCIe multifonction et pont, USB composite reconfiguré, ACPI/DT, virtuel, vue incomplète, notification perdue, retrait et réapparition identifiés | Énumérateurs et provenance des descriptions d’un véritable bus |
| T06 — groupe isolable | Refus d’un sous-groupe d’alias, d’une dépendance inactive et d’un conflit de reset avec le stockage actif. Parent USB obligatoire. Garde Q35 exercée | Indépendance matérielle du domaine, portée de reset, routes IRQ et continuité des E/S |
| T07 — capacités | Feuilles directionnelles, titulaire/incarnation, anciennes générations, classe médiée et refus d’un rôle étranger. Aucun handle matériel créé | Processus Ring 3/EL0, tables de capacités et de traduction, validation du courtier |
| T08 — automate | 196 couples de 14 états confrontés au diagramme 4A, 31 arêtes entre états, 165 refus et entrée initiale. Annulation, reprise, quarantaine, restitution partielle et rejeu de publication exercés | Effets réels de l’arrêt, de la suspension et de la restitution |
| T11 — mort et reprise | Décès du pilote, paire indépendante, intentions différées, deux prétendants à la reprise, ancienne époque refusée, issue consultable jusqu’à son acquittement unique | Décès d’un processus NovaOS et véritable autorité survivante |
| T14 — épuisement | Dernière génération, registres, deux plans, huit contrôleurs, quotas de processus/IPC, dernière attente globale, table de handles à 31/32, 64 reçus, huit demandes par paire et rétention sous incertitude | Mesure des pools noyau, de la fragmentation et de la progression du retrait |

La suite fonctionnelle historique compte 26 succès par ISA. Son benchmark interne marqué ignoré est distinct de ces 26 tests. La suite 4C compte 28 succès par ISA. Les deux bibliothèques compilent pour `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Les scénarios Ethernet et NovaFS historiques conservent leur provenance, leurs sources et leurs résultats d’origine. Le protocole vérifie les sources protégées contre 4B.1, compte 48 sélecteurs et retrouve la borne de huit crédits Ethernet. [Matrice 4A](../../docs/gestionnaire-pilotes/qualification.md) · [Journal de qualification](qualification.txt)

## Performances et endurance

Trois processus de mesure indépendants par architecture exécutent chacun 256 cycles, après échauffement, avec 128 fonctions, 64 paquets candidats et 32 groupes enregistrés. Le cycle inclut dix requêtes DATA de gestion, dix acquittements, des transitions locales, une autorisation de feuille et les trois faits de restitution synthétique. Les temps écoulés incluent le coût du banc instrumenté et ses assertions. Les lignes de sélection seule proviennent d’un processus distinct par série, 256 sélections par cellule, avec invalidation en dehors du chronométrage. La fiche donne, en **microsecondes**, la médiane des trois p50, p95 et p99 de série, puis le maximum des trois maxima. Les valeurs par série en nanosecondes permettent un autre regroupement. [Protocole Rust](tests/memoire.rs) · [Résultats bruts](qualification.txt)

| Exécution Darwin | Opération mesurée | p50 | p95 | p99 | Maximum |
|---|---|---:|---:|---:|---:|
| aarch64 | Inventaire, 128 fonctions | 26,1 | 31,7 | 36,2 | 40,7 |
| aarch64 | Sélection avec DATA et acquittement | 636,5 | 665,0 | 677,2 | 745,5 |
| aarch64 | Cycle complet | 640,6 | 669,4 | 681,3 | 750,8 |
| aarch64 | Sélection, 1 paquet / 1 groupe | 9,6 | 13,2 | 14,5 | 27,5 |
| aarch64 | Sélection, 1 paquet / 32 groupes | 9,5 | 12,9 | 13,4 | 23,0 |
| aarch64 | Sélection, 64 paquets / 1 groupe | 627,1 | 663,1 | 677,2 | 724,0 |
| aarch64 | Sélection, 64 paquets / 32 groupes | 626,3 | 650,3 | 668,5 | 735,6 |
| x86_64 | Inventaire, 128 fonctions | 52,7 | 54,2 | 54,2 | 58,0 |
| x86_64 | Sélection avec DATA et acquittement | 1444,8 | 1481,6 | 1496,0 | 1606,0 |
| x86_64 | Cycle complet | 1452,7 | 1489,8 | 1507,4 | 1616,5 |
| x86_64 | Sélection, 1 paquet / 1 groupe | 21,0 | 23,1 | 24,3 | 34,8 |
| x86_64 | Sélection, 1 paquet / 32 groupes | 21,0 | 23,4 | 26,9 | 29,1 |
| x86_64 | Sélection, 64 paquets / 1 groupe | 1444,2 | 1466,9 | 1499,1 | 1536,5 |
| x86_64 | Sélection, 64 paquets / 32 groupes | 1443,2 | 1479,7 | 1512,3 | 1700,3 |

La sélection parmi 64 paquets occupe environ 99 % de la médiane du cycle dans le profil chronométré. Au terme de chaque cycle, le banc compare le tas vivant, l’absence de plan et de bail du groupe, son état et les huit compteurs de ressources. Les 768 contrôles par ISA convergent vers la même base. Les objectifs temporels QEMU de 4A nécessitent des mesures dans l’invité avec le véritable chemin IPC et les délais matériels.

## Reproduire et préparer la phase 4D

Le banc demande Darwin ARM64, Rosetta, les quatre cibles Rust présentes, `llvm-size` et les dépendances Cargo déjà disponibles. Le script effectue les tests avec `--locked --offline --release` et vérifie le formatage du seul composant. La variable `NOVAOS_TMPDIR` désigne l’emplacement temporaire privé du projet. Depuis la racine du worktree, utiliser un répertoire de compilation personnel. [Script de qualification](qualifier.py)

```sh
export CARGO_TARGET_DIR="$NOVAOS_TMPDIR/gestionnaire-4c-reproduction"
python3 -B novaOS/libs/gestionnaire-pilotes/qualifier.py
```

L’analyse Clippy avec avertissements traités en erreurs a réussi sur Darwin ARM64. La distribution Homebrew de Clippy ne disposait pas de la bibliothèque standard x86_64. Les essais fonctionnels x86_64 ont été exécutés avec la chaîne Rustup et Rosetta, contrôlée par `sysctl.proc_translated=1`.

Pour 4D, la MOA doit arbitrer A4 avant l’introduction d’un lanceur réel. Le premier profil devra associer une image immuable à la politique vérifiée, provisionner et révoquer les feuilles dans les tables natives, maintenir une autorité de ressources indépendante du gestionnaire, et établir les preuves de retrait du contrôleur. Il faudra mesurer le binaire lié, les piles, les cadres uniques et l’IPC réel, puis comparer le premier groupe ARM au témoin Ethernet 3E dans son profil QEMU. La garde Q35 demeure une condition d’admission. La persistance, la récupération après perte du superviseur et la qualification physique restent des travaux distincts. [Budget à valider](BUDGET.md) · [Jalons et arbitrages 4A](../../docs/gestionnaire-pilotes/qualification.md)
