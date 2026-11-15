//! Remplissage physique d'une copie P9 exclusive, puis inspection indépendante.
#![allow(dead_code)]
extern crate alloc;
#[path = "../user/novafs/files/config_store.rs"]
mod config_store;
#[path = "../user/novafs/files/journal_store.rs"]
mod journal_store;
use novafs::{Device, Engine, Error, Kind, Mode, codec::Anchor, device::FileImage};
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FILL: &str = "sentinelle-saturation.bin";
fn mount(file: File, sector: u32, mode: Mode) -> Result<Engine<FileImage>> {
    let length = file.metadata()?.len();
    if length % 4096 != 0 || !matches!(sector, 512 | 4096) {
        return Err("géométrie".into());
    }
    let dev = FileImage::from_file(file, length / 4096, sector)?;
    let ids = Anchor::decode(&dev.read(0)?, 0)?.ids;
    let fs = Engine::mount_current(dev, ids, mode)?;
    if fs.atime_policy().is_none() {
        return Err("P9 requis".into());
    }
    fs.verify()?;
    Ok(fs)
}
fn pattern(block: u32) -> [u8; 4096] {
    let mut x = block + 1;
    let mut p = [0; 4096];
    for w in p.chunks_exact_mut(4) {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        w.copy_from_slice(&x.to_le_bytes());
    }
    p
}
fn prepare(src: &Path, dst: &Path, sector: u32, remain: u64) -> Result<()> {
    if !(16..80).contains(&remain) {
        return Err("seuil d'inauguration requis".into());
    }
    let mut inwards = File::open(src)?;
    let mut target = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dst)?;
    io::copy(&mut inwards, &mut target)?;
    let mut fs = mount(target, sector, Mode::ReadWrite)?;
    let root = fs.root_id();
    if journal_store::Journal::load(&fs)?.status().revision != 0 {
        return Err("journal préexistant".into());
    }
    let file = fs.create_at(root, FILL, Kind::File)?;
    for block in 0..fs.accounting()?.capacity_blocks as u32 {
        if fs.accounting()?.available_blocks <= remain {
            break;
        }
        fs.prepare_write_id(file.id, u64::from(block) * 4096, &pattern(block))?
            .commit()?;
    }
    let a = fs.accounting()?;
    if !(16..80).contains(&a.available_blocks) {
        return Err("réserve simulée introuvable".into());
    }
    fs.unmount()?.into_file().sync_all()?;
    inspect(dst, sector)
}
fn inspect(image: &Path, sector: u32) -> Result<()> {
    let fs = mount(File::open(image)?, sector, Mode::ReadOnly)?;
    let root = fs.root_id();
    let f = fs.lookup_at(root, FILL)?;
    if f.size == 0 || f.size % 4096 != 0 {
        return Err("sentinelle invalide".into());
    }
    for b in 0..f.size / 4096 {
        let bytes = fs.inspect_id(f.id, b * 4096, 4096)?;
        if bytes.as_slice() != pattern(b as u32) {
            return Err("sentinelle altérée".into());
        }
    }
    let st = fs.lookup_at(root, "documents/exemple.txt")?;
    if fs.inspect_id(st.id, 0, 32)? != b"NovaOS\n" {
        return Err("exemple altéré".into());
    }
    for name in [journal_store::NAME, "\x1fNovaOS-journal-new"] {
        if !matches!(fs.lookup_at(root, name), Err(Error::NotFound)) {
            return Err("objet journal écrit".into());
        }
    }
    let s = journal_store::Journal::load(&fs)?.status();
    let a = fs.accounting()?;
    println!(
        "{{\"blocs_ordinaires_libres\":{},\"reserve_maintenance\":{},\"octets_sentinelle\":{},\"oid_sentinelle\":{},\"oid_exemple\":{},\"revision_journal\":{},\"prochaine_sequence\":{}}}",
        a.available_blocks, a.maintenance_reserved, f.size, f.id, st.id, s.revision, s.next
    );
    Ok(())
}
// Rejoue l'inauguration sur un duplicata exclusif sans toucher au disque invité de preuve.
fn probe(src: &Path, dst: &Path, sector: u32) -> Result<()> {
    let mut original = File::open(src)?;
    let mut copy = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dst)?;
    io::copy(&mut original, &mut copy)?;
    let mut fs = mount(copy, sector, Mode::ReadWrite)?;
    let old = fs.receipt();
    let mut event = [0; 96];
    // EID, séquence, BootID, sujet et émetteur. INFO/SYSTEM/LOGGER/ONLINE.
    for (idx, word) in [
        1u64,
        1,
        3,
        4,
        5,
        6,
        5,
        6,
        1 | 2 << 8 | 1 << 16 | 1 << 20 | 3 << 24 | 12 << 32 | 16 << 48,
        12,
        0,
        0,
    ]
    .iter()
    .enumerate()
    {
        journal_store::wire::put(&mut event, idx, *word);
    }
    let j = journal_store::Journal::load(&fs)?;
    assert_eq!(j.status().revision, 0);
    assert!(matches!(j.append(&mut fs, 1, &event), Err(Error::NoSpace)));
    assert_eq!(old, fs.receipt());
    fs.unmount()?.into_file().sync_all()?;
    println!("NoSpace");
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    match a.as_slice() {
        [op, src, dst, sector, n] if op == "prepare" => {
            prepare(src.as_ref(), dst.as_ref(), sector.parse()?, n.parse()?)
        }
        [op, src, dst, sector] if op == "probe" => probe(src.as_ref(), dst.as_ref(), sector.parse()?),
        [op, path, sector] if op == "inspect" => inspect(path.as_ref(), sector.parse()?),
        _ => Err("prepare ORIGINAL COPIE SECTEUR BLOCS_LIBRES | probe SOURCE COPIE SECTEUR | inspect COPIE SECTEUR".into()),
    }
}
