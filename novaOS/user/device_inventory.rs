//! Contrat privé du lanceur avec le gestionnaire. Tout est copié et descriptif.
pub const MAGIC: &[u8; 4] = b"NDI1";
pub const HEADER: usize = 8;
pub const RECORD: usize = 16;
pub const MAX_DEVICES: usize = 128;
pub const PRIVATE_PAGE: usize = 4096;

// segment LE, bus, devfn, constructeur LE, modèle LE, classe, sous-classe,
// interface, masque BAR, pin INTx, flags MSI/MSI-X/PCIe, ligne IRQ, en-tête.
pub fn model(vendor: u16, device: u16) -> Option<(usize, &'static [u8])> {
    const KNOWN: [(u16, u16, &[u8]); 6] = [
        (0x8086, 0x9a49, b"Intel Iris Xe"),
        (0x8086, 0x15fc, b"Intel Ethernet I219-V"),
        (0x8086, 0xa0f0, b"Intel Wi-Fi AX201"),
        (0x8086, 0xa0ed, b"Intel USB xHCI"),
        (0x10ec, 0x522a, b"Realtek SD"),
        (0x1e0f, 0x0001, b"Kioxia NVMe"),
    ];
    KNOWN
        .iter()
        .enumerate()
        .find_map(|(i, &(v, d, name))| (vendor == v && device == d).then_some((i, name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tecra_inventory_without_driver() {
        for (i, (v, d)) in [
            (0x8086, 0x9a49),
            (0x8086, 0x15fc),
            (0x8086, 0xa0f0),
            (0x8086, 0xa0ed),
            (0x10ec, 0x522a),
            (0x1e0f, 0x0001),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(model(v, d).unwrap().0, i);
        }
        assert!(model(0x1af4, 0x1001).is_none());
    }
}
