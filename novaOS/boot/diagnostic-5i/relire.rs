//! Relire les clichés du modèle suspendu avec le même résolveur Rust que l'EFI.
extern crate alloc;
#[allow(dead_code)]
#[path = "../diagnostic-5e/topologie5i.rs"]
mod topologie5i;
use std::{env, fs, io, path::Path};
use topologie5i::{
    Dmar, Function, Rid, attribution, bridge_range, possible_group, validate_bridges,
};

fn verify(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let f = dir.join("DMAR.bin");
    let bounded = |p: &Path, max: u64| -> io::Result<Vec<u8>> {
        use std::io::Read;
        let mut data = Vec::new();
        fs::File::open(p)?.take(max + 1).read_to_end(&mut data)?;
        if data.len() as u64 > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fichier hors limite",
            ));
        }
        Ok(data)
    };
    let table = match bounded(&f, 16384) {
        Ok(data)
            if data.len() >= 48
                && data.len() <= 16384
                && &data[..4] == b"DMAR"
                && u32::from_le_bytes(data[4..8].try_into()?) as usize == data.len()
                && data.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) == 0 =>
        {
            Some(data)
        }
        Ok(_) => return Err("en-tête DMAR invalide".into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let d = match table {
        Some(ref t) => Dmar::parse(t).map_err(|_| "DMAR refusée")?,
        None => Dmar::default(),
    };
    let file = dir.join("fonctions.txt");
    if fs::metadata(&file)?.len() > 8192 {
        return Err("inventaire trop grand".into());
    }
    let mut devices = Vec::<Function>::new();
    let rows = bounded(&file, 8192)?;
    for line in std::str::from_utf8(&rows)?.lines() {
        let v = line
            .split_ascii_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let [seg, bus, dev, fun, ident, command, class, header, buses] = v[..] else {
            return Err("ligne PCI invalide".into());
        };
        let id = Rid {
            seg: seg.try_into()?,
            bus: bus.try_into()?,
            dev: dev.try_into()?,
            fun: fun.try_into()?,
        };
        if id.dev > 31
            || id.fun > 7
            || devices.len() == 64
            || devices.iter().any(|f| f.id == id)
            || ident as u16 == 0xffff
            || header > 255
            || header & 127 > 1
            || (header & 127 == 1) != (class >> 8 == 0x0604)
        {
            return Err("identité ou géométrie PCI invalide".into());
        }
        let bridge = if header & 127 == 1 {
            Some(bridge_range(id, buses).map_err(|_| "pont invalide")?)
        } else if buses == 0 {
            None
        } else {
            return Err("bus d'un endpoint".into());
        };
        devices.push(Function {
            id,
            ident,
            command: command.try_into()?,
            class,
            header: header as u8,
            bridge,
        });
    }
    if devices.is_empty() {
        return Err("aucune fonction PCI".into());
    }
    validate_bridges(&devices).map_err(|_| "fenêtres de pont ambiguës")?;
    for (index, f) in devices.iter().enumerate() {
        let id = f.id;
        let (group, count) = possible_group(index, &devices);
        let fmt = |r: Rid| format!("{:04x}:{:02x}:{:02x}.{}", r.seg, r.bus, r.dev, r.fun);
        let (base, n) = match attribution(&d, id, &devices) {
            Some((base, n)) => (
                base.map_or("none".into(), |v| format!("{v:x}")),
                n.to_string(),
            ),
            None => ("unknown".into(), "unknown".into()),
        };
        println!("{} {} {} {} {}", fmt(id), base, n, fmt(group), count);
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    verify(Path::new(
        &env::args_os().nth(1).ok_or("répertoire de cliché requis")?,
    ))
}
