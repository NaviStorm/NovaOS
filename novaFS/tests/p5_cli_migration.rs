//! Les outils de migration lisent l'ancienne image par un descripteur RO.
//! Le nouvel espace d'adressage n'est émis qu'après validation de la transaction.
use novafs::device::FileImage;
use novafs::pool::{Pool, Type};
use novafs::{Engine, Ids, Result};
use serde_json::Value;
use std::fs::File;
use std::process::Command;
use tempfile::{NamedTempFile, tempfile};

const IDS: Ids = Ids {
    fs: [0x12; 16],
    lv: [0x23; 16],
};
const POOL: &str = "91919191919191919191919191919191";
const OWNER: &str = "57575757575757575757575757575757";
const DATA: &[u8] = b"migration complete du volume";
fn fill<D: novafs::Device>(mut fs: Engine<D>) -> Result<()> {
    fs.mkdir("/docs")?;
    fs.create("/docs/valeur")?;
    fs.write("/docs/valeur", 0, DATA)?;
    fs.unmount()?;
    Ok(())
}
fn image(file: &NamedTempFile, blocks: u64, sector: u32) -> Result<FileImage> {
    file.as_file().set_len(blocks * 4096)?;
    FileImage::from_file(file.reopen()?, blocks, sector)
}

#[test]
fn migration_explicite_en_cli_sur_image_distincte() -> Result<()> {
    let cli = env!("CARGO_BIN_EXE_novafs");
    for sector in [512, 4096] {
        for is_pool in [false, true] {
            let src = NamedTempFile::new()?;
            let args = if is_pool {
                let mut pool = Pool::format(image(&src, 768, sector)?)?;
                pool.create(IDS.lv, Some(IDS.fs), 1)?;
                fill(Engine::format(
                    pool.view(IDS.lv, Type::Preparing, IDS.fs)?,
                    IDS,
                )?)?;
                pool.mark_ready(IDS.lv, IDS.fs)?;
                vec![
                    "migrate-pool".into(),
                    "2048".into(),
                    "4096".into(),
                    POOL.into(),
                    "768".into(),
                    sector.to_string(),
                ]
            } else {
                fill(Engine::format(image(&src, 256, sector)?, IDS)?)?;
                vec![
                    "migrate-lv".into(),
                    "2048".into(),
                    "4096".into(),
                    POOL.into(),
                    "256".into(),
                    sector.to_string(),
                    OWNER.into(),
                    "12".repeat(16),
                    "23".repeat(16),
                ]
            };
            let before = std::fs::read(src.path())?;
            let dst = NamedTempFile::new()?;
            let migrated = Command::new(cli)
                .args(&args)
                .stdin(File::open(src.path())?)
                .stdout(dst.reopen()?)
                .output()?;
            assert!(migrated.status.success(), "{migrated:?}");
            assert_eq!(std::fs::read(src.path())?, before);
            let receipt: Value = serde_json::from_slice(&migrated.stderr).unwrap();
            let id = receipt["id"].as_u64().unwrap().to_string();
            assert_eq!(receipt["pool"], POOL);
            let checked = Command::new(cli)
                .args([
                    "check",
                    "2048",
                    "4096",
                    POOL,
                    &id,
                    receipt["empreinte"].as_str().unwrap(),
                ])
                .stdin(File::open(dst.path())?)
                .output()?;
            assert!(checked.status.success(), "{checked:?}");
            let read = Command::new(cli)
                .args(["file-read", "2048", "4096", &id, "/docs/valeur", "0", "64"])
                .stdin(File::open(dst.path())?)
                .output()?;
            assert!(read.status.success(), "{read:?}");
            assert_eq!(read.stdout, DATA);
        }
    }
    Ok(())
}

#[test]
fn sortie_binaire_incompatible_avec_diagnostics_sur_meme_fichier() -> Result<()> {
    let sink = tempfile()?;
    let result = Command::new(env!("CARGO_BIN_EXE_novafs"))
        .args(["pool-format", "2048", "512", POOL])
        .stdout(sink.try_clone()?)
        .stderr(sink.try_clone()?)
        .status()?;
    assert_eq!(result.code(), Some(2));
    assert_eq!(sink.metadata()?.len(), 0);
    Ok(())
}
