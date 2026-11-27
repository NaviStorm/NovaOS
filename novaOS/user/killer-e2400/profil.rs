//! Géométrie proposée, sur des copies CPU. Aucun accès à une adresse de bus.
pub const PAGE: usize = 4096;
pub const ENTREES: usize = 256;
pub const TAMPON: usize = 2048;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AccesNic {
    Lecture,
    Ecriture,
}

pub struct Region {
    pub role: &'static str,
    pub acces: AccesNic,
    pub octets: usize,
}
impl Region {
    pub const fn cadres(&self) -> usize {
        self.octets.div_ceil(PAGE)
    }
}
pub const REGIONS: [Region; 5] = [
    Region {
        role: "tpd:r",
        acces: AccesNic::Lecture,
        octets: ENTREES * 16,
    },
    Region {
        role: "rfd:r",
        acces: AccesNic::Lecture,
        octets: ENTREES * 8,
    },
    Region {
        role: "rrd:w",
        acces: AccesNic::Ecriture,
        octets: ENTREES * 16,
    },
    Region {
        role: "tx:r",
        acces: AccesNic::Lecture,
        octets: ENTREES * TAMPON,
    },
    Region {
        role: "rx:w",
        acces: AccesNic::Ecriture,
        octets: ENTREES * TAMPON,
    },
];

pub const fn cadres_dma() -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < REGIONS.len() {
        total += REGIONS[i].cadres();
        i += 1;
    }
    total
}

/// Chaque anneau occupe sa propre page. Ils partagent les 32 bits hauts.
/// Les données, adressées sur 64 bits, ne sont pas concernées par cette contrainte.
pub fn bases_anneaux_valides(bases: [u64; 3]) -> bool {
    bases.iter().enumerate().all(|(i, base)| {
        base.is_multiple_of(PAGE as u64)
            && !bases[..i].contains(base)
            && base >> 32 == bases[0] >> 32
            && base
                .checked_add(REGIONS[i].octets as u64 - 1)
                .is_some_and(|fin| fin >> 32 == bases[0] >> 32)
    })
}
