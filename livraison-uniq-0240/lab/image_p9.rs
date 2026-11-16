//! Générateur hôte pour les volumes dépassant la limite du frontal novafs script.
use novafs::{Engine, Ids, device::FileImage, time::Atime};
use std::{
    env,
    fs::OpenOptions,
    io::{self, Read},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let [_, raw, blocks, sector, options @ ..] = args.as_slice() else {
        return Err("IMAGE BLOCS SECTEUR".into());
    };
    let blocks: u64 = blocks.parse()?;
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(raw)?;
    file.set_len(blocks.checked_mul(4096).ok_or("taille de volume")?)?;
    let dev = FileImage::from_file(file, blocks, sector.parse()?)?;
    let ids = Ids {
        fs: [0x27; 16],
        lv: [0x28; 16],
    };
    let mut fs = Engine::format_p9(dev, ids, Atime::Relative, None)?;
    let name = "/gros-fichier.txt";
    fs.create(name)?;
    let mut offset = 0;
    let mut input = io::stdin().lock();
    let mut bytes = [0u8; 65536];
    loop {
        let mut n = 0;
        while n < bytes.len() {
            let got = input.read(&mut bytes[n..])?;
            if got == 0 {
                break;
            }
            n += got;
        }
        if n == 0 {
            break;
        }
        fs.write(name, offset, &bytes[..n])?;
        offset += n as u64;
    }
    assert_eq!(fs.stat(name)?.size, offset);
    if options == ["--mesures"] {
        fs.create("/court")?;
        fs.write("/court",0,&b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n".repeat(128))?;
        fs.create("/long")?;
        let mut line = vec![b'L';32768];line[32767]=b'\n';
        for i in 0..4 { fs.write("/long",i*32768,&line)?; }
        fs.create("/compteur")?;
        fs.write("/compteur",10_000_018,&[0])?;
    } else if !options.is_empty() {
        return Err("option de laboratoire inconnue".into());
    }
    fs.verify()?;
    println!(
        "P9 : {offset} octets, {} blocs libres.",
        fs.space_stats().free
    );
    fs.unmount()?;
    Ok(())
}
