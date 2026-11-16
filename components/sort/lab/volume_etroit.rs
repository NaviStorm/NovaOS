//! Remplit une image P9 jetable, puis permet de relire le témoin après le test.
use novafs::{Device, Engine, Kind, Mode, codec::Anchor, device::FileImage};
use std::{env, fs::File, io::Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let [_, path, sector, action] = args.as_slice() else {
        return Err("image secteur lire|blocs".into());
    };
    let writing = !matches!(action.as_str(), "lire" | "lire-corpus" | "etat");
    let file = File::options().read(true).write(writing).open(path)?;
    let blocks = file.metadata()?.len() / 4096;
    let dev = FileImage::from_file(file, blocks, sector.parse()?)?;
    let ids = Anchor::decode(&dev.read(0)?, 0)?.ids;
    let mut fs = Engine::mount_current(
        dev,
        ids,
        if writing {
            Mode::ReadWrite
        } else {
            Mode::ReadOnly
        },
    )?;
    if writing {
        let target: u64 = action.parse()?;
        fs.create("/remplissage")?;
        let mut pos = 0;
        while fs.accounting()?.available_blocks > target {
            fs.write("/remplissage", pos, &[0x5a; 4096])?;
            pos += 4096;
        }
        eprintln!(
            "remplissage={pos} blocs_disponibles={}",
            fs.accounting()?.available_blocks
        );
    } else if action == "etat" {
        let mut stack = vec![fs.root_id()];
        let mut lots = 0;
        while let Some(id) = stack.pop() {
            let mut after = None;
            loop {
                let names = fs.inspect_dir(id, after.as_deref(), 64)?;
                if names.is_empty() {
                    break;
                }
                for name in &names {
                    let item = fs.lookup_at(id, name)?;
                    if item.kind == Kind::Directory {
                        stack.push(item.id);
                    } else if name.starts_with("sort-") {
                        lots += 1;
                    }
                }
                if names.len() < 64 {
                    break;
                }
                after = names.last().cloned();
            }
        }
        println!("{lots}");
    } else {
        let s = fs.stat(if action == "lire-corpus" {
            "/corpus-sort"
        } else {
            "/donnees"
        })?;
        let mut out = std::io::stdout().lock();
        for offset in (0..s.size).step_by(65536) {
            out.write_all(&fs.inspect_id(s.id, offset, (s.size - offset).min(65536) as usize)?)?;
        }
        eprintln!("blocs_disponibles={}", fs.accounting()?.available_blocks);
    }
    fs.unmount()?;
    Ok(())
}
