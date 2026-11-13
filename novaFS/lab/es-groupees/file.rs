//! Transport du laboratoire Darwin. Deux sockets distinctes sont héritées par le
//! pilote. Seul le superviseur possède le pair de contrôle ; aucune commande de
//! données ne peut recréer le bail. Le pilote reçoit le descripteur de l'image.
use novafs::{BlocGroupe, Block, Device, Error, Result, device::FileImage};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use zeroize::Zeroizing;

const MAX: usize = 128;
const READ: u8 = 1;
const WRITE: u8 = 2;
const FLUSH: u8 = 3;
const FENCE: u8 = 4;
const REVOKE: u8 = 5;
const PAUSE_AFTER_ONE: u8 = 6;
const WAIT_PAUSED: u8 = 7;

#[derive(Default)]
struct Etat {
    retire: bool,
    reste: Option<usize>,
    suspendu: bool,
}

type Porte = (Mutex<Etat>, Condvar);
fn controle(mut canal: UnixStream, porte: Arc<Porte>) {
    let mut q = [0];
    loop {
        let recu = canal.read_exact(&mut q).is_ok();
        let mut e = porte.0.lock().unwrap();
        match (recu, q[0]) {
            (true, PAUSE_AFTER_ONE) if !e.retire => e.reste = Some(1),
            (true, WAIT_PAUSED) => {
                e = porte.1.wait_while(e, |v| !v.suspendu && !v.retire).unwrap();
            }
            _ => {
                e.retire = true;
                porte.1.notify_all();
            }
        }
        drop(e);
        if !recu || canal.write_all(&[0]).is_err() {
            break;
        }
    }
}

fn execute<T>(porte: &Porte, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let mut e = porte.0.lock().unwrap();
    if e.reste == Some(0) && !e.retire {
        e.suspendu = true;
        porte.1.notify_all();
        e = porte.1.wait_while(e, |v| !v.retire).unwrap();
    }
    if e.retire {
        return Err(Error::Unavailable);
    }
    if let Some(n) = &mut e.reste {
        *n -= 1;
    }
    // Le retour du retrait attend la fin de cette commande physique.
    f()
}

pub fn pilote(blocs: u64, secteur: u32) -> Result<()> {
    let image = File::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let mut donnees = UnixStream::from(std::io::stdout().as_fd().try_clone_to_owned()?);
    let controle_fd = UnixStream::from(std::io::stderr().as_fd().try_clone_to_owned()?);
    donnees.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut disque = FileImage::from_file(image, blocs, secteur)?;
    let porte = Arc::<Porte>::default();
    let p = porte.clone();
    std::thread::spawn(move || controle(controle_fd, p));
    let mut pages = Zeroizing::new(Vec::<Block>::with_capacity(MAX));
    let mut adresses = Vec::with_capacity(MAX);
    let mut dernier = 0u64;
    loop {
        let mut entete = [0; 11];
        if donnees.read(&mut entete[..1])? == 0 {
            return Ok(());
        }
        donnees.read_exact(&mut entete[1..])?;
        let op = entete[0];
        let tag = u64::from_le_bytes(entete[1..9].try_into().unwrap());
        let n = usize::from(u16::from_le_bytes(entete[9..].try_into().unwrap()));
        if tag != dernier.checked_add(1).ok_or(Error::Unavailable)?
            || n > MAX
            || !matches!((op, n), (READ | WRITE | FENCE, 1..=MAX) | (FLUSH, 0))
        {
            return Err(Error::Invalid("commande de file"));
        }
        dernier = tag;
        adresses.clear();
        for _ in 0..n {
            let mut b = [0; 8];
            donnees.read_exact(&mut b)?;
            let addr = u64::from_le_bytes(b);
            if addr >= blocs {
                return Err(Error::Invalid("adresse hors bail"));
            }
            adresses.push(addr);
        }
        pages.resize(n, [0; 4096]);
        if matches!(op, WRITE | FENCE) {
            for page in &mut *pages {
                donnees.read_exact(page)?;
            }
        }
        let r: Result<()> = (|| {
            for (&b, page) in adresses.iter().zip(pages.iter_mut()) {
                execute(&porte, || {
                    if op == READ {
                        *page = disque.read(b)?;
                        Ok(())
                    } else {
                        disque.write(b, page)
                    }
                })?;
            }
            if matches!(op, FLUSH | FENCE) {
                execute(&porte, || disque.flush())?;
            }
            Ok(())
        })();
        if r.is_err() {
            porte.0.lock().unwrap().retire = true;
            porte.1.notify_all();
        }
        // Un bail retiré avant la livraison ne peut divulguer le tampon lu.
        // Le retrait attend une livraison déjà engagée, bornée par le délai socket.
        let e = porte.0.lock().unwrap();
        let refus = r.is_err() || e.retire;
        donnees.write_all(&tag.to_le_bytes())?;
        donnees.write_all(&[u8::from(refus)])?;
        if op == READ && !refus {
            for page in &*pages {
                donnees.write_all(page)?;
            }
        }
        drop(e);
        // Le prochain message a sa propre taille. Le tampon ne sort jamais du pilote.
        pages.iter_mut().for_each(|p| p.fill(0));
    }
}

pub struct Autorite {
    controle: UnixStream,
}
impl Autorite {
    fn ordre(&mut self, op: u8) -> Result<()> {
        self.controle.write_all(&[op])?;
        let mut ack = [1];
        self.controle.read_exact(&mut ack)?;
        if ack != [0] {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn retire(&mut self) -> Result<()> {
        self.ordre(REVOKE)
    }
    pub fn suspend_apres_un(&mut self) -> Result<()> {
        self.ordre(PAUSE_AFTER_ONE)
    }
    pub fn attendre_suspension(&mut self) -> Result<()> {
        self.ordre(WAIT_PAUSED)
    }
}

pub struct FileBloc {
    pair: Mutex<Canal>,
    child: Child,
    relais: Child,
    stats_relais: UnixStream,
    blocs: u64,
    // Le moteur P9 dépose ses métadonnées avant F1 puis son ancre avant F2.
    attente: Vec<(u64, Box<zeroize::Zeroizing<Block>>)>,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub lectures: u64,
    pub ecritures: u64,
    pub barrieres: u64,
    pub pages_lues: u64,
    pub pages_ecrites: u64,
}
impl Stats {
    pub fn tours(&self) -> u64 {
        self.lectures + self.ecritures + self.barrieres
    }
}
struct Canal {
    socket: UnixStream,
    tag: u64,
    stats: Stats,
    panne: bool,
}
impl Canal {
    fn entete(&mut self, op: u8, blocs: &[u64]) -> Result<u64> {
        if self.panne || blocs.len() > MAX {
            return Err(Error::Unavailable);
        }
        self.tag = self.tag.checked_add(1).ok_or(Error::Unavailable)?;
        let mut h = [0; 11];
        h[0] = op;
        h[1..9].copy_from_slice(&self.tag.to_le_bytes());
        h[9..].copy_from_slice(&(blocs.len() as u16).to_le_bytes());
        self.socket.write_all(&h)?;
        for &b in blocs {
            self.socket.write_all(&b.to_le_bytes())?;
        }
        Ok(self.tag)
    }
    fn ack(&mut self, tag: u64) -> Result<()> {
        let mut a = [0; 9];
        self.socket.read_exact(&mut a)?;
        if a[..8] != tag.to_le_bytes() || a[8] != 0 {
            self.panne = true;
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    fn envoyer<'a>(
        &mut self,
        op: u8,
        blocs: &[u64],
        pages: impl Iterator<Item = &'a Block>,
    ) -> Result<()> {
        let tag = self.entete(op, blocs)?;
        for p in pages {
            self.socket.write_all(p)?;
        }
        if matches!(op, FENCE | FLUSH) {
            self.stats.barrieres += 1;
        } else {
            self.stats.ecritures += 1;
        }
        self.stats.pages_ecrites += blocs.len() as u64;
        self.ack(tag)
    }
}

impl FileBloc {
    pub fn ouvrir(f: &File, blocs: u64, secteur: u32) -> Result<(Self, Autorite)> {
        let (pair, r) = UnixStream::pair()?;
        let (sortie, d) = UnixStream::pair()?;
        let (stats_relais, s) = UnixStream::pair()?;
        let (controle, c) = UnixStream::pair()?;
        let child = Command::new(std::env::current_exe()?)
            .arg("--pilote")
            .arg(blocs.to_string())
            .arg(secteur.to_string())
            .stdin(Stdio::from(f.try_clone()?))
            .stdout(Stdio::from(OwnedFd::from(d)))
            .stderr(Stdio::from(OwnedFd::from(c)))
            .spawn()?;
        let relais = Command::new(std::env::current_exe()?)
            .arg("--relais")
            .stdin(Stdio::from(OwnedFd::from(r)))
            .stdout(Stdio::from(OwnedFd::from(sortie)))
            .stderr(Stdio::from(OwnedFd::from(s)))
            .spawn()?;
        Ok((
            Self {
                relais,
                stats_relais,
                pair: Mutex::new(Canal {
                    socket: pair,
                    tag: 0,
                    stats: Stats::default(),
                    panne: false,
                }),
                child,
                blocs,
                attente: Vec::with_capacity(64),
            },
            Autorite { controle },
        ))
    }
    pub fn stats(&self) -> Stats {
        self.pair.lock().unwrap().stats
    }
    pub fn mesurer(&self, debut: bool) -> Result<()> {
        let mut c = self.pair.lock().unwrap();
        let mut h = [0u8; 11];
        h[0] = if debut { 20 } else { 21 };
        c.socket.write_all(&h)?;
        let mut ack = [1];
        c.socket.read_exact(&mut ack)?;
        if ack != [0] {
            return Err(Error::Unavailable);
        }
        if debut {
            c.stats = Stats::default();
        }
        Ok(())
    }
    pub fn bilan_relais(&mut self) -> Result<[u64; 2]> {
        self.pair
            .get_mut()
            .unwrap()
            .socket
            .shutdown(std::net::Shutdown::Write)?;
        let mut a = [0; 16];
        self.stats_relais.read_exact(&mut a)?;
        Ok([
            u64::from_le_bytes(a[..8].try_into().unwrap()),
            u64::from_le_bytes(a[8..].try_into().unwrap()),
        ])
    }
    // Deux commandes distinctes ont quitté le client avant la révocation.
    pub fn deux_en_attente(&self) -> Result<()> {
        let mut c = self.pair.lock().unwrap();
        let a = c.entete(WRITE, &[2, 3])?;
        for _ in 0..2 {
            c.socket.write_all(&[0x53; 4096])?;
        }
        let b = c.entete(WRITE, &[4])?;
        c.socket.write_all(&[0x74; 4096])?;
        if c.ack(a).is_ok() || c.ack(b).is_ok() {
            return Err(Error::Invalid("retrait incomplet"));
        }
        Ok(())
    }
}

impl Drop for FileBloc {
    fn drop(&mut self) {
        let _ = self
            .pair
            .get_mut()
            .unwrap()
            .socket
            .shutdown(std::net::Shutdown::Both);
        let _ = self.relais.wait();
        let _ = self.child.wait();
    }
}
impl BlocGroupe for FileBloc {
    fn lire_lot(&self, b: &[u64], pages: &mut [Block]) -> Result<()> {
        if b.is_empty() || b.len() != pages.len() || !self.attente.is_empty() {
            return Err(Error::Busy);
        }
        let mut c = self.pair.lock().unwrap();
        let tag = c.entete(READ, b)?;
        c.stats.lectures += 1;
        c.stats.pages_lues += b.len() as u64;
        c.ack(tag)?;
        for p in pages {
            c.socket.read_exact(p)?;
        }
        Ok(())
    }
    fn ecrire_lot(&self, b: &[u64], pages: &[Block]) -> Result<()> {
        if b.is_empty() || b.len() != pages.len() || !self.attente.is_empty() {
            return Err(Error::Busy);
        }
        self.pair.lock().unwrap().envoyer(WRITE, b, pages.iter())
    }
}
impl Device for FileBloc {
    fn blocks(&self) -> u64 {
        self.blocs
    }
    fn read(&self, b: u64) -> Result<Block> {
        if let Some((_, p)) = self.attente.iter().rev().find(|(k, _)| *k == b) {
            return Ok(***p);
        }
        let mut v = [0; 4096];
        self.lire_lot(&[b], std::slice::from_mut(&mut v))?;
        Ok(v)
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        if b >= self.blocs || self.attente.len() == 64 {
            return Err(Error::Limit("lot de métadonnées"));
        }
        self.attente
            .push((b, Box::new(zeroize::Zeroizing::new(*p))));
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let b: Vec<_> = self.attente.iter().map(|x| x.0).collect();
        let op = if b.is_empty() { FLUSH } else { FENCE };
        let result =
            self.pair
                .get_mut()
                .unwrap()
                .envoyer(op, &b, self.attente.iter().map(|x| &**x.1));
        self.attente.clear();
        result
    }
}

// Relais isolé sur une paire déjà provisionnée. Chaque trame montante et
// descendante a un tag et une taille unique. Le relais ne possède aucun fichier.
pub fn relais() -> Result<()> {
    let mut moteur = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let mut pilote = UnixStream::from(std::io::stdout().as_fd().try_clone_to_owned()?);
    let mut trace = UnixStream::from(std::io::stderr().as_fd().try_clone_to_owned()?);
    let mut journal = [0u64; 2];
    let mut recus = 0u64;
    let mut termines = 0u64;
    let mut fenetre = false;
    loop {
        let mut h = [0; 11];
        if moteur.read(&mut h[..1])? == 0 {
            break;
        }
        moteur.read_exact(&mut h[1..])?;
        let n = usize::from(u16::from_le_bytes(h[9..].try_into().unwrap()));
        let op = h[0];
        // Commandes du superviseur pour le compteur uniquement. Elles restent
        // étrangères au protocole de sauvegarde et ne transitent pas au pilote.
        if matches!(op, 20 | 21) && n == 0 {
            if op == 20 {
                fenetre = true;
                recus = 0;
                termines = 0;
            } else {
                fenetre = false;
                journal = [recus, termines];
            }
            moteur.write_all(&[0])?;
            continue;
        }
        if !matches!((op, n), (READ | WRITE | FENCE, 1..=MAX) | (FLUSH, 0)) {
            return Err(Error::Invalid("relais"));
        }
        let adresses = n * 8;
        let texte = if matches!(op, WRITE | FENCE) {
            n * 4096
        } else {
            0
        };
        if fenetre {
            recus += 1;
        }
        pilote.write_all(&h)?;
        copier(&mut moteur, &mut pilote, adresses + texte)?;
        let mut ack = [0; 9];
        pilote.read_exact(&mut ack)?;
        if h[1..9] != ack[..8] {
            return Err(Error::Invalid("tag de complétion"));
        }
        moteur.write_all(&ack)?;
        if op == READ && ack[8] == 0 {
            copier(&mut pilote, &mut moteur, n * 4096)?;
        }
        if fenetre {
            termines += 1;
        }
    }
    for n in journal {
        trace.write_all(&n.to_le_bytes())?;
    }
    Ok(())
}
fn copier(de: &mut UnixStream, vers: &mut UnixStream, mut n: usize) -> Result<()> {
    let mut buf = Zeroizing::new([0; 8192]);
    while n > 0 {
        let len = n.min(buf.len());
        de.read_exact(&mut buf[..len])?;
        vers.write_all(&buf[..len])?;
        n -= len;
    }
    Ok(())
}
