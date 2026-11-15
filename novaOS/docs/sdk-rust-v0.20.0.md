# SDK Rust nova-system 0.2.0 pour NovaOS

Cette archive source permet de construire sur l’hôte des applications Rust natives pour NovaOS v0.20.0, phase 2. Elle regroupe la crate `no_std`, ses définitions ABI, les sources du transport fichier, le script de liaison, le lanceur de compilation et un projet exemple doté de son verrou Cargo. La licence MIT figure dans `LICENSE`. Les versions de l’ABI native (1), du protocole fichier (2) et de la crate (0.2.0) sont indépendantes.

## Construire l’exemple extrait

Prérequis sur l’hôte : Python 3, Rust stable avec Cargo, rust-lld et les deux cibles nues installées. La qualification emploie rustc 1.99.1. Vérifier la chaîne locale et, si les cibles manquent, les installer une fois avant la construction hors ligne.

```sh
rustup run stable rustc -Vv
rustup target list --installed --toolchain stable
rustup target add --toolchain stable aarch64-unknown-none-softfloat x86_64-unknown-none
```

Placer l’archive et son fichier `.sha256` dans le même répertoire. Sous macOS, les commandes suivantes vérifient le fichier puis compilent le programme pour les deux architectures. La construction lit uniquement l’extraction et la chaîne Rust installée.

```sh
shasum -a 256 -c nova-system-0.2.0.tar.gz.sha256
tar -xzf nova-system-0.2.0.tar.gz
cd nova-system-0.2.0
python3 build.py aarch64
python3 build.py x86_64
```

Cargo écrit les exécutables dans `examples/threads/target/aarch64-unknown-none-softfloat/release/temoin` et `examples/threads/target/x86_64-unknown-none/release/temoin`. `CARGO_TARGET_DIR` permet de choisir un autre répertoire de sortie. Le lanceur accepte le nom d’une chaîne déjà installée par `NOVA_RUST_TOOLCHAIN`. Le profil est `release`, la panique termine le processus, la liaison est statique, la red zone est désactivée et les segments sont alignés sur 4 Kio.

## Écrire une application

Le projet `examples/threads` fournit un point de départ autonome. Son manifeste référence `libs/nova-system` comme dépendance locale. Pour créer un autre projet, copier le manifeste, le verrou et `main.rs`, puis ajuster le nom du paquet et le chemin de cette dépendance. Actualiser le verrou par `cargo generate-lockfile --offline` après le changement de nom. La compilation d’un projet extérieur à l’extraction se lance avec son propre manifeste :

```sh
python3 build.py aarch64 /chemin/vers/application/Cargo.toml
```

L’exemple montre le point d’entrée `_start` propre à chaque ISA, la pile d’entrée fournie par NovaOS et le transfert dans une fonction Rust. Il lance deux tâches issues du même processus, stocke 7 et 11 dans leur TLS, protège un compteur par mutex et vérifie une somme de 432 ainsi que les deux résultats de jonction avant de sortir avec le statut zéro. Sa configuration ordinaire n’exige aucune donnée de contrôle du banc. La feature de test `qualification` active des scénarios supplémentaires et exige un contrôleur de test NovaOS.

L’ELF64 statique obtenu entre dans le catalogue d’une image par décision de son constructeur. Le lanceur valide l’ISA, les segments, les permissions, l’entrée et les capacités accordées. Dans le profil de qualification, `NOVA_THREAD_PROBE_ELF` sélectionne le binaire et la feature noyau `thread-probe` permet son admission. Copier le fichier sur un volume P9 ne donne aucun droit d’exécution.

## Contrats utilisables

| Module | Contrat |
|---|---|
| `process`, `clock` | Identifiant du processus courant, terminaison, attente avec le droit prévu et heure UTC. |
| `memory` | Consultation de la borne de tas et redimensionnement réservé au propriétaire de l’allocateur. |
| `ipc` | Propriété d’un endpoint local, duplication à droits réduits, délégation, révocation, émission, réception et fermeture. La portée dépend de la table de capacités du processus. |
| `files` | Session créée avec deux endpoints délégués. Ouverture relative, lecture, écriture, synchronisation, énumération et fermeture s’appliquent aux vues autorisées par le service. |
| `thread` | `spawn` crée une tâche avec sa pile et renvoie son seul `JoinHandle`. Une jonction réussie épuise le droit. Une erreur de jonction laisse le propriétaire en possession du handle. Après annulation coopérative du thread appelant, le propriétaire peut transférer ce handle à un autre thread du processus pour reprendre la jonction. Le détachement explicite réussi ou le `Drop` d’un droit actif abandonnent la collecte. L’ABI de jonction n’offre pas d’échéance. |
| `sync` | Mutex, attente sur mot atomique avec délai relatif et notification. Le prédicat doit être revérifié après un réveil. |
| `Result<T, Error>`, `raw` | Les API faillibles conservent le statut numérique, y compris les erreurs inconnues. La porte brute `unsafe` exige que l’appelant garantisse la durée de vie et la validité de ses tampons. |

Les signatures et les types publics sont dans les sources de la crate. Pour produire la documentation locale avec rustdoc depuis l’extraction :

```sh
RUSTC="$(rustup which --toolchain stable rustc)" \
RUSTDOC="$(rustup which --toolchain stable rustdoc)" \
rustup run stable cargo doc --offline --no-deps --target aarch64-unknown-none-softfloat \
  --manifest-path libs/nova-system/Cargo.toml
```

Le SDK utilise `core`. Le chargeur accepte les exécutables NovaOS du profil statique et refuse notamment `PT_INTERP`, `PT_DYNAMIC` et `PT_TLS`. L’accès TLS passe par l’API indexée de `nova-system`. Le runtime Rust `std`, le chargement dynamique, les binaires Linux et les compilateurs exécutés dans NovaOS restent hors de ce SDK.
