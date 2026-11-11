# Cœur portable de NovaFS 0.9.12

Le paquet `novafs` fournit le moteur des deux environnements depuis `src/`. Sur les cibles Rust dont `target_os = "none"`, la bibliothèque utilise `core` et `alloc`. Le laboratoire, les exécutables macOS, le simulateur, le backend image et les sondes de latence hôtes restent des composants Darwin. Le service utilisateur de NovaOS dépend du même manifeste Cargo. L’édition de liens élimine du petit exécutable d’initialisation les fonctions auxquelles il ne fait pas appel.

| Frontière | macOS | Bibliothèque nue et intégration |
|---|---|---|
| Bloc | `FileImage`, essais P1, E/S positionnelles et synchronisation de l’image | Même trait `Device` et même géométrie. Le service assemble ses pages via IPC. Le FLUSH attend la réponse de la chaîne bloc. |
| Allocation | Allocateur du programme hôte et banc de comptage existant | `Vec`, `Box`, collections et secrets utilisent l’allocateur du processus hôte du moteur. NovaOS fournit un tas privé à la demande. |
| Synchronisation | Verrous de la bibliothèque standard, y compris leur comportement en cas de panique | Gardes RAII de `spin` et atomiques de `core`. Les erreurs de contention des essais non bloquants sont conservées. Le profil NovaOS termine le processus en cas de panique. Son premier service est mono-thread. Les verrous locaux ne constituent pas un bail d’écriture de pool entre processus. |
| Archives et erreurs | Traits et erreurs de `std::io`, image temporaire pour la vérification | Flux `Read` et `Write` privés de l’environnement, lecture exacte contrôlée, erreur de transport avec statut natif. La vérification d’archive matérialise la géométrie validée dans un vecteur de pages nettoyé en fin de vie. Le format borne ce volume à quatre grains, soit au plus 4 Mio de pages. La réservation est faillible. Cet espace de travail volatil refuse toute demande de persistance. |
| Horloge et diagnostic | `Instant` et instrumentation P4.1 de l’hôte | `scrub_with` reçoit une fonction de temps écoulé, une source d’entropie et un rappel de progression. Les erreurs et rapports restent structurés. Le processus utilise le diagnostic borné de NovaOS pour signaler l’initialisation. |
| Cryptographie | `getrandom` fournit le CSPRNG Darwin | Même interface `Entropy` et mêmes primitives. La source par défaut retourne `Unavailable` sur cible nue. Le service contrôle ce refus. Les implémentations logicielles évitent de dépendre d’un état SIMD utilisateur. |

Le portage ajoute une entrée de scrub à environnement injecté et des adaptateurs. Les mêmes fonctions traitent la publication F1/F2, la reprise, les réservations, les volumes, le partage, la compression avant chiffrement, l’authentification avant décompression, les clés, les exports et la réparation contrôlée. Les structures persistantes, les identités cryptographiques, les paramètres Argon2id et les versions de format sont ceux de P8. Aucun changement de comportement de fichier ni de format ne motive une nouvelle version. La licence du moteur demeure MPL-2.0.

Cargo lit les options de backends cryptographiques dans `.cargo/config.toml` lors d’une compilation lancée depuis `novaFS/`. Un consommateur externe doit appliquer les options de sa propre cible. La construction NovaOS les transmet explicitement à l’édition de liens de son service et conserve le contrôle des débordements en profil optimisé.

Un `Device` sans exclusion d’écrivain ne fournit pas de `mutation_lock`. Le service préparatoire ne distribue aucune autorité de mutation du pool. Une future session native devra garantir l’exclusivité pendant la reprise et les deux barrières, provisionner les clés et l’entropie nécessaires et qualifier le montage sur une vraie image. Les tests actuels en invité exercent l’allocation, le codec d’arbre, le refus d’entropie et le transport brut de 4 Kio. Les essais de montage, de chiffrement, d’export et de scrub de cette livraison utilisent le moteur sur macOS.

## Reproduction

Depuis la racine, avec les deux cibles Rust installées et Rosetta disponible :

```sh
(
    cd novaFS || exit 1
    export RUSTC="$(rustup which --toolchain stable rustc)"
    for t in x86_64-unknown-none aarch64-unknown-none-softfloat; do
        rustup run stable cargo build --lib --release --all-features --locked --offline --target "$t" || exit 1
        rustup run stable cargo tree --locked --offline --target "$t" -e normal || exit 1
    done
)
for a in aarch64 x86_64; do
    python3 novaFS/lab/p6-validation.py "$a" --sortie "novaFS/target/preparation-$a.json" || exit 1
done
python3 novaFS/lab/p6-historiques.py --support formats
python3 novaFS/lab/p6-historiques.py --support pannes
python3 novaFS/lab/p6-lz4-interop.py
python3 novaFS/lab/p7-validation.py interop --sortie novaFS/target/preparation-p7
python3 novaFS/lab/p8-validation.py cli --sortie novaFS/target/preparation-scrub.json
```

[Résultats macOS et QEMU](../../novaOS/docs/validation/preparation-novafs-v0.11.0.md) · [Contrat du service NovaOS](../../novaOS/docs/novafs-integration-v0.11.0.md)
