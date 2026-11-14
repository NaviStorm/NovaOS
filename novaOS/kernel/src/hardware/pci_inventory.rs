//! Inventaire de configuration PCI en lecture seule. Les BAR décrivent les adresses
//! annoncées par le firmware ; leur taille et leur attribution relèvent de la plateforme.
//! PCI-SIG, PCI Express Base Specification, chapitre 7 (espaces de configuration).
//! UEFI PI 1.9, volume V, chapitre 10 (plages de bus des ponts hôtes).

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Address {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

/// La plateforme atteste le bus racine et sa fenêtre de configuration accessible.
#[derive(Clone, Copy, Debug)]
pub struct BusRange {
    pub segment: u16,
    pub first: u8,
    pub last: u8,
}

pub trait ConfigRead {
    /// Dword aligné du domaine de configuration autorisé. Une fonction absente
    /// rend le Vendor ID 0xffff. None indique un accès matériel impossible.
    fn read32(&mut self, address: Address, offset: u16) -> Option<u32>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarKind {
    Io,
    Memory32,
    MemoryBelow1M,
    Memory64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bar {
    pub kind: BarKind,
    pub base: u64,
    pub prefetchable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Function {
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub interface: u8,
    pub header: u8,
    pub bars: [Option<Bar>; 6],
    pub msi: bool,
    pub msix: bool,
    pub pcie: bool,
    pub irq_pin: u8,
    pub irq_line: u8,
    pub bus_master: bool,
    pub secondary: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Range,
    Read,
    Identity,
    NotReady,
    UnsupportedHeader,
    Bridge,
    AmbiguousBridge,
    Bar,
    Capability,
    InterruptPin,
    Changed,
    Budget,
}

// Une fenêtre très peuplée reste inventoriable. Le résultat global signale toute
// troncature avant de laisser considérer l'inventaire comme complet.
const MAX_READS: u32 = 131_072;

struct Reader<'a, C> {
    config: &'a mut C,
    remaining: u32,
}

impl<C: ConfigRead> Reader<'_, C> {
    fn get(&mut self, addr: Address, offset: u16) -> Result<u32, Error> {
        self.remaining = self.remaining.checked_sub(1).ok_or(Error::Budget)?;
        self.config.read32(addr, offset).ok_or(Error::Read)
    }
}

fn read_bars<C: ConfigRead>(
    reader: &mut Reader<'_, C>,
    addr: Address,
    count: usize,
) -> Result<([Option<Bar>; 6], u8), Error> {
    let mut bars = [None; 6];
    let mut non_memory_slots = 0;
    let mut i = 0;
    while i < count {
        let low = reader.get(addr, 0x10 + 4 * i as u16)?;
        if low == u32::MAX {
            return Err(Error::Bar);
        }
        let (kind, base, prefetchable) = if low & 1 != 0 {
            if low & 2 != 0 {
                return Err(Error::Bar);
            }
            non_memory_slots |= 1 << i;
            (BarKind::Io, u64::from(low & !3), false)
        } else {
            let base = u64::from(low & !15);
            let (kind, base) = match (low >> 1) & 3 {
                0 => (BarKind::Memory32, base),
                1 if base < 1 << 20 => (BarKind::MemoryBelow1M, base),
                2 if i + 1 < count => {
                    let high = reader.get(addr, 0x14 + 4 * i as u16)?;
                    non_memory_slots |= 1 << (i + 1);
                    (BarKind::Memory64, u64::from(high) << 32 | base)
                }
                _ => return Err(Error::Bar),
            };
            (kind, base, low & 8 != 0)
        };
        if base != 0 {
            bars[i] = Some(Bar {
                kind,
                base,
                prefetchable,
            });
        }
        i += if kind == BarKind::Memory64 { 2 } else { 1 };
    }
    Ok((bars, non_memory_slots))
}

fn capabilities<C: ConfigRead>(
    reader: &mut Reader<'_, C>,
    addr: Address,
    function: &mut Function,
    bar_count: usize,
    non_memory_slots: u8,
) -> Result<(), Error> {
    let mut pointer = (reader.get(addr, 0x34)? & 0xff) as u16;
    if pointer == 0 {
        return Err(Error::Capability);
    }
    let mut occupied = 0u64;
    while pointer != 0 {
        if !(0x40..=0xfc).contains(&pointer) || pointer % 4 != 0 {
            return Err(Error::Capability);
        }
        let slot = pointer / 4;
        if occupied & (1u64 << slot) != 0 {
            return Err(Error::Capability);
        }
        let head = reader.get(addr, pointer)?;
        let id = head as u8;
        let control = (head >> 16) as u16;
        let mut words: u16 = 1;
        match id {
            5 => {
                let capable = (control >> 1) & 7;
                if function.msi || capable > 5 || (control >> 4) & 7 > capable {
                    return Err(Error::Capability);
                }
                words =
                    3 + u16::from(control & (1 << 7) != 0) + 2 * u16::from(control & (1 << 8) != 0);
                function.msi = true;
            }
            0x10 => {
                if function.pcie || control & 15 == 0 {
                    return Err(Error::Capability);
                }
                // En-tête, Device Capabilities et Device Control/Status sont communs.
                words = 3;
                function.pcie = true;
            }
            0x11 => {
                if function.msix || pointer > 0xf4 {
                    return Err(Error::Capability);
                }
                let table = reader.get(addr, pointer + 4)?;
                let pba = reader.get(addr, pointer + 8)?;
                for descriptor in [table, pba] {
                    let index = (descriptor & 7) as usize;
                    if index >= bar_count || non_memory_slots & (1 << index) != 0 {
                        return Err(Error::Capability);
                    }
                }
                let vectors = u64::from((control & 0x7ff) + 1);
                if table & 7 == pba & 7 {
                    let start = u64::from(table & !7);
                    let pending = u64::from(pba & !7);
                    if start < pending + vectors.div_ceil(64) * 8 && pending < start + vectors * 16
                    {
                        return Err(Error::Capability);
                    }
                }
                words = 3;
                function.msix = true;
            }
            0xff => return Err(Error::Capability),
            _ => (),
        }
        if slot + words > 64 {
            return Err(Error::Capability);
        }
        let mask = ((1u64 << words) - 1) << slot;
        if occupied & mask != 0 {
            return Err(Error::Capability);
        }
        occupied |= mask;
        pointer = ((head >> 8) & 0xff) as u16;
    }
    Ok(())
}

struct Topology {
    // Pour chaque numéro de bus, le pont parent qui a réservé sa fenêtre.
    // La limite est renseignée uniquement pour les bus secondaires à explorer.
    parent: [Option<u8>; 256],
    limit: [Option<u8>; 256],
}

impl Topology {
    fn attach(&mut self, bus: u8, reg: u32) -> Result<Option<u8>, Error> {
        let primary = reg as u8;
        let secondary = (reg >> 8) as u8;
        let last = (reg >> 16) as u8;
        if secondary == 0 && last == 0 {
            return Ok(None); // Pont non configuré.
        }
        if primary != bus
            || secondary <= bus
            || last < secondary
            || last > self.limit[usize::from(bus)].ok_or(Error::Bridge)?
        {
            return Err(Error::Bridge);
        }
        let upstream = self.parent[usize::from(bus)];
        let child = &mut self.parent[usize::from(secondary)..=usize::from(last)];
        if child.iter().any(|parent| *parent != upstream) {
            // Le cycle de configuration pourrait être revendiqué par deux ponts.
            return Err(Error::AmbiguousBridge);
        }
        child.fill(Some(bus));
        self.limit[usize::from(secondary)] = Some(last);
        Ok(Some(secondary))
    }
}

fn inspect<C: ConfigRead>(
    reader: &mut Reader<'_, C>,
    topology: &mut Topology,
    addr: Address,
    id: u32,
    header: u8,
) -> Result<Function, Error> {
    let class = reader.get(addr, 8)?;
    let command_status = reader.get(addr, 4)?;
    let header = header & 0x7f;
    let bar_count = match header {
        0 => 6,
        1 if class >> 16 == 0x0604 => 2,
        1 => return Err(Error::Bridge),
        _ => return Err(Error::UnsupportedHeader),
    };
    if header == 0 && class >> 16 == 0x0604 {
        return Err(Error::Bridge);
    }
    let irq = reader.get(addr, 0x3c)?;
    let irq_pin = (irq >> 8) as u8;
    if irq_pin > 4 {
        return Err(Error::InterruptPin);
    }
    let (bars, non_memory_slots) = read_bars(reader, addr, bar_count)?;
    let mut function = Function {
        vendor: id as u16,
        device: (id >> 16) as u16,
        class: (class >> 24) as u8,
        subclass: (class >> 16) as u8,
        interface: (class >> 8) as u8,
        header,
        bars,
        msi: false,
        msix: false,
        pcie: false,
        irq_pin,
        irq_line: irq as u8,
        bus_master: command_status & 4 != 0,
        secondary: None,
    };
    if command_status & (1 << 20) != 0 {
        capabilities(reader, addr, &mut function, bar_count, non_memory_slots)?;
    }
    let bridge = if header == 1 {
        Some(reader.get(addr, 0x18)?)
    } else {
        None
    };
    if reader.get(addr, 0)? != id {
        return Err(Error::Changed);
    }
    if let Some(register) = bridge {
        function.secondary = topology.attach(addr.bus, register)?;
    }
    Ok(function)
}

/// Le callback reçoit les fonctions présentes, même celles dont la description
/// est refusée. Une erreur globale signale un relevé incomplet (transport,
/// budget ou routage de pont invalide).
/// Les erreurs locales n'autorisent ni la visite d'un pont invalide, ni un pilote.
pub fn enumerate(
    config: &mut impl ConfigRead,
    range: BusRange,
    mut report: impl FnMut(Address, Result<Function, Error>),
) -> Result<usize, Error> {
    if range.first > range.last {
        return Err(Error::Range);
    }
    let mut reader = Reader {
        config,
        remaining: MAX_READS,
    };
    let mut topology = Topology {
        parent: [None; 256],
        limit: [None; 256],
    };
    topology.limit[usize::from(range.first)] = Some(range.last);
    let mut count = 0;
    for bus in range.first..=range.last {
        if topology.limit[usize::from(bus)].is_none() {
            continue;
        }
        for device in 0..32 {
            for function in 0..8 {
                let addr = Address {
                    segment: range.segment,
                    bus,
                    device,
                    function,
                };
                let id = reader.get(addr, 0)?;
                if id as u16 == 0xffff {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                // Le Vendor ID réservé CRS indique un périphérique en initialisation.
                if id as u16 <= 1 {
                    report(
                        addr,
                        Err(if id == 0xffff_0001 {
                            Error::NotReady
                        } else {
                            Error::Identity
                        }),
                    );
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let header = (reader.get(addr, 0xc)? >> 16) as u8;
                let result = inspect(&mut reader, &mut topology, addr, id, header);
                match result {
                    Err(error @ (Error::Budget | Error::Read)) => return Err(error),
                    Err(error) if header & 0x7f != 0 || error == Error::Bridge => {
                        // Un pont refusé peut encore router des cycles vers la
                        // fenêtre d'un frère. N'accéder à aucun bus en aval.
                        report(addr, result);
                        return Err(error);
                    }
                    Ok(_) => count += 1,
                    Err(_) => (),
                }
                report(addr, result);
                if function == 0 && header & 0x80 == 0 {
                    break;
                }
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
#[path = "pci_inventory/tests.rs"]
mod tests;
