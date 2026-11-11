# Validation de la réorganisation du monorepo

Relevés historiques effectués lors du déplacement des composants. Les commandes se lancent depuis la racine Git.

NovaFS dispose de son propre manifeste, de son verrou Cargo et de son lanceur Darwin. Sous Rosetta, installer aussi la cible Rust `x86_64-apple-darwin`. Le banc de validation ci-dessous exécute les suites standard et instrumentée ainsi que le million d’opérations différentielles pour chaque architecture.

```sh
novaFS/lab/run build --offline
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "novaFS/target/validation-$isa.json"
done
python3 novaFS/lab/p8-validation.py cli --sortie novaFS/target/scrub.json
python3 novaFS/lab/p8-validation.py mesures --sortie novaFS/target/mesures.json
```

Les deux workspaces Cargo sont indépendants. La configuration bare-metal appartient à `novaOS/.cargo/`. Les produits de compilation vont dans les répertoires `target/` respectifs. Les anciens répertoires locaux `build/` et `target/` à la racine restent ignorés par Git. Les bancs historiques NovaFS qui extraient un commit Git utilisent les chemins de cette révision. [Qualification et récupération NovaFS](../../novaFS/docs/novafs-p8-scrub.md)

## Vérification de la collecte mémoire

Le banc P8 a effectué trois répétitions de chacune des quatre opérations sur des images de 16 Mio, pour chaque secteur logique (512 et 4 096 octets) et chaque architecture hôte. Les maxima suivants décrivent ces 48 observations. La pile est la taille réservée et le RSS inclut le processus Darwin.

| NovaFS | RSS maximal (octets) | Pic du tas (octets) | Pile réservée (octets) | Allocations maximales par opération |
|---|---:|---:|---:|---:|
| ARM64 natif | 3 457 024 | 219 312 | 2 109 440 | 6 077 |
| x86_64 sous Rosetta | 5 349 376 | 218 400 | 2 097 152 | 6 077 |

Le tas vivant a retrouvé sa valeur initiale dans les 16 séries. Pour reproduire ce contrôle de collecte, ajouter `--repetitions 3` à la commande de mesure ci-dessus. Dans les VM NovaOS de 128 Mio, la phase `repos_ui` a relevé 234 cadres résidents sur Q35 et 200 sur AArch64, en pages de 4 Kio. La matrice `make -C novaOS ARCH=… ram-test` conserve ses résultats dans `novaOS/build/`. À 64 Mio, les deux configurations de firmware et de chargeur s’arrêtent avant l’entrée du noyau.
