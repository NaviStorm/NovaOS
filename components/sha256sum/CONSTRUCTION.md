# Construire sha256sum 0.24.0

Depuis la racine de la branche `codex/sha256sum-natif-0240`, sur Mac ARM64 avec les dépendances NovaOS et les cibles Rust stable installées, préparer les temporaires du projet.

```sh
export NOVAOS_TMPDIR="$(cd .. && pwd)/tmp"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
export RUSTC="$(rustup which --toolchain stable rustc)"
```

**Exécutable macOS.** La construction optimisée utilise le verrou du composant. Le Mach-O se trouve dans `components/sha256sum/target/release/sha256sum`. Il utilise les bibliothèques système de macOS.

```sh
rustup run stable cargo build --release --locked --manifest-path components/sha256sum/Cargo.toml
rustup run stable cargo test --release --locked --manifest-path components/sha256sum/Cargo.toml --test flux
components/sha256sum/validation/tester-options-sha256sum.sh --novaOS
components/sha256sum/validation/tester-options-sha256sum.sh --macOS
```

**ELF NovaOS et catalogue ordinaire.** Les deux cibles installées sont `aarch64-unknown-none-softfloat` et `x86_64-unknown-none`. Le constructeur du noyau compile les commandes avec le script `novaOS/user/novafs/service.ld`, des pages de 4 Kio et le moteur logiciel. Dans chaque `novaOS/target/<cible>/release/build/novaos-*/out`, `sha256sum.elf` contient les octets admis au catalogue. Le conditionnement relit l’ELF et vérifie sa présence dans le noyau choisi.

```sh
unset NOVA_DOC_TRACE NOVA_SHA256_MEMORY NOVA_KERNEL_ELF CARGO_TARGET_DIR
make -C novaOS ARCH=aarch64 kernel
make -C novaOS ARCH=x86_64 kernel
NOVAFS_TARGET=aarch64-apple-darwin novaFS/lab/run build --offline --bin novafs
python3 -B components/sha256sum/validation/invite.py aarch64 512 "$NOVAOS_TMPDIR/recette-sha-a64" --ordinaire
```

Chaque banc invité exige un répertoire de sortie neuf. Rejouer aussi avec `4096` et `x86_64`. Pour les observations de capacités et la révocation au milieu d’une lecture, reconstruire avec `NOVA_DOC_TRACE=1` puis lancer le même banc sans `--ordinaire`. Pour mesurer le tas à chaque acquisition de pages, construire séparément avec `NOVA_SHA256_MEMORY=1` puis exécuter `debit-invite.py ARCH SECTEUR REPERTOIRE`. L’option explicite `--above-ram` y ajoute le gros fichier creux et son pipeline. Le banc utilise deux vCPU TCG et le compteur Darwin du processus QEMU sur la fenêtre de commande. Pour retrouver le rôle pipe et les pages d’admission dans ces relevés, garder le profil de session `cli-test` du banc.

**Mesures Darwin.** Les scripts portant le suffixe `.sh` proposent `--help`. `--novaOS` sélectionne le Mach-O du projet et `--macOS` sélectionne GNU Homebrew. Les gros corpus sont synthétiques. Leurs graines, tailles, références, temps CPU, temps réel et RSS figurent dans les résultats. Les zones creuses d’un fichier sont lues comme des zéros.

```sh
components/sha256sum/validation/mesurer-performances-sha256sum.sh --novaOS
components/sha256sum/validation/mesurer-performances-sha256sum.sh --macOS
components/sha256sum/validation/mesurer-performances-sha256sum.sh --novaOS --large --above-ram
rustup run stable cargo run --release --locked --manifest-path components/sha256sum/Cargo.toml --example moteur
```

Les sources associées aux images incluses sont celles de la branche, au commit de construction indiqué dans `QUALIFICATION.md`. Le noyau et les commandes sont sous MIT, NovaFS sous MPL-2.0. Les licences tierces sont dans `licences/` dans la livraison. [Sources de la construction](https://github.com/NaviStorm/NovaOS/tree/7c45dd05bdb77149a615137e3bb2b2003986acce)

Pour comparer le moteur logiciel sur le même Mac, répéter la construction ou l’exemple avec `RUSTFLAGS='--cfg sha2_backend="soft"'` et un `--target-dir` distinct. L’exemple « moteur » réutilise un tampon de 65 536 octets et hache 256 Mio par répétition, sans appels fichier dans la boucle.
