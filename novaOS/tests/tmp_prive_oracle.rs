//! Oracle du banc : montage P9 hors invité, strictement en lecture seule.
extern crate alloc;
#[allow(dead_code)]
#[path = "../user/novafs/files/tmp_store.rs"]
mod tmp_store;
#[allow(dead_code)]
#[path = "../user/novafs/files/cache_store.rs"]
mod cache_store;
use novafs::{Device, Engine, Kind, Mode, Stat, codec::Anchor, device::FileImage};
use std::{
    env,
    error::Error,
    fs::File,
    io::{self, Write},
};

fn hex(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0; 1024];
    for chunk in bytes.chunks(buf.len() / 2) {
        for (byte, pair) in chunk.iter().zip(buf.chunks_exact_mut(2)) {
            pair[0] = DIGITS[usize::from(byte >> 4)];
            pair[1] = DIGITS[usize::from(byte & 15)];
        }
        out.write_all(&buf[..chunk.len() * 2])?;
    }
    Ok(())
}

/// L'ordre des pages de noms P9 est canonique et indépendant des générations
/// CoW. Chaque entrée occupe une ligne ASCII, même pour des noms avec retours
/// à la ligne ou un contenu binaire. Le comparateur peut relire tout le flux.
fn catalogue(
    fs: &Engine<FileImage>,
    out: &mut impl Write,
    stat: Stat,
    path: &str,
    depth: usize,
) -> Result<(), Box<dyn Error>> {
    if depth > Engine::<FileImage>::MAX_DIRECTORY_DEPTH {
        return Err("profondeur de catalogue P9".into());
    }
    let tag = if stat.kind == Kind::Directory {
        'D'
    } else {
        'F'
    };
    write!(out, "{tag} {} ", stat.id)?;
    hex(out, path.as_bytes())?;
    if stat.kind == Kind::File {
        write!(out, " {} ", stat.size)?;
        let mut offset = 0;
        while offset < stat.size {
            let n = (stat.size - offset).min(64 * 1024) as usize;
            let bytes = fs.inspect_id(stat.id, offset, n)?;
            if bytes.len() != n {
                return Err("lecture du contenu P9 incomplète".into());
            }
            hex(out, &bytes)?;
            offset += n as u64;
        }
    }
    writeln!(out)?;
    if stat.kind == Kind::Directory {
        let mut after = None;
        loop {
            let names = fs.inspect_dir(stat.id, after.as_deref(), 16)?;
            if names.is_empty() {
                break;
            }
            for name in names {
                if depth != 0 || !(tmp_store::hidden_root_name(&name) || cache_store::hidden(&name)) {
                    let child = fs.lookup_at(stat.id, &name)?;
                    let child_path = format!("{}/{name}", path.trim_end_matches('/'));
                    catalogue(fs, out, child, &child_path, depth + 1)?;
                }
                after = Some(name);
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let file = File::open(&args[1])?;
    let len = file.metadata()?.len();
    let disk = FileImage::from_file(file, len / 4096, args[2].parse()?)?;
    let ids = Anchor::decode(&disk.read(0)?, 0)?.ids;
    let fs = Engine::mount_current(disk, ids, Mode::ReadOnly)?;
    assert!(fs.atime_policy().is_some());
    fs.verify()?;
    if args[3] == "catalogue" {
        let mut out = io::BufWriter::new(io::stdout().lock());
        catalogue(&fs, &mut out, fs.stat_id(fs.root_id())?, "/", 0)?;
        out.flush()?;
        return Ok(());
    }
    if args[3] == "catalogue-cache" {
        let mut out = io::BufWriter::new(io::stdout().lock());
        let store = fs.lookup_at(fs.root_id(), cache_store::ROOT)?;
        assert_eq!(store.kind, Kind::Directory);
        catalogue(&fs, &mut out, store, "/", 1)?;
        out.flush()?;
        return Ok(());
    }
    let docs = fs.lookup_at(fs.root_id(), "documents")?.id;
    let sentinel = fs.lookup_at(docs, "exemple.txt")?;
    assert_eq!(fs.inspect_id(sentinel.id, 0, 100)?, b"NovaOS\n");
    let store = fs.lookup_at(fs.root_id(), tmp_store::ROOT_NAME)?.id;
    let names = fs.inspect_dir(store, None, 32)?;
    match args[3].as_str() {
        "propre" => assert_eq!(names, ["registre"]),
        "services-actifs" => {
            let seal = fs.lookup_at(store, "registre")?.id;
            let roots: Vec<_> = names.iter().filter_map(|n| n.strip_prefix('t')).collect();
            assert_eq!(roots.len(), 5, "nsh, gestionnaire et trois services prêts");
            assert_eq!(names.len(), 1 + 2 * roots.len());
            for suffix in roots {
                assert_eq!(suffix.len(), 64);
                let owner: Vec<u8> = (0..32)
                    .map(|i| u8::from_str_radix(&suffix[i * 2..i * 2 + 2], 16))
                    .collect::<Result<_, _>>()?;
                let marker = fs.lookup_at(store, &format!("r{suffix}"))?;
                let root = fs.lookup_at(store, &format!("t{suffix}"))?;
                assert!(root.kind == Kind::Directory && marker.kind == Kind::File);
                assert_eq!(marker.size, 104);
                let bytes = fs.inspect_id(marker.id, 0, 104)?;
                assert_eq!(&bytes[..8], b"NVTMP001");
                assert_eq!(&bytes[8..24], &ids.fs);
                assert_eq!(&bytes[24..40], &ids.lv);
                assert_eq!(&bytes[40..48], &store.to_le_bytes());
                assert_eq!(&bytes[48..56], &seal.to_le_bytes());
                assert_eq!(&bytes[56..88], &owner);
                assert_eq!(&bytes[88..96], &marker.id.to_le_bytes());
                assert_eq!(&bytes[96..104], &root.id.to_le_bytes());
                println!("racine={suffix} oid={}", root.id);
            }
        }
        "coupure" => {
            let roots: Vec<_> = names.iter().filter(|n| n.starts_with('t')).collect();
            assert_eq!(roots.len(), 2, "nsh et le principal interrompu");
            let mut files = 0;
            for root in roots {
                assert!(names.contains(&format!("r{}", &root[1..])));
                let dir = fs.lookup_at(store, root)?.id;
                match fs.inspect_dir(dir, None, 8)?.as_slice() {
                    [] => (),
                    [file] if file == "commun" => {
                        let id = fs.lookup_at(dir, file)?.id;
                        assert_eq!(fs.inspect_id(id, 0, 2)?, b"A");
                        files += 1;
                    }
                    extra => panic!("contenu inattendu {extra:?}"),
                }
            }
            assert_eq!(files, 1);
        }
        _ => return Err("état inattendu".into()),
    }
    println!(
        "generation={} sentinelle={} entrees={}",
        fs.generation(),
        sentinel.id,
        names.len()
    );
    Ok(())
}
