// Audit de NovaFS v0.9.13 sur un support simulé privé à chaque essai.
use novafs::device::{Persistence, SimDevice};
use novafs::time::{Atime, SystemClock};
use novafs::{Engine, Error, Ids, Kind, Mode, Result};
use std::sync::Arc;

fn verifier(secteur: u32) -> Result<()> {
    let ids = Ids {
        fs: [1; 16],
        lv: [2; 16],
    };
    let disque = SimDevice::new(512, secteur)?;
    let controle = disque.control();
    let mut fs = Engine::format_p9(disque, ids, Atime::Relative, Some(Arc::new(SystemClock)))?;
    fs.create("/source")?;
    fs.write("/source", 0, b"Nouveau")?;
    fs.create("/destination")?;
    fs.write("/destination", 0, b"Ancien")?;
    let racine = fs.root_id();
    let source = fs.lookup_at(racine, "source")?.id;
    let destination = fs.lookup_at(racine, "destination")?.id;
    let generation = fs.generation();
    let avant = controle.snapshot();
    controle.trace(None, None);
    assert!(matches!(
        fs.rename_at(racine, "source", racine, "destination"),
        Err(Error::Exists)
    ));
    assert_eq!(fs.generation(), generation);
    assert!(controle.events().is_empty());
    assert!(controle.snapshot().bytes()? == avant.bytes()?);
    let relu = Engine::mount_current(controle.snapshot().device(), ids, Mode::ReadOnly)?;
    relu.verify()?;
    assert_eq!(relu.lookup_at(racine, "source")?.id, source);
    assert_eq!(relu.lookup_at(racine, "destination")?.id, destination);
    assert_eq!(relu.inspect_id(source, 0, 64)?, b"Nouveau");
    assert_eq!(relu.inspect_id(destination, 0, 64)?, b"Ancien");

    // Contre-exemple à « effacer, puis renommer ». Couper entre les deux appels.
    fs.unlink_at(racine, "destination", Kind::File)?;
    controle.power_cut(Persistence::None)?;
    drop(fs);
    let relu = Engine::mount_current(controle.snapshot().device(), ids, Mode::ReadOnly)?;
    relu.verify()?;
    assert!(matches!(
        relu.lookup_at(racine, "destination"),
        Err(Error::NotFound)
    ));
    assert_eq!(relu.lookup_at(racine, "source")?.id, source);
    assert_eq!(relu.inspect_id(source, 0, 64)?, b"Nouveau");
    println!(
        "secteur={secteur} refus=Exists génération={generation} écritures=0 après_unlink={} destination=NotFound source=intacte",
        relu.generation()
    );
    Ok(())
}

fn main() -> Result<()> {
    for secteur in [512, 4096] {
        verifier(secteur)?;
    }
    Ok(())
}
