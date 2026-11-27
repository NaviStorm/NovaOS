//! Courtier CPU de laboratoire. Les adresses de descripteur sont des offsets du
//! modèle. Aucune primitive n'accède aux registres PCI, à une IRQ ou à une IOVA.
use super::adaptateur::{Adaptateur, Erreur, Filtre, Reception, Retrait};
use alloc::vec::Vec;
use core::sync::atomic::{Ordering, compiler_fence};
use ethernet_v1::{
    Binding,
    wire::{FRAME, LIMIT},
};
use novaos_gestionnaire_sim::{
    Error, Result, dma::Direction, e2400::Instance, lifecycle::Lease, manager::Manager,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phy {
    pub oui_msb: u16,
    pub model_revision: u16,
}
impl Phy {
    pub fn verify(self, expected: Self) -> Result<()> {
        if self != expected
            || [self.oui_msb, self.model_revision]
                .iter()
                .any(|v| matches!(v, 0 | 0xffff))
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    PhyId1,
    PhyId2,
    TxProducer,
    RxConsumer,
    IrqPending,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Fault,
    Stopped,
    Reset,
}
#[derive(Clone)]
struct Slot {
    bytes: [u8; FRAME],
    descriptor: [u8; 16],
    published: bool,
}
const EMPTY: Slot = Slot {
    bytes: [0; FRAME],
    descriptor: [0; 16],
    published: false,
};
const FIRST_TX: usize = super::profil::REGIONS[0].cadres()
    + super::profil::REGIONS[1].cadres()
    + super::profil::REGIONS[2].cadres();
const FIRST_RX: usize = FIRST_TX + super::profil::REGIONS[3].cadres();
fn model_offset(first_page: usize, index: u8) -> u64 {
    first_page as u64 * super::profil::PAGE as u64 + u64::from(index) * super::profil::TAMPON as u64
}
fn packet_error(e: Erreur) -> Error {
    match e {
        Erreur::Quarantaine => Error::Uncertain,
        Erreur::Plein => Error::Full,
        Erreur::Filtre | Erreur::Liaison => Error::Denied,
        _ => Error::Format,
    }
}
pub struct Broker {
    instance: Instance,
    binding: Binding,
    engine: Adaptateur,
    phy: Phy,
    phase: Phase,
    tx: Vec<Slot>,
    prepared: u8,
    fenced: u8,
    producer: u8,
    rx_cons: u8,
    rx_prod: u8,
    irq_armed: bool,
    pending: bool,
}
impl Broker {
    pub fn open(
        instance: Instance,
        m: &Manager<'_, '_>,
        caller: Lease,
        binding: Binding,
        observed_phy: Phy,
        expected_phy: Phy,
    ) -> Result<Self> {
        instance.check(m, caller, "tx")?;
        instance.check(m, caller, "rx")?;
        observed_phy.verify(expected_phy)?;
        let filter = Filtre::laboratoire(super::boucle::STATION, super::boucle::PAIR)
            .map_err(packet_error)?;
        let mut tx = Vec::new();
        tx.try_reserve_exact(LIMIT).map_err(|_| Error::Memory)?;
        tx.resize(LIMIT, EMPTY);
        Ok(Self {
            instance,
            binding,
            engine: Adaptateur::nouveau(binding, filter).map_err(packet_error)?,
            phy: observed_phy,
            phase: Phase::Ready,
            tx,
            prepared: 0,
            fenced: 0,
            producer: 0,
            rx_cons: 0,
            rx_prod: 0,
            irq_armed: false,
            pending: false,
        })
    }
    pub fn heap_bytes(&self) -> usize {
        self.tx.capacity() * size_of::<Slot>()
    }
    fn active(&self, m: &Manager<'_, '_>, l: Lease, right: &str) -> Result<()> {
        self.instance.check(m, l, right)?;
        if self.phase != Phase::Ready {
            return Err(Error::Transition);
        }
        Ok(())
    }
    pub fn read(&self, m: &Manager<'_, '_>, l: Lease, r: Register) -> Result<u16> {
        self.active(
            m,
            l,
            if matches!(r, Register::TxProducer) {
                "tx"
            } else {
                "rx"
            },
        )?;
        Ok(match r {
            Register::PhyId1 => self.phy.oui_msb,
            Register::PhyId2 => self.phy.model_revision,
            Register::TxProducer => self.producer.into(),
            Register::RxConsumer => self.rx_cons.into(),
            Register::IrqPending => self.pending.into(),
        })
    }
    /// Scelle les octets dans le slot privé et construit un TPD à un tampon, sans offload.
    pub fn prepare(
        &mut self,
        m: &Manager<'_, '_>,
        l: Lease,
        coupon: usize,
        frame: &[u8],
    ) -> Result<[u8; 16]> {
        self.active(m, l, "tx")?;
        let index = self.producer.wrapping_add(self.prepared);
        self.instance.page(m, l, 0, Direction::Read)?;
        self.instance
            .page(m, l, FIRST_TX + usize::from(index) / 2, Direction::Read)?;
        let slot = self.tx.get_mut(coupon).ok_or(Error::Format)?;
        let n = self
            .engine
            .preparer(self.binding, coupon, frame, &mut slot.bytes)
            .map_err(packet_error)?;
        let mut desc = [0; 16];
        desc[..2].copy_from_slice(&(n as u16).to_le_bytes());
        desc[4..8].copy_from_slice(&(1u32 << 31).to_le_bytes());
        desc[8..].copy_from_slice(&model_offset(FIRST_TX, index).to_le_bytes());
        slot.descriptor = desc;
        self.prepared += 1;
        Ok(desc)
    }
    pub fn order_tx(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<()> {
        self.active(m, l, "tx")?;
        compiler_fence(Ordering::Release);
        self.fenced = self.prepared;
        Ok(())
    }
    pub fn publish(&mut self, m: &Manager<'_, '_>, l: Lease, n: u8) -> Result<u8> {
        self.active(m, l, "tx")?;
        if n == 0 || n > self.fenced {
            return Err(Error::Transition);
        }
        let p = self
            .engine
            .publier(self.binding, n.into())
            .map_err(packet_error)?;
        for &id in &p.coupons[..p.nombre] {
            self.tx[usize::from(id)].published = true;
        }
        self.prepared -= n;
        self.fenced -= n;
        self.producer = p.producteur;
        Ok(self.producer)
    }
    /// Vue immuable du pair de laboratoire, disponible entre publication et complétion.
    pub fn tx_image(
        &self,
        m: &Manager<'_, '_>,
        l: Lease,
        coupon: usize,
    ) -> Result<(&[u8; 16], &[u8])> {
        self.active(m, l, "tx")?;
        let s = self.tx.get(coupon).ok_or(Error::Format)?;
        if !s.published {
            return Err(Error::Transition);
        }
        let n = u16::from_le_bytes([s.descriptor[0], s.descriptor[1]]) as usize;
        Ok((&s.descriptor, &s.bytes[..n]))
    }
    pub fn tx_progress(&mut self, m: &Manager<'_, '_>, l: Lease, consumer: u16) -> Result<u8> {
        self.active(m, l, "tx")?;
        compiler_fence(Ordering::Acquire);
        let mask = self
            .engine
            .completions(self.binding, consumer)
            .map_err(|e| {
                self.phase = Phase::Fault;
                packet_error(e)
            })?;
        for (i, slot) in self.tx.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                *slot = EMPTY;
            }
        }
        self.pending |= mask != 0;
        Ok(mask)
    }
    pub fn post_rx(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<[u8; 8]> {
        self.active(m, l, "rx")?;
        if usize::from(self.rx_prod.wrapping_sub(self.rx_cons)) >= LIMIT {
            return Err(Error::Full);
        }
        self.instance.page(m, l, 1, Direction::Read)?;
        self.instance.page(
            m,
            l,
            FIRST_RX + usize::from(self.rx_prod) / 2,
            Direction::Write,
        )?;
        self.engine
            .preposter_rx(self.binding, 1)
            .map_err(packet_error)?;
        let descriptor = model_offset(FIRST_RX, self.rx_prod).to_le_bytes();
        compiler_fence(Ordering::Release);
        self.rx_prod = self.rx_prod.wrapping_add(1);
        Ok(descriptor)
    }
    /// Le pair CPU fournit une copie pour une réception. Le résultat emprunte cette seule copie.
    pub fn rx_progress<'a>(
        &mut self,
        m: &Manager<'_, '_>,
        l: Lease,
        rrd: &[u8; 16],
        copie: &'a [u8],
    ) -> Result<Reception<'a>> {
        self.active(m, l, "rx")?;
        self.instance.page(m, l, 2, Direction::Write)?;
        self.instance.page(
            m,
            l,
            FIRST_RX + usize::from(self.rx_cons) / 2,
            Direction::Write,
        )?;
        compiler_fence(Ordering::Acquire);
        let rx = self
            .engine
            .reception(self.binding, rrd, copie)
            .map_err(|e| {
                self.phase = Phase::Fault;
                packet_error(e)
            })?;
        if rx != Reception::Attente {
            self.rx_cons = self.rx_cons.wrapping_add(1);
            self.pending = true;
        }
        Ok(rx)
    }
    /// Réarmer revérifie le niveau. L'acquittement consomme la notification groupée.
    pub fn arm(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<bool> {
        self.active(m, l, "rx")?;
        self.irq_armed = true;
        Ok(self.pending)
    }
    pub fn acknowledge(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<bool> {
        self.active(m, l, "rx")?;
        let delivered = self.irq_armed && self.pending;
        if delivered {
            self.irq_armed = false;
            self.pending = false;
        }
        Ok(delivered)
    }
    pub fn stop(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<Retrait> {
        self.instance.retiring(m, l)?;
        if self.phase == Phase::Reset {
            return Err(Error::Transition);
        }
        self.irq_armed = false;
        self.phase = Phase::Stopped;
        self.engine.retirer(self.binding).map_err(packet_error)
    }
    /// Reset du modèle seulement. Les cadres retenus dans le gestionnaire restent comptés.
    pub fn reset(&mut self, m: &Manager<'_, '_>, l: Lease) -> Result<()> {
        self.instance.retiring(m, l)?;
        if self.phase != Phase::Stopped {
            return Err(Error::Transition);
        }
        self.pending = false;
        self.tx.fill(EMPTY);
        self.phase = Phase::Reset;
        Ok(())
    }
}
