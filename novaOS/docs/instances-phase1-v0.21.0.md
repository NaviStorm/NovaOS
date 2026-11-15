# NovaOS v0.21.0 · qualification de la phase 1 des instances

Branche de développement `codex/instances-0210`, issue de `3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e`. Les manifestes livrés restent à NovaOS v0.20.0, SDK nova-system v0.2.0 et NovaFS v0.9.13 P9. Le périmètre est le lancement d’applications du catalogue et la durée de vie de leurs ressources. [Contrat d’architecture](reconciliation-v0300/contrat-instances-services.md)

## Interface interne expérimentale

Les arguments sont trois entiers natifs de 64 bits. Le statut et la valeur sont retournés dans les registres habituels de l’ABI v1. Les droits restent dans les tables protégées du noyau. Le chemin ancien de la CLI continue à transmettre son en-tête à `SPAWN`.

| Porte | Arguments | Effet autorisé |
|---|---|---|
| `INFO` 0x10070 | sélecteur, 0, 0 | Lire son incarnation (0) ou son installation (1). Un thread secondaire peut lire les deux. |
| `WORKER` 0x10071 | incarnation propre, argument, 0 | Le thread principal d’un programme autorisé crée un travailleur du même ELF. Retourne un droit de collecte. Un droit déjà occupé donne `LIMIT`. |
| `STOP` 0x10072 | droit de suivi, incarnation offerte, 0 | Arrêter le groupe consenti et révoquer ses droits. La collecte du principal reste possible une seule fois. |
| `OFFER_STOP` 0x10073 | incarnation propre, 0, 0 | Un principal applicatif accepte le contrôle par le détenteur actuel de son droit de suivi. Le paquet système et le shell ne proposent pas cette offre. |
| `WATCH` 0x10074 | étape, incarnation, 0 | Le gestionnaire invité déclare le profil multiprocessus à l’étape 0, puis attend la restitution finale à l’étape 1. Le rendez-vous vérifie son rôle et l’incarnation liée. |
| `OFFERED` 0x10075 | droit de suivi, 0, 0 | Lire l’incarnation du principal suivi uniquement après son consentement. Le PID ou un droit d’observation de session est insuffisant. |

L’entrée de catalogue 16 permet de lancer l’application de référence depuis un client possédant `LAUNCH`, par `SPAWN(16, en-tête, 40)` avec cinq mots nuls. Le principal crée et collecte un enfant sans flux terminal ni vue de fichiers. L’indice 15 reste réservé au témoin SDK dans son profil de qualification. Chaque profil applicatif historique conserve son identité et ses droits d’exécution existants. [Extension interne](../abi/instances.rs) · [Application de référence](../user/instance_sample.rs)

## Recette reproductible

Depuis `novaOS/`, sur le Mac ARM64 disposant des cibles Rust Darwin et des deux cibles NovaOS, de Rosetta et des émulateurs QEMU du projet :

```sh
make ARCH=aarch64 instances-modele-test instances-test threads-test lifecycle-test elf-test ipc-races cli-test smp-test
make ARCH=x86_64 instances-modele-test instances-test threads-test lifecycle-test elf-test ipc-races cli-test smp-test
(cd .. && python3 -B distribution/scripts/sdk_rust.py --check)
```

Le modèle hôte vérifie les transitions, la réutilisation d’un slot après destruction, la saturation et les plafonds du manifeste. Le banc invité passe par le vrai lanceur avec une application en espace utilisateur. Un appel d’un thread secondaire à `SPAWN` est refusé et un second lancement avec le droit de suivi encore occupé échoue. Le banc crée ensuite deux familles concurrentes de la même application, puis relance l’application pour deux scénarios distincts. Le contrôleur injecte les conditions concurrentes et les placements CPU. Les appels d’identité, d’arrêt autorisé, de notification et de collecte s’exécutent depuis les processus utilisateurs. [Sonde invitée](../kernel/src/sched/process/ipc/instance_probe.rs) · [Vérificateur hôte](../tests/instances.py)

| Mode | Événement | Observation exigée |
|---|---|---|
| 0 | Sortie normale du principal avec travailleur actif | Collecte du principal avec 0. L’enfant actif et son thread sont retirés. La terminaison d’un premier enfant, collecté avec 41, avait laissé le principal actif. |
| 1 | Arrêt forcé pendant un calcul | Mauvaise incarnation refusée. Appel autorisé sur CPU0, calculateur sur le dernier CPU. Deux descendants IPC testés depuis un processus extérieur deviennent inutilisables. La racine indépendante reste utilisable. Collecte avec 125 et refus du handle périmé. |
| 2 | Faute matérielle du principal en mode utilisateur | Exception observée, collecte d’une faute et retrait du groupe. |
| 3 | Perte injectée du gestionnaire lié | Le banc termine le gestionnaire pendant l’activité de la famille. Le principal est collecté avec 125 et le groupe est nettoyé. L’injection utilise la primitive noyau de terminaison. |

Le banc mesure les progrès du thread calculateur d’abord sur CPU0 puis sur le dernier CPU. Après les quatre familles, il réutilise le même slot de processus et vérifie deux PID et deux incarnations distincts. Le PID public est générationnel et n’est pas recyclé pendant le boot. Le compteur des cadres et la table des TCB doivent retrouver exactement leur référence initiale. Les enregistrements de destruction sont des numéros en mémoire et ne retiennent aucun cadre de l’application.

## Résultats du 15 novembre 2026

Code qualifié `49e907321e89909396835f3eaa8c978226d2e875`. Hôte macOS 27.2 (26B123), ARM64, Rust 1.99.1, QEMU 11.1.3 en TCG. Les 24 fins de famille ont atteint la destruction. Le tableau donne le nombre de cadres libres avant lancement et après restitution, identique dans chaque cellule.

| Invité | 1 vCPU | 2 vCPU | 4 vCPU | TCB avant et après |
|---|---:|---:|---:|---|
| aarch64 | 62536 | 62502 | 62453 | 2 / 3 / 5 |
| x86_64 | 63097 | 63040 | 62958 | 2 / 3 / 5 |

Sur chaque architecture, trois tests unitaires passent sur l’hôte (ARM64 natif ou x86_64 sous Rosetta). Les six démarrages de régression threads et les six démarrages SMP passent. Les suites cycle de vie, chargeur ELF et courses IPC passent sur les deux architectures. La CLI passe sur quatre couples architecture / secteur (512 et 4 096 octets), avec terminal, ELF isolés, révocation, faute et remontages G43/G44. La vérification de l’archive publique du SDK réussit. Les répertoires du moteur NovaFS, du SDK distribué et des composants CLI ne présentent aucun changement par rapport à la base.

Les journaux série complets des six essais d’instance sont archivés. Le fichier JSON indique leur SHA-256, les PID, les incarnations au format décimal exact, le CPU cible de chaque scénario, le bilan des cadres et les résultats des régressions. [Résultats détaillés](validation/instances-phase1-v0.21.0.json) · [Six journaux QEMU](validation/instances-phase1-v0.21.0-journaux.tar.gz)

## Limites et arbitrage

Le manifeste fixe les identités de l’installation embarquée. L’enrôlement d’applications installées dynamiquement, la provenance d’une réinstallation et l’identité utilisateur durable requièrent leur propre contrat. Un gestionnaire temporaire consomme un slot de processus et un espace d’adressage pendant l’admission. Il reste résident pour une famille multiprocessus. Les tables existantes bornent la concurrence à 32 slots partagés avec les autres tâches. La qualification porte sur un enfant suivi à la fois et sur 1, 2 et 4 vCPU.

Les preuves couvrent QEMU et les tests hôtes Darwin. Les régressions CLI exercent le stockage normal, tandis que la qualification historique reste la référence pour les coupures, la quarantaine DMA, les pilotes matériels, NovaFS complet et UTM. Les mesures de cadres ci-dessous portent sur la restitution après collecte, sans objectif de débit ni de pic mémoire. [Qualification du retrait DMA](materiel-phase4-v0.18.0.md) · [Qualification historique des threads et du SDK](rust-sdk-phase2-v0.20.0.md)

Avant de matérialiser `tmp`, la MOA/MOE doit choisir comment attester l’époque de démarrage sur stockage et comment isoler puis purger un orphelin. Pour `cache` et `permissions`, elle doit valider les identités durables des installations et des utilisateurs, l’autorité de décision et la politique de purge. Les numéros d’incarnation de cette phase ne peuvent pas, seuls, servir de propriétaire persistant.
