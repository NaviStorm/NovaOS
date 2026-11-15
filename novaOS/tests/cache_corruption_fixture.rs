//! Sur COPIE d'une image de qualification 78 fournie par le banc. Cet outil
//! d'hôte ne fait partie d'aucun service ou image NovaOS.
use novafs::{Device, Engine, Kind, Mode, codec::Anchor, device::FileImage};
use std::{
    error::Error,
    fs::{File, OpenOptions},
    path::Path,
};
fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage : cache-corruption <copie-image-test> <secteur>".into());
    }
    let p = Path::new(&a[1]);
    let meta = std::fs::symlink_metadata(p)?;
    use std::os::unix::fs::MetadataExt;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err("copie régulière isolée requise".into());
    }
    let sector: u32 = a[2].parse()?;
    if ![512, 4096].contains(&sector) || meta.len() % 4096 != 0 {
        return Err("géométrie".into());
    }
    let device = |f: File| FileImage::from_file(f, meta.len() / 4096, sector);
    let f = OpenOptions::new().read(true).write(true).open(p)?;
    let d = device(f)?;
    let ids = Anchor::decode(&d.read(0)?, 0)?.ids;
    let mut fs = Engine::mount_current(d, ids, Mode::ReadWrite)?;
    if fs.atime_policy().is_none() {
        return Err("P9 attendu".into());
    }
    let reg = fs.lookup_at(fs.root_id(), "\x1fNovaOS-cache-v1")?;
    if reg.kind != Kind::Directory {
        return Err("registre introuvable".into());
    }
    let mut owner = [0; 32];
    owner[..16].copy_from_slice(&1u128.to_le_bytes());
    owner[16..].copy_from_slice(&1u128.to_le_bytes());
    let suffix: String = owner.iter().map(|b| format!("{b:02x}")).collect();
    let dir = fs.lookup_at(reg.id, &format!("a{suffix}"))?;
    let seal = fs.lookup_at(reg.id, &format!("r{:016x}", dir.id))?;
    let before = fs.inspect_id(seal.id, 0, 104)?;
    if dir.kind != Kind::Directory
        || seal.kind != Kind::File
        || seal.size != 104
        || &before[0..8] != b"NVCACH01"
        || before[56..88] != owner
        || seal.id.checked_add(1) != Some(dir.id)
        || before[8..24] != ids.fs
        || before[24..40] != ids.lv
    {
        return Err("filiation initiale U1/A inattendue".into());
    }
    let mut after = before.clone();
    after[56] ^= 0x80;
    // Le moteur calcule ses contrôles normaux ; la corruption est sémantique.
    fs.prepare_write_id(seal.id, 56, &after[56..57])?.commit()?;
    fs.unmount()?.into_file().sync_all()?;
    let verify = Engine::mount_current(device(File::open(p)?)?, ids, Mode::ReadOnly)?;
    assert_eq!(verify.inspect_id(seal.id, 0, 104)?, after);
    verify.verify()?;
    println!(
        "CACHE-P4 U1/A sceau={} gen={} octet=56 avant=01 après=81 P9-vérifié; autres caches non ciblés",
        seal.id, dir.id
    );
    Ok(())
}
