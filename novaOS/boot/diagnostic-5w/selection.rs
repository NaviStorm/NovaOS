use novaos_catalogue::{Bus, Error};

pub(crate) fn categorie(classe: u32) -> usize {
    match classe >> 16 {
        1 => 0,
        2 => 1,
        3 => 2,
        _ if classe >> 8 == 0x0c03 => 3,
        _ => 4,
    }
}
pub(crate) fn select_pci(
    cat: &novaos_catalogue::Catalog,
    bus: Bus,
    attachment: &str,
) -> Result<Option<novaos_catalogue::Plan>, Error> {
    use novaos_catalogue::{Limits, Target};
    let cible = Target {
        revision: 1,
        profile: None,
        attachment,
        bus,
        class: "diagnostic",
        isa: "x86_64",
        os: [0, 20, 0],
        abi: 1,
        features: &[],
        caps: &[],
        dma: &[],
        mmio: &[],
        irq: &[],
        allow_class_match: false,
        limits: Limits {
            private_pages: 0,
            roots: 0,
            handles: 0,
            boxes: 0,
            pending: 0,
            dma_pages: 0,
            queues: 0,
            entries: 0,
            irq_routes: 0,
        },
    };
    match cat.select(&cible, None) {
        Ok(plan) => Ok(Some(plan)),
        Err(Error::NoMatch) => Ok(None),
        Err(e) => Err(e),
    }
}
