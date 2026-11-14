use super::*;
use std::collections::BTreeMap;

const PID: u64 = 19;
const SECOND: u64 = 31;
const BASE: u64 = 0x9000;
const ALL: Rights = Rights::READ.with(Rights::WRITE).with(Rights::IRQ);
const ATTR: DeviceMemory = if cfg!(target_arch = "x86_64") {
    DeviceMemory::X86Uncacheable
} else {
    DeviceMemory::ArmDevice
};
const POLICY: Policy = Policy {
    window: Some(Window {
        base: BASE,
        bytes: 16,
        width: 4,
        attribute: ATTR,
    }),
    irq: Some(58),
    rights: ALL,
};

#[derive(Default)]
struct DeviceState {
    epoch: u64,
    masked: bool,
    asserted: bool,
    uncertain_irq: bool,
    uncertain_dma: bool,
    register: u64,
}

#[derive(Default)]
struct TestBus {
    policies: BTreeMap<(u32, u64), Policy>,
    devices: BTreeMap<u32, DeviceState>,
    io: Vec<(bool, u64, u8)>,
    retirements: usize,
    partial_prepare: bool,
    failed_access: bool,
}

impl TestBus {
    fn ordinary() -> Self {
        let mut bus = Self::default();
        bus.policies.insert((0, PID), POLICY);
        bus.policies.insert((0, SECOND), POLICY);
        bus
    }
}

impl Backend for TestBus {
    fn policy(&self, dev: u32, pid: u64) -> Option<Policy> {
        self.policies.get(&(dev, pid)).copied()
    }
    fn prepare(&mut self, dev: u32, cap: Capability, policy: Policy, rights: Rights) -> Result<()> {
        assert!(policy.rights.includes(rights));
        let was = self.devices.insert(
            dev,
            DeviceState {
                epoch: cap.epoch,
                masked: true,
                ..Default::default()
            },
        );
        assert!(was.is_none(), "préparation sans retrait confirmé");
        if self.partial_prepare {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    fn read(&mut self, dev: u32, address: u64, width: u8) -> Result<u64> {
        self.io.push((false, address, width));
        if self.failed_access {
            return Err(Error::Invalid);
        }
        Ok(self.devices[&dev].register)
    }
    fn write(&mut self, dev: u32, address: u64, width: u8, value: u64) -> Result<()> {
        self.io.push((true, address, width));
        self.devices.get_mut(&dev).unwrap().register = value;
        Ok(())
    }
    fn set_mask(&mut self, dev: u32, masked: bool) -> Result<()> {
        self.devices.get_mut(&dev).unwrap().masked = masked;
        Ok(())
    }
    fn claim_and_mask(&mut self, dev: u32) -> Result<bool> {
        let hw = self.devices.get_mut(&dev).unwrap();
        if hw.masked || !hw.asserted {
            return Ok(false);
        }
        hw.masked = true;
        Ok(true)
    }
    fn acknowledge(&mut self, dev: u32) -> Result<()> {
        let hw = self.devices.get_mut(&dev).unwrap();
        if hw.uncertain_irq {
            return Err(Error::Busy);
        }
        assert!(hw.masked && hw.asserted);
        hw.asserted = false;
        Ok(())
    }
    fn retire(&mut self, dev: u32, cap: Capability) -> Result<()> {
        self.retirements += 1;
        let hw = self.devices.get_mut(&dev).unwrap();
        assert_eq!(hw.epoch, cap.epoch);
        hw.masked = true;
        if hw.uncertain_dma || hw.uncertain_irq {
            return Err(Error::Busy);
        }
        self.devices.remove(&dev);
        Ok(())
    }
}

#[test]
fn decouverte_seule_sans_autorisation_et_isolation_du_client() {
    let mut broker = Broker::<_, 1>::new(TestBus::ordinary());
    assert_eq!(broker.grant(0, 0, ALL), Err(Error::Denied));
    assert_eq!(broker.grant(1, PID, ALL), Err(Error::Denied));
    assert_eq!(broker.grant(0, 99, ALL), Err(Error::Denied));
    assert_eq!(broker.grant(0, PID, Rights(0)), Err(Error::Denied));
    assert_eq!(broker.grant(0, PID, Rights(0x81)), Err(Error::Denied));
    assert!(broker.backend.devices.is_empty());
    let cap = broker.grant(0, PID, Rights::READ).unwrap();
    assert_eq!(broker.grant(0, SECOND, ALL), Err(Error::Busy));
    assert_eq!(broker.read(SECOND, cap, 0, 4), Err(Error::Denied));
    assert_eq!(broker.write(PID, cap, 0, 4, 5), Err(Error::Denied));
    assert_eq!(broker.set_mask(PID, cap, false), Err(Error::Denied));
    assert!(broker.backend.io.is_empty());
    assert_eq!(broker.read(PID, cap, 0, 4), Ok(0));
    broker.revoke(cap).unwrap();
    let next = broker.grant(0, PID, ALL).unwrap();
    assert_ne!(cap, next);
    assert_eq!(broker.read(PID, cap, 0, 4), Err(Error::Denied));
    assert_eq!(broker.revoke(cap), Err(Error::Denied));
    assert_eq!(broker.read(PID, next, 0, 4), Ok(0));
}

#[test]
fn mmio_bornes_alignement_largeur_et_valeur() {
    let mut broker = Broker::<_, 1>::new(TestBus::ordinary());
    let cap = broker.grant(0, PID, ALL).unwrap();
    for (off, width) in [(0, 0), (4, 1), (2, 4), (13, 4), (16, 4), (u64::MAX - 3, 4)] {
        assert_eq!(broker.read(PID, cap, off, width), Err(Error::Invalid));
        assert_eq!(broker.write(PID, cap, off, width, 0), Err(Error::Invalid));
    }
    assert_eq!(
        broker.write(PID, cap, 0, 4, u32::MAX as u64 + 1),
        Err(Error::Invalid)
    );
    assert!(broker.backend.io.is_empty());
    broker.write(PID, cap, 12, 4, 17).unwrap();
    assert_eq!(broker.read(PID, cap, 12, 4), Ok(17));
    assert_eq!(
        broker.backend.io,
        [(true, BASE + 12, 4), (false, BASE + 12, 4)]
    );
}

#[test]
fn politique_mmio_incoherente_et_recouvrements_de_ressources() {
    let mut broker = Broker::<_, 3>::new(TestBus::ordinary());
    for (base, bytes, width) in [
        (0, 4, 4),
        (1, 4, 4),
        (BASE, 0, 4),
        (BASE, 6, 4),
        (BASE, 8, 3),
        (u64::MAX - 3, 4, 4),
    ] {
        let policy = Policy {
            window: Some(Window {
                base,
                bytes,
                width,
                attribute: ATTR,
            }),
            ..POLICY
        };
        broker.backend.policies.insert((1, PID), policy);
        assert_eq!(broker.grant(1, PID, ALL), Err(Error::Denied));
    }
    for policy in [
        Policy {
            window: None,
            ..POLICY
        },
        Policy {
            irq: None,
            ..POLICY
        },
        Policy {
            rights: Rights(0x80),
            ..POLICY
        },
    ] {
        broker.backend.policies.insert((1, PID), policy);
        assert_eq!(broker.grant(1, PID, ALL), Err(Error::Denied));
    }
    let cap = broker.grant(0, PID, ALL).unwrap();
    let make = |base, irq| Policy {
        window: Some(Window {
            base,
            bytes: 8,
            width: 4,
            attribute: ATTR,
        }),
        irq: Some(irq),
        ..POLICY
    };
    for policy in [make(BASE + 8, 59), make(BASE + 16, 58)] {
        broker.backend.policies.insert((1, SECOND), policy);
        assert_eq!(broker.grant(1, SECOND, ALL), Err(Error::Busy));
    }
    broker
        .backend
        .policies
        .insert((1, SECOND), make(BASE + 16, 59));
    broker.grant(1, SECOND, ALL).unwrap();
    broker.revoke(cap).unwrap();
    assert_eq!(broker.backend.devices.len(), 1);
}

#[test]
fn irq_masquee_parasite_et_acquittement_unique() {
    let mut broker = Broker::<_, 1>::new(TestBus::ordinary());
    let cap = broker.grant(0, PID, Rights::IRQ).unwrap();
    assert_eq!(broker.interrupt(cap, 58), Ok(None));
    broker.set_mask(PID, cap, false).unwrap();
    assert_eq!(broker.interrupt(cap, 58), Ok(None)); // source non revendiquée
    broker.backend.devices.get_mut(&0).unwrap().asserted = true;
    assert_eq!(broker.interrupt(cap, 57), Ok(None));
    let event = broker.interrupt(cap, 58).unwrap().unwrap();
    assert_eq!(
        (event.owner, event.capability, event.sequence),
        (PID, cap, 1)
    );
    assert!(broker.backend.devices[&0].masked);
    assert_eq!(broker.interrupt(cap, 58), Ok(None)); // doublon
    assert_eq!(broker.set_mask(PID, cap, false), Err(Error::Pending));
    assert_eq!(broker.ack(SECOND, cap, event.sequence), Err(Error::Denied));
    assert_eq!(broker.ack(PID, cap, 0), Err(Error::Invalid));
    broker.ack(PID, cap, event.sequence).unwrap();
    assert_eq!(broker.ack(PID, cap, event.sequence), Err(Error::Invalid));
    broker.set_mask(PID, cap, false).unwrap();
    broker.set_mask(PID, cap, true).unwrap();
    assert!(broker.backend.devices[&0].masked);
}

#[test]
fn mort_pendant_irq_et_dma_retrait_incertain_reprise_et_pas_de_double_liberation() {
    let mut broker = Broker::<_, 2>::new(TestBus::ordinary());
    broker.backend.policies.insert(
        (1, PID),
        Policy {
            window: None,
            irq: Some(59),
            rights: Rights::IRQ,
        },
    );
    let cap = broker.grant(0, PID, ALL).unwrap();
    let second = broker.grant(1, PID, Rights::IRQ).unwrap();
    broker.set_mask(PID, cap, false).unwrap();
    broker.backend.devices.get_mut(&0).unwrap().asserted = true;
    let pending = broker.interrupt(cap, 58).unwrap().unwrap();
    let hw = broker.backend.devices.get_mut(&0).unwrap();
    hw.uncertain_dma = true;
    hw.uncertain_irq = true;
    assert_eq!(broker.driver_died(PID), Err(Error::Quarantined));
    assert_eq!(
        broker.active(PID, second, Rights::IRQ).err(),
        Some(Error::Denied)
    );
    assert_eq!(
        broker.ack(PID, cap, pending.sequence),
        Err(Error::Quarantined)
    );
    assert_eq!(broker.read(PID, cap, 0, 4), Err(Error::Quarantined));
    assert_eq!(broker.interrupt(cap, 58), Ok(None));
    assert_eq!(broker.grant(0, SECOND, ALL), Err(Error::Busy));
    assert!(broker.backend.devices.contains_key(&0));
    broker.backend.devices.get_mut(&0).unwrap().uncertain_dma = false;
    assert_eq!(broker.retry_retirements(), Err(Error::Quarantined));
    broker.backend.devices.get_mut(&0).unwrap().uncertain_irq = false;
    broker.retry_retirements().unwrap();
    let count = broker.backend.retirements;
    broker.retry_retirements().unwrap();
    assert_eq!(broker.backend.retirements, count);
    let next = broker.grant(0, SECOND, ALL).unwrap();
    assert_ne!(next, cap);
    assert_eq!(
        broker.ack(SECOND, cap, pending.sequence),
        Err(Error::Denied)
    );
    assert_eq!(broker.interrupt(cap, 58), Ok(None));
    assert!(broker.backend.devices[&0].masked);
}

#[test]
fn erreurs_de_preparation_acces_et_acquittement_ferment_admission() {
    let mut broker = Broker::<_, 1>::new(TestBus::ordinary());
    broker.backend.partial_prepare = true;
    assert_eq!(broker.grant(0, PID, ALL), Err(Error::Quarantined));
    assert_eq!(broker.grant(0, SECOND, ALL), Err(Error::Busy));
    broker.retry_retirements().unwrap();
    broker.backend.partial_prepare = false;
    let first = broker.grant(0, SECOND, ALL).unwrap();
    broker.backend.failed_access = true;
    assert_eq!(broker.read(SECOND, first, 0, 4), Err(Error::Quarantined));
    assert_eq!(
        broker.write(SECOND, first, 0, 4, 1),
        Err(Error::Quarantined)
    );
    broker.retry_retirements().unwrap();
    let second = broker.grant(0, PID, ALL).unwrap();
    broker.set_mask(PID, second, false).unwrap();
    let hw = broker.backend.devices.get_mut(&0).unwrap();
    hw.asserted = true;
    hw.uncertain_irq = true;
    let irq = broker.interrupt(second, 58).unwrap().unwrap();
    assert_eq!(
        broker.ack(PID, second, irq.sequence),
        Err(Error::Quarantined)
    );
    assert_eq!(broker.set_mask(PID, second, false), Err(Error::Quarantined));
    assert_eq!(broker.retry_retirements(), Err(Error::Quarantined));
}

#[test]
fn exhaustion_sans_recyclage_de_generation_et_quota_de_table() {
    let mut broker = Broker::<_, 1>::new(TestBus::ordinary());
    broker.backend.policies.insert(
        (1, SECOND),
        Policy {
            window: None,
            irq: Some(77),
            rights: Rights::IRQ,
        },
    );
    let cap = broker.grant(0, PID, ALL).unwrap();
    assert_eq!(broker.grant(1, SECOND, Rights::IRQ), Err(Error::Exhausted));
    broker.set_mask(PID, cap, false).unwrap();
    broker.slots[0].as_mut().unwrap().sequence = u64::MAX;
    broker.backend.devices.get_mut(&0).unwrap().asserted = true;
    assert_eq!(broker.interrupt(cap, 58), Err(Error::Exhausted));
    assert_eq!(broker.ack(PID, cap, 1), Err(Error::Quarantined));
    broker.retry_retirements().unwrap();
    broker.epoch = u64::MAX;
    assert_eq!(broker.grant(0, PID, ALL), Err(Error::Exhausted));
    assert!(broker.backend.devices.is_empty());
}

#[test]
fn attribut_mmio_adapte_au_processeur() {
    let mut bus = TestBus::ordinary();
    let foreign = if cfg!(target_arch = "aarch64") {
        DeviceMemory::X86Uncacheable
    } else {
        DeviceMemory::ArmDevice
    };
    let window = Window {
        attribute: foreign,
        ..POLICY.window.unwrap()
    };
    bus.policies.insert(
        (0, PID),
        Policy {
            window: Some(window),
            ..POLICY
        },
    );
    let mut broker = Broker::<_, 1>::new(bus);
    assert_eq!(broker.grant(0, PID, Rights::READ), Err(Error::Denied));
    assert!(broker.backend.devices.is_empty());
}
