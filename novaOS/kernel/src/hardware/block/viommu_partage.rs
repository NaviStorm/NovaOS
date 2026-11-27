//! Contrôleur unique de la fixture DXE/GICv3. Les deux endpoints ne possèdent
//! jamais la file matérielle. Une commande publiée conserve ses deux tampons.
use super::io::EMPTY;
use super::*;
use crate::{hal::timer, memory, sched::sync::Mutex};

const NIC: u8 = 56;
const NET_BASE: u64 = 0x1000_0000;
static CONTROLE: Mutex<Option<Controle>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Titulaire {
    sid: u8,
    domaine: u32,
    incarnation: u64,
}
impl Titulaire {
    fn index(self) -> usize {
        usize::from(self.sid == NIC)
    }
    fn iova(self, n: usize) -> u64 {
        if self.sid == NIC {
            NET_BASE + n as u64 * 4096
        } else {
            addr(n)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Probe,
    Attache,
    Map(usize, u64, u32),
    Unmap(usize),
    Detache,
}
#[derive(Clone, Copy)]
struct Vol {
    titulaire: Titulaire,
    numero: u64,
    op: Op,
    index: u16,
    longueur: u32,
}
struct Attribution {
    titulaire: Titulaire,
    sondee: bool,
    attachee: bool,
    fermeture: bool,
    pages: [Option<(u64, u32)>; 22],
    dernier: Option<(u64, Op, Result<()>)>,
}
struct Controle {
    unite: Device,
    cadres: [Frame; 9],
    req: Option<Queue>,
    ev: Option<Queue>,
    endpoints: [Option<Attribution>; 2],
    numero: u64,
    vol: Option<Vol>,
    incertain: bool,
}

fn effacer(f: Frame) {
    for i in (0..PAGE).step_by(8) {
        f.mem.w64(i, 0);
    }
}

impl Controle {
    fn ouvrir() -> Result<Self> {
        let d = Device::discover(40, 0x1057, 272)?;
        d.cfg.intx(false);
        if d.cfg.get(4, 2) & 0x400 == 0
            || d.msix
                .is_some_and(|(c, _, _)| d.cfg.get(c + 2, 2) & 0xc000 != 0x4000)
        {
            return Err(Error::Route);
        }
        d.start((1 << 32) | 0x57)?;
        let plage = d.specific.r32(24)..=d.specific.r32(28);
        if d.specific.r64(0) & 4096 == 0
            || d.specific.r8(36) != 0
            || d.specific.r32(32) != 512
            || d.specific.r64(8) > IOVA
            || d.specific.r64(16) < NET_BASE + 22 * 4096 - 1
            || !plage.contains(&1)
            || !plage.contains(&3)
        {
            return Err(Error::Feature);
        }
        let mut cadres = [EMPTY; 9];
        for i in 0..9 {
            match Frame::allocate() {
                Ok(f) => {
                    effacer(f);
                    cadres[i] = f;
                }
                Err(e) => {
                    for f in &cadres[..i] {
                        unsafe { memory::release_dma(f.pa) }
                    }
                    return Err(e);
                }
            }
        }
        Ok(Self {
            unite: d,
            cadres,
            req: None,
            ev: None,
            endpoints: [None, None],
            numero: 0,
            vol: None,
            incertain: true,
        })
    }

    fn demarrer(&mut self) -> Result<()> {
        let f = self.cadres;
        let a = [f[0], f[1], f[2]];
        let b = [f[3], f[4], f[5]];
        self.req = Some(Queue::new(self.unite, 0, a, a.map(|p| p.pa))?);
        self.ev = Some(Queue::new(self.unite, 1, b, b.map(|p| p.pa))?);
        self.unite.ready()?;
        self.ev.as_mut().unwrap().faults(f[8]);
        self.incertain = false;
        Ok(())
    }

    fn etat(&self, id: Titulaire) -> Result<&Attribution> {
        self.endpoints[id.index()]
            .as_ref()
            .filter(|e| e.titulaire == id)
            .ok_or(Error::State)
    }
    fn faute(&self) -> Result<()> {
        barrier();
        if self.incertain || self.ev.as_ref().ok_or(Error::State)?.pages[2].mem.r16(2) != 0 {
            Err(Error::Quarantine)
        } else {
            Ok(())
        }
    }
    fn soumettre(&mut self, id: Titulaire, op: Op) -> Result<()> {
        self.faute()?;
        // Aucune écriture des tampons possible tant qu'une complétion est due.
        if self.vol.is_some() {
            return Err(Error::Quarantine);
        }
        let e = self.etat(id)?;
        let (code, n, res) = match op {
            Op::Probe if !e.sondee && !e.fermeture => (5, 72, 516),
            Op::Attache if e.sondee && !e.attachee && !e.fermeture => (1, 20, 4),
            Op::Detache if e.attachee && e.fermeture && e.pages.iter().all(Option::is_none) => {
                (2, 20, 4)
            }
            Op::Map(i, pa, rights)
                if e.attachee
                    && !e.fermeture
                    && i < e.pages.len()
                    && e.pages[i].is_none()
                    && pa != 0
                    && pa % 4096 == 0
                    && matches!(rights, 1..=3)
                    && !self.cadres.iter().any(|f| f.pa == pa)
                    && !self
                        .endpoints
                        .iter()
                        .flatten()
                        .flat_map(|e| e.pages.iter().flatten())
                        .any(|(old, _)| *old == pa) =>
            {
                (3, 36, 4)
            }
            Op::Unmap(i) if e.attachee && i < e.pages.len() && e.pages[i].is_some() => (4, 28, 4),
            _ => return Err(Error::State),
        };
        let seq = self.numero.checked_add(1).ok_or(Error::Quarantine)?;
        let req = self.cadres[6].mem;
        for i in (0..72).step_by(4) {
            req.w32(i, 0);
        }
        req.w32(0, code);
        req.w32(
            4,
            if op == Op::Probe {
                u32::from(id.sid)
            } else {
                id.domaine
            },
        );
        match op {
            Op::Attache | Op::Detache => req.w32(8, u32::from(id.sid)),
            Op::Map(i, _, _) | Op::Unmap(i) => {
                req.w64(8, id.iova(i));
                req.w64(16, id.iova(i) + 4095);
                if let Op::Map(_, pa, r) = op {
                    req.w64(24, pa);
                    req.w32(32, r);
                }
            }
            Op::Probe => (),
        }
        for i in 0..res {
            self.cadres[7].mem.w8(i, 0xff);
        }
        let q = self.req.as_mut().ok_or(Error::State)?;
        let flight = Vol {
            titulaire: id,
            numero: seq,
            op,
            index: q.index.wrapping_add(1),
            longueur: res as u32,
        };
        self.vol = Some(flight);
        if let Err(e) = q.submit(&[
            Segment::new(self.cadres[6].pa, n, false),
            Segment::new(self.cadres[7].pa, res as u32, true),
        ]) {
            // Queue::submit retourne toutes ses erreurs avant l'écriture de avail.idx.
            self.vol = None;
            return Err(e);
        }
        self.numero = seq;
        Ok(())
    }
    fn collecter(&mut self, id: Titulaire) -> Result<Op> {
        let f = self.vol.ok_or(Error::State)?;
        self.etat(id)?;
        if f.titulaire != id {
            return Err(Error::Quarantine);
        }
        self.faute()?;
        let q = self.req.as_ref().ok_or(Error::State)?;
        if q.index != f.index || f.numero != self.numero {
            self.incertain = true;
            return Err(Error::Quarantine);
        }
        let avance = q.pages[2].mem.r16(2).wrapping_sub(f.index.wrapping_sub(1));
        if avance > 1 {
            self.incertain = true;
            return Err(Error::Quarantine);
        }
        let len = match q.completed() {
            Ok(v) => v,
            Err(Error::Timeout) => return Err(Error::Timeout),
            Err(_) => {
                self.incertain = true;
                return Err(Error::Quarantine);
            }
        };
        let tail = self.cadres[7].mem.r32(f.longueur as usize - 4);
        if len != f.longueur || tail > 9 {
            self.incertain = true;
            return Err(Error::Quarantine);
        }
        self.vol = None;
        let resultat = if tail != 0 {
            Err(Error::Fault)
        } else if f.op == Op::Probe && (0..512).any(|i| self.cadres[7].mem.r8(i) != 0) {
            Err(Error::Route)
        } else {
            Ok(())
        };
        let e = self.endpoints[id.index()].as_mut().ok_or(Error::State)?;
        e.dernier = Some((f.numero, f.op, resultat));
        resultat?;
        match f.op {
            Op::Probe => e.sondee = true,
            Op::Attache => e.attachee = true,
            Op::Detache => e.attachee = false,
            Op::Map(i, pa, r) => e.pages[i] = Some((pa, r)),
            Op::Unmap(i) => e.pages[i] = None,
        }
        Ok(f.op)
    }
    fn commande(&mut self, id: Titulaire, op: Op) -> Result<()> {
        if let Some(f) = self.vol {
            if f.titulaire != id || f.op != op {
                return Err(Error::Quarantine);
            }
        } else {
            self.soumettre(id, op)?;
        }
        self.collecter(id).map(|_| ())
    }
}

pub(super) struct Domain(Titulaire);
#[cfg(feature="gestionnaire-4e1")]
pub(super) fn carte_bail(generation:u64,cadres:[u64;22])->Result<u64>{
    Domain(Titulaire{sid:NIC,domaine:3,incarnation:generation}).carte_nic(cadres)
}
impl Domain {
    fn reserver(sid: u8) -> Result<Self> {
        let index = usize::from(sid == NIC);
        let mut guard = CONTROLE.lock();
        if guard.is_none() {
            *guard = Some(Controle::ouvrir()?);
            guard.as_mut().unwrap().demarrer()?;
        }
        let c = guard.as_mut().unwrap();
        c.faute()?;
        if c.endpoints[index].is_some() {
            return Err(Error::Quarantine);
        }
        let generation = GENERATION
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| Error::State)?
            + 1;
        let titulaire = Titulaire {
            sid,
            domaine: if index == 0 { 1 } else { 3 },
            incarnation: generation,
        };
        c.endpoints[index] = Some(Attribution {
            titulaire,
            sondee: false,
            attachee: false,
            fermeture: false,
            pages: [None; 22],
            dernier: None,
        });
        Ok(Self(titulaire))
    }
    pub fn preparer(sid: u8) -> Result<Self> {
        if sid != 48 {
            return Err(Error::Route);
        }
        super::pci::exclusive_intx(sid)?;
        Self::reserver(sid)
    }
    fn controle<T>(&self, f: impl FnOnce(&mut Controle, Titulaire) -> Result<T>) -> Result<T> {
        let mut guard = CONTROLE.lock();
        let c = guard.as_mut().ok_or(Error::State)?;
        c.etat(self.0)?;
        f(c, self.0)
    }
    fn attacher(&self) -> Result<()> {
        self.controle(|c, id| {
            c.commande(id, Op::Probe)?;
            c.commande(id, Op::Attache)
        })
    }
    pub fn start(&mut self, _: &Pages) -> Result<()> {
        self.attacher()
    }
    pub fn map(&mut self, p: &Pages, n: usize, pa: u64, rights: u32) -> Result<()> {
        if self.0.sid != 48 || n >= 7 || (rights != 0 && p.0[16 + n].pa != pa) {
            return Err(Error::Geometry);
        }
        self.controle(|c, id| {
            c.commande(
                id,
                if rights == 0 {
                    Op::Unmap(n)
                } else {
                    Op::Map(n, pa, rights)
                },
            )
        })
    }
    pub fn retirer_slot(&mut self, _: &Pages, n: usize) -> Result<()> {
        self.controle(|c, id| {
            c.faute()?;
            c.endpoints[id.index()].as_mut().unwrap().fermeture = true;
            if c.vol.is_some() {
                c.collecter(id)?;
            }
            if c.etat(id)?.pages.get(n).ok_or(Error::Geometry)?.is_some() {
                c.commande(id, Op::Unmap(n))?;
            }
            Ok(())
        })
    }
    fn fermer(&self) -> Result<()> {
        let mut guard = CONTROLE.lock();
        let c = guard.as_mut().ok_or(Error::State)?;
        c.etat(self.0)?;
        c.endpoints[self.0.index()].as_mut().unwrap().fermeture = true;
        c.faute()?;
        if c.vol.is_some() {
            c.collecter(self.0)?;
        }
        if c.etat(self.0)?.attachee {
            c.commande(self.0, Op::Detache)?;
        }
        if c.endpoints[1 - self.0.index()].is_some() {
            c.endpoints[self.0.index()] = None;
            return Ok(());
        }
        c.unite.reset()?;
        c.unite.cfg.master(false);
        wait(|| c.unite.cfg.get(4, 2) & 4 == 0)?;
        let c = guard.take().unwrap();
        for f in c.cadres {
            unsafe { memory::release_dma(f.pa) }
        }
        Ok(())
    }
    pub fn stop(&mut self, _: &Pages) -> Result<()> {
        self.fermer()
    }
    pub fn temoins(&self, mut frames: [u64; 24]) -> Result<[u64; 24]> {
        self.controle(|c, _| {
            for (i, f) in c.cadres.iter().enumerate() {
                frames[i] = f.pa;
            }
            Ok(frames)
        })
    }
    pub fn faults(&self, _: &Pages) -> Result<()> {
        self.controle(|c, _| c.faute())
    }
    pub fn route(&mut self, _: &Pages, d: Device, on: bool) -> Result<()> {
        if d.msix.is_some() {
            return Err(Error::Route);
        }
        timer::disk_line(on);
        d.cfg.intx(on);
        Ok(())
    }
    pub fn clear_pending(&self, d: Device) -> Result<()> {
        let _ = d.isr.r8(0);
        timer::purge_disk()
    }
    pub fn garer(self) {}
    pub fn hold(&self, _: Device) -> Result<()> {
        timer::disk_line(false);
        Ok(())
    }
    pub fn resume_line(&self, _: Device) -> Result<()> {
        timer::disk_line(true);
        Ok(())
    }
    pub fn pending_on_mask(&self, _: Device) -> bool {
        timer::disk_pending()
    }
}

impl Domain {
    pub(super) fn reseau() -> Result<Self> {
        let nic = Self::reserver(NIC)?;
        if let Err(e) = nic.attacher() {
            // Une réponse tardive peut encore viser cette incarnation.
            if nic.fermer().is_err() {
                return Err(Error::Quarantine);
            }
            return Err(e);
        }
        Ok(nic)
    }
    #[cfg(feature="gestionnaire-4e1")]
    pub(super) fn carte_nic(&self, cadres:[u64;22])->Result<u64>{
        self.controle(|c,id|{
            c.faute()?;
            let attribution=c.etat(id)?;
            if id.sid!=NIC || !attribution.attachee{return Err(Error::State)}
            let mut masque=0;
            for (i,mapping) in attribution.pages.iter().enumerate(){
                if let Some((pa,droits))=*mapping{
                    let attendus=if matches!(i,2|5|6..=13){2}else{1};
                    if cadres[i]!=pa || pa==0 || droits!=attendus{return Err(Error::State)}
                    masque |= 1<<i;
                }
            }
            Ok(masque)
        })
    }
    pub(super) fn mapper_nic(&self, n: usize, f: Frame) -> Result<()> {
        if self.0.sid != NIC || n >= 22 {
            return Err(Error::Geometry);
        }
        let droits = if matches!(n, 2 | 5 | 6..=13) { 2 } else { 1 };
        self.controle(|c, id| c.commande(id, Op::Map(n, f.pa, droits)))
    }
    pub(super) fn retirer_nic(&self, n: usize) -> Result<()> {
        self.controle(|c, id| {
            if id.sid != NIC || n >= 22 {
                return Err(Error::Geometry);
            }
            c.endpoints[id.index()].as_mut().unwrap().fermeture = true;
            if c.vol.is_some() {
                c.collecter(id)?;
            }
            if c.etat(id)?.pages[n].is_some() {
                c.commande(id, Op::Unmap(n))?;
            }
            Ok(())
        })
    }
    pub(super) fn faults_preparation(&self) -> Result<()> {
        self.controle(|c, _| c.faute())
    }
    pub(super) fn generation(&self) -> u64 {
        self.0.incarnation
    }
    pub(super) fn dimensions() -> (usize, usize, usize, usize) {
        (
            core::mem::size_of::<Controle>(),
            core::mem::size_of::<Attribution>(),
            core::mem::size_of::<Vol>(),
            core::mem::size_of_val(&CONTROLE),
        )
    }
    pub(super) fn detacher_nic(&self) -> Result<()> {
        self.fermer()
    }
    // Deux appels du banc de qualification autour de la vraie commande publiée.
    pub(super) fn differer_nic(&self) -> Result<u64> {
        self.controle(|c, id| {
            if id.sid != NIC {
                return Err(Error::Route);
            }
            c.soumettre(id, Op::Unmap(0))?;
            Ok(c.numero)
        })
    }
    pub(super) fn verifier_pression(&self, numero: u64) -> Result<()> {
        self.controle(|c, id| {
            let avant = c.etat(id)?.pages;
            // Le collecteur lui-même refuse une réponse appartenant à l'autre endpoint.
            if c.collecter(id) != Err(Error::Quarantine) {
                return Err(Error::Fault);
            }
            if !c.vol.is_some_and(|v| v.numero == numero) {
                return Err(Error::Fault);
            }
            if id.sid != 48
                || c.commande(id, Op::Unmap(6)) != Err(Error::Quarantine)
                || c.numero != numero
                || c.etat(id)?.pages != avant
                || !c.vol.is_some_and(|f| {
                    f.titulaire.sid == NIC && f.numero == numero && f.op == Op::Unmap(0)
                })
            {
                return Err(Error::Fault);
            }
            Ok(())
        })
    }
    pub(super) fn reconcilier_nic(&self, numero: u64) -> Result<()> {
        self.controle(|c, id| {
            c.faute()?;
            if let Some((n, Op::Unmap(0), resultat)) = c.etat(id)?.dernier
                && id.sid == NIC
                && n == numero
            {
                return resultat;
            }
            if id.sid != NIC
                || !c.vol.is_some_and(|f| {
                    f.numero == numero && f.titulaire == id && f.op == Op::Unmap(0)
                })
            {
                return Err(Error::State);
            }
            if c.collecter(id)? != Op::Unmap(0) {
                return Err(Error::State);
            }
            Ok(())
        })
    }
}
