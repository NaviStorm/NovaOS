//! Moteur DMA du seul EDU qualifié, commandé par IPC depuis Ring 3.
//! L'appelant garde l'objet Device exclusif et épingle les cadres du domaine.
use super::lab_irq;
use super::physical_dma::Direction::{ReadByDevice, WrittenByDevice};
use super::vtd_lab::{Engine, Lease};
use crate::{memory, sched::Guard};
use core::sync::atomic::{AtomicBool, Ordering, fence};

// Les profils EDU de phase 2 et de phase 3 sont mutuellement exclusifs.
const REGISTERS: u64 = memory::IO_BASE + 323 * memory::PAGE;
const BUFFER: u64 = 0x40000;
const LENGTH: u64 = 4096;
const MASK: u64 = (1 << 28) - 1;
const RUN: u64 = 1;
const TO_RAM: u64 = 2;
static MAPPED: AtomicBool = AtomicBool::new(false);

fn read32(offset: u64) -> u32 {
    // Sûreté : seule cette implémentation utilise la page superviseur UC.
    unsafe { ((REGISTERS + offset) as *const u32).read_volatile() }
}
fn read64(offset: u64) -> u64 {
    // Sûreté : registres de 64 bits alignés de la banque DMA de l'EDU.
    unsafe { ((REGISTERS + offset) as *const u64).read_volatile() }
}
fn write64(offset: u64, value: u64) {
    unsafe { ((REGISTERS + offset) as *mut u64).write_volatile(value) };
}
fn idle() -> bool {
    read32(0) == 0x010000ed && read32(0x20) == 0 && read32(0x24) == 0 && read64(0x98) & !TO_RAM == 0
}

/// Le hook de mort appelle cette sonde après avoir identifié le PID de l'instance.
pub(crate) fn running_at_death() -> bool {
    let _guard = Guard::new();
    MAPPED.load(Ordering::Acquire) && read64(0x98) & RUN != 0
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Transfer {
    ReadSource,
    WriteDestination,
    WriteSource,
    ReadDestination,
    WriteUnmapped,
}

#[derive(Clone, Copy)]
struct Command {
    transfer: Transfer,
    iova: u64,
}

impl Command {
    fn registers(self) -> (u64, u64, u64) {
        match self.transfer {
            Transfer::ReadSource | Transfer::ReadDestination => (self.iova, BUFFER, 0),
            Transfer::WriteDestination | Transfer::WriteSource | Transfer::WriteUnmapped => {
                (BUFFER, self.iova, TO_RAM)
            }
        }
    }

    /// Dans le modèle EDU, les registres d'adresse et de longueur persistent
    /// après le DMA et le contrôleur efface uniquement RUN à sa terminaison.
    fn finished(self) -> bool {
        let (source, destination, direction) = self.registers();
        idle()
            && read64(0x80) == source
            && read64(0x88) == destination
            && read64(0x90) == LENGTH
            && read64(0x98) == direction
    }
}

#[derive(Clone, Copy)]
enum Flight {
    Idle,
    Started(Command),
    Unconfirmed(Command),
}

/// Une preuve emprunte le périphérique : il ne peut pas être rouvert tant
/// qu'elle est utilisée pour l'invalidation et la restitution des pages.
pub(crate) struct Drained<'a> {
    device: &'a Device,
    lease: Lease,
}
impl Drained<'_> {
    pub(crate) fn lease(&self) -> Lease {
        self.lease
    }
    pub(crate) fn confirmed(&self) -> bool {
        self.device.quiet == Some(self.lease) && self.device.fenced()
    }
}

pub(crate) struct Device {
    bar: u64,
    owner: Option<Lease>,
    quiet: Option<Lease>,
    closing: bool,
    flight: Flight,
    source_read: bool,
    buffered: bool,
    post_probes: u8,
    mapped: bool,
}

impl Device {
    pub(crate) fn adopt(bar: u64) -> Option<Self> {
        let _guard = Guard::new();
        if MAPPED.load(Ordering::Acquire)
            || !lab_irq::dma_fenced(bar)
            || !memory::firmware::unlisted_extent(bar, 1 << 20)
            || memory::map_io(bar, REGISTERS).is_err()
        {
            return None;
        }
        // Un RUN éteint seul ne suffit pas pour accepter un appareil hérité.
        // Exiger aussi un contrôleur vierge, à l'arrêt et sans cause en attente.
        if !idle()
            || [0x80, 0x88, 0x90, 0x98]
                .iter()
                .any(|&offset| read64(offset) != 0)
        {
            if memory::unmap_lab_io(bar, REGISTERS).is_err() {
                MAPPED.store(true, Ordering::Release);
            }
            return None;
        }
        MAPPED.store(true, Ordering::Release);
        Some(Self {
            bar,
            owner: None,
            quiet: None,
            closing: false,
            flight: Flight::Idle,
            source_read: false,
            buffered: false,
            post_probes: 0,
            mapped: true,
        })
    }

    fn fenced(&self) -> bool {
        self.mapped
            && matches!(self.flight, Flight::Idle)
            && idle()
            && lab_irq::dma_fenced(self.bar)
    }

    pub(crate) fn open(&mut self, engine: &Engine, lease: Lease) -> bool {
        if self.owner.is_some()
            || self.quiet == Some(lease)
            || !self.fenced()
            || !engine.active(lease)
            || !engine.permits(lease, 0, ReadByDevice)
            || engine.permits(lease, 0, WrittenByDevice)
            || !engine.permits(lease, 1, WrittenByDevice)
            || engine.permits(lease, 1, ReadByDevice)
            || engine.fault_pending()
        {
            return false;
        }
        // Enregistrer l'instance avant le changement PCI, même si sa relecture échoue.
        self.owner = Some(lease);
        self.quiet = None;
        self.flight = Flight::Idle;
        self.source_read = false;
        self.buffered = false;
        self.post_probes = 0;
        self.closing = !lab_irq::dma_master(self.bar, true);
        !self.closing
    }

    /// À l'intérieur du noyau, la géométrie d'une transaction est univoque :
    /// exactement une page et tout le tampon interne EDU. Le pilote ne choisit
    /// ni le compteur, ni les pointeurs, ni les bits du mot de commande.
    fn program(&self, command: Command) -> bool {
        if command.iova % LENGTH != 0 || command.iova > MASK - (LENGTH - 1) || !idle() {
            return false;
        }
        let (src, dst, direction) = command.registers();
        let cmd = RUN | direction;
        write64(0x80, src);
        write64(0x88, dst);
        write64(0x90, LENGTH);
        if (read64(0x80), read64(0x88), read64(0x90)) != (src, dst, LENGTH) {
            return false;
        }
        // x86/PCI cohérent ; publier les stores RAM avant le doorbell UC.
        fence(Ordering::SeqCst);
        write64(0x98, cmd);
        // L'EDU met son timer en file pour 100 ms de temps virtuel. Un START
        // non observé ne constitue pas l'acquittement de cette soumission.
        read64(0x98) == cmd
    }

    fn ready(&self, engine: &Engine, lease: Lease) -> bool {
        self.owner == Some(lease)
            && !self.closing
            && matches!(self.flight, Flight::Idle)
            && engine.active(lease)
            && !engine.fault_pending()
            && idle()
    }

    fn start(&mut self, transfer: Transfer, iova: u64) -> bool {
        let command = Command { transfer, iova };
        let to_ram = command.registers().2 == TO_RAM;
        if to_ram && !self.buffered {
            return false;
        }
        if !to_ram {
            self.source_read = false;
            self.buffered = false;
        }
        self.flight = Flight::Unconfirmed(command);
        if !self.program(command) {
            self.closing = true;
            return false;
        }
        self.flight = Flight::Started(command);
        true
    }

    /// Le dispatcher du lanceur n'offre à Ring 3 que ReadSource / WriteDestination.
    /// Les trois sondes de faute sont réservées à l'oracle du profil.
    pub(crate) fn submit(&mut self, engine: &Engine, lease: Lease, transfer: Transfer) -> bool {
        if !self.ready(engine, lease) {
            return false;
        }
        let (slot, to_ram, permitted) = match transfer {
            Transfer::ReadSource => (0, false, true),
            Transfer::WriteDestination => (1, true, true),
            Transfer::WriteSource => (0, true, false),
            Transfer::ReadDestination => (1, false, false),
            Transfer::WriteUnmapped => (2, true, false),
        };
        let dir = if to_ram {
            WrittenByDevice
        } else {
            ReadByDevice
        };
        if engine.permits(lease, slot, dir) != permitted {
            return false;
        }
        self.start(transfer, lease.iova(slot))
    }

    /// Le seul témoin hors domaine est une page RO d'un autre processus de
    /// laboratoire, retenue par le superviseur jusqu'au drainage.
    pub(crate) fn probe_other(
        &mut self,
        engine: &Engine,
        lease: Lease,
        witness: &memory::user::PrivateWitness,
    ) -> bool {
        let iova = witness.iova();
        if !self.ready(engine, lease) || (0..4).any(|slot| lease.iova(slot) == iova) {
            return false;
        }
        self.start(Transfer::WriteUnmapped, iova)
    }

    pub(crate) fn running(&self, lease: Lease) -> bool {
        self.owner == Some(lease) && !matches!(self.flight, Flight::Idle) && read64(0x98) & RUN != 0
    }
    pub(crate) fn completed(&mut self, lease: Lease) -> bool {
        if self.owner != Some(lease) || self.closing || !idle() {
            return false;
        }
        let Flight::Started(command) = self.flight else {
            return false;
        };
        if !command.finished() {
            self.flight = Flight::Unconfirmed(command);
            self.closing = true;
            return false;
        }
        fence(Ordering::SeqCst);
        self.flight = Flight::Idle;
        self.source_read = command.transfer == Transfer::ReadSource;
        true
    }
    /// N'exporter aucun octet du tampon hérité avant une lecture intégrale dans
    /// le domaine courant, achevée sans faute VT-d.
    pub(crate) fn confirm_source(&mut self, engine: &Engine, lease: Lease) -> bool {
        if !self.source_read
            || !self.ready(engine, lease)
            || !engine.permits(lease, 0, ReadByDevice)
        {
            return false;
        }
        self.buffered = true;
        true
    }

    /// Après le chargement du tampon, le seul écrivain logiciel prépare un
    /// motif distinct dans la source synthétique pour l'oracle de permission.
    pub(crate) fn can_reseed(&self, lease: Lease) -> bool {
        self.owner == Some(lease)
            && self.buffered
            && !self.closing
            && matches!(self.flight, Flight::Idle)
            && self.mapped
            && idle()
    }

    /// Témoin avant le premier doorbell de la VM qualifiée.
    pub(crate) fn never_programmed(&self, lease: Lease) -> bool {
        self.owner == Some(lease)
            && matches!(self.flight, Flight::Idle)
            && idle()
            && [0x80, 0x88, 0x90, 0x98]
                .into_iter()
                .all(|off| read64(off) == 0)
    }

    /// Précondition contrôlée avant de pouvoir réexporter un octet du tampon.
    /// Cette lecture logicielle reste indépendante des fautes et du droit d'admission.
    pub(crate) fn has_buffered_output(&self, lease: Lease) -> bool {
        self.owner == Some(lease) && self.buffered && matches!(self.flight, Flight::Idle)
    }

    /// Injection du seul banc terminal : après une vraie lecture DMA achevée,
    /// effacer le registre longueur sans écrire le registre de démarrage. La
    /// réservation et l'état privé du courtier restent intacts. drain() doit
    /// détecter lui-même la divergence entre le transfert et sa preuve MMIO.
    pub(crate) fn erase_completed_length(&self, lease: Lease) -> bool {
        let Flight::Started(command) = self.flight else {
            return false;
        };
        if self.owner != Some(lease)
            || command.transfer != Transfer::ReadSource
            || command.iova != lease.iova(0)
            || !command.finished()
        {
            return false;
        }
        write64(0x90, 0);
        read64(0x90) == 0 && idle()
    }

    pub(crate) fn held_without_completion(&self, lease: Lease) -> bool {
        self.owner == Some(lease)
            && self.closing
            && self.quiet.is_none()
            && matches!(self.flight, Flight::Unconfirmed(c) if c.transfer == Transfer::ReadSource
                && c.iova == lease.iova(0))
            && idle()
            && lab_irq::dma_fenced(self.bar)
    }

    pub(crate) fn drain(&mut self, lease: Lease) -> Option<Drained<'_>> {
        if self.owner == Some(lease) {
            self.closing = true;
            if !idle() {
                return None;
            }
            // L'arrêt du moteur permet de fermer le bus. Le seul BME fermé
            // n'identifie pas la commande terminée et ne rend aucune page.
            if !lab_irq::dma_master(self.bar, false) || !lab_irq::dma_fenced(self.bar) {
                return None;
            }
            match self.flight {
                Flight::Unconfirmed(_) => return None,
                Flight::Started(command) if !command.finished() => {
                    self.flight = Flight::Unconfirmed(command);
                    return None;
                }
                _ => {}
            }
            fence(Ordering::SeqCst);
            // Conserver la commande mémorisée si cette relecture échoue.
            if !idle() || !lab_irq::dma_fenced(self.bar) {
                return None;
            }
            self.flight = Flight::Idle;
            self.owner = None;
            self.quiet = Some(lease);
            self.source_read = false;
            self.buffered = false;
        }
        if self.quiet == Some(lease) && self.fenced() {
            Some(Drained {
                device: self,
                lease,
            })
        } else {
            None
        }
    }

    /// Sous contrôle du superviseur après l'invalidation confirmée : seule
    /// l'ancienne IOVA de destination peut être sondée. Le contexte a disparu,
    /// le FRCD atteste son absence. Les anciens cadres sont déjà restitués.
    pub(crate) fn post_revoke(&mut self, engine: &Engine, old: Lease) -> bool {
        if self.owner.is_some()
            || self.quiet != Some(old)
            || self.post_probes >= 2
            || !self.fenced()
            || !engine.denies_edu(old)
            || engine.fault_pending()
        {
            return false;
        }
        self.post_probes += 1;
        if !lab_irq::dma_master(self.bar, true) {
            let _ = lab_irq::dma_master(self.bar, false);
            return false;
        }
        let probe = Command {
            transfer: Transfer::WriteDestination,
            iova: old.iova(1),
        };
        let started = self.program(probe);
        let begin = crate::sched::ticks();
        while read64(0x98) & RUN != 0
            && crate::sched::ticks().wrapping_sub(begin) < u64::from(crate::sched::HZ)
        {
            crate::sched::sleep(1);
        }
        // BME coupé même en cas de timeout. Le succès inclut le drainage matériel.
        let stopped = probe.finished();
        let disabled = lab_irq::dma_master(self.bar, false);
        started && stopped && disabled && self.fenced()
    }

    pub(crate) fn close(&mut self) -> bool {
        if !self.mapped {
            return lab_irq::shutdown();
        }
        if self.owner.is_some() || !self.fenced() {
            return false;
        }
        let _guard = Guard::new();
        if memory::unmap_lab_io(self.bar, REGISTERS).is_err() {
            return false;
        }
        MAPPED.store(false, Ordering::Release);
        self.mapped = false;
        lab_irq::shutdown()
    }
}
