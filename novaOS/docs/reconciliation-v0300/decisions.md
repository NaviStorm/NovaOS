# Registre des décisions d’architecture

Registre arrêté au 15 novembre 2026. Ses deux tables se lisent avec le même identifiant. La première donne le texte et l’autorité, la seconde indique ce que la décision engage et comment en vérifier la provenance. La date est celle de la publication historique ou de l’inscription de la proposition, en heure de Paris.

Le mandat MOA *Arbitrages architecturaux et préparation de la feuille de route officielle v0.20.0–v0.30.0* comporte des propositions MOE aux §§2–4, des engagements à conserver au §7 et une autorisation de publication **documentaire** au §10. Son empreinte SHA-256 est `1a64bf1996067e3388a55c2115a1ce15bc4e4e37f6a4eb4c56b8ffb1f28d39c2`. Le renvoi « mandat » désigne cette pièce. Le dépôt publie les références des pièces antérieures dans l’[historique](historique.md).

## Texte soumis et autorité

| Identifiant | Date | Sujet | Décision ou proposition | Origine | Statut |
|---|---|---|---|---|---|
| ARC-001 | 2026-11-14 | SMP | Conserver la publication SMP x86_64/AArch64 et FPU/SIMD dans les profils décrits. | MOA, mandat de fusion SMP | Publié |
| ARC-002 | 2026-11-15 | Matériel | Conserver la publication des fondations ACPI/PCIe, des ressources pilotes et du profil VT-d expérimental. | MOA, mandat de fusion matérielle | Publié |
| ARC-003 | 2026-11-15 | NVMe | Conserver la publication du pilote isolé et du chemin NovaFS sur le profil Q35 qualifié. | MOA, mandat de fusion NVMe | Publié |
| ARC-004 | 2026-11-15 | Première stable et bureau | Conserver v0.30.0 comme première stable et conserver NovaWM et NovaUI dans la planification. | MOA, mandat §7 | Approuvé |
| ARC-005 | 2026-11-15 | Reports | Conserver CPython et l’hyperviseur natif après la première stable. | MOA, mandat §7 | Approuvé |
| ARC-006 | 2026-11-15 | Compatibilité stockage | Préserver le P9 livré pendant cette préparation. Toute évolution incompatible nécessite une proposition et un arbitrage distincts. | MOA, mandat §5 et contrats du dépôt | Approuvé |
| ARC-007 | 2026-11-15 | Plateforme | Qualifier QEMU sur les deux architectures et un bureau natif sur chacune. Garder le Tecra A50-J prioritaire, avec preuves physiques progressives et critères distincts pour le Live USB et l’installation. | MOE, mandat §2 | Proposé |
| ARC-008 | 2026-11-15 | Bibliothèque Rust | Donner la priorité à l’API minimale statique pour les deux architectures sur interfaces publiées. Fixer le profil d’admission d’un programme construit hors du dépôt. | MOE, mandat §3 | Proposé |
| ARC-009 | 2026-11-15 | Threads et runtime | Séparer l’API minimale, le runtime externe et la compatibilité étendue avec `std`. Spécifier les threads utilisateur avant de décider leur ABI et leur livraison. | MOE, mandat §3 ; détail du contrat proposé par Codex | Proposé |
| ARC-010 | 2026-11-15 | Autorité sur les données | Distinguer les identités durables, les ACL persistantes et les capacités dérivées. Refus par défaut. Séparer administration, octroi des droits et lecture du contenu. | MOE, mandat §4 | Proposé |
| ARC-011 | 2026-11-15 | Persistance des ACL | Soumettre séparément le choix du rattachement entre version des objets et politique. Exiger un refus sur version indémontrable et un traitement explicite des anciens lecteurs. | Codex, instruction d’étude MOA §5 | Proposé |
| ARC-012 | 2026-11-15 | Clés et récupération | Séparer secours d’identité et récupération cryptographique. Choisir le détenteur des clés et la politique de secours avant de promettre un accès après perte. | MOE pour la séparation, options instruites par Codex au §4 | Proposé |
| ARC-013 | 2026-11-15 | Séquence future | Reprendre les treize tranches du scénario documentaire, compléter leurs dépendances et soumettre les affectations à la MOA. | Codex, consolidation de la proposition soumise à la MOE | Proposé |

## Justifications, conséquences et validation

| Identifiant | Justification | Conséquences | Versions affectées | Références | Validation MOA |
|---|---|---|---|---|---|
| ARC-001 | Fusion et rapports concordants. | Garder le numéro et la portée 1/2/4 vCPU en QEMU. Les threads applicatifs restent ouverts. | v0.17.0 | [Fusion SMP](../fusion-smp-v0.17.0.md) | Fusion autorisée, publication reconduite au §7. |
| ARC-002 | Attribution et retrait démontrés sur machines de laboratoire. | Rattacher les garanties à chaque topologie. | v0.18.0 | [Fusion matérielle](../fusion-materiel-v0.18.0.md) | Fusion autorisée, publication reconduite au §7. |
| ARC-003 | Source fusionnée et qualification NVMe identifiée. | Conserver le profil explicite pour les E/S NVMe. Le numéro de l’installateur UTM reste v0.17.0. | v0.19.0 | [Qualification NVMe](../nvme-phase3-v0.19.0.md) · [Distribution](../../../distribution/README.md) | Fusion autorisée, publication reconduite au §7. |
| ARC-004 | Objectif de produit réaffirmé. | Préserver fenêtres et widgets dans les travaux à instruire. Le placement des deux composants reste soumis à décision. | v0.30.0 ; numéros graphiques ouverts | [Historique des plans](historique.md) | Explicite, §7, pour l’objectif et la conservation des deux composants. |
| ARC-005 | Bornes explicites du produit stable. | Une réouverture exige un nouveau mandat. | Après v0.30.0 | [Registre des reports](arbitrages.md#reports-et-fonctionnalités-conditionnelles) | Explicite, §7. |
| ARC-006 | Le volume autonome et sa récupération ont des lecteurs et des garanties déjà publiés. | Préserver images, refus de montage ambigu et garanties de publication. Une ACL portable ou un nouveau lecteur exige ses propres preuves. | NovaFS v0.9.13 P9, transversal | [Contrats NovaFS](../../../novaFS/AGENTS.md) | Explicite, §5, sur la préservation de P9. |
| ARC-007 | Les preuves QEMU, UTM et matériel physique ont des périmètres distincts. | Fixer les deux cibles du SDK et le parcours graphique final. Un engagement physique intégral ajoute pilotes, médias dédiés et qualification. | Base v0.20.0 ; v0.28.x–v0.30.0 proposés | [Plateforme](contrat-plateforme.md) | À statuer sur la référence et le caractère bloquant du parcours physique. |
| ARC-008 | Le catalogue actuel permet des programmes isolés et une bibliothèque interne ; une API publique doit avoir un contrat durable. | Geler uniquement les opérations et erreurs vérifiables de ce profil. Une nouvelle primitive exige son mandat. | v0.20.0 proposée | [Bibliothèque et threads](contrat-rust-threads.md) | À statuer sur le périmètre et le lancement d’une première tranche. |
| ARC-009 | SMP ne suffit pas à gérer plusieurs piles, TLS, sorties et droits dans un processus. | Prévoir qualification de l’isolation, de la terminaison et du réemploi sur les deux ISA avant exposition. | Tranche v0.21.0 conditionnelle ; `std` sans numéro | [Contrat d’exécution](contrat-rust-threads.md) | À statuer sur le profil applicatif. Toute extension de l’ABI se décide séparément. |
| ARC-010 | La possession d’un handle local n’établit pas une identité durable. | Borner le SDK aux pouvoirs délégués. Fixer les règles d’ouverture, de retrait et de traçabilité avant une session multiutilisateur. | Contrainte de conception dès v0.20.0, fonctions sans numéro | [Autorisations](contrat-autorisations.md) | À statuer sur le principe, la frontière de confiance et l’engagement du SDK. |
| ARC-011 | Une association périmée peut rendre les droits d’un objet supprimé ou d’une vue restaurée. | Évaluer la cohérence après coupure, restauration et import. Durant l’étude, les profils actuels gardent leur portée. | Sans affectation ; avant toute promesse d’ACL durables | [Options de persistance](dependances-p9.md) | À statuer lors du choix de persistance. Aucune migration autorisée. |
| ARC-012 | Récupérer un compte après perte d’un facteur ne restitue pas une clé de déchiffrement perdue. | L’option sans séquestre peut rendre des données irrécupérables. Un secours indépendant introduit une autorité à protéger. | Sans affectation ; avant les engagements sur les secrets | [Administration et récupération](contrat-autorisations.md) | À statuer sur adversaire, dépositaire éventuel et cas de perte définitive. |
| ARC-013 | Les affectations historiques individuelles v0.20–v0.24 restent inconnues. | Présenter l’effort, les prérequis et le sort de tout lot déplacé avant ratification. | v0.20.0–v0.30.0 | [Scénario consolidé](proposition.md) · [Écarts historiques](historique.md) | Ratification des affectations en attente. |

## Mise à jour du registre

Les valeurs admises sont **Proposé**, **Approuvé**, **Rejeté**, **Implémenté**, **Qualifié** et **Publié**. Pour approuver ou rejeter une proposition, consigner sa révision Git, la date, l’auteur de la décision, son périmètre et le renvoi à l’accord explicite. Une qualification précise le profil, la preuve et ses limites. Une publication identifie le commit, et le manifeste si un binaire a été distribué. L’existence d’un commit documentaire atteste la rédaction du texte.

Conserver l’historique des changements de statut. Pour déplacer un jalon approuvé, inscrire une nouvelle proposition qui cite l’identifiant précédent, le travail déplacé, les conséquences et son successeur éventuel. Appliquer le déplacement à la feuille de route officielle après arbitrage MOA/MOE traçable. Un accord partiel laisse les points restants au statut **Proposé**.
