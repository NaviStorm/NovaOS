# Bibliothèque système Rust et threads utilisateur · v0.20.0, phase 1

Cette branche de développement part de la fusion documentaire `2bcb64575e396790089e3078d443b63d69ac05d7`. Le noyau publié porte la version v0.19.0. Le numéro de la bibliothèque source est 0.1.0, celui de la porte native est 1 et celui du protocole fichier est 2. L’admission du témoin au catalogue est propre à la construction de qualification. [Mandat et compatibilité](reconciliation-v0300/contrat-rust-threads.md)

## Ce que réutilise la phase 1

Le chargeur contrôle le type, l’ISA, le point d’entrée et les permissions des segments d’un ELF64 statique. Le lancement continue d’installer une table locale de capacités, une racine mémoire et une pile principale. L’ordonnanceur SMP possède déjà les générations de threads, les piles superviseur, la préemption, les affinités et la commutation des registres. La phase 1 associe plusieurs de ces contextes à un même processus. Le protocole fichier continue de s’exécuter entre services sur des canaux délégués. [Chargeur](../kernel/src/memory/user/elf.rs) · [Ordonnanceur](../kernel/src/sched/mod.rs) · [Capacités IPC](ipc-security.md)

## API applicative

La crate `nova-system` est une bibliothèque Rust `no_std` sans allocateur imposé ni dépendance Cargo. Les applications l’emploient sous le nom `nova_system`. Un appel faillible renvoie `Result<T, Error>` et conserve le code numérique du noyau ou du service, y compris un code inconnu. [Sources du SDK](../libs/nova-system/src/lib.rs)

| Module | Opérations implémentées et droits |
|---|---|
| `process`, `clock` | PID courant, terminaison du processus, attente avec la clé de suivi, secondes UTC de la RTC. |
| `memory` | Interrogation de la borne du tas, redimensionnement par comparaison avec l’ancienne borne. Le changement de borne est une opération Rust `unsafe` réservée au propriétaire de l’allocateur. |
| `ipc` | Endpoint possédé, fermeture explicite ou par `Drop`, duplication avec masque réduit, délégation dérivée, révocation des descendants, envoi de message et réception. Les handles et générations sont résolus dans le processus appelant. |
| `files` | Ouverture d’une session à partir de deux endpoints explicitement délégués, lecture des vues déjà déléguées, ouverture relative au répertoire autorisé, lecture, écriture, synchronisation, énumération par fragments et fermeture. Le client conserve l’exclusivité du dialogue sur cette paire. Une fermeture explicite permet d’observer l’erreur du service. |
| `thread`, `sync` | Création et retour d’une tâche, jonction, détachement, demande et observation d’annulation, TLS, cession du CPU, mutex et attente/notification sur mot atomique. Le contrat source de ces API est expérimental. |
| `raw` | Constantes et structures exactes de l’ABI et porte brute `unsafe`. Les interfaces ordinaires empruntent les tampons le temps de l’appel. |

L’extension se limite aux applications compilées pour une cible nue NovaOS. Le fichier ELF utilise les conventions du chargeur existant. Les appels de lancement général d’un fichier, le TLS du compilateur via segment `PT_TLS`, le runtime pour `std::thread` et les SDK C/C++ restent des travaux futurs. Les contrôles d’accès aux fichiers restent dans le service propriétaire de la session. [Client fichier](../libs/nova-system/src/files.rs) · [Protocole des trames](../abi/files.rs)

## Contrat des appels natifs

Le lanceur applicatif accorde l’extension à la création depuis son catalogue. Un service ou un pilote démarré par son lanceur privé reçoit `DENIED` sur la plage entière, y compris pour une attente ou une demande de TLS. Les appels historiques `THREAD_ID` et `THREAD_YIELD` conservent leur sémantique. Ce contrôle maintient le titulaire unique des baux matériels pendant les commandes en cours.

Tous les sélecteurs ci-dessous utilisent les mêmes portes et conventions de retour que l’ABI v1. Les arguments réservés valent zéro. Un noyau v0.19.0 renvoie `UNKNOWN_CALL` à un numéro inconnu de la version 1. [Sélecteurs](../abi/native.rs) · [Dispatch](../kernel/src/sched/process/syscall.rs)

| Sélecteur | Arguments | Effet |
|---|---|---|
| `0x10060 THREAD_CREATE` | entrée RX, argument scalaire, pages de pile | Crée un membre du même processus avec l’affinité du créateur. Renvoie son droit local de jonction. |
| `0x10061 THREAD_EXIT` | résultat, 0, 0 | Termine le membre courant. Le dernier membre vivant termine le processus. |
| `0x10062 THREAD_JOIN` | droit, 0, 0 | Attend la collecte puis renvoie le résultat. Une seule jonction consomme le droit. |
| `0x10063 THREAD_DETACH` | droit, 0, 0 | Abandonne le droit à la collecte du résultat. Le thread vivant continue. |
| `0x10064 THREAD_CANCEL` | droit, 0, 0 | Demande l’annulation coopérative au membre vivant de la génération visée. |
| `0x10065 THREAD_CANCELLED` | 0, 0, 0 | Observe la demande pour le thread appelant. |
| `0x10066 THREAD_TLS` | 0, 0, 0 | Adresse de la page RW/NX propre au membre appelant. |
| `0x10067 THREAD_WAIT` | adresse RW alignée sur 8, valeur attendue, délai relatif | Attend si le mot garde sa valeur. Changement préalable → `WOULD_BLOCK`, échéance → `TIMED_OUT`, annulation → `CANCELLED`. Zéro teste immédiatement et `u64::MAX` permet une attente sans échéance. |
| `0x10068 THREAD_NOTIFY` | même adresse, maximum de réveils positif, 0 | Réveille au plus ce nombre d’attentes du même processus et renvoie le nombre effectif. |

`thread::Task::run(usize) -> usize` reçoit l’argument transmis en RDI ou X0. Le trampoline du SDK convertit le retour Rust en `THREAD_EXIT`. Une faute utilisateur ou `PROCESS_EXIT` clôt tout le processus. Un numéro de thread lu dans un autre processus ne confère aucun droit. Les ports matériels, le terminal et les droits privés du lanceur gardent leurs restrictions. [Cycle de vie](../kernel/src/sched/process/threads.rs)

Le mutex associe un mot atomique Acquire/Release à une garde Rust exclusive. Quand il est occupé, la comparaison et l’inscription de l’attente sont protégées par la même garde d’ordonnancement que la notification. La libération publie la valeur puis réveille les concurrents, qui vérifient à nouveau le prédicat. Une notification ne mémorise aucun crédit. Le délai est exprimé en ticks du timer NovaOS à 250 Hz. L’annulation reste coopérative. Un programme doit libérer ses gardes avant de terminer explicitement un thread. Cette phase ne définit pas de mutex robuste permettant de reprendre les données d’un détenteur mort. [Synchronisation](../libs/nova-system/src/sync.rs)

## Mémoire, collecte et isolation

Le processus partage ses pages utilisateur entre ses threads, y compris le tas. Chaque membre secondaire possède une pile de 1 à 64 pages de 4 Kio, deux gardes non mappées, une page TLS initialisée à zéro et une pile superviseur. Le principal obtient son TLS au premier usage. Les 512 cases TLS de la bibliothèque contiennent des mots machine, sans destructeur applicatif automatique. L’isolation matérielle porte sur les processus. Deux threads d’un même processus partagent leur espace de confiance. [Gestion des plages](../kernel/src/memory/user/threads.rs)

Le noyau borne à huit les piles secondaires simultanées et les droits de jonction par processus. La table d’ordonnancement existante contient 32 slots pour l’ensemble du système. Avec `user-fpu`, les zones de sauvegarde architecturales par slot mesurent 528 octets sur AArch64 et 832 octets sur x86_64. Le TLS occupe 4 Kio et les deux gardes consomment uniquement de l’espace virtuel. Chaque profil garde sa politique d’admission des instructions FPU/SIMD. La collecte intervient sur une autre pile après la dernière commutation du membre. Le retrait invalide les traductions avant de rendre les cadres. Les registres utilisateur d’entrée reçoivent uniquement l’argument explicitement transmis. [Contexte AArch64](../kernel/src/arch/aarch64/context.rs) · [Contexte x86_64](../kernel/src/arch/x86_64/context.rs)

## Reproduire le témoin externe

Depuis la racine du monorepo, avec les cibles Rust nues installées, QEMU, le firmware UEFI du banc et xorriso disponibles.

```sh
make -C novaOS ARCH=aarch64 threads-test
make -C novaOS ARCH=x86_64 threads-test
```

Le banc crée un projet Cargo indépendant dans un répertoire temporaire extérieur au monorepo, y compile l’application avec le SDK comme dépendance de chemin, puis vérifie l’ELF et le sélectionne explicitement avec `NOVA_THREAD_PROBE_ELF` pour la feature `thread-probe`. Le profil de qualification observe l’application et les ressources dans une VM neuve à chaque géométrie. Le programme de référence crée deux tâches avec les valeurs 7 et 11. Chacune met à jour 24 fois le même entier sous mutex, conserve son propre TLS et rend son résultat par jonction. La somme attendue est 432. [Source de l’exemple](../tests/external-threads/main.rs) · [Banc de reproduction](../tests/threads.py)

## Régressions ciblées

Les régressions utilisent le noyau ordinaire de chaque architecture. Les profils ELF, cycle de vie et IPC conservent leurs clients mono-thread. Le scénario ABI fait recycler la page d’un résultat `PROCESS_WAIT` avant la copie et vérifie les octets sentinelles, puis la collecte avec le même droit. Le trajet fichier exerce le transport partagé extrait pour le SDK. Il écrit sur un volume P9 jetable, le vérifie avec l’outil hôte et le relit dans une seconde VM en lecture seule.

```sh
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
for arch in aarch64 x86_64
do
    make -C novaOS ARCH="$arch" elf-test lifecycle-test ipc-test ipc-races
    (
        cd novaOS || exit 1
        NOVA_ARCH="$arch" PYTHONPATH=tests python3 - <<'PY'
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from demarrage import abi_scenario
from fichiers import Bench, ROOT, normal

arch = os.environ["NOVA_ARCH"]
abi_scenario(arch, 256)
with TemporaryDirectory(dir=ROOT / "build") as directory:
    normal(Bench(Path(directory), arch, 512))
PY
    )
done
```

La suite unitaire de la bibliothèque exerce les contrôles qui précèdent la porte noyau sur Darwin ARM64 et sur x86_64 sous Rosetta. Les appels natifs et les changements de privilège sont exercés dans les invités QEMU.

```sh
cd novaOS
for cible in aarch64-apple-darwin x86_64-apple-darwin
do
    RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo test \
        --locked --offline -p nova-system --lib --target "$cible"
done
```

## Résultats du 15 novembre 2026

Le commit de code qualifié est `9eb14a82adb2f493d1aea49c505ef6934c6f3e8b`. Sous QEMU 11.1.3 TCG, chaque cellule a démarré un invité neuf et chargé la même application pour l’ISA concernée. Les deux nombres de préemptions correspondent aux deux threads de calcul du banc. Les migrations ont été provoquées par changement d’affinité pendant leur exécution. [Relevés, versions et empreintes](validation/rust-threads-v0.20.0.json)

| Invité | vCPU | Préemptions individuelles observées | Threads ayant migré | Cadres libres avant → après | Contextes d’ordonnancement restants |
|---|---:|---|---:|---|---:|
| AArch64 EL0 | 1 | 3 / 3 | 0 | 62 529 → 62 529 | 2 |
| AArch64 EL0 | 2 | 6 / 5 | 2 | 62 495 → 62 495 | 3 |
| AArch64 EL0 | 4 | 4 / 4 | 2 | 62 446 → 62 446 | 5 |
| x86_64 Ring 3 | 1 | 3 / 3 | 0 | 63 056 → 63 056 | 2 |
| x86_64 Ring 3 | 2 | 5 / 5 | 2 | 62 999 → 62 999 | 3 |
| x86_64 Ring 3 | 4 | 5 / 4 | 2 | 62 917 → 62 917 | 5 |

Les six invités ont rendu la somme 432 et les valeurs TLS individuelles 7 et 11. Le témoin a vérifié la conservation des registres de calcul et du mode d’arrondi, le refus d’une double jonction, l’épuisement des huit places, le détachement et l’annulation. La notification a réveillé successivement un, un puis zéro thread. Une faute matérielle a confirmé la garde de pile. La réallocation à la même adresse a retrouvé des pages mises à zéro. Le test de pénurie a fait échouer une allocation après deux cadres obtenus et comparé les compteurs de cadres et de tables après restitution.

La destruction d’un processus a collecté deux attentes sur mot, un appel IPC bloqué et un calcul actif. Le test inverse a collecté le principal pendant qu’un secondaire restait vivant, puis observé le résultat final 19. Le retrait d’une page destinée à une réception bloquée a produit `BAD_ADDRESS`. Le même ELF sous un lanceur privé s’est vu refuser les nouveaux appels tout en gardant les appels historiques. Un processus tiers connaissant le numéro d’un thread n’a pu ni le joindre ni l’annuler. [Contrôles en espace utilisateur](../tests/external-threads/checks.rs) · [Contrôleur et observations du noyau](../kernel/src/sched/process/ipc/thread_probe.rs)

| Contrôle complémentaire | Résultat observé |
|---|---|
| SDK sur Darwin ARM64 et x86_64 sous Rosetta | 3 tests unitaires réussis par architecture. Les deux ELF natifs du témoin mesuraient 66 680 octets sur AArch64 et 53 864 sur x86_64. |
| Chargeur et cycle de vie historiques | Profils `elf-test` et `proc-lifecycle` réussis sur les deux invités. Chaque campagne de cycle de vie a collecté 128 processus en 64 familles. |
| ABI et IPC historiques | `abi-test-256`, `ipc-test-256` et `ipc-races-256` réussis sur les deux ISA. La sonde ABI a observé la révocation et vérifié une nouvelle collecte après refus de la destination recyclée. |
| Fichiers sur NovaFS v0.9.13 P9 | Dans chaque invité, création, deux transactions, vérification indépendante de la génération 6, lecture en nouvelle VM et comparaison de l’image restée en lecture seule. Secteurs de 512 octets. Le témoin SDK a en outre vérifié le refus d’opérations fichier et répertoire sans l’autorité d’envoi requise. |
| Profils matériels | `materiel-lab,gicv3-lab` construit sur AArch64. `nvme-lab` construit et démarré sur Q35 x86_64 à 2 vCPU, secteurs de 512 octets. Deux sessions sur namespace virtuel ont livré six lectures, zéro écriture, zéro FLUSH et zéro faute DMA. SHA-256 avant et après `0bdeae2c9e25d26126f1d7ba12e4e9040fdde6879e979632fb09ff68a02e6458`. |

L’essai NVMe cible la frontière entre le catalogue applicatif et le pilote à titulaire unique. Pour le reproduire depuis `novaOS` sur un namespace virtuel neuf :

```sh
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
    --locked --offline --release -p novaos --features nvme-lab --target x86_64-unknown-none
python3 tests/nvme_qemu.py --elf target/x86_64-unknown-none/release/novaos \
    --profil lecture --vcpus 2 --secteur 512
```

La qualification couvre QEMU à 1, 2 et 4 vCPU. UTM, le matériel physique, l’endurance IPC et les campagnes étendues de stockage restent hors de cette phase. Le modèle de distribution continue d’exiger une admission au catalogue. La fusion du développement v0.20.0 requiert une nouvelle décision MOA/MOE.
