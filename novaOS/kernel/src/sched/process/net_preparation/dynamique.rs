//! Profil ARM fermé. Le superviseur garde la seule autorité matérielle 3E.
use super::*;
use core::sync::atomic::{AtomicU64, Ordering};
const MANAGER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/gestionnaire.elf"));
const PEER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pair.elf"));
const PACKAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pilote.pkg01"));
static INVENTORY: AtomicU64 = AtomicU64::new(0);

static OBSERVER: AtomicU64 = AtomicU64::new(0);
static CHANNEL: AtomicU64 = AtomicU64::new(0);
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
}
impl Decision {
    pub(crate) fn actuelle(&self, id: u64, sid: u64) -> bool {
        self.epoch == INVENTORY.load(Ordering::Acquire)
            && self.id == id
            && self.sid == sid
            && self.catalog == 1
            && self.policy == 1
    }
}
fn actors(epoch: u64, id: u64, sid: u64, mode: u64, boot: u64) -> Option<(Child, Child, u64)> {
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
        let notify = s.ipc.lab_sender(m.0)?;
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
        MARK.store(0, Ordering::Relaxed);
        CHANNEL.store(notify, Ordering::Relaxed);
        OBSERVER.store(m.0.number, Ordering::Release);
        Ok::<_, u64>(notify)
    });
    match result {
        Ok(key) => Some((m, p, key)),
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
        Self { initial }
    }
}
impl Drop for Cycle {
    fn drop(&mut self) {
        let (min, end) =
            heap::with_pages(|m| Ok((m.cycle_minimum(false).unwrap(), m.free_pages()))).unwrap();
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

pub(crate) fn selection(id: u64, sid: u64) -> Option<Decision> {
    let epoch = INVENTORY
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        .ok()?
        + 1;
    let mode = match option_env!("NOVAOS_4D_REFUS") {
        Some("signature") => 1,
        Some("revision") => 2,
        Some("racines") => 3,
        Some("lancement") => 4,
        _ => 0,
    };
    let boot = crate::boot_id::current()?;
    let lo = u64::from_le_bytes(boot[..8].try_into().ok()?);
    let hi = u64::from_le_bytes(boot[8..].try_into().ok()?);
    let boot = if lo != 0 { lo } else { hi };
    if boot == 0 {
        return None;
    }
    let _sample = Sample::new("selection");
    let before = heap::with_pages(|mem| Ok(mem.free_pages())).ok()?;
    let start = hal::timer::counter();
    let Some((m, p, key)) = actors(epoch, id, sid, mode, boot) else {
        hal::log(format_args!(
            "D4 refus provisionnement epoch={epoch} mode={mode} ipc_pages={}",
            with(|s| s.ipc.pages())
        ));
        return None;
    };
    let mp = m.0;
    let pp = p.0;
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
    let outcomes = if report.is_some() {
        [m.join(), p.join()]
    } else {
        [stop(m), stop(p)]
    };
    hal::log(format_args!(
        "D4 controle epoch={epoch} recus={ready} acteurs={outcomes:?} mode={mode}"
    ));
    let data = report?;
    if outcomes != [Outcome::Exit(0); 2] || data[4] != epoch || data[5] != 1 {
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
    let decision = Decision {
        epoch,
        id,
        sid,
        digest,
        plan: words(12),
        catalog: data[20],
        policy: data[5],
    };
    decision.actuelle(id, sid).then_some(decision)
}

fn sceller(d: &Decision) -> Option<user::SealedImage> {
    let mut digest = d.digest;
    if option_env!("NOVAOS_4D_REFUS") == Some("lancement") {
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

pub(super) fn lancer(d: Decision, generation: u64) -> Option<(Child, u64)> {
    if !d.actuelle(0x10411af4, 56) || generation == 0 {
        return None;
    }
    let start = hal::timer::counter();
    let _sample = Sample::new("lancement");
    let image = sceller(&d)?;
    let space =
        heap::with_pages(|mem| UserSpace::installed(mem, &image, &3u64.to_le_bytes())).ok()?;
    let (private, stack) = space.page_counts();
    drop(image);
    // Portée de la garde de setup : le thread ne reçoit pas le CPU avant ses propres liaisons.
    // Identifiant applicatif du seul paquet de ce profil. Le bit haut le place
    // hors des identités système. La capacité native reste liée à l’incarnation complète.
    let app = u64::from_le_bytes(d.digest[..8].try_into().ok()?) | (1 << 63);
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
