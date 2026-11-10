# Témoin P0 pour l’audit P5

Ce test suit un fichier contenant une page sentinelle de 4 Kio à travers la suppression et la réattribution de son volume. Le support `SimDevice` comporte exactement deux grains de 256 blocs. Le premier contient les métadonnées du pool. Le second est donc le seul grain attribuable. Le témoin utilise les API publiques du commit `b865c58`.

```rust
use novafs::device::SimDevice;
use novafs::pool::{GRAIN, Pool, Type};
use novafs::{Device, Engine, Error, Ids, Mode, Result};

fn verifier(secteur: u32) -> Result<()> {
    let ids = Ids { fs: [0x21; 16], lv: [0x11; 16] };
    let nouveau_lv = [0x31; 16];
    let nouveau_domaine = [0x41; 16];
    let sentinelle = [0xa7; 4096];
    let disque = SimDevice::new(2 * GRAIN, secteur)?;
    let controle = disque.control();
    let mut pool = Pool::format(disque)?;
    pool.create(ids.lv, Some(ids.fs), 1)?;
    {
        let mut fs = Engine::format(pool.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
        fs.create("/temoin")?;
        fs.write("/temoin", 0, &sentinelle)?;
        drop(fs.unmount()?);
    }
    pool.mark_ready(ids.lv, ids.fs)?;
    for errone in [Ids { lv: nouveau_lv, ..ids }, Ids { fs: [0x51; 16], ..ids }] {
        let vue = pool.view(ids.lv, Type::Ready, ids.fs)?;
        assert!(matches!(Engine::mount(vue, errone, Mode::ReadOnly), Err(Error::Corrupt(_))));
    }
    let fs = Engine::mount(pool.view(ids.lv, Type::Ready, ids.fs)?, ids, Mode::ReadOnly)?;
    fs.verify()?;
    assert_eq!(fs.read("/temoin", 0, 4096)?, sentinelle);
    drop(fs.unmount()?);

    // Retrouver la page sans présumer l’emplacement choisi par le FS P0.
    let stable = controle.snapshot().device();
    let mut adresse = None;
    for bloc in GRAIN..2 * GRAIN {
        if stable.read(bloc)? == sentinelle {
            assert!(adresse.replace(bloc).is_none());
        }
    }
    let adresse = adresse.expect("page physique de la sentinelle");
    drop(stable);

    pool.remove(ids.lv)?;
    let stats = pool.space_stats()?;
    assert_eq!((stats.assigned_blocks, stats.deferred_blocks, stats.available_blocks),
               (0, GRAIN, 0));
    assert!(matches!(pool.create(ids.lv, Some(ids.fs), 1), Err(Error::Busy)));
    assert!(matches!(pool.create_for(nouveau_domaine, nouveau_lv, None, 1), Err(Error::PoolFull)));
    assert_eq!(controle.snapshot().device().read(adresse)?, sentinelle);

    pool.advance()?;
    let stats = pool.space_stats()?;
    assert_eq!((stats.assigned_blocks, stats.deferred_blocks, stats.available_blocks),
               (0, 0, GRAIN));
    assert_eq!(controle.snapshot().device().read(adresse)?, sentinelle);
    pool.create_for(nouveau_domaine, nouveau_lv, None, 1)?;
    drop(pool);

    let stable = controle.snapshot().device();
    assert_eq!(stable.read(adresse)?, [0; 4096]);
    let mut reouvert = Pool::import(stable)?;
    let brut = reouvert.raw_for(nouveau_domaine, nouveau_lv)?;
    for bloc in 0..GRAIN {
        assert_eq!(brut.read(bloc)?, [0; 4096]);
    }
    println!("{} secteur={secteur} sentinelle_PD={adresse} blocs_remis_a_zero={GRAIN}",
             std::env::consts::ARCH);
    Ok(())
}

fn main() -> Result<()> {
    for secteur in [512, 4096] {
        verifier(secteur)?;
    }
    Ok(())
}
```

## Reproduction sur le Mac

Depuis la racine du monorepo, les commandes suivantes compilent la bibliothèque de référence et le témoin dans un dossier temporaire. Elles demandent Rust stable, les deux cibles Darwin et Rosetta sur Apple Silicon.

```sh
set -eu
travail=$(mktemp -d)
git archive b865c58 novaFS | tar -x -C "$travail"
awk '/^```rust$/ {copier=1; next} /^```$/ {copier=0} copier' \
    novaFS/docs/novafs-p5-preuve-p0.md > "$travail/temoin.rs"
for arch in aarch64 x86_64; do
    cible="$arch-apple-darwin"
    CARGO_TARGET_DIR="$travail/cibles" NOVAFS_TARGET="$cible" \
        "$travail/novaFS/lab/run" build --lib --offline
    sortie="$travail/cibles/$cible/release"
    rustup run stable rustc --edition=2024 --target "$cible" -O \
        -L "dependency=$sortie/deps" --extern "novafs=$sortie/libnovafs.rlib" \
        "$travail/temoin.rs" -o "$travail/temoin-$arch"
    file "$travail/temoin-$arch"
    "$travail/temoin-$arch"
done
```

## Résultat observé

Le 10 novembre 2026 sur le Mac Apple Silicon, les deux exécutables Mach-O ont terminé avec le code 0. Chaque exécution a retrouvé la sentinelle au bloc physique 265.

| Exécution | Secteur logique | Bloc de la sentinelle | Blocs nuls après réattribution et réimport |
| --- | ---: | ---: | ---: |
| ARM64 natif | 512 o | 265 | 256 sur 256 |
| ARM64 natif | 4 096 o | 265 | 256 sur 256 |
| x86_64 sous Rosetta | 512 o | 265 | 256 sur 256 |
| x86_64 sous Rosetta | 4 096 o | 265 | 256 sur 256 |

Dans les quatre cas, chacune des deux substitutions d’identité a provoqué le rejet du montage. Après suppression de l’origine, la tentative avec les anciens identifiants a renvoyé `Busy` et celle avec le nouveau volume a renvoyé `PoolFull`. La sentinelle restait lisible après la rotation du catalogue. C’est la création du nouveau volume qui a effacé le grain avant de le rendre au nouveau propriétaire.

Le témoin exerce un cycle de vie normal, avec barrières réussies sur stockage simulé. L’inférence pour P5 dépend d’une condition précise. Si une future racine retenait le bloc 265 sans participer à la décision de réattribution du pool, cette référence pointerait vers la page effacée. Les essais de coupure et les mesures de performances d’un format partagé exigent une implémentation de ce format.
