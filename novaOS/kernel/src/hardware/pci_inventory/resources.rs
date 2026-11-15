//! Ressources PCI. L’inventaire générique ne possède aucun accès en écriture.
//! Les registres de pont décrivent des fenêtres, pas les tailles des BAR en aval.
use super::{Address, ConfigRead, Error, Reader};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub base: u64,
    /// Dernière adresse incluse, également représentable en haut de l’espace 64 bits.
    pub last: u64,
}

impl Span {
    fn new(base: u64, last: u64) -> Option<Self> {
        (base <= last).then_some(Self { base, last })
    }
    #[cfg(any(test, feature = "materiel-lab"))]
    fn intersects(self, other: Self) -> bool {
        self.base <= other.last && other.base <= self.last
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeWindows {
    pub io: Option<Span>,
    pub memory: Option<Span>,
    pub prefetch: Option<Span>,
}

pub(super) fn bridge_windows<C: ConfigRead>(
    reader: &mut Reader<'_, C>,
    a: Address,
) -> Result<BridgeWindows, Error> {
    // En-tête 1 standard, fenêtres E/S de granularité 4 Kio et mémoire de 1 Mio.
    // Le couple base > limite signale une fenêtre fermée.
    let io = reader.get(a, 0x1c)? as u16;
    let memory = reader.get(a, 0x20)?;
    let pref = reader.get(a, 0x24)?;
    if io & 0x0f != (io >> 8) & 0x0f
        || io & 0x0f > 1
        || memory & 0x000f_000f != 0
        || pref & 0xf != (pref >> 16) & 0xf
        || pref & 0xf > 1
    {
        return Err(Error::Bridge);
    }
    let mut io_start = u64::from(io & 0x00f0) << 8;
    let mut io_last = u64::from(io & 0xf000) | 0xfff;
    if io & 1 != 0 {
        let hi = reader.get(a, 0x30)?;
        io_start |= u64::from(hi & 0xffff) << 16;
        io_last |= u64::from(hi & 0xffff0000);
    }
    let mut pref_start = u64::from(pref & 0xfff0) << 16;
    let mut pref_last = u64::from(pref & 0xfff0_0000) | 0xf_ffff;
    if pref & 1 != 0 {
        pref_start |= u64::from(reader.get(a, 0x28)?) << 32;
        pref_last |= u64::from(reader.get(a, 0x2c)?) << 32;
    }
    Ok(BridgeWindows {
        io: Span::new(io_start, io_last),
        memory: Span::new(
            u64::from(memory & 0xfff0) << 16,
            u64::from(memory & 0xfff0_0000) | 0xf_ffff,
        ),
        prefetch: Span::new(pref_start, pref_last),
    })
}

#[cfg(any(test, feature = "materiel-lab"))]
#[path = "lab_resources.rs"]
pub mod lab;
