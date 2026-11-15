# NovaOS v0.20.0 · qualification finale de la phase 1 Rust et threads

**Verdict du 15 novembre 2026.** La phase 1 satisfait les critères d’intégration sur QEMU AArch64 et x86_64. Ses binaires portent la version 0.20.0. La MOA/MOE réserve la clôture de v0.20.0 et la définition de la phase suivante à une décision ultérieure. L’archive UTM installable distribuée reste celle de v0.17.0. [Décision ARC-023](reconciliation-v0300/decisions.md) · [Distribution](../../distribution/README.md)

La reprise de jonction identifiée lors de cette qualification fait l’objet de la [phase 2 du SDK Rust](rust-sdk-phase2-v0.20.0.md). Les observations ci-dessous décrivent les binaires de phase 1.

## Périmètre des preuves

| Référence | Objet contrôlé |
|---|---|
| `2bcb64575e396790089e3078d443b63d69ac05d7` | Base de `main` avant fusion. |
| `37a8008daddccb60ae2a5fb36288e0b1d791181f` | Bibliothèque Rust native et transport du client fichier. |
| `9eb14a82adb2f493d1aea49c505ef6934c6f3e8b` | Noyau et témoin de la campagne initiale. |
| `47e3ff53d15610795f54bc55933848d16165bdfc` | Candidat communiqué. Seuls les documents et le relevé JSON ont changé depuis la campagne initiale. |
| `108ef24b30c177f6b5b35dbcac812d73d75a5e55` | Versions finales, assertions de bannière et témoins supplémentaires. Le seul fichier modifié sous `kernel/src` depuis le candidat communiqué appartient au profil `thread-probe`. |

Les six empreintes des journaux série historiques à 1, 2 et 4 vCPU et les empreintes des noyaux de leur profil correspondent aux artefacts conservés. Les tailles et empreintes des deux applications externes correspondent aux relevés de construction. Les essais ci-dessous exercent les nouveaux cas limites et les binaires renumérotés. Les deux campagnes ont leurs propres empreintes. [Campagne initiale](validation/rust-threads-v0.20.0.json) · [Mesures finales et SHA-256](validation/rust-threads-finale-v0.20.0.json)

## Résultats des essais finaux

Invités neufs, QEMU 11.1.3 TCG et 256 Mio. Le témoin Rust est compilé dans un projet Cargo hors dépôt pour chaque ISA, admis explicitement au catalogue de la sonde et exécuté au niveau de privilège utilisateur.

| Essai | AArch64 | x86_64 |
|---|---|---|
| Sonde `thread-probe,user-fpu`, 2 vCPU | EL0, 62 491 cadres libres avant et après | Ring 3, 63 019 cadres libres avant et après |
| Deux travailleurs, calcul partagé sous mutex | Somme 432, TLS 7 et 11, 5 et 5 préemptions, 2 migrations | Somme 432, TLS 7 et 11, 5 et 5 préemptions, 2 migrations |
| Collecte et restitution | 4 blocs, 3 contextes résidents en fin de scénario | 4 blocs, 3 contextes résidents en fin de scénario |
| Noyau ordinaire, bannière `NovaOS v0.20.0` | Démarrage et capture de l’écran de l’invité | Démarrage, capture de l’écran et trois états du clavier |
| `nsh` et `vi` sur volume P9 jetable, secteurs de 512 octets | `cp --version`, aide de `vi`, écriture par `:wq`, relecture par `cat` et refus de `vi /../secret` | Mêmes opérations et refus |
| Vérification P9 indépendante après arrêt de l’invité | Génération 43 vérifiée, 10 octets identiques | Génération 43 vérifiée, 10 octets identiques |

Sur les deux ISA, la sonde contrôle le refus d’une auto-jonction, d’une deuxième collecte et d’un deuxième attenteur. Elle observe l’attente de jonction dans le noyau avant de l’annuler et vérifie que la cible demeure collectable via son droit natif. Le même slot passe de la génération 22 à 24 et l’ancien droit échoue pour joindre, détacher et annuler. La destruction d’un processus collecte deux attentes sur mot, une réception IPC, une jonction et un calcul actif. La libération concurrente des deux derniers membres laisse zéro survivant. [Témoin utilisateur](../tests/external-threads/checks.rs) · [Observations dans le noyau](../kernel/src/sched/process/ipc/thread_probe.rs)

Pour reproduire la sonde finale depuis `novaOS/` avec les prérequis de la [première campagne](rust-threads-v0.20.0.md#reproduire-le-témoin-externe) :

```sh
NOVA_ARCH=aarch64 python3 -B tests/threads.py --vcpus 2
NOVA_ARCH=x86_64 python3 -B tests/threads.py --vcpus 2
```

Pour les bannières, construire le noyau ordinaire avec `make ARCH=<isa> kernel`, générer son ISO avec `scripts/iso.sh` en pointant `NOVA_KERNEL_ELF` vers cet ELF, puis exécuter `NOVA_ARCH=<isa> python3 -B tests/demarrage.py --banniere`. Les cinq images ont aussi retrouvé leurs empreintes v0.19.0 après substitution inverse des seuls glyphes de version. Pour le parcours CLI, les fixtures `fixture_p9` et `Terminal` ont démarré un volume neuf par invité. Le fichier `/documents/v020.txt` contenait exactement `NovaOS-020`, sans fin de ligne. Après fermeture de QEMU, l’outil hôte `novafs verify` a contrôlé le volume et `novafs salvage` a rendu les dix octets attendus. [Banc de démarrage](../tests/demarrage.py) · [Terminal de test](../tests/cli.py) · [Fixture P9](../scripts/session_cli.py)

## Garanties et limites

- **Mémoire et CPU.** Le processus possède l’espace d’adressage commun. Chaque secondaire dispose d’une pile RW/NX de 1 à 64 pages de 4 Kio, de deux gardes virtuelles et d’une page TLS initialisée à zéro. Les gardes détectent un dépassement et deux processus conservent des cadres distincts à adresse virtuelle identique. Les essais couvrent la restitution après pénurie, le réemploi à zéro et la préservation des registres entiers, FPU/SIMD et du contrôle d’arrondi sur le profil explicite `user-fpu`. La bibliothèque expose 512 mots TLS par membre. [Mesures de la première campagne](rust-threads-v0.20.0.md#résultats-du-15-novembre-2026)
- **Synchronisation.** Le même verrou d’ordonnancement protège l’inscription de l’attente et la notification. Le mutex publie avec Release et reprend avec Acquire après réexamen du mot. Les sondes vérifient le prédicat déjà modifié, le réveil de 1 puis 1 puis 0 attenteur, le délai et l’annulation. Toutes les tâches utilisateur ordinaires ont la même priorité. L’ordre de reprise du mutex, l’héritage de priorité, une borne de latence et la récupération d’un verrou détenu par un thread mort restent sans garantie. [Mutex](../libs/nova-system/src/sync.rs) · [Ordonnanceur](../kernel/src/sched/mod.rs)
- **Capacités.** L’admission appartient au lanceur applicatif. Un service matériel mono-titulaire est refusé sur l’extension. Les droits de jonction appartiennent au processus et portent une génération. Le noyau refuse les droits étrangers, absents, détachés ou périmés. L’annulation coopérative réveille les attentes sur mot et sur jonction. Une attente IPC relève de son propre protocole de retrait. Les journaux historiques T11 sur les deux ISA confirment la révocation de l’envoi déjà bloqué et l’absence de message livré. [Cycle de vie](../kernel/src/sched/process/threads.rs) · [Contrat IPC](ipc-security.md)
- **Jonction côté Rust.** Le noyau laisse le droit de collecte disponible après l’annulation d’une attente. Dans l’API source expérimentale, `JoinHandle::join(self)` prend possession du handle. S’il retourne une erreur, son destructeur tente un détachement. Un appelant de ce wrapper ne récupère donc pas le handle pour réessayer. Cette différence est à trancher avant le gel de l’API source. [Implémentation du handle](../libs/nova-system/src/thread.rs)
- **Compatibilité.** Les neuf sélecteurs `0x10060..=0x10068` s’ajoutent à l’ABI v1. Les numéros antérieurs, le chargeur ELF64 statique et le protocole fichier v2 sont conservés. Les preuves réutilisées couvrent les anciens clients mono-thread, le cycle de vie, l’ABI et les courses IPC ainsi que le parcours P9 avec remontage. Le chargeur refuse toujours `PT_TLS`, `PT_INTERP` et `PT_DYNAMIC`. Le témoin emploie une cible Rust nue. [ABI native](abi.md) · [Chargeur](../kernel/src/memory/user/elf.rs)
- **Stockage et distribution.** Les arbres Git du moteur NovaFS v0.9.13 P9 et des images de distribution sont identiques à ceux de la base. Le service embarqué `novaos-storage` suit le numéro du système. VirtIO-blk et son polling restent les choix ordinaires, NVMe et VT-d requièrent leurs profils explicites. La qualification finale porte sur QEMU. UTM, matériel physique, campagnes d’endurance IPC, million d’opérations NovaFS et coupures de stockage n’ont pas été rejoués pour ce changement. [Distribution](../../distribution/README.md) · [Preuves des régressions antérieures](rust-threads-v0.20.0.md#régressions-ciblées)

## Version ouverte et décisions restantes

Les manifestes et verrous de `novaos`, `nova-cli` et `novaos-storage` portent 0.20.0. La crate `nova-system` porte 0.1.0. La phase 1 qualifie les appels Rust pour les capacités, les processus, la mémoire, l’IPC, l’horloge et les fichiers délégués ainsi que la première API expérimentale de threads. Les essais QEMU et les binaires ordinaires décrits ici identifient cette phase.

Pour clore v0.20.0, la MOA/MOE doit encore fixer la surface Rust à figer, les comportements d’erreur et de reprise à garantir, les couples ancien/nouveau à certifier et les artefacts à distribuer. Le contrat prévoit aussi un environnement autonome pour auteurs d’applications et une étude de Rust `std`. Leur rattachement à une phase ultérieure exige une décision. Le lancement général d’un ELF depuis un volume, le TLS du compilateur et `std::thread` restent ouverts. [Conditions de gel et niveaux du SDK](reconciliation-v0300/contrat-rust-threads.md#conditions-de-gel-preuves-futures-et-choix-moa) · [Répartition des tranches](reconciliation-v0300/proposition.md)

Pour la stable serveur v0.30.0, ARC-022 exige les compilateurs Rust, C et C++, un éditeur de liens, les bibliothèques nécessaires et les outils de construction dont Cargo, exécutables dans NovaOS. L’acceptation comporte six parcours depuis `nsh`, un par langage et par ISA, avec compilation, liaison et exécution dans l’invité. Ces parcours restent à réaliser et leur chantier n’a pas de numéro intermédiaire attribué. [Registre des décisions](reconciliation-v0300/decisions.md) · [Matrice d’acceptation](reconciliation-v0300/acceptation-serveur.md#compilation-native-depuis-nsh)
