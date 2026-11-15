//! Injection sur copie exclusive d'un volume fictif, puis relecture indépendante.
#![allow(dead_code)]
extern crate alloc;
#[path = "../user/services/catalog.rs"]
mod catalog;
#[path = "../user/novafs/files/config_store.rs"]
mod config_store;
#[path = "../user/services/policy.rs"]
mod policy;
#[path = "../user/services/settings.rs"]
mod settings;

use novafs::{Device, Engine, Mode, codec::Anchor, device::FileImage};
use std::{
    error::Error,
    fs::{File, OpenOptions},
    io::{self, Write},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const SLOTS: [&str; 2] = ["\x1fNovaOS-services-0", "\x1fNovaOS-services-1"];

fn mount(file: File, sector: u32, mode: Mode) -> Result<Engine<FileImage>> {
    let length = file.metadata()?.len();
    if length % 4096 != 0 || ![512, 4096].contains(&sector) {
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

// La disposition historique définie par NVSVCC01 est quatre déclarations de
// 136 octets puis le mot de dépendance de 10. Le cinquième est absent du volume.
fn save_historical(
    fs: &mut Engine<FileImage>,
    gen_id: u64,
    records: &settings::Records,
) -> Result<()> {
    let mut buf = [0; settings::BYTES];
    let n = settings::encode_legacy(records, &mut buf).ok_or("codage historique")?;
    if n != 568 {
        return Err("largeur historique inattendue".into());
    }
    if config_store::save(fs, gen_id, &buf[..n])? != gen_id + 1 {
        return Err("génération imprévue".into());
    }
    Ok(())
}

fn copy_volume(src: &str, target: &str, sector: u32) -> Result<Engine<FileImage>> {
    let mut original = File::open(src)?;
    let mut copy = OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(target)?;
    io::copy(&mut original, &mut copy)?;
    mount(copy, sector, Mode::ReadWrite)
}

fn prepare(src: &str, target: &str, sector: u32, cas: &str) -> Result<()> {
    if !matches!(cas, "enveloppe" | "capacite" | "sans-secours") {
        return Err("cas inconnu".into());
    }
    let mut fs = copy_volume(src, target, sector)?;
    if config_store::load(&fs, false)?.is_some() {
        return Err("configurations préexistantes".into());
    }
    let mut previous = settings::defaults();
    previous
        .iter_mut()
        .find(|d| d.id == 9)
        .ok_or("service 9 absent")?
        .auto = false;
    save_historical(&mut fs, 0, &previous)?;
    let mut next = settings::defaults();
    if cas == "capacite" {
        // Enveloppe et format intacts, tentative de déclarer un droit hors du catalogue.
        next.iter_mut()
            .find(|d| d.id == 9)
            .ok_or("service absent")?
            .caps |= catalog::CACHE_BACKEND;
    }
    save_historical(&mut fs, 1, &next)?;
    if cas != "capacite" {
        let n = if cas == "sans-secours" { 2 } else { 1 };
        for name in &SLOTS[..n] {
            let id = fs.lookup_at(fs.root_id(), name)?.id;
            fs.prepare_write_id(id, 0, b"X")?.commit()?;
        }
    }
    fs.unmount()?.into_file().sync_all()?;
    Ok(())
}

fn mutate_copy(op: &str, src: &str, target: &str, sector: u32, generation: u64) -> Result<()> {
    let mut fs = copy_volume(src, target, sector)?;
    let seen = config_store::load(&fs, false)?.map_or(0, |r| r.generation);
    if matches!(op, "legacy" | "legacy-custom") {
        if seen != 0 || !(1..=13).contains(&generation) {
            return Err("volume de départ ou génération invalide".into());
        }
        let mut records = settings::defaults();
        if op == "legacy-custom" {
            for d in &mut records {
                if matches!(d.id, 9 | 10) {
                    d.retries = 1;
                }
            }
        }
        for g in 0..generation {
            save_historical(&mut fs, g, &records)?;
        }
    } else {
        if seen == 0 || seen != generation {
            return Err("génération à injecter inattendue".into());
        }
        let name = if op == "corrupt-latest" {
            SLOTS[(generation % 2) as usize]
        } else {
            config_store::AUTHORITY
        };
        let file = fs.lookup_at(fs.root_id(), name)?;
        fs.prepare_write_id(file.id, 0, b"X")?.commit()?;
        if !matches!(
            config_store::load(&fs, false),
            Err(novafs::Error::Corrupt(_))
        ) || !matches!(
            config_store::load(&fs, true),
            Err(novafs::Error::Corrupt(_))
        ) {
            return Err("injection sans refus attendu".into());
        }
    }
    fs.unmount()?.into_file().sync_all()?;
    Ok(())
}

// Sortie binaire de taille fixe par slot. Les tests comparent OID et octets complets
// avant/après l'invité, indépendamment des temps d'accès du catalogue public.
fn inspect(path: &str, sector: u32) -> Result<()> {
    let fs = mount(File::open(path)?, sector, Mode::ReadOnly)?;
    let mut out = io::stdout().lock();
    for name in SLOTS {
        let st = fs.lookup_at(fs.root_id(), name)?;
        out.write_all(&st.id.to_le_bytes())?;
        out.write_all(&st.size.to_le_bytes())?;
        out.write_all(&fs.inspect_id(st.id, 0, usize::try_from(st.size)?)?)?;
    }
    let doc = fs.lookup_at(fs.root_id(), "documents")?;
    let st = fs.lookup_at(doc.id, "exemple.txt")?;
    let bytes = fs.inspect_id(st.id, 0, 100)?;
    if bytes != b"NovaOS\n" {
        return Err("sentinelle publique modifiée".into());
    }
    out.write_all(&st.id.to_le_bytes())?;
    out.write_all(&bytes)?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [op, src, target, sector, cas] if op == "prepare" => {
            prepare(src, target, sector.parse()?, cas)
        }
        [op, src, target, sector, generation]
            if matches!(
                op.as_str(),
                "legacy" | "legacy-custom" | "corrupt-latest" | "corrupt-authority"
            ) =>
        {
            mutate_copy(op, src, target, sector.parse()?, generation.parse()?)
        }
        [op, path, sector] if op == "inspect" => inspect(path, sector.parse()?),
        _ => Err("prepare SOURCE COPIE SECTEUR CAS | inspect COPIE SECTEUR".into()),
    }
}
