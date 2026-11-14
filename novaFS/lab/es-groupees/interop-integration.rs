//! Banc Darwin à compiler séparément avec le moteur publié et le candidat.
//! Un seul processus ouvre l'image à la fois. Chaque étape remonte les deux générations.
use novafs::{Block, Device, Engine, Ids, Mode, Result, device::FileImage, time::Atime};
use std::{cell::RefCell, fs::OpenOptions, path::Path};

const BLOCS: u64 = 4096;
const IDS: Ids = Ids {
    fs: [0x65; 16],
    lv: [0x66; 16],
};
const TAILLE: usize = 4 << 20;
const MILIEU: usize = TAILLE / 2 - 1;
const AJOUT: &[u8] = "été\n".as_bytes();
const RETIRE: usize = 3;
const TEMOIN: &[u8] = b"ecrit par le moteur publie apres le retour";

struct Image(RefCell<FileImage>);
impl Device for Image {
    fn blocks(&self) -> u64 {
        self.0.borrow().blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.borrow().read(b)
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        self.0.get_mut().write(b, p)
    }
    fn flush(&mut self) -> Result<()> {
        self.0.get_mut().flush()
    }
}

#[cfg(feature = "prototype-es-groupees")]
impl novafs::BlocGroupe for Image {
    fn lire_lot(&self, blocs: &[u64], pages: &mut [Block]) -> Result<()> {
        assert_eq!(blocs.len(), pages.len());
        for (&b, p) in blocs.iter().zip(pages) {
            *p = self.read(b)?;
        }
        Ok(())
    }
    fn ecrire_lot(&self, blocs: &[u64], pages: &[Block]) -> Result<()> {
        assert_eq!(blocs.len(), pages.len());
        let mut dev = self.0.borrow_mut();
        for (&b, p) in blocs.iter().zip(pages) {
            dev.write(b, p)?;
        }
        Ok(())
    }
}

fn ouvrir(path: &Path, secteur: u32, creer: bool, ecrire: bool) -> Result<Image> {
    let file = OpenOptions::new()
        .read(true)
        .write(ecrire)
        .create_new(creer)
        .open(path)?;
    if creer {
        file.set_len(BLOCS * 4096)?;
    }
    Ok(Image(RefCell::new(FileImage::from_file(
        file, BLOCS, secteur,
    )?)))
}

fn comparer(fs: &Engine<Image>, nom: &str, attendu: &[u8]) -> Result<()> {
    let objet = fs.stat(nom)?;
    assert_eq!(objet.size, attendu.len() as u64);
    for (i, bloc) in attendu.chunks(65536).enumerate() {
        assert_eq!(
            fs.inspect_id(objet.id, (i * 65536) as u64, bloc.len())?,
            bloc
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let [_, action, chemin, secteur] = args.as_slice() else {
        panic!("action image secteur")
    };
    let creation = action == "creer";
    let ecrire = creation || matches!(action.as_str(), "grouper" | "retour");
    let dev = ouvrir(
        Path::new(chemin),
        secteur.parse().unwrap(),
        creation,
        ecrire,
    )?;
    let mut fs = if creation {
        Engine::format_p9(dev, IDS, Atime::Relative, None)?
    } else {
        Engine::mount_current(
            dev,
            IDS,
            if ecrire {
                Mode::ReadWrite
            } else {
                Mode::ReadOnly
            },
        )?
    };
    assert_eq!(fs.atime_policy(), Some(Atime::Relative));
    let source: Vec<_> = (0..TAILLE)
        .map(|i| b'A' + ((i + i / 4096) % 26) as u8)
        .collect();
    if creation {
        fs.create("/source")?;
        for (i, page) in source.chunks(65536).enumerate() {
            fs.write("/source", (i * 65536) as u64, page)?;
        }
    }
    comparer(&fs, "/source", &source)?;
    match action.as_str() {
        "creer" => (),
        #[cfg(feature = "prototype-es-groupees")]
        "grouper" => {
            let origine = fs.stat("/source")?;
            fs.sauvegarde_plages(
                Some(origine),
                fs.root_id(),
                "copie",
                &[[MILIEU as u64, RETIRE as u64, AJOUT.len() as u64]],
                AJOUT.len() as u64,
                |offset, buf| {
                    let i = offset as usize;
                    buf.copy_from_slice(&AJOUT[i..i + buf.len()]);
                    Ok(())
                },
            )?;
        }
        "retour" => {
            fs.create("/retour")?;
            fs.write("/retour", 0, TEMOIN)?;
        }
        "lire" => (),
        _ => panic!("action incompatible avec ce moteur"),
    }
    if !creation {
        let attendu = [&source[..MILIEU], AJOUT, &source[MILIEU + RETIRE..]].concat();
        comparer(&fs, "/copie", &attendu)?;
    }
    if matches!(action.as_str(), "retour" | "lire") {
        comparer(&fs, "/retour", TEMOIN)?;
    }
    fs.verify()?;
    println!("{}", fs.generation());
    fs.unmount()?;
    Ok(())
}
