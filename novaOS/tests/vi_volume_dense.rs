//! Construit et relit une image P9 ordinaire NEUVE, sans charger le fichier de test en RAM.
use novafs::{
    Engine, Ids, Mode,
    device::FileImage,
    time::{Atime, Clock, Timestamp},
};
use std::{error::Error, fs::OpenOptions, sync::Arc};
struct Civil;
impl Clock for Civil {
    fn now(&self) -> Timestamp {
        Timestamp::utc(1735689600, 0).unwrap()
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let [_, path, mib] = args.as_slice() else {
        return Err("image neuve et taille en Mio requises".into());
    };
    let mib: usize = mib.to_str().ok_or("taille invalide")?.parse()?;
    if !matches!(mib, 16 | 100 | 1024) {
        return Err("taille hors banc".into());
    }
    let len = mib << 20;
    let blocks = (2 * len / 4096) as u64;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    f.set_len(blocks * 4096)?;
    let dev = FileImage::from_file(f, blocks, 4096)?;
    let ids = Ids {
        fs: [0x31; 16],
        lv: [0x62; 16],
    };
    let creation = std::time::Instant::now();
    let mut fs = Engine::format_p9(dev, ids, Atime::Relative, Some(Arc::new(Civil)))?;
    let name = format!("/f{len}");
    fs.create(&name)?;
    let mut data = vec![b'A'; 65536];
    for line in data.chunks_exact_mut(128) {
        line[126] = b'Z';
        line[127] = b'\n';
    }
    for off in (0..len).step_by(data.len()) {
        fs.write(&name, off as u64, &data)?;
        if (off + data.len()) % (16 << 20) == 0 {
            eprintln!("{} Mio écrits", (off + data.len()) >> 20);
        }
    }
    let dev = fs.unmount()?;
    let creation_ms = creation.elapsed().as_secs_f64() * 1000.;
    let montage = std::time::Instant::now();
    let fs = Engine::mount_current(dev, ids, Mode::ReadOnly)?;
    let montage_ms = montage.elapsed().as_secs_f64() * 1000.;
    let lecture = std::time::Instant::now();
    let st = fs.stat(&name)?;
    assert_eq!(st.size, len as u64);
    for off in (0..len).step_by(data.len()) {
        assert_eq!(fs.inspect_id(st.id, off as u64, data.len())?, data);
    }
    fs.unmount()?;
    println!(
        "creation_ms={creation_ms:.3} montage_ms={montage_ms:.3} relecture_ms={:.3}",
        lecture.elapsed().as_secs_f64() * 1000.
    );
    println!("{len} octets denses relus à l’identique");
    Ok(())
}
