//! Banc isolé Darwin, cinq processus pour le scénario, sans publication de droits NovaOS.
mod file;
mod front;
#[allow(dead_code)]
#[path = "../heap_meter.rs"]
mod heap_meter;
mod images;
use file::FileBloc;
use novafs::{
    BlocGroupe, Block, Device, Engine, Error, Ids, Mode, Result, Stat,
    device::{Event, FileImage, Persistence, SimDevice, SimSnapshot},
    time::Atime,
};
use serde_json::{Value, json};
use std::{cell::RefCell, time::Instant};
const IDS: Ids = Ids {
    fs: [0x61; 16],
    lv: [0x62; 16],
};
const BLOCS: u64 = 4096;
const N: usize = 4 << 20;
const SOURCE: &str = "/source";

fn contenu(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| match i % 128 {
            126 => b'Z',
            127 => b'\n',
            _ => b'A',
        })
        .collect()
}
fn fixture<D: Device>(d: D, n: usize) -> Result<D> {
    let mut fs = Engine::format_p9(d, IDS, Atime::Relative, None)?;
    fs.create(SOURCE)?;
    for (i, b) in contenu(n).chunks(65536).enumerate() {
        fs.write(SOURCE, (i * 65536) as u64, b)?;
    }
    fs.unmount()
}
fn verifie<D: Device>(fs: &Engine<D>, nom: &str, attendu: &[u8]) -> Result<()> {
    let st = fs.stat(nom)?;
    assert_eq!(st.size, attendu.len() as u64);
    for (i, b) in attendu.chunks(65536).enumerate() {
        assert_eq!(fs.inspect_id(st.id, (i * 65536) as u64, b.len())?, b);
    }
    Ok(())
}

struct Vue<'a>(&'a RefCell<FileBloc>);
impl Device for Vue<'_> {
    fn blocks(&self) -> u64 {
        self.0.borrow().blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.borrow().read(b)
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        self.0.borrow_mut().write(b, p)
    }
    fn flush(&mut self) -> Result<()> {
        self.0.borrow_mut().flush()
    }
}
impl BlocGroupe for Vue<'_> {
    fn lire_lot(&self, b: &[u64], p: &mut [Block]) -> Result<()> {
        self.0.borrow().lire_lot(b, p)
    }
    fn ecrire_lot(&self, b: &[u64], p: &[Block]) -> Result<()> {
        self.0.borrow().ecrire_lot(b, p)
    }
}

fn reference<D: Device>(fs: &mut Engine<D>, st: Stat) -> Result<()> {
    fs.create("/candidate")?;
    fs.write("/candidate", 0, b"X")?;
    let mut off = 0;
    while off < st.size {
        let n = if off == 0 { 65535 } else { 65536 };
        let octets = fs.read_access_id(st.id, off, n)?;
        fs.write("/candidate", off + 1, &octets)?;
        off += octets.len() as u64;
    }
    fs.rename("/candidate", "/copie")
}

fn magasin(secteur: u32, groupe: bool) -> Result<Value> {
    let f = fixture(FileImage::create(BLOCS, secteur)?, N)?.into_file();
    let (transport, mut autorite) = FileBloc::ouvrir(&f, BLOCS, secteur)?;
    let backend = RefCell::new(transport);
    let mut fs = Engine::mount_current(Vue(&backend), IDS, Mode::ReadWrite)?;
    let source = fs.stat(SOURCE)?;
    let parent = fs.root_id();
    let mut clients = front::Session::nouvelle()?;
    backend.borrow().mesurer(true)?;
    use std::sync::atomic::Ordering::Relaxed;
    let base_tas = heap_meter::LIVE.load(Relaxed);
    heap_meter::PHASE_PEAK.store(base_tas, Relaxed);
    let allocations = heap_meter::ALLOCATIONS.load(Relaxed);
    let debut = Instant::now();
    clients.lancer()?;
    if groupe {
        fs.sauvegarde_labo(source, parent, "copie", b'X')?;
    } else {
        reference(&mut fs, source)?;
    }
    let persistance_ms = debut.elapsed().as_secs_f64() * 1000.;
    autorite.retire()?;
    clients.acquitter()?;
    let temps = debut.elapsed().as_secs_f64() * 1000.;
    let pic_tas = heap_meter::PHASE_PEAK.load(Relaxed);
    let allocations = heap_meter::ALLOCATIONS.load(Relaxed) - allocations;
    backend.borrow().mesurer(false)?;
    let s = backend.borrow().stats();
    let _ = fs.unmount()?;
    let liens = backend.borrow_mut().bilan_relais()?;
    assert_eq!(liens, [s.tours(), s.tours()]);
    drop(backend);
    let fs = Engine::mount_current(
        FileImage::from_file(f.try_clone()?, BLOCS, secteur)?,
        IDS,
        Mode::ReadOnly,
    )?;
    fs.verify()?;
    let origine = contenu(N);
    verifie(&fs, SOURCE, &origine)?;
    verifie(&fs, "/copie", &[b"X".as_slice(), &origine].concat())?;
    // Vérificateur historique construit indépendamment, sans feature expérimentale.
    let reader = std::env::var_os("NOVAFS_LECTEUR").expect("NOVAFS_LECTEUR requis");
    use std::io::{Seek, SeekFrom};
    let mut f = f;
    f.seek(SeekFrom::Start(0))?;
    let ancien = std::process::Command::new(reader)
        .args([
            "verify",
            &BLOCS.to_string(),
            &secteur.to_string(),
            &"61".repeat(16),
            &"62".repeat(16),
        ])
        .stdin(f)
        .output()?;
    assert!(
        ancien.status.success(),
        "{}",
        String::from_utf8_lossy(&ancien.stderr)
    );
    Ok(
        json!({"secteur":secteur,"groupe":groupe,"ms":temps,"persistance_ms":persistance_ms,"file_tours":s.tours(),
        "tours_par_liaison":{"editeur_courtier":1,"courtier_moteur":1,"moteur_bloc":liens[0],"bloc_pilote":liens[1],"controle_retrait":1,"declencheur_externe":1},
        "total_interservices":liens.iter().sum::<u64>()+3,"total_avec_declencheur":liens.iter().sum::<u64>()+4,
        "tas_moteur_debut_octets":base_tas,"tas_moteur_pic_octets":pic_tas,"allocations_moteur":allocations,
        "lectures":s.lectures,"ecritures":s.ecritures,"barrieres":s.barrieres,
        "pages_lues":s.pages_lues,"pages_ecrites":s.pages_ecrites,"retrait_final":1,"lecteur_v0165":true}),
    )
}

struct Sim(RefCell<SimDevice>);
impl Device for Sim {
    fn blocks(&self) -> u64 {
        self.0.borrow().blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.borrow().read(b)
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        self.0.borrow_mut().write(b, p)
    }
    fn flush(&mut self) -> Result<()> {
        self.0.borrow_mut().flush()
    }
}
impl BlocGroupe for Sim {
    fn lire_lot(&self, b: &[u64], p: &mut [Block]) -> Result<()> {
        for (b, p) in b.iter().zip(p) {
            *p = self.read(*b)?;
        }
        Ok(())
    }
    fn ecrire_lot(&self, b: &[u64], p: &[Block]) -> Result<()> {
        for (b, p) in b.iter().zip(p) {
            self.0.borrow_mut().write(*b, p)?;
        }
        Ok(())
    }
}

fn montage(s: &SimSnapshot) -> Result<Engine<Sim>> {
    Engine::mount_current(Sim(RefCell::new(s.device())), IDS, Mode::ReadWrite)
}

fn coupures(secteur: u32, plages: bool) -> Result<Value> {
    let n = 16384 + 127;
    let original = contenu(n);
    let mut litteral = "É".repeat(2053).into_bytes();
    litteral.extend_from_slice("🦊".as_bytes());
    let positions = [[4095, 7, 4106], [8194, 33, 0], [n as u64, 0, 4]];
    let nouveau = if plages {
        [
            &original[..4095],
            &litteral[..4106],
            &original[4102..8194],
            &original[8227..],
            &litteral[4106..],
        ]
        .concat()
    } else {
        [b"X".as_slice(), &original].concat()
    };
    let publier = |fs: &mut Engine<Sim>, st, nom| {
        if plages {
            fs.sauvegarde_plages(
                Some(st),
                fs.root_id(),
                nom,
                &positions,
                litteral.len() as u64,
                |pos, buf| {
                    let i = pos as usize;
                    buf.copy_from_slice(&litteral[i..i + buf.len()]);
                    Ok(())
                },
            )
        } else {
            fs.sauvegarde_labo(st, fs.root_id(), nom, b'X')
        }
    };
    let snap = fixture(SimDevice::new(512, secteur)?, n)?
        .control()
        .snapshot();
    let mut frontieres = Vec::new();
    let mut issues = [0u32; 3];
    let mut courues = 0;
    let anchor;
    {
        let disque = snap.device();
        let ctl = disque.control();
        let mut fs = Engine::mount_current(Sim(RefCell::new(disque)), IDS, Mode::ReadWrite)?;
        let st = fs.stat(SOURCE)?;
        anchor = (fs.generation() + 1) % 2;
        ctl.trace(None, None);
        publier(&mut fs, st, "copie")?;
        frontieres.extend(ctl.events());
        assert_eq!(
            frontieres
                .iter()
                .filter(|e| matches!(e, Event::FlushCompleted(_)))
                .count(),
            2
        );
        ctl.power_cut(Persistence::None)?;
        let cold = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadOnly)?;
        verifie(&cold, "/copie", &nouveau)?;
    }
    for i in 0..frontieres.len() {
        for how in [
            Persistence::None,
            Persistence::Newest(usize::MAX),
            Persistence::Prefix {
                block: anchor,
                bytes: 96,
            },
        ] {
            let d = snap.device();
            let ctl = d.control();
            let mut fs = Engine::mount_current(Sim(RefCell::new(d)), IDS, Mode::ReadWrite)?;
            let st = fs.stat(SOURCE)?;
            ctl.trace(Some(i), None);
            assert!(publier(&mut fs, st, "copie").is_err());
            ctl.power_cut(how)?;
            let cold = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadOnly);
            match cold {
                Ok(fs) => {
                    verifie(&fs, SOURCE, &original)?;
                    match fs.stat("/copie") {
                        Ok(_) => {
                            verifie(&fs, "/copie", &nouveau)?;
                            issues[1] += 1;
                        }
                        Err(Error::NotFound) => issues[0] += 1,
                        autre => panic!("issue {autre:?}"),
                    }
                }
                Err(Error::Corrupt(_)) if matches!(how, Persistence::Prefix { .. }) => {
                    issues[2] += 1
                }
                Err(e) => return Err(e),
            }
            courues += 1;
        }
    }
    assert!(issues.iter().all(|&i| i > 0));
    let mut fs = montage(&snap)?;
    let ancien = fs.stat(SOURCE)?;
    fs.write(SOURCE, 1, b"B")?;
    assert!(publier(&mut fs, ancien, "copie").is_err());
    assert!(matches!(fs.stat("/copie"), Err(Error::NotFound)));
    fs.create("/copie")?;
    fs.write("/copie", 0, b"intact")?;
    let concurrent = fs.stat(SOURCE)?;
    assert!(matches!(
        publier(&mut fs, concurrent, "copie"),
        Err(Error::Exists)
    ));
    verifie(&fs, "/copie", b"intact")?;
    let ancienne_revision = fs.stat(SOURCE)?;
    fs.unlink(SOURCE)?;
    for remplace in [false, true] {
        if remplace {
            fs.create(SOURCE)?;
            fs.write(SOURCE, 0, b"nouvel objet")?;
            assert_ne!(fs.stat(SOURCE)?.id, ancienne_revision.id);
        }
        assert!(publier(&mut fs, ancienne_revision, "interdit").is_err());
        assert!(matches!(fs.stat("/interdit"), Err(Error::NotFound)));
    }
    let fs = montage(&fs.unmount()?.0.into_inner().control().snapshot())?;
    verifie(&fs, SOURCE, b"nouvel objet")?;
    verifie(&fs, "/copie", b"intact")?;
    assert!(matches!(fs.stat("/interdit"), Err(Error::NotFound)));
    Ok(
        json!({"secteur":secteur,"plages":plages,"frontieres":frontieres.len(),"injections":courues,
        "ancien":issues[0],"nouveau":issues[1],"refus":issues[2],"acquit_persistant":true,"conflits":4}),
    )
}

// Chaque échec conserve le nom libre et la génération active. Tester aussi un
// scalaire sur deux lots de 1 Mio et un CR/LF sur deux pages.
fn refus_et_validation(secteur: u32) -> Result<()> {
    let d = fixture(SimDevice::new(1024, secteur)?, 32)?;
    let mut fs = Engine::mount_current(Sim(RefCell::new(d)), IDS, Mode::ReadWrite)?;
    let parent = fs.root_id();
    let st = fs.stat(SOURCE)?;
    let generation = fs.generation();
    for (ops, lit) in [
        (vec![[35, 1, 0]], vec![]),
        (vec![[6, 9, 0], [9, 5, 1]], vec![65]),
        (vec![[0, 0, 2]], vec![65]),
        (vec![[0, 0, 2]], vec![0xc0, 0x80]),
        (vec![[0, 0, 3]], vec![0xed, 0xa0, 0x80]),
        (vec![[0, 0, 2]], vec![0xc2, 0x81]),
        (vec![[32, 0, 1]], vec![0xf0]),
        (vec![[32, 0, 1]], vec![13]),
    ] {
        assert!(
            fs.sauvegarde_plages(
                Some(st),
                parent,
                "refus",
                &ops,
                lit.len() as u64,
                |at, b| {
                    b.copy_from_slice(&lit[at as usize..at as usize + b.len()]);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(fs.generation(), generation);
        assert!(matches!(fs.stat("/refus"), Err(Error::NotFound)));
    }
    let mut texte = vec![b'a'; (1 << 20) + 8];
    texte[4095..4097].copy_from_slice(b"\r\n");
    texte[(1 << 20) - 1..(1 << 20) + 3].copy_from_slice("🦊".as_bytes());
    let ops = [[0, 0, texte.len() as u64]];
    for retrait in [Some(0usize), Some(1 << 20), None] {
        let resultat = fs.sauvegarde_plages(
            None,
            parent,
            "frontiere",
            &ops,
            texte.len() as u64,
            |at, b| {
                if retrait.is_some_and(|p| at as usize >= p) {
                    return Err(Error::Invalid("révocation simulée"));
                }
                b.copy_from_slice(&texte[at as usize..at as usize + b.len()]);
                Ok(())
            },
        );
        if retrait.is_some() {
            assert!(resultat.is_err());
            assert_eq!(fs.generation(), generation);
            assert!(matches!(fs.stat("/frontiere"), Err(Error::NotFound)));
        } else {
            resultat?;
        }
    }
    let cold = Engine::mount_current(
        fs.unmount()?.0.into_inner().control().snapshot().device(),
        IDS,
        Mode::ReadOnly,
    )?;
    cold.verify()?;
    verifie(&cold, "/frontiere", &texte)
}

fn concurrence(secteur: u32) -> Result<()> {
    let f = FileImage::create(512, secteur)?.into_file();
    let (backend, mut autorite) = FileBloc::ouvrir(&f, 512, secteur)?;
    autorite.suspend_apres_un()?;
    std::thread::scope(|scope| -> Result<()> {
        let charge = scope.spawn(|| backend.deux_en_attente());
        autorite.attendre_suspension()?;
        autorite.retire()?;
        charge.join().unwrap()?;
        Ok(())
    })?;
    let mut image = FileImage::from_file(f, 512, secteur)?;
    assert_eq!(image.read(2)?, [0x53; 4096]);
    assert_eq!(image.read(3)?, [0; 4096]);
    assert_eq!(image.read(4)?, [0; 4096]);
    assert!(backend.read(2).is_err());
    image.flush()
}

// Deux demandeurs concurrents pour une même destination libre. Le verrou du
// service sérialise la vérification de révision, le plan et F2.
fn ecrivains(secteur: u32) -> Result<()> {
    use std::sync::{Barrier, Mutex};
    let n = 32768;
    let dev = fixture(SimDevice::new(512, secteur)?, n)?;
    let fs = Engine::mount_current(Sim(RefCell::new(dev)), IDS, Mode::ReadWrite)?;
    let source = fs.stat(SOURCE)?;
    let parent = fs.root_id();
    let fs = Mutex::new(fs);
    let depart = Barrier::new(2);
    let essai = |octet| {
        depart.wait();
        fs.lock()
            .unwrap()
            .sauvegarde_labo(source, parent, "copie", octet)
    };
    let (un, deux) = std::thread::scope(|scope| {
        let un = scope.spawn(|| essai(b'X'));
        let deux = essai(b'Y');
        (un.join().unwrap(), deux)
    });
    let elu = match (un, deux) {
        (Ok(()), Err(Error::Exists | Error::Invalid("révision source"))) => b'X',
        (Err(Error::Exists | Error::Invalid("révision source")), Ok(())) => b'Y',
        autre => panic!("résultats concurrents {autre:?}"),
    };
    let disk = fs.into_inner().unwrap().unmount()?.0.into_inner();
    let froid = Engine::mount_current(disk, IDS, Mode::ReadOnly)?;
    verifie(&froid, SOURCE, &contenu(n))?;
    verifie(&froid, "/copie", &[&[elu][..], &contenu(n)].concat())
}

fn retrait_transaction(secteur: u32) -> Result<()> {
    let n = 256 * 1024;
    let f = fixture(FileImage::create(BLOCS, secteur)?, n)?.into_file();
    let (transport, mut autorite) = FileBloc::ouvrir(&f, BLOCS, secteur)?;
    let backend = RefCell::new(transport);
    let mut fs = Engine::mount_current(Vue(&backend), IDS, Mode::ReadWrite)?;
    let st = fs.stat(SOURCE)?;
    let generation = fs.generation();
    autorite.suspend_apres_un()?;
    std::thread::scope(|scope| -> Result<()> {
        let retrait = scope.spawn(move || -> Result<()> {
            autorite.attendre_suspension()?;
            autorite.retire()
        });
        assert!(fs.sauvegarde_labo(st, fs.root_id(), "copie", b'X').is_err());
        retrait.join().unwrap()
    })?;
    drop(fs);
    drop(backend);
    let fs = Engine::mount_current(
        FileImage::from_file(f, BLOCS, secteur)?,
        IDS,
        Mode::ReadOnly,
    )?;
    fs.verify()?;
    assert_eq!(fs.generation(), generation);
    assert!(matches!(fs.stat("/copie"), Err(Error::NotFound)));
    verifie(&fs, SOURCE, &contenu(n))
}

fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    if let [_, op, chemin, secteur, profil] = a.as_slice()
        && op == "--image"
    {
        if !matches!(profil.as_str(), "dense" | "fragmente" | "sature" | "utm") {
            return Err(Error::Invalid("profil de qualification"));
        }
        println!(
            "{}",
            images::creer(
                chemin.as_ref(),
                secteur.parse().map_err(|_| Error::Invalid("secteur"))?,
                profil
            )?
        );
        return Ok(());
    }
    if a.get(1).is_some_and(|s| s == "--pilote") {
        return file::pilote(a[2].parse().unwrap(), a[3].parse().unwrap());
    }
    match a.get(1).map(String::as_str) {
        Some("--editeur") => return front::client(true),
        Some("--courtier") => return front::client(false),
        Some("--relais") => return file::relais(),
        _ => (),
    }
    let mut mesures = Vec::new();
    let mut pannes = Vec::new();
    for secteur in [512, 4096] {
        concurrence(secteur)?;
        retrait_transaction(secteur)?;
        ecrivains(secteur)?;
        pannes.push(coupures(secteur, false)?);
        pannes.push(coupures(secteur, true)?);
        refus_et_validation(secteur)?;
        for _ in 0..3 {
            for groupe in [false, true] {
                mesures.push(magasin(secteur, groupe)?);
            }
        }
    }
    println!(
        "{}",
        json!({"mesures":mesures,"coupures":pannes,"plages_invalides_et_unicode":[512,4096],"retrait_deux_commandes":[512,4096],"deux_ecrivains_concurrents":[512,4096],"transaction_revoquee":[512,4096]})
    );
    Ok(())
}
