//! Autorité volatile mono-groupe. La mémoire du NIC reste la propriété de Preparation.
//! Le journal ne possède aucune opération de restitution de cadre.
use super::*;
use crate::abi;
use crate::sched::sync::Mutex;
use core::mem::size_of_val;
use core::sync::atomic::AtomicBool;
#[path = "../../../../../../user/gestionnaire-4d/reprise_wire.rs"]
mod wire;
use wire::*;
type Identity = crate::instance_abi::Identity;

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static FROZEN: AtomicBool = AtomicBool::new(false);
static NEXT: AtomicU64 = AtomicU64::new(0);
static BEFORE: AtomicBool = AtomicBool::new(false);
// Un seul démarrage, un seul témoin interrompu avant que le premier plan existe.
static AVANT: Mutex<Option<(Pid, Identity, u64, u64, u64)>> = Mutex::new(None);
pub(super) fn garder_avant(p: Pid, cle: u64) -> Option<()> {
    let id = identity(p)?;
    let link = with(|s| {
        heap::with_pages(|m| {
            Ok(s.processes
                .get(p)
                .space
                .as_ref()
                .unwrap()
                .read_data(m, LINK))
        })
        .ok()
    })?;
    *AVANT.lock() = Some((p, id, cle, link, hal::timer::counter()));
    Some(())
}
pub(super) fn ancienne_selection() -> (u64, u64) {
    AVANT.lock().as_ref().map_or((0, 0), |v| (v.2, v.3))
}

pub(crate) fn scenario() -> &'static str {
    option_env!("NOVAOS_4E1_CAS").unwrap_or("nominal")
}
fn prochain() -> Option<u64> {
    NEXT.try_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
        .ok()?
        .checked_add(1)
}
static SELECTION: AtomicBool = AtomicBool::new(false);
pub(super) struct ReservationSelection;
impl Drop for ReservationSelection {
    fn drop(&mut self) {
        SELECTION.store(false, Ordering::Release)
    }
}
pub(super) fn reserver_selection() -> Option<ReservationSelection> {
    SELECTION
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()?;
    let guard = ReservationSelection;
    if FROZEN.load(Ordering::Acquire) || SESSION.lock().is_some() {
        return None;
    }
    if scenario() == "limite-numeros" {
        let _ = NEXT.compare_exchange(0, u64::MAX - 1, Ordering::AcqRel, Ordering::Acquire);
    }
    Some(guard)
}
pub(super) fn premier_avant() -> bool {
    scenario() == "avant-selection" && !BEFORE.swap(true, Ordering::AcqRel)
}
fn identity(p: Pid) -> Option<Identity> {
    with(|s| {
        s.processes
            .slots
            .get(p.slot)?
            .as_ref()
            .filter(|process| process.pid == p)?
            .instance
            .identity(crate::boot_id::current())
    })
}
fn fields(p: Pid, values: &[(usize, u64)]) -> Option<()> {
    with(|s| {
        let actor = s.processes.get(p);
        if actor.outcome.is_some() {
            return None;
        }
        let space = actor.space.as_ref()?;
        heap::with_pages(|m| {
            for &(i, v) in values {
                space.write_data(m, i, v)
            }
            Ok(())
        })
        .ok()
    })
}
pub(super) fn installer(s: &mut Core, p: Pid, tx: u64, old: (u64, u64)) -> Result<(), u64> {
    let rx = s.ipc.lab_receiver(p)?;
    let link = prochain().ok_or_else(|| {
        hal::log(format_args!(
            "E41 refus numero_epuise precedent={} effet_materiel=0",
            NEXT.load(Ordering::Acquire)
        ));
        abi::REFUSED
    })?;
    let space = s.processes.get(p).space.as_ref().ok_or(abi::REVOKED)?;
    heap::with_pages(|m| {
        for (i, v) in [
            (TX, tx),
            (RX, rx),
            (LINK, link),
            (OLD_KEY, old.0),
            (OLD_LINK, old.1),
        ] {
            space.write_data(m, i, v);
        }
        Ok(())
    })
    .map_err(|_| abi::NO_MEMORY)
}
// Tout candidat, même refusé au milieu de sa prise de contrôle, est récolté.
struct Enfant {
    enfant: Option<Child>,
    pid: Pid,
}
impl Enfant {
    fn arreter(mut self) -> Outcome {
        stop(self.enfant.take().unwrap())
    }
    fn join(mut self) -> Outcome {
        self.enfant.take().unwrap().join()
    }
}
impl Drop for Enfant {
    fn drop(&mut self) {
        if let Some(c) = self.enfant.take() {
            stop(c);
        }
    }
}
struct Actor {
    child: Enfant,
    id: Identity,
    role: u64,
    tx: u64,
    rx: u64,
    link: u64,
    sequence: u64,
    old: u64,
}
impl Actor {
    fn attacher(child: Child, role: u64) -> Option<Self> {
        let p = child.0;
        let params = with(|s| {
            let space = s.processes.get(p).space.as_ref()?;
            heap::with_pages(|m| Ok([TX, RX, LINK, OLD_LINK].map(|i| space.read_data(m, i)))).ok()
        });
        let Some(([tx, rx, link, old], id)) = params.zip(identity(p)) else {
            stop(child);
            return None;
        };
        Some(Self {
            child: Enfant {
                enfant: Some(child),
                pid: p,
            },
            id,
            role,
            tx,
            rx,
            link,
            sequence: 0,
            old,
        })
    }
    fn nouveau(role: u64, old: (u64, u64)) -> Option<Self> {
        let _guard = Guard::new();
        let child = create_image(
            if role == wire::MANAGER {
                MANAGER_IMAGE
            } else {
                PEER
            },
            &[],
            None,
            None,
        )
        .ok()?;
        let p = child.0;
        let result = with(|s| {
            let tx = s.ipc.lab_sender(p)?;
            installer(s, p, tx, old)?;
            if s.ipc.racines_pilotes(p) != 2 {
                return Err(abi::REFUSED);
            }
            Ok(())
        });
        if result.is_err() || fields(p, &[(RESTART, 1)]).is_none() {
            with(|s| {
                s.ipc.death(p, ticks());
                s.ipc.collect()
            });
            ipc::undo_unpublished(child);
            return None;
        }
        drop(_guard);
        Self::attacher(child, role)
    }
    fn notifier(&self, bits: u64) -> Option<()> {
        with(|s| {
            if s.processes.get(self.child.pid).outcome.is_some()
                || !s.ipc.lab_notify(self.child.pid, self.rx, bits)
            {
                return None;
            }
            s.flush_ipc();
            Some(())
        })
    }
    fn message(&self) -> Option<(u64, u64, u64)> {
        demande(self.child.pid, self.tx, true)
    }
    fn commencer(&mut self, op: u64, intention: u64) -> Option<()> {
        if identity(self.child.pid) != Some(self.id) {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        fields(
            self.child.pid,
            &[
                (COMMAND, op),
                (NUMBER, self.sequence),
                (OPERATION, intention),
            ],
        )?;
        self.notifier(1)
    }
    fn recevoir(&self, op: u64, intention: u64) -> Option<()> {
        let numero = if op == LAUNCH {
            intention
        } else {
            self.sequence
        };
        let valide = |m: (u64, u64, u64)| m == (op, self.link, numero);
        if self.old != 0 && self.sequence == 1 {
            let ancien = self.message()?;
            if valide(ancien) || ancien != (op, self.old, numero) {
                return None;
            }
            self.notifier(4)?;
        }
        valide(self.message()?).then_some(())
    }
    fn transmettre(&self, vue: &[u64; WORDS]) -> Option<()> {
        let p = self.child.pid;
        with(|s| {
            let space = s.processes.get(p).space.as_ref()?;
            heap::with_pages(|m| {
                for (i, &v) in vue.iter().enumerate() {
                    space.write_data(m, SNAP + i, v)
                }
                space.write_data(m, STATUS, 0);
                Ok(())
            })
            .ok()
        })?;
        self.notifier(2)?;
        for (i, word) in digest(self.role, self.link, self.sequence, vue)
            .into_iter()
            .enumerate()
        {
            if self.message() != Some((ACK + i as u64, self.link, word)) {
                return None;
            }
        }
        Some(())
    }
}
const MANAGER_IMAGE: &[u8] = super::MANAGER;
#[derive(Default)]
struct Journal {
    boot: [u8; 16],
    inventory: u64,
    plan: [u8; 32],
    generation: u64,
    frames: [u64; 22],
    mapped: u64,
    result: u64,
    intent: u64,
    broker: u64,
    driver: Option<(Pid, Identity)>,
    receipt: u64,
    pub_seq: u64,
    pub_result: Option<usize>,
    pub_delivered: bool,
    state: u64,
    launches: u32,
    controller: u64,
    group: u64,
}
impl Journal {
    fn vue(&self) -> [u64; WORDS] {
        let mut s = [0; WORDS];
        s[0] = u64::from_le_bytes(self.boot[..8].try_into().unwrap());
        s[1] = u64::from_le_bytes(self.boot[8..].try_into().unwrap());
        s[2] = self.controller;
        s[3] = self.group;
        s[4] = self.inventory;
        s[5] = self.generation;
        s[6] = self.result;
        if let Some((p, id)) = self.driver {
            s[7] = p.number;
            s[8] = id.instance;
            s[9] = id.generation
        }
        s[10] = self
            .frames
            .iter()
            .enumerate()
            .fold(0, |m, (i, &p)| m | (u64::from(p != 0) << i));
        s[11] = self.mapped;
        s[12] = self.receipt;
        s[13] = self.pub_seq;
        for (i, b) in self.plan.chunks_exact(8).enumerate() {
            s[14 + i] = u64::from_le_bytes(b.try_into().unwrap())
        }
        s[18] = self.state;
        s[19] = self.intent;
        s[20] = u64::from(self.pub_result.is_some());
        s[21] = self.pub_result.unwrap_or(0) as u64;
        s[22] = u64::from(self.pub_delivered);
        s
    }
    fn corroborer(&self) -> bool {
        self.boot == crate::boot_id::current().unwrap_or([0; 16])
            && self.controller == CONTROLEUR
            && self.group == 56
            && self.inventory == INVENTORY.load(Ordering::Acquire)
            && (self.generation == 0
                || self.state >= 5
                || crate::hardware::block::carte_bail_nic(self.generation, self.frames)
                    == Some(self.mapped))
            && (self.state >= 2
                || self.driver.is_none_or(|(p, id)| {
                    identity(p) == Some(id)
                        && with(|s| {
                            s.processes.get(p).outcome.is_none()
                                && s.ipc.lab_can_submit(p, self.broker)
                        })
                }))
    }
    fn autoriser(&mut self, g: u64, intention: u64) -> Option<bool> {
        if intention == 0
            || intention != self.intent
            || g == 0
            || self.generation != g
            || !self.corroborer()
        {
            return None;
        }
        match self.result {
            RESERVED if self.mapped == FULL && self.state == 0 => {
                self.result = ENGAGED;
                Some(true)
            }
            CONFIRMED | ENGAGED | DENIED_LAUNCH => Some(false),
            _ => None,
        }
    }
}
struct Vacance {
    role: u64,
    start: u64,
    pid: u64,
    ident: Identity,
    ancien: u64,
    epoch: u64,
}
struct Session {
    vacance: Option<Vacance>,
    journal: Journal,
    manager: Option<Actor>,
    launcher: Option<Actor>,
    injected: bool,
}
impl Session {
    fn slot(&mut self, role: u64) -> &mut Option<Actor> {
        if role == wire::MANAGER {
            &mut self.manager
        } else {
            &mut self.launcher
        }
    }
    fn dialogue(&mut self, role: u64, op: u64) -> Option<()> {
        if self.echanger(role, op).is_some() {
            return Some(());
        }
        // Une expiration ou une réponse illisible ne prouvent jamais un décès.
        if op == END || !self.deces_confirme(role)? {
            return None;
        }
        self.constater_vacance(role, hal::timer::counter())?;
        self.reprendre(if role == LAUNCHER {
            "lanceur-observe"
        } else {
            "deces-observe"
        })?;
        self.echanger(role, op)
    }
    fn echanger(&mut self, role: u64, op: u64) -> Option<()> {
        if !self.journal.corroborer() {
            return None;
        }
        let vue = self.journal.vue();
        EtatRepris::lire(vue, None)?;
        let a = self.slot(role).as_mut()?;
        a.commencer(op, vue[19])?;
        a.recevoir(op, vue[19])?;
        a.transmettre(&vue)
    }
    fn deces_confirme(&mut self, role: u64) -> Option<bool> {
        let p = self.slot(role).as_ref()?.child.pid;
        with(|s| {
            s.processes
                .slots
                .get(p.slot)?
                .as_ref()
                .filter(|v| v.pid == p)
                .map(|v| v.outcome.is_some())
        })
    }
    fn injecter_deces(&mut self, role: u64) -> Option<()> {
        let p = self.slot(role).as_ref()?.child.pid;
        with(|s| {
            if s.processes.get(p).outcome.is_some() {
                return None;
            }
            stop_process(s, p, Outcome::Exit(125), hal::timer::counter());
            Some(())
        })
    }
    fn interrompre(&mut self, role: u64) -> Option<()> {
        let debut = hal::timer::counter();
        self.injecter_deces(role)?;
        self.constater_vacance(role, debut)
    }
    fn constater_vacance(&mut self, role: u64, start: u64) -> Option<()> {
        if self.vacance.is_some() || !self.deces_confirme(role)? {
            return None;
        }
        let old = self.slot(role).take()?;
        let (pid, ident, ancien, epoch) = (old.child.pid.number, old.id, old.tx, old.link);
        let old_pid = old.child.pid;
        old.child.join();
        if with(|s| s.ipc.lab_can_submit(old_pid, ancien)) {
            return None;
        }
        super::super::mesurer("4e1_sans_controle");
        self.vacance = Some(Vacance {
            role,
            start,
            pid,
            ident,
            ancien,
            epoch,
        });
        hal::log(format_args!(
            "E41 vacance role={role} pid={pid} generation={} epinglees={}",
            self.journal.generation,
            self.journal.vue()[10]
        ));
        Some(())
    }
    fn attribuer(&mut self, candidat: Actor, ancien: Identity, epoch: u64) -> Result<(), Actor> {
        let role = candidat.role;
        if self.slot(role).is_some()
            || candidat.old != epoch
            || candidat.link == epoch
            || candidat.id.boot != ancien.boot
            || candidat.id.instance == ancien.instance
            || identity(candidat.child.pid) != Some(candidat.id)
            || !self.journal.corroborer()
        {
            return Err(candidat);
        }
        *self.slot(role) = Some(candidat);
        Ok(())
    }
    fn reprendre(&mut self, phase: &str) -> Option<()> {
        let Vacance {
            role,
            start,
            pid,
            ident,
            ancien,
            epoch,
        } = self.vacance.take()?;
        let mut new = Actor::nouveau(role, (ancien, epoch))?;
        let id = new.id;
        let np = new.child.pid;
        let fresh = new.link;
        if id == ident
            || id.boot != ident.boot
            || fresh == epoch
            || with(|s| s.ipc.lab_can_submit(np, ancien))
        {
            new.child.arreter();
            return None;
        }
        // Les deux candidats peuvent déposer leur demande avant l'attribution du rôle.
        let concurrence = matches!(scenario(), "concurrence" | "double-deces");
        let mut second = if concurrence {
            Some(Actor::nouveau(role, (ancien, epoch))?)
        } else {
            None
        };
        if concurrence {
            super::super::mesurer("4e1_candidats");
        }
        let view = self.journal.vue();
        new.commencer(READ, 0)?;
        if let Some(a) = second.as_mut() {
            a.commencer(READ, 0)?;
        }
        new.recevoir(READ, 0)?;
        if let Some(a) = second.as_ref() {
            a.recevoir(READ, 0)?;
        }
        self.attribuer(new, ident, epoch).ok()?;
        if let Some(a) = second {
            let a = self.attribuer(a, ident, epoch).err()?;
            fields(a.child.pid, &[(STATUS, abi::REVOKED)])?;
            a.notifier(2)?;
            let loser = a.child.pid.number;
            let exit = a.child.join();
            if exit != Outcome::Exit(abi::REVOKED) {
                return None;
            }
            hal::log(format_args!(
                "E41 concurrence gagnant={} refuse={loser} sortie={exit:?} vues_perdant=0",
                np.number
            ));
        }
        self.slot(role).as_ref()?.transmettre(&view)?;
        let dt = hal::timer::counter() - start;
        let v = self.journal.vue();
        hal::log(format_args!(
            "E41 reprise phase={phase} role={role} ancien_pid={pid} nouveau_pid={} ancien_inc={} nouvel_inc={} ancienne_liaison={epoch} nouvelle_liaison={fresh} generation={} issue={} pilote={} epingles={} mappes={} recu={} publication={} etat={} intention={} ticks={dt} hz={} ancien_refuse=true",
            np.number,
            ident.instance,
            id.instance,
            v[5],
            v[6],
            v[7],
            v[10],
            v[11],
            v[12],
            v[13],
            v[18],
            v[19],
            hal::timer::counter_hz()
        ));
        super::super::mesurer("4e1_reprise");
        Some(())
    }
    fn panne(&mut self, phase: &str, role: u64) -> Option<()> {
        let choisis = scenario() == phase
            || (phase == "attribution"
                && matches!(scenario(), "endurance" | "concurrence" | "pression"))
            || (phase == "reconciliation" && scenario() == "double-deces");
        if !self.injected && choisis {
            self.injected = true;
            self.interrompre(role)?;
            self.reprendre(phase)?
        }
        Some(())
    }
    fn fermer_acteurs(&mut self) {
        for role in [wire::MANAGER, LAUNCHER] {
            let ok = !FROZEN.load(Ordering::Acquire) && self.dialogue(role, END).is_some();
            if let Some(a) = self.slot(role).take() {
                if ok && a.notifier(8).is_some() {
                    if a.child.join() != Outcome::Exit(0) {
                        FROZEN.store(true, Ordering::Release)
                    }
                } else {
                    a.child.arreter();
                    FROZEN.store(true, Ordering::Release);
                }
            }
        }
    }
}
fn avec<T>(f: impl FnOnce(&mut Session) -> Option<T>) -> Option<T> {
    let mut lock = SESSION.lock();
    if FROZEN.load(Ordering::Acquire) {
        return None;
    }
    let r = f(lock.as_mut()?);
    if r.is_none() {
        FROZEN.store(true, Ordering::Release)
    }
    r
}
pub(super) fn ouvrir(controls: &mut Controle, decision: &Decision) -> Option<()> {
    let mut lock = SESSION.lock();
    if lock.is_some() || FROZEN.load(Ordering::Acquire) {
        return None;
    }
    let boot = crate::boot_id::current()?;
    let (m, p) = controls.actors.take()?;
    let Some(manager) = Actor::attacher(m, wire::MANAGER) else {
        stop(p);
        return None;
    };
    let Some(launcher) = Actor::attacher(p, LAUNCHER) else {
        manager.child.arreter();
        return None;
    };
    let journal = Journal {
        boot,
        inventory: decision.epoch,
        plan: decision.plan,
        generation: 0,
        frames: [0; 22],
        mapped: 0,
        result: RESERVED,
        intent: 0,
        broker: 0,
        driver: None,
        receipt: 0,
        pub_seq: 0,
        pub_result: None,
        pub_delivered: false,
        state: 0,
        launches: 0,
        controller: CONTROLEUR,
        group: decision.sid,
    };
    *lock = Some(Session {
        vacance: None,
        journal,
        manager: Some(manager),
        launcher: Some(launcher),
        injected: false,
    });
    drop(lock);
    hal::log(format_args!(
        "E41 autorite boot={:032x} controleur={CONTROLEUR} groupe={} inventaire={}",
        u128::from_le_bytes(boot),
        decision.sid,
        decision.epoch
    ));
    hal::log(format_args!(
        "E41 journal octets={} autorite_octets={} auxiliaires_octets={} slots_lancement=1 reserve_cloture=1",
        size_of::<Journal>(),
        size_of::<Mutex<Option<Session>>>(),
        size_of_val(&NEXT)
            + size_of_val(&FROZEN)
            + size_of_val(&BEFORE)
            + size_of_val(&AVANT)
            + size_of_val(&SELECTION)
    ));
    avec(|s| {
        s.dialogue(wire::MANAGER, READ)?;
        s.dialogue(LAUNCHER, READ)?;
        if let Some((p, id, _, old, start)) = AVANT.lock().take() {
            let a = s.manager.as_ref()?;
            if a.id.boot != id.boot
                || a.id.instance == id.instance
                || a.link == old
                || p == a.child.pid
            {
                return None;
            }
            hal::log(format_args!(
                "E41 reprise_selection ancien_pid={} nouveau_pid={} ancien_inc={} nouvel_inc={} ancienne_liaison={old} nouvelle_liaison={} ticks={} hz={} ancien_refuse=true",
                p.number,
                a.child.pid.number,
                id.instance,
                a.id.instance,
                a.link,
                hal::timer::counter() - start,
                hal::timer::counter_hz()
            ));
        }
        s.panne("apres-selection", wire::MANAGER)
    })
}
/// Le Q35 publie la table entière avant l'invalidation. Un seul relevé du
/// propriétaire atteste les 22 traductions dans la même génération.
#[cfg(feature = "gestionnaire-4f")]
pub(crate) fn lot_vtd(g: u64, frames: [u64; 22]) -> bool {
    avec(|s| {
        if g == 0 || s.journal.generation != 0 || frames.contains(&0)
            || crate::hardware::block::carte_bail_nic(g, frames) != Some(FULL) {
            return None;
        }
        s.journal.generation = g;
        s.journal.frames = frames;
        s.journal.mapped = FULL;
        s.panne("reservation", wire::MANAGER)
    }).is_some()
}

#[cfg(target_arch = "aarch64")]
pub(crate) fn map(g: u64, n: usize, pa: u64) -> bool {
    avec(|s| {
        let j = &mut s.journal;
        if n >= 22 || pa == 0 || j.frames[n] != 0 || (j.generation != 0 && g != j.generation) {
            return None;
        }
        j.generation = g;
        j.frames[n] = pa;
        j.mapped |= 1 << n;
        if n == 2 {
            s.panne("reservation", wire::MANAGER)?
        }
        Some(())
    })
    .is_some()
}
pub(super) fn preparer(g: u64) -> Option<()> {
    avec(|s| {
        if s.journal.generation != g || s.journal.mapped != FULL {
            return None;
        }
        s.journal.intent = prochain()?;
        s.panne("lanceur-avant", LAUNCHER)?;
        s.dialogue(wire::MANAGER, PREPARE)?;
        s.dialogue(LAUNCHER, PREPARE)
    })
}
pub(in super::super) fn refuser(g: u64) -> Option<()> {
    avec(|s| {
        if s.journal.generation != g
            || !matches!(s.journal.result, RESERVED | ENGAGED)
            || s.journal.driver.is_some()
        {
            return None;
        }
        s.journal.result = DENIED_LAUNCH;
        s.panne("refus-certain", LAUNCHER)?;
        s.dialogue(LAUNCHER, RECEIPT)?;
        hal::log(format_args!(
            "E41 refus certain generation={g} intention={} creations=0",
            s.journal.intent
        ));
        Some(())
    })
}
pub(super) fn engager(g: u64) -> Option<()> {
    avec(|s| {
        s.dialogue(LAUNCHER, LAUNCH)?;
        if !s.journal.autoriser(g, s.journal.intent)? {
            return None;
        }
        s.panne("lanceur-engage", LAUNCHER)?;
        if scenario() == "lanceur-engage" {
            s.dialogue(LAUNCHER, LAUNCH)?;
            if s.journal.autoriser(g, s.journal.intent)? || s.journal.result != ENGAGED {
                return None;
            }
            hal::log(format_args!(
                "E41 creation en_attente generation={g} intention={} rejeu=sans_effet",
                s.journal.intent
            ));
        }
        Some(())
    })
}
pub(super) fn cree(p: Pid, g: u64, broker: u64) -> bool {
    avec(|s|{
        let id=identity(p)?;
        if id.boot!=s.journal.boot || s.journal.generation!=g || s.journal.result!=ENGAGED{return None}
        s.journal.broker=broker;s.journal.driver=Some((p,id));s.journal.result=CONFIRMED;s.journal.launches+=1;
        if matches!(scenario(),"deces-observe"|"lanceur-observe"){
            let role=if scenario()=="lanceur-observe"{LAUNCHER}else{wire::MANAGER};
            s.injecter_deces(role)?;
        }
        s.panne("lanceur-apres",LAUNCHER)?;s.panne("attribution",wire::MANAGER)?;
        s.dialogue(LAUNCHER,RECEIPT)?;
        s.dialogue(LAUNCHER,LAUNCH)?;
        if s.journal.autoriser(g,s.journal.intent)? || s.journal.autoriser(g,s.journal.intent.checked_add(1)?).is_some(){return None}
        hal::log(format_args!("E41 quota intention_occupee=true autre_id_refuse=true cloture_reservee=true generation={g}"));
        s.dialogue(wire::MANAGER,RECEIPT)?;
        hal::log(format_args!("E41 recu generation={g} intention={} pilote={} incarnation={} creations=1 rejeu=sans_effet",s.journal.intent,p.number,id.instance));
        Some(())
    }).is_some()
}
pub(crate) fn intention_publication(g: u64, n: u64) -> bool {
    avec(|s| {
        let j = &mut s.journal;
        if j.generation != g || j.result != CONFIRMED || j.pub_seq != 0 || n == 0 {
            return None;
        }
        j.pub_seq = n;
        j.state = 1;
        Some(())
    })
    .is_some()
}
pub(crate) fn publication(
    g: u64,
    n: u64,
    resultat: usize,
    pendant: impl FnOnce() -> bool,
) -> Option<usize> {
    avec(|s| {
        if s.journal.generation != g || s.journal.pub_seq != n || s.journal.state != 1 {
            return None;
        }
        s.journal.pub_result = Some(resultat);
        let role = if scenario() == "lanceur-actif" {
            LAUNCHER
        } else {
            wire::MANAGER
        };
        s.injected = true;
        s.interrompre(role)?;
        if !pendant() {
            return None;
        }
        hal::log(format_args!(
            "E41 stockage sans_controle role={role} generation={g} sequence={n}"
        ));
        if scenario() == "publication" {
            s.reprendre("publication")?;
        }
        s.journal.pub_result
    })
}
pub(crate) fn accuser_publication(g: u64, n: u64) -> bool {
    avec(|s| {
        let j = &mut s.journal;
        if j.generation != g || j.pub_seq != n || j.pub_result.is_none() || j.pub_delivered {
            return None;
        }
        j.pub_delivered = true;
        hal::log(format_args!(
            "E41 publication generation={g} sequence={n} resultat={} remise=1",
            j.pub_result?
        ));
        Some(())
    })
    .is_some()
}
pub(crate) fn reception_active(g: u64, n: u64) -> bool {
    avec(|s| {
        if s.vacance.is_some() {
            if s.journal.generation != g || s.journal.pub_seq >= n {
                return None;
            }
            hal::log(format_args!(
                "E41 RX sans_controle generation={g} sequence={n} pilote={}",
                s.journal.driver?.0.number
            ));
            s.reprendre(scenario())?;
        }
        Some(())
    })
    .is_some()
}
pub(crate) fn port_collecte() -> bool {
    let lock = SESSION.lock();
    let Some(s) = lock.as_ref() else { return false };
    let (Some(m), Some(l)) = (s.manager.as_ref(), s.launcher.as_ref()) else {
        return false;
    };
    with(|k| {
        let mut n = 0;
        for process in k.processes.slots.iter().flatten() {
            if process.space.is_some() {
                if process.pid != m.child.pid && process.pid != l.child.pid {
                    return false;
                }
                n += 1
            }
        }
        n == 2
    })
}
pub(crate) fn etat_port(g: u64) -> bool {
    avec(|s| {
        if s.journal.generation != g {
            return None;
        }
        s.journal.state = 2;
        Some(())
    })
    .is_some()
}
pub(crate) fn geler_carte(g: u64) {
    FROZEN.store(true, Ordering::Release);
    hal::log(format_args!(
        "E41 gel domaine non corroborable generation={g}"
    ));
}
pub(crate) fn ressources(g: u64, frames: [u64; 22], mapped: u64, numero: u64, phase: &str) -> bool {
    avec(|s|{
        let j=&mut s.journal;
        if j.generation!=g || frames!=j.frames || (mapped & !j.mapped)!=0{return None}
        if j.receipt!=0 && j.receipt!=numero{return None}
        j.mapped=mapped;j.receipt=numero;j.state=3;
        if phase=="reconciliation" && !s.injected && matches!(scenario(),"journal-perdu"|"non-corroborable"){
            s.injected=true;
            if scenario()=="journal-perdu" {
                // Perte de la copie du journal. Le bail physique de Preparation survit.
                *j=Journal{state:4,..Journal::default()};
            } else {
                // Altérer seulement la copie du journal, jamais une traduction ni un cadre.
                j.frames[1]=0;
            }
            if j.corroborer(){return None}
            hal::log(format_args!("E41 gel cause={} generation={g} recu_observe={numero} traductions={mapped} epingles_proprietaire={}",scenario(),frames.iter().filter(|&&f|f!=0).count()));
            return None
        }
        s.panne(phase,wire::MANAGER)?;
        if phase=="reconciliation" && scenario()=="double-deces"{
            s.dialogue(LAUNCHER,LAUNCH)?;
            if s.journal.autoriser(g,s.journal.intent)? || s.journal.launches!=1{return None}
            hal::log(format_args!("E41 rejeu apres_deces generation={g} intention={} creations=1",s.journal.intent));
        }
        Some(())
    }).is_some() && !FROZEN.load(Ordering::Acquire)
}
pub(crate) fn liberation_permises(g: u64, frames: [u64; 22]) -> bool {
    !FROZEN.load(Ordering::Acquire)
        && avec(|s| {
            let j = &s.journal;
            (j.generation == g && j.result != ENGAGED && j.frames == frames && j.corroborer())
                .then_some(())
        })
        .is_some()
}
pub(crate) fn rendu(g: u64, restant: [u64; 22]) {
    let _ = avec(|s| {
        let j = &mut s.journal;
        if j.generation != g || restant.iter().zip(j.frames).any(|(&a, b)| a != 0 && a != b) {
            return None;
        }
        j.frames = restant;
        j.mapped &= j.vue()[10];
        j.state = if j.vue()[10] == 0 { 5 } else { 3 };
        Some(())
    });
}
pub(crate) fn fin() {
    let mut lock = SESSION.lock();
    let Some(s) = lock.as_mut() else { return };
    s.fermer_acteurs();
    let v = s.journal.vue();
    let gel = FROZEN.load(Ordering::Acquire);
    let compact = !gel
        && s.journal.result != ENGAGED
        && (s.journal.pub_seq == 0 || s.journal.pub_delivered)
        && v[10] == 0
        && (v[18] == 5 || (s.journal.intent == 0 && s.journal.driver.is_none()));
    hal::log(format_args!(
        "E41 fin generation={} creations={} epingles={} mappes={} recu={} gel={gel} compactage={compact}",
        v[5], s.journal.launches, v[10], v[11], v[12]
    ));
    if compact {
        lock.take();
    }
}
