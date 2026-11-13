//! Images autonomes jetables pour les bancs des invités, sans migration ni réallocation hôte.
use super::*;
use std::{fs::OpenOptions, path::Path};

pub(super) fn creer(path: &Path, secteur: u32, profil: &str) -> Result<Value> {
    let blocs = if profil == "sature" { 1280 } else { BLOCS };
    let f = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)?;
    f.set_len(blocs * 4096)?;
    let d = FileImage::from_file(f, blocs, secteur)?;
    let mut ids = IDS;
    if profil == "utm" {
        ids.fs = ids.lv;
        ids.fs[15] ^= 1;
    }
    let mut fs = Engine::format_p9(d, ids, Atime::Relative, None)?;
    fs.create(SOURCE)?;
    let mut original = contenu(N);
    original[N / 2..N / 2 + 6].copy_from_slice(b"MILIEU");
    let fragmenter = profil == "fragmente";
    if fragmenter || profil == "sature" {
        fs.create("/tiers")?;
    }
    let pas = if fragmenter { 8192 } else { 65536 };
    for (i, texte) in original.chunks(pas).enumerate() {
        fs.write(SOURCE, (i * pas) as u64, texte)?;
        if fragmenter {
            fs.write("/tiers", (i * 4096) as u64, &[0x53; 4096])?;
        }
    }
    if profil == "sature" {
        let mut off = 0;
        while fs.space_stats().free > 48 {
            match fs.write("/tiers", off, &[0x53; 65536]) {
                Ok(_) => (),
                Err(Error::NoSpace) => break,
                Err(e) => return Err(e),
            }
            off += 65536;
        }
    }
    let libre = fs.space_stats().free;
    let extents = fs.extent_count(SOURCE)?;
    let tiers = if fragmenter || profil == "sature" {
        Some(fs.stat("/tiers")?.size)
    } else {
        None
    };
    fs.verify()?;
    fs.unmount()?;
    Ok(json!({"blocs":blocs,"blocs_libres":libre,"extents_source":extents,"octets_tiers":tiers}))
}
