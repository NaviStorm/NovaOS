//! Banc privé de coupure. Le parent fournit sur stdin le descripteur de son
//! fichier temporaire. Aucun chemin de stockage n'est ouvert ici.
use novafs::{Block, Device, Engine, Ids, Mode, Result, device::FileImage};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsFd;

const BLOCKS: u64 = 320;
const IDS: Ids = Ids {
    fs: [0x31; 16],
    lv: [0x52; 16],
};
const BASE: u64 = 5;
const OFFSET: usize = 4059;
const GUARD: &[u8] = b"recu independant du temoin";

fn old() -> Vec<u8> {
    (0..11000).map(|i| (i % 239) as u8).collect()
}

fn change() -> Vec<u8> {
    (0..4250).map(|i| (251 - i % 241) as u8).collect()
}

fn stop_at(step: usize) -> ! {
    println!("HALTE {step}");
    io::stdout().flush().expect("accusé du banc");
    loop {
        std::thread::park();
    }
}

struct Trace {
    file: FileImage,
    step: usize,
    cut: Option<usize>,
    mounted: bool,
}

impl Trace {
    fn checkpoint(&self) {
        if self.cut == Some(self.step) {
            stop_at(self.step);
        }
    }
}

impl Device for Trace {
    fn blocks(&self) -> u64 {
        self.file.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.file.read(b)
    }

    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        assert!(self.mounted && self.step < 67);
        self.file.write(b, data)?;
        if self.cut.is_none() {
            let mut line = io::BufWriter::new(io::stdout().lock());
            write!(line, "W {b} ")?;
            for byte in data {
                write!(line, "{byte:02x}")?;
            }
            writeln!(line)?;
            line.flush()?;
        }
        self.step += 1;
        self.checkpoint();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        if self.mounted {
            if self.cut.is_none() {
                println!("F");
            }
            self.step += 1;
        } else {
            // La stabilisation du montage RW précède le début de la mutation.
            self.mounted = true;
        }
        self.checkpoint();
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or("action manquante")?;
    let sector = args.next().ok_or("secteur manquant")?.parse()?;
    let fd = io::stdin().as_fd().try_clone_to_owned()?;
    let image = FileImage::from_file(File::from(fd), BLOCKS, sector)?;
    match action.as_str() {
        "seed" => {
            let mut fs = Engine::format(image, IDS)?;
            fs.create("/garde")?;
            fs.write("/garde", 0, GUARD)?;
            fs.create("/donnees")?;
            fs.write("/donnees", 0, &old())?;
            assert_eq!(fs.generation(), BASE);
            fs.verify()?;
            fs.unmount()?;
        }
        "journal" | "cut" => {
            let cut = if action == "cut" {
                Some(args.next().ok_or("frontière manquante")?.parse()?)
            } else {
                None
            };
            let mut device = Trace {
                file: image,
                step: 0,
                cut,
                mounted: false,
            };
            let mut fs = Engine::mount(&mut device, IDS, Mode::ReadWrite)?;
            assert_eq!(fs.generation(), BASE);
            fs.write("/donnees", OFFSET as u64, &change())?;
            assert_eq!(fs.generation(), BASE + 1);
            fs.unmount()?;
            // Frontière supplémentaire pour tuer après réception du succès.
            if cut == Some(device.step + 1) {
                stop_at(device.step + 1);
            }
            assert!(cut.is_none(), "frontière hors journal");
        }
        "check" => {
            let expected = args.next().ok_or("état attendu manquant")?;
            let fs = Engine::mount(image, IDS, Mode::ReadOnly)?;
            fs.verify()?;
            assert_eq!(fs.list("/", None, 10)?, ["donnees", "garde"]);
            assert_eq!(fs.read("/garde", 0, 4096)?, GUARD);
            let mut bytes = old();
            let recovered = match fs.generation() {
                BASE => "ancien",
                n if n == BASE + 1 => {
                    let delta = change();
                    bytes[OFFSET..OFFSET + delta.len()].copy_from_slice(&delta);
                    "nouveau"
                }
                n => return Err(format!("génération inattendue {n}").into()),
            };
            assert!(expected == "incertain" || expected == recovered);
            assert_eq!(fs.read("/donnees", 0, 65536)?, bytes);
            fs.unmount()?;
            println!("{recovered}");
        }
        _ => return Err("action inconnue".into()),
    }
    Ok(())
}
