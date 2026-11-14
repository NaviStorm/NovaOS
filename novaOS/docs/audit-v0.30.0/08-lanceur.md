# Stratégie proposée du lanceur central de qualification

Le point d'entrée proposé prépare et appelle les bancs existants. La MOA/MOE doit valider son périmètre et ses seuils de publication avant développement. Aujourd'hui, Make sélectionne des familles de tests, le banc SMP sélectionne des cas dans une cellule et le laboratoire NovaFS vérifie ses binaires Darwin. Les 108 groupes SMP référencés dans v0.17.0 couvrent une sélection identifiée. Les 36 coupures et 4 000 abandons appartiennent à des campagnes distinctes. [Makefile](../../Makefile) · [Sélection SMP](../../tests/smp_phase4_regressions.py) · [Validation Darwin](../../../novaFS/lab/p51-validation-suites.py) · [Résultats versionnés](../validation/reservations-dma-v0.17.0/regressions.json)

## Contrat de sélection proposé

| Dimension | Valeurs et effet |
|---|---|
| Architecture | `aarch64` ou `x86_64`, répétable. L'architecture désigne le binaire testé. L'environnement est distinct, par exemple `qemu`, `utm`, `darwin-natif`, `darwin-rosetta` ou un futur `materiel`. La paire x86_64/Darwin-Rosetta exige un exécutable Mach-O x86_64 vérifié. |
| Fonctionnalité | Identifiant du catalogue, par exemple `nsh`, `vi`, `vfs`, `novafs-p9`, `dma`, `ordonnanceur`. Les fonctions expérimentales portent une option dédiée et le manifeste du binaire correspondant. Le plan ordinaire utilise le stockage classique et le polling. |
| Famille | `fonctionnel`, `securite`, `smp`, `ipc`, `endurance`, `benchmark`. Une même cellule peut porter plusieurs étiquettes. Un essai d'endurance ou de performance conserve ses propres seuils, observations et durée. |
| Paramètres d'instance | Architecture hôte/invitée, accélérateur, modèle de CPU, topologie et nombre de vCPU, RAM, secteur 512/4 096, support raw/QCOW2, profil de firmware, sondes, graine, répétition. Une géométrie appartient à l'instance qui l'a réellement exercée. |
| Plan et périmètre | `liste` décrit les cas et dépendances, `plan` résout les combinaisons sans exécuter, `executer` réalise le plan, `rejouer` repart d'un manifeste identifié. Les filtres de dimensions différentes s'intersectent. Plusieurs valeurs d'une dimension s'unissent. Dédupliquer par identifiant, profil et paramètres. Rejeter un identifiant inconnu ou une sélection vide avec un diagnostic. |

Un profil de publication fige la liste des cellules obligatoires, leur applicabilité et les critères du jalon. Un filtre ad hoc produit un rapport de sélection partielle. Les topologies 1, 2 et 4 vCPU des bancs SMP actuels restent distinctes des futures séries 8, 16 et 32, à ajouter après vérification de la capacité du noyau et des oracles. [Topologie](../../kernel/src/smp.rs) · [Banc SMP actuel](../../tests/smp_phase4_regressions.py)

## Portes d'entrée conservées

| Famille | Commande ou point d'entrée prenant le verdict | Observations à retenir |
|---|---|---|
| Fonctionnel | [Makefile](../../Makefile), [ELF](../../tests/elf.py), [espace de noms](../../tests/namespace.py), [scripts](../../tests/nsh_scripts.py), [écran de vi](../../tests/vi_ecran.py) | Statut, contenu des fichiers, écran et position du curseur suivant le cas. La validation d'un rendu dans UTM exige sa propre observation visuelle. |
| Sécurité | [Droits et révocation DMA](../../tests/dma_propriete.py), [erreurs sur le chemin classique](../../tests/erreurs_stockage_classique.py), [phases CoW](../../tests/stockage_phases.py) | Refus, aucune écriture non autorisée, comptabilité de la quarantaine, deux générations et octets après remontage. |
| SMP | [Régressions SMP par cellule](../../tests/smp_phase4_regressions.py), [phase 2](../../tests/smp_phase2.py) | Rendez-vous CPU, arrêt coordonné, préemptions, migrations et registres FP/SIMD des profils disponibles. |
| IPC | [Contrats et courses](../../tests/ipc.py), [équité](../../tests/ipc_equite.py), [réponses de stockage](../../tests/reponses_stockage.py) | Capacités, générations, identités des réponses, délais et droits du client. Inclure perf ou endurance seulement si ces familles sont sélectionnées. |
| Endurance | [Réservations](../../tests/reservations_endurance.py), [cycles de sessions](../../tests/endurance_sessions.py), [suites NovaFS](../../../novaFS/lab/p51-validation-suites.py) | Nombre exact d'abandons, ressources en régime chaud et après fermeture, oracles différentiels et erreurs. Traiter les injections historiques sur les ancres comme un banc à part. [Reproduction des injections](../../../novaFS/docs/novafs-p9-ancres-dechirees.md) |
| Benchmarks | [Mesure de publication](../../tests/integration_main_performances.py), [débit IPC](../../tests/ipc_phase4_performances.py) | Temps brut jusqu'au succès durable, IPC avec contrôles, F1/F2, pic mémoire, latences individuelles, p50, p95, p99, maximum et effectif. Documenter si un quantile se confond avec l'extrême par faible effectif. |

**Commande existante**, depuis la racine, pour le test de scripts sur un invité AArch64.

```sh
make -C novaOS ARCH=aarch64 nsh-scripts-test
```

**Invocation proposée**, à titre de spécification. La cellule `nsh-scripts` du catalogue déléguerait à la commande ci-dessus et vérifierait son oracle existant.

```sh
nova-test executer --architecture aarch64 --environnement qemu \
  --fonctionnalite nsh --famille fonctionnel --cas nsh-scripts
```

Une entrée du catalogue associe l'identifiant stable à la liste d'arguments de la commande, au répertoire courant, aux variantes supportées, aux prérequis, à l'oracle et aux artefacts attendus. L'appel manuel, ses arguments, ses délais et son code retour restent disponibles. Des dépendances explicites, par exemple compiler le lecteur hôte puis le noyau d'un profil avant la cellule, permettent un plan exécutable à partir d'un checkout neuf. [Prérequis de construction](../../kernel/build.rs) · [Reconstruction par profil](../../tests/reconstruire_es_0166.py)

## Exécution, isolation et preuves

1. **Préparer.** Résoudre les chemins dans un export du SHA demandé. Relever le diff si le demandeur choisit de tester un arbre modifié, avec identification explicite de cet état. Vérifier toolchain, version du moteur, firmware, RAM et espace libre. Comparer les SHA du noyau, des fixtures et du lecteur aux identités du plan. Attribuer à chaque tentative un répertoire privé créé exclusivement. [Provenance des binaires](../../tests/integration_main_performances.py)
2. **Isoler.** Employer uniquement les supports synthétiques créés ou clonés dans la campagne. Conserver et hacher la base en lecture seule puis lancer la VM sur sa copie. Un seul détenteur par disque mutable. Isoler également sockets QMP, ports, chemins ISO, profils UTM, sorties série et noms de VM. Déclarer exclusifs les essais de latence hôte, de GUI UTM et de matériel physique. Le nettoyage vise uniquement les fichiers de la tentative. [Images fictives](../../tests/preuves_stockage.py) · [Préparation des cellules SMP](../../tests/smp_phase4_regressions.py)
3. **Exécuter.** Appeler la commande d'origine avec les seules variables nécessaires. Conserver identifiants de l'hôte, versions, modèle CPU, accélérateur, expériences actives et limites de chaque oracle. Un délai de sûreté protège une cellule bloquée. La durée d'endurance annoncée fait partie du plan et doit être accomplie. Sur interruption du lanceur, récolter les journaux et fermer uniquement ses enfants. [Enregistrement d'une VM](../../tests/preuves_stockage.py)
4. **Vérifier.** Collecter le retour, les marqueurs attendus et l'oracle indépendant. Lors d'une coupure de VM, figer l'image arrêtée, hacher la copie de reprise avant démarrage, contrôler ses ancres et relire les octets. Le comportement du SSD après perte d'alimentation exige un essai physique distinct. Pour un test visuel, attacher l'écran et l'observation demandée. [Qualification stockage](../qualification-stockage-v0.17.0.md) · [Tests visuels existants](../vi-v0.16.2.md)
5. **Conserver.** Produire un manifeste de campagne et un manifeste de tentative, incluant sélection, nécessité pour le profil, prérequis constatés, commit du banc et du binaire, commande, versions, UTC, durée monotone, code retour, oracle, compteurs, fichiers relatifs et SHA-256. Conserver les séries brutes, les supports et leurs journaux lors d'un échec. Distinguer captures d'origine et fichiers dérivés. Un rejeu crée une tentative liée à l'original. Expurger secrets et données privées avant un éventuel transfert d'archive. [Collecteur existant](../../tests/preuves_stockage.py) · [Manifestes de qualification](../validation/reservations-dma-v0.17.0/regressions.json)

## Verdict et critères d'adoption

- **Réussi** exige la fin de la commande, un statut conforme, l'oracle et toutes les preuves requises. **Échoué** couvre un résultat contraire à l'oracle. **Incomplet** couvre interruption, expiration ou perte de preuve. **Non exécuté** documente précisément le prérequis absent, par exemple UTM indisponible, Rosetta manquante ou banc d'alimentation physique absent. **Non applicable** exige une règle explicite du catalogue, par exemple NEON sur x86_64. Toute cellule obligatoire échouée, incomplète ou non exécutée empêche un verdict global de qualification.
- Afficher les dénominateurs par famille, architecture et environnement, ainsi que les cas ignorés dans les suites Cargo. Une campagne antérieure utilisée comme preuve conserve son SHA, ses résultats et la démonstration que le code exercé est identique. Elle compte comme vérification d'une qualification antérieure. Garder les valeurs défavorables et les tentatives successives dans l'ordre. [Résultats et preuves antérieures](../reservations-dma-v0.17.0.md)
- Pour valider le lanceur, comparer sur une image synthétique les verdicts des appels directs et orchestrés, avec le même SHA sur les deux architectures. Exercer une cellule réussie, un échec volontaire de l'oracle, un prérequis absent, une interruption, un rejeu et deux cellules concurrentes. Vérifier l'identité des données de référence, l'indépendance des répertoires, l'absence de succès global indu et l'intégrité des manifestes après archivage.
