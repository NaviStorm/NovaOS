//! Profil ARM fermé. Le superviseur garde la seule autorité matérielle 3E.
use super::*;
#[cfg(gestionnaire_reprise)]
pub(crate) mod reprise;
use core::sync::atomic::{AtomicU64, Ordering};
const MANAGER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/gestionnaire.elf"));
const PEER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pair.elf"));
const PACKAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pilote.pkg01"));
static INVENTORY: AtomicU64 = AtomicU64::new(0);

static OBSERVER: AtomicU64 = AtomicU64::new(0);
static CHANNEL: AtomicU64 = AtomicU64::new(0);
static ACTIVATION: AtomicU64 = AtomicU64::new(0);
static RETRAIT: AtomicU64 = AtomicU64::new(0);
pub(crate) fn session_message(service: bool, client: bool, tag: u64, buf: &[u8]) {
    if tag != 0x45544831 || buf.len() < 32 {
        return;
    }
    if service && buf.len() == 48 && buf[4..8] == [2, 0, 1, 0] {
        let start = ACTIVATION.swap(0, Ordering::Relaxed);
        if start != 0 {
            hal::log(format_args!(
                "D4 activation publication_open_ticks={}",
                hal::timer::counter() - start
            ));
        }
    } else if client && buf[4..8] == [12, 0, 0, 0] {
        debut_retrait();
    }
}
pub(crate) fn debut_retrait() {
    let _ = RETRAIT.compare_exchange(
        0,
        hal::timer::counter(),
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
}
pub(crate) fn fin_retrait() {
    let start = RETRAIT.swap(0, Ordering::Relaxed);
    if start != 0 {
        hal::log(format_args!(
            "D4 fermeture restitution_ticks={}",
            hal::timer::counter() - start
        ));
    }
}
static MARK: AtomicU64 = AtomicU64::new(0);
static TIMES: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
/// Horodate l'entrée ABI des quatre dépôts du seul gestionnaire observé.
pub(super) fn horodater(pid: u64, id: u64, key: u64) {
    if OBSERVER.load(Ordering::Relaxed) != pid {
        return;
    }
    if id == crate::abi::IPC_SEND && CHANNEL.load(Ordering::Relaxed) == key {
        if let Some(t) = TIMES.get(MARK.fetch_add(1, Ordering::Relaxed) as usize) {
            t.store(hal::timer::counter(), Ordering::Release);
        }
    }
}
pub(crate) struct Decision {
    epoch: u64,
    id: u64,
    sid: u64,
    digest: [u8; 32],
    plan: [u8; 32],
    catalog: u64,
    policy: u64,
    liaison: Option<Controle>,
    cadres_controle: u64,
    #[cfg(all(feature="gestionnaire-4f", feature="net-q35-duo"))]
    titulaire: Option<NumeroReserve>,
}
impl Decision {
    #[cfg(all(feature="gestionnaire-4f", feature="net-q35-duo"))]
    pub(in crate::sched::process) fn reserver_titulaire(&mut self) -> Option<u64> {
        if self.titulaire.is_some() || !self.actuelle(self.id, self.sid) { return None; }
        let numero = NumeroReserve::reserver()?;
        let pid = numero.0;
        self.titulaire = Some(numero);
        Some(pid)
    }
    pub(crate) fn actuelle(&self, id: u64, sid: u64) -> bool {
        self.epoch == INVENTORY.load(Ordering::Acquire)
            && self.id == id
            && self.sid == sid
            && self.catalog == 1
            && self.policy == 1
    }
}
fn actors(epoch: u64, id: u64, sid: u64, mode: u64, boot: u64) -> Option<(Child, Child, u64, u64)> {
    #[cfg(gestionnaire_reprise)]
    let ancienne=reprise::ancienne_selection();
    let _guard = Guard::new();
    let m = create_image(MANAGER, &[], None, None).ok()?;
    let p = match create_image(PEER, &[], None, None) {
        Ok(p) => p,
        Err(_) => {
            ipc::undo_unpublished(m);
            return None;
        }
    };
    let result = with(|s| {
        if mode == 3 {
            for _ in 0..2 {
                ipc::liaison_ethernet::controle(s, p.0, m.0)?;
            }
            if s.ipc.racines_pilotes(p.0) != 4 {
                return Err(crate::abi::REFUSED);
            }
            hal::log(format_args!("D4 saturation reelle racines=4"));
        }
        let (client, manager) = ipc::liaison_ethernet::controle(s, p.0, m.0)?;
        let (request, server) = ipc::liaison_ethernet::controle(s, m.0, p.0)?;
        let notify = s.ipc.lab_sender(m.0)?;
        let lanceur = s.ipc.lab_sender(p.0)?;
        let roots = [s.ipc.racines_pilotes(m.0), s.ipc.racines_pilotes(p.0)];
        if roots != [3, 3] {
            return Err(crate::abi::REFUSED);
        }
        hal::log(format_args!(
            "D4 liaisons gestionnaire={} lanceur={} racines={roots:?} boites={} cloture=independante",
            m.0.number,
            p.0.number,
            s.ipc.pages_bulk_count()
        ));
        for (child, ports) in [(p.0, client), (m.0, manager)] {
            heap::with_pages(|mem| {
                let space = s.processes.get(child).space.as_ref().unwrap();
                for (i, v) in [
                    ports[0],
                    ports[1],
                    epoch,
                    id,
                    sid,
                    mode,
                    if child == m.0 { notify } else { 0 },
                    boot,
                    if child == m.0 { request[0] } else { server[0] },
                    if child == m.0 { request[1] } else { server[1] },
                    if child == p.0 { lanceur } else { 0 },
                ]
                .into_iter()
                .enumerate()
                {
                    space.write_data(mem, i, v);
                }
                Ok(())
            })
            .map_err(|_| crate::abi::NO_MEMORY)?;
        }
        #[cfg(gestionnaire_reprise)]
        {
            reprise::installer(s,m.0,notify,ancienne)?;
            reprise::installer(s,p.0,lanceur,(0,0))?;
            let racines=[s.ipc.racines_pilotes(m.0),s.ipc.racines_pilotes(p.0)];
            if racines!=[4,4]{return Err(crate::abi::REFUSED)}
            hal::log(format_args!("E41 liaisons initiales racines={racines:?} pages={}",s.ipc.pages()));
        }
        MARK.store(0, Ordering::Relaxed);
        CHANNEL.store(notify, Ordering::Relaxed);
        OBSERVER.store(m.0.number, Ordering::Release);
        Ok::<_, u64>((notify, lanceur))
    });
    match result {
        Ok((key, lanceur)) => Some((m, p, key, lanceur)),
        Err(_) => {
            for child in [p, m] {
                with(|s| {
                    s.ipc.death(child.0, ticks());
                    s.ipc.collect();
                });
                ipc::undo_unpublished(child);
            }
            None
        }
    }
}
fn stop(child: Child) -> Outcome {
    with(|s| {
        if s.processes.get(child.0).outcome.is_none() {
            stop_process(s, child.0, Outcome::Exit(125), hal::timer::counter())
        }
    });
    child.join()
}

/// Mesure imbriquée indépendante de l'échantillon de sélection ou de scellement.
#[cfg(feature = "gestionnaire-4f")]
static INTERVALLE: crate::sched::sync::Mutex<Option<(&'static str, u64, u64)>> =
    crate::sched::sync::Mutex::new(None);

#[cfg(feature = "gestionnaire-4f")]
fn terminer_intervalle(minimum: u64, fin: u64, suivant: Option<&'static str>) -> u64 {
    let mut courant = INTERVALLE.lock();
    let (phase, debut, precedent) = courant.take().expect("cycle Q35 ouvert");
    let total = precedent.min(minimum);
    *courant = suivant.map(|nom| (nom, fin, total));
    hal::log(format_args!("F4 physique phase={phase} avant={debut} minimum={minimum} apres={fin} pic={}",
        debut.saturating_sub(minimum)));
    total
}

// Chaque allocation du noyau met déjà à jour ce minimum sous le verrou des cadres.
// Les trois intervalles contigus couvrent aussi la réconciliation et la collecte.
#[cfg(feature = "gestionnaire-4f")]
pub(super) fn intervalle(nom: &'static str) {
    let (min, fin) = heap::with_pages(|m| Ok((m.cycle_minimum(true).unwrap(), m.free_pages()))).unwrap();
    terminer_intervalle(min, fin, Some(nom));
}

pub(crate) struct Cycle {
    initial: u64,
}
impl Cycle {
    pub(crate) fn ouvrir() -> Self {
        let initial = heap::with_pages(|m| {
            assert!(m.cycle_minimum(true).is_none());
            Ok(m.free_pages())
        })
        .unwrap();
        #[cfg(feature = "gestionnaire-4f")]
        { *INTERVALLE.lock() = Some(("admission", initial, initial)); }
        Self { initial }
    }
}
impl Drop for Cycle {
    fn drop(&mut self) {
        #[cfg(gestionnaire_reprise)]
        reprise::fin();
        let (min, end) =
            heap::with_pages(|m| Ok((m.cycle_minimum(false).unwrap(), m.free_pages()))).unwrap();
        #[cfg(feature = "gestionnaire-4f")]
        let min = terminer_intervalle(min, end, None);
        hal::log(format_args!(
            "D4 cycle avant={} minimum={min} apres={end} pic={} ipc={} dma={}",
            self.initial,
            self.initial.saturating_sub(min),
            with(|s| s.ipc.pages()),
            crate::memory::pinned_dma_frames()
        ));
        super::mesurer("4d_fin_cycle");
    }
}
pub(crate) struct Duree(&'static str, u64);
impl Duree {
    pub(crate) fn new(name: &'static str) -> Self {
        Self(name, hal::timer::counter())
    }
}
impl Drop for Duree {
    fn drop(&mut self) {
        hal::log(format_args!(
            "D4 duree phase={} ticks={}",
            self.0,
            hal::timer::counter() - self.1
        ));
    }
}
struct Sample {
    name: &'static str,
    initial: u64,
}
impl Sample {
    fn new(name: &'static str) -> Self {
        let initial = heap::with_pages(|m| {
            m.begin_frame_sample();
            Ok(m.free_pages())
        })
        .unwrap();
        Self { name, initial }
    }
}
impl Drop for Sample {
    fn drop(&mut self) {
        let (end, stats) =
            heap::with_pages(|m| Ok((m.free_pages(), m.end_frame_sample()))).unwrap();
        hal::log(format_args!(
            "D4 physique phase={} avant={} minimum={} apres={end} pic={} allocations={}",
            self.name,
            self.initial,
            stats.minimum_free,
            self.initial.saturating_sub(stats.minimum_free),
            stats.allocations
        ));
    }
}
fn measure(space: &UserSpace, mem: &mut crate::memory::Memory) -> (usize, usize, usize) {
    let (private, stack) = space.page_counts();
    // Pages privées initialement nulles. Le premier bloc non nul fournit une borne observée.
    let (mut at, top) = (space.stack_bottom(), space.stack_top());
    let mut bytes = [0u8; 128];
    while at < top {
        if !space.copy_from_user(mem, at, &mut bytes) || bytes.iter().any(|&b| b != 0) {
            break;
        }
        at += 128;
    }
    (private, stack, (top - at) as usize)
}

pub(super) fn piles_port(ids: [Pid; 3], op: u64) {
    with(|s| {
        heap::with_pages(|mem|{
        for (role,pid) in ["pilote","service","client"].into_iter().zip(ids){
            if let Some(space)=s.processes.get(pid).space.as_ref(){
                let (pages,stack,touched)=measure(space,mem);
                hal::log(format_args!("D4 pile role={role} op={op} privees={pages} pile_pages={stack} pile_bloc_touche={touched}"));
            }
        }
        Ok(())
    }).unwrap();
    });
}
pub(crate) fn selection(id: u64, sid: u64) -> Option<Decision> {
    #[cfg(gestionnaire_reprise)]
    let _selection=reprise::reserver_selection()?;
    let epoch = INVENTORY
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            n.checked_add(1).filter(|v| v >> 63 == 0)
        })
        .ok()?
        + 1;
    let mode = match option_env!("NOVAOS_4D_REFUS") {
        Some("signature") => 1,
        Some("revision") => 2,
        Some("racines") => 3,
        Some("lancement") => 4,
        Some("attribution") => 5,
        Some("droits") => 6,
        Some("cle-revoquee") => 7,
        Some("distribution") => 8,
        Some("deces-gestionnaire") => 9,
        Some("reservation") => 10,
        _ => 0,
    };
    let boot = crate::boot_id::current()?;
    #[cfg(gestionnaire_reprise)]
    hal::log(format_args!("E41 demarrage boot={:032x} inventaire={epoch}",u128::from_le_bytes(boot)));
    let lo = u64::from_le_bytes(boot[..8].try_into().ok()?);
    let hi = u64::from_le_bytes(boot[8..].try_into().ok()?);
    let boot = if lo != 0 { lo } else { hi };
    if boot == 0 {
        return None;
    }
    let _sample = Sample::new("selection");
    let before = heap::with_pages(|mem| Ok(mem.free_pages())).ok()?;
    let start = hal::timer::counter();
    let Some((m, p, key, lanceur)) = actors(epoch, id, sid, mode, boot) else {
        hal::log(format_args!(
            "D4 refus provisionnement epoch={epoch} mode={mode} ipc_pages={}",
            with(|s| s.ipc.pages())
        ));
        return None;
    };
    #[cfg(gestionnaire_reprise)]
    if reprise::premier_avant(){
        let dep=demande(m.0,key,true);
        if dep!=Some((1,epoch,1)){stop(m);stop(p);return None}
        let old=m.0;
        let inc=with(|s|s.processes.get(old).instance.serial);
        if reprise::garder_avant(old,key).is_none(){stop(m);stop(p);return None}
        stop(m);stop(p);OBSERVER.store(0,Ordering::Release);
        if with(|s|s.ipc.lab_can_submit(old,key)){return None}
        hal::log(format_args!("E41 avant-selection pid={} incarnation={inc} inventaire={epoch} anciennes_racines_refusees=true",old.number));
        drop(_sample);drop(_selection);
        return selection(id,sid);
    }
    let mp = m.0;
    let pp = p.0;
    let controle = Controle {
        actors: Some((m, p)),
        key: lanceur,
        epoch,
        mode,
    };
    let ready = (0..4).all(|i| {
        let msg = demande(mp, key, false);
        if msg != Some((i as u64 + 1, epoch, 1)) {
            hal::log(format_args!(
                "D4 controle interrompu marque={} depot={msg:?} appels={} temps={:?}",
                i + 1,
                MARK.load(Ordering::Relaxed),
                TIMES.each_ref().map(|a| a.load(Ordering::Relaxed))
            ));
        }

        msg == Some((i as u64 + 1, epoch, 1))
    });
    OBSERVER.store(0, Ordering::Release);
    if ready {
        let times = TIMES.each_ref().map(|a| a.load(Ordering::Acquire));
        hal::log(format_args!(
            "D4 phases horloge={} crypto_ticks={} inventaire_ticks={} resolution_ipc_ticks={}",
            hal::timer::counter_hz(),
            times[1] - times[0],
            times[2] - times[1],
            times[3] - times[2]
        ));
    }
    if ready {
        super::mesurer("4d_controle");
    }
    let report = if ready {
        with(|s| {
            if s.processes.get(mp).outcome.is_some() || !s.ipc.lab_can_submit(mp, key) {
                return None;
            }
            heap::with_pages(|mem|{
                let space=s.processes.get(mp).space.as_ref().unwrap();
                let data:[u64;27]=core::array::from_fn(|i|space.read_data(mem,i+24));
                let (pages,stack,touched)=measure(space,mem);
                hal::log(format_args!("D4 manager pid={} pair={} epoch={epoch} id={id:x} sid={sid} ELF={} pair_ELF={} privees={pages} pile_pages={stack} pile_bloc_touche={touched} libres={} avant={before} ipc_pages={} index={} documents={} inventaire={} registre={} plans={} recus={} ticks_controle={}",
                    mp.number,pp.number,MANAGER.len(),PEER.len(),mem.free_pages(),s.ipc.pages(),data[6],data[7],data[8],data[9],data[10],data[11],hal::timer::counter()-start));
                hal::log(format_args!("D4 tas politique={} plan={} reserve={} vivant={} pic_vivant={} bloc_libre_max={} octets_libres_et_metadonnees={}",
                    data[21],data[22],data[23],data[24],data[25],data[26],data[23].saturating_sub(data[24])));
                let peer=s.processes.get(pp).space.as_ref().ok_or(crate::memory::Error::Missing)?;
                let (privates,reserved,used)=measure(peer,mem);
                hal::log(format_args!("D4 pair privees={privates} pile_pages={reserved} pile_bloc_touche={used}"));
                peer.write_data(mem,19,epoch);
                space.write_data(mem,19,epoch);
                Ok(data)
            }).ok()
        })
    } else {
        None
    };
    let data = report?;
    if data[4] != epoch || data[5] != 1 {
        return None;
    }
    let mut digest = [0; 32];
    for (w, v) in digest.chunks_exact_mut(8).zip(&data[..4]) {
        w.copy_from_slice(&v.to_le_bytes())
    }
    let words = |offset: usize| -> [u8; 32] {
        let mut bytes = [0; 32];
        for (chunk, word) in bytes.chunks_exact_mut(8).zip(&data[offset..offset + 4]) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    };
    use sha2::{Digest, Sha256};
    if data[20] != 1 || Sha256::digest(PACKAGE)[..] != words(16) {
        return None;
    }
    if mode == 2 {
        INVENTORY.fetch_add(1, Ordering::AcqRel);
        hal::log(format_args!("D4 plan perime revision={epoch}"));
    }
    #[allow(unused_mut)]
    let mut decision = Decision {
        epoch,
        id,
        sid,
        digest,
        plan: words(12),
        catalog: data[20],
        policy: data[5],
        liaison: Some(controle),
        cadres_controle: before.checked_sub(heap::audit(false).free_pages)?,
        #[cfg(all(feature="gestionnaire-4f", feature="net-q35-duo"))]
        titulaire: None,
    };
    if !decision.actuelle(id, sid) {
        return None;
    }
    #[cfg(gestionnaire_reprise)]
    {
        let mut c=decision.liaison.take()?;
        reprise::ouvrir(&mut c,&decision)?;
        decision.liaison=Some(c);
    }
    ACTIVATION.store(hal::timer::counter(), Ordering::Relaxed);
    RETRAIT.store(0, Ordering::Relaxed);
    Some(decision)
}

fn sceller(d: &Decision) -> Option<user::SealedImage> {
    let mut digest = d.digest;
    if option_env!("NOVAOS_4D_REFUS") == Some("lancement") || (cfg!(gestionnaire_reprise) && option_env!("NOVAOS_4E1_CAS")==Some("refus-certain")) {
        digest[0] ^= 1;
    }
    let epoch = d.epoch;
    let header = u32::from_le_bytes(PACKAGE.get(..4)?.try_into().ok()?) as usize;
    let payload = PACKAGE.get(header.checked_add(68)?..)?;
    let object = user::ObjectRevision {
        volume: crate::boot_id::current()?,
        object: 1,
        revision: 1,
        length: payload.len(),
    };
    let mut staging = user::ImageStaging::new(object, digest).ok()?;
    for (i, chunk) in payload.chunks(4096).enumerate() {
        staging = staging.append(object, i * 4096, chunk).ok()?;
    }
    let image = staging
        .seal()
        .map_err(|_| hal::log(format_args!("D4 refus lancement empreinte epoch={epoch}")))
        .ok()?;
    hal::log(format_args!(
        "D4 objet scelle epoch={epoch} octets={} sha256={:02x?}",
        payload.len(),
        image.digest()
    ));
    Some(image)
}

pub(super) struct Controle {
    actors: Option<(Child, Child)>,
    key: u64,
    epoch: u64,
    mode: u64,
}
impl Controle {
    fn configurer(&self, pair: bool, fields: &[(usize, u64)]) -> Option<()> {
        let (m, p) = self.actors.as_ref()?;
        let pid = if pair { p.0 } else { m.0 };
        with(|s| {
            let actor = s.processes.get(pid);
            if actor.outcome.is_some() {
                return None;
            }
            let space = actor.space.as_ref()?;
            heap::with_pages(|mem| {
                for &(index, v) in fields {
                    space.write_data(mem, index, v);
                }
                Ok(())
            })
            .ok()
        })
    }
    fn demande(&self, op: u64, id: u64) -> bool {
        self.actors.as_ref().is_some_and(|(_, p)| {
            demande(p.0, self.key, false) == Some((op, self.epoch | (1 << 63), id))
        })
    }
    #[cfg(gestionnaire_reprise)]
    pub(super) fn confirmer(self,pid:Pid,g:u64,broker:u64)->bool{reprise::cree(pid,g,broker)}
    #[cfg(not(gestionnaire_reprise))]
    pub(super) fn confirmer(mut self, pid: Pid) -> bool {
        let inc = with(|s| s.processes.get(pid).instance.serial);
        if self
            .configurer(true, &[(14, pid.number), (15, inc), (13, 2)])
            .is_none()
            || !self.demande(7, 4)
            || self.configurer(true, &[(16, 1)]).is_none()
        {
            return false;
        }
        let (m, p) = self.actors.take().unwrap();
        let outcomes = [m.join(), p.join()];
        hal::log(format_args!(
            "D4 controle epoch={} recus=true acteurs={outcomes:?} mode={}",
            self.epoch, self.mode
        ));
        hal::log(format_args!(
            "D4 lanceur prepare=1 creation=1 recu=consulte rejeu=sans_effet ack=4 pid={} incarnation={inc}",
            pid.number
        ));
        outcomes == [Outcome::Exit(0); 2]
    }
}
impl Drop for Controle {
    fn drop(&mut self) {
        if let Some((m, p)) = self.actors.take() {
            let outcomes = [stop(m), stop(p)];
            hal::log(format_args!(
                "D4 controle epoch={} recus=false acteurs={outcomes:?} mode={}",
                self.epoch, self.mode
            ));
        }
    }
}
pub(super) struct Ticket {
    decision: Decision,
    image: user::SealedImage,
    generation: u64,
}
impl Decision {
    #[cfg(gestionnaire_reprise)]
    pub(super) fn preparer(mut self,generation:u64)->Option<(Ticket,Controle,u64)>{
        if !self.actuelle(self.id,self.sid){return None}
        let controle=self.liaison.take()?;
        let base=heap::audit(false).free_pages;
        reprise::preparer(generation)?;
        let Some(image)=sceller(&self) else {reprise::refuser(generation)?;return None};
        reprise::engager(generation)?;
        Some((Ticket{decision:self,image,generation},controle,base))
    }
    #[cfg(not(gestionnaire_reprise))]
    pub(super) fn preparer(mut self, generation: u64) -> Option<(Ticket, Controle, u64)> {
        if !self.actuelle(self.id, self.sid) || generation == 0 {
            return None;
        }
        let controle = self.liaison.take()?;
        let base = heap::audit(false)
            .free_pages
            .checked_add(self.cadres_controle)?;
        controle.configurer(true, &[(12, generation)])?;
        controle.configurer(false, &[(12, generation)])?;
        if !controle.demande(5, 1) {
            return None;
        }
        let image = sceller(&self)?;
        controle.configurer(true, &[(13, 1)])?;
        if !controle.demande(6, 2) {
            return None;
        }
        Some((
            Ticket {
                decision: self,
                image,
                generation,
            },
            controle,
            base,
        ))
    }
}
pub(super) fn lancer(ticket: Ticket, generation: u64) -> Option<(Child, u64)> {
    let Ticket {
        decision: d,
        image,
        generation: attendue,
    } = ticket;
    if option_env!("NOVAOS_4D_REFUS") == Some("attribution") {
        INVENTORY.fetch_add(1, Ordering::AcqRel);
    }
    if !d.actuelle(d.id, d.sid)
        || generation != attendue
        || !crate::hardware::block::verifier_port_selectionne(d.id, d.sid)
    {
        hal::log(format_args!(
            "D4 refus attribution revision={} generation={generation}",
            d.epoch
        ));
        return None;
    }
    #[cfg(all(feature="gestionnaire-4f", feature="net-q35-duo"))]
    let titulaire = d.titulaire?;
    let start = hal::timer::counter();
    let _sample = Sample::new("lancement");

    let (space, (fichier, zeros)) = heap::with_pages(|mem| {
        let space = UserSpace::installed(mem, &image, &3u64.to_le_bytes())?;
        match space.verifier_image_chargee(mem, &image) {
            Ok(n) => Ok((space, n)),
            Err(e) => {
                space.release(mem);
                Err(e)
            }
        }
    })
    .ok()?;
    hal::log(format_args!(
        "D4 PT_LOAD compares={fichier} zeros={zeros} avant_capacites=true"
    ));
    let (private, stack) = space.page_counts();
    drop(image);
    // Portée de la garde de setup : le thread ne reçoit pas le CPU avant ses propres liaisons.
    // Identifiant applicatif du seul paquet de ce profil. Le bit haut le place
    // hors des identités système. La capacité native reste liée à l’incarnation complète.
    let app = u64::from_le_bytes(d.digest[..8].try_into().ok()?) | (1 << 63);
    #[cfg(all(feature="gestionnaire-4f", feature="net-q35-duo"))]
    let child = publish_image_numerote(space, None, app, None, Some(titulaire)).ok()?;
    #[cfg(not(all(feature="gestionnaire-4f", feature="net-q35-duo")))]
    let child = publish_image(space, None, app, None).ok()?;
    let pid = child.0;
    let granted = installer_courtier(child, 3, None, generation)?;
    let inc = with(|s| s.processes.get(pid).instance.serial);
    hal::log(format_args!(
        "D4 lancement scelle epoch={} pid={} incarnation={inc} generation={generation} privees={private} pile_pages={stack} ticks={}",
        d.epoch,
        pid.number,
        hal::timer::counter() - start
    ));
    hal::log(format_args!(
        "D4 plan empreinte={:02x?} catalogue={} politique={}",
        d.plan, d.catalog, d.policy
    ));
    Some(granted)
}
