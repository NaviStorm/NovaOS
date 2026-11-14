//! Volume neuf du banc classique. Noatime évite qu'une lecture de contrôle déclenche F1.
use novafs::{Engine, Ids, device::FileImage, time::Atime};
use std::{env, error::Error, fs::OpenOptions};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let [_, image, secteur] = args.as_slice() else {
        return Err("image neuve et taille de secteur requises".into());
    };
    let secteur = secteur.parse()?;
    let fichier = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(image)?;
    fichier.set_len(256 * 4096)?;
    let ids = Ids {
        fs: [0x17; 16],
        lv: [0x70; 16],
    };
    let dev = FileImage::from_file(fichier, 256, secteur)?;
    let mut fs = Engine::format_p9(dev, ids, Atime::No, None)?;
    fs.create("/source")?;
    fs.write("/source", 0, b"hote-Nova!")?;
    fs.create("/sortie")?;
    fs.write("/sortie", 0, b"ancien")?;
    fs.verify()?;
    fs.unmount()?;
    Ok(())
}
