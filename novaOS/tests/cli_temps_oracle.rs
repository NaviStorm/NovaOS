//! Outil Darwin du banc. Chaque appel ouvre une image temporaire hors ligne.
use novafs::{Device, Engine, Mode, codec::Anchor, device::FileImage, time::Timestamp};
use std::{
    env,
    error::Error,
    fs::OpenOptions,
    io::{self, Write},
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let seed = args[1] == "initialiser";
    let file = OpenOptions::new().read(true).write(seed).open(&args[2])?;
    let blocks = file.metadata()?.len() / 4096;
    let dev = FileImage::from_file(file, blocks, args[3].parse()?)?;
    let ids = Anchor::decode(&dev.read(0)?, 0)?.ids;
    let mut fs = Engine::mount_current(
        dev,
        ids,
        if seed {
            Mode::ReadWrite
        } else {
            Mode::ReadOnly
        },
    )?;
    assert!(fs.atime_policy().is_some());
    fs.verify()?;
    match args[1].as_str() {
        "initialiser" => {
            for (path, nanos) in [("/alpha", 800_000_001), ("/zeta", 200_000_123)] {
                fs.create(path)?;
                fs.set_times(path, None, Some(Timestamp::utc(978307200, nanos)?))?;
            }
            for name in ["/chrono-a", "/chrono-m"] {
                fs.create(name)?;
                fs.write(name, 0, b"horloge")?;
                fs.set_times(
                    name,
                    Some(Timestamp::utc(123, 456)?),
                    Some(Timestamp::utc(789, 123)?),
                )?;
            }
            fs.unmount()?;
        }
        "stat" => {
            let st = fs.stat(&args[4])?;
            let t = fs.times_id(st.id)?;
            print!("{} {} {}", st.id, st.size, st.revision);
            for v in [t.btime, t.mtime, t.ctime, t.atime] {
                if let Some((s, n)) = v.parts() {
                    print!(" {s}.{n:09}")
                } else {
                    print!(" ?")
                }
            }
            println!();
        }
        "lire" => {
            let st = fs.stat(&args[4])?;
            let mut offset = 0;
            while offset < st.size {
                let data = fs.inspect_id(st.id, offset, 4096)?;
                assert!(!data.is_empty());
                io::stdout().lock().write_all(&data)?;
                offset += data.len() as u64;
            }
        }
        _ => return Err("opération inconnue".into()),
    }
    Ok(())
}
