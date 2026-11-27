use super::*;

const RSDP: usize = 0x100;
const ROOT: usize = 0x200;
const FADT: usize = 0x400;
const EXTRA: usize = 0x800;

fn put32(buffer: &mut [u8], at: usize, value: u32) {
    buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(buffer: &mut [u8], at: usize, value: u64) {
    buffer[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn sign(bytes: &mut [u8], at: usize) {
    bytes[at] = 0;
    bytes[at] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b)));
}
fn table(signature: &[u8; 4], length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    bytes[..4].copy_from_slice(signature);
    put32(&mut bytes, 4, length as u32);
    bytes[8] = 1;
    bytes
}
fn image(legacy: bool, mut extra: Vec<u8>) -> Vec<u8> {
    let mut ram = vec![0; EXTRA + extra.len()];
    let mut root = table(
        if legacy { b"RSDT" } else { b"XSDT" },
        if legacy { 44 } else { 52 },
    );
    if legacy {
        put32(&mut root, 36, FADT as u32);
        put32(&mut root, 40, EXTRA as u32);
    } else {
        put64(&mut root, 36, FADT as u64);
        put64(&mut root, 44, EXTRA as u64);
    }
    sign(&mut root, 9);
    ram[ROOT..ROOT + root.len()].copy_from_slice(&root);
    let mut fadt = table(b"FACP", 116);
    put32(&mut fadt, 40, 0x10000);
    sign(&mut fadt, 9);
    ram[FADT..FADT + fadt.len()].copy_from_slice(&fadt);
    let rsdp = &mut ram[RSDP..RSDP + 36];
    rsdp[..8].copy_from_slice(b"RSD PTR ");
    if legacy {
        put32(rsdp, 16, ROOT as u32);
    } else {
        rsdp[15] = 2;
        put32(rsdp, 20, 36);
        put64(rsdp, 24, ROOT as u64);
    }
    sign(&mut rsdp[..20], 8);
    if !legacy {
        sign(rsdp, 32);
    }
    sign(&mut extra, 9);
    ram[EXTRA..].copy_from_slice(&extra);
    ram
}
fn probe(ram: &[u8]) -> Result<Inventory> {
    discover(RSDP as u64, |address, dst| {
        let Some(src) = ram.get(address as usize..).and_then(|s| s.get(..dst.len())) else {
            return false;
        };
        dst.copy_from_slice(src);
        true
    })
}
fn mcfg(windows: &[(u64, u16, u8, u8)]) -> Vec<u8> {
    let mut table = table(b"MCFG", 44 + 16 * windows.len());
    for (entry, &(base, segment, first, last)) in table[44..].chunks_exact_mut(16).zip(windows) {
        put64(entry, 0, base);
        entry[8..10].copy_from_slice(&segment.to_le_bytes());
        entry[10] = first;
        entry[11] = last;
    }
    table
}

#[test]
fn racine_32_et_64_et_fenetre_distincte_de_bus_zero() {
    for legacy in [true, false] {
        let window = mcfg(&[(0x80000000, 1, 4, 7), (0x80000000, 2, 8, 8)]);
        let inv = probe(&image(legacy, window)).unwrap();
        assert_eq!(inv.xsdt, !legacy);
        assert_eq!(inv.revision, if legacy { 0 } else { 2 });
        assert_eq!(inv.tables, [*b"FACP", *b"MCFG"]);
        assert_eq!(
            inv.ecam,
            [
                Ecam {
                    base: 0x80000000,
                    segment: 1,
                    first_bus: 4,
                    last_bus: 7
                },
                Ecam {
                    base: 0x80000000,
                    segment: 2,
                    first_bus: 8,
                    last_bus: 8
                }
            ]
        );
    }
}

#[test]
fn sommes_racines_et_enfants_independantes() {
    let good = image(false, table(b"SSDT", 40));
    for position in [RSDP + 8, RSDP + 32, ROOT + 9, FADT + 9, EXTRA + 9] {
        let mut bad = good.clone();
        bad[position] ^= 1;
        assert_eq!(probe(&bad).unwrap_err(), Error::Checksum);
    }
    let mut bad = good;
    bad[RSDP] = b'X';
    assert_eq!(probe(&bad).unwrap_err(), Error::Signature);
}

#[test]
fn pointeur_inaccessible_taille_tronquee_et_relecture_incoherente() {
    let mut ram = image(false, table(b"SSDT", 40));
    assert_eq!(probe(&ram[..ram.len() - 1]).unwrap_err(), Error::Address);
    put32(&mut ram, EXTRA + 4, 35);
    assert_eq!(probe(&ram).unwrap_err(), Error::Length);
    put32(&mut ram, EXTRA + 4, u32::MAX);
    assert_eq!(probe(&ram).unwrap_err(), Error::Limit);
    put64(&mut ram, ROOT + 44, u64::MAX);
    sign(&mut ram[ROOT..ROOT + 52], 9);
    assert_eq!(probe(&ram).unwrap_err(), Error::Address);
    let ram = image(false, table(b"SSDT", 40));
    let mut reads = 0;
    let result = discover(RSDP as u64, |address, dst| {
        dst.copy_from_slice(&ram[address as usize..address as usize + dst.len()]);
        if address == EXTRA as u64 {
            reads += 1;
            if reads == 2 {
                dst[3] ^= 1;
            }
        }
        true
    });
    assert_eq!(result.unwrap_err(), Error::Format);
}

#[test]
fn pointeurs_dupliques_et_chevauchements_refuses() {
    let mut ram = image(false, table(b"SSDT", 40));
    put64(&mut ram, ROOT + 44, FADT as u64);
    sign(&mut ram[ROOT..ROOT + 52], 9);
    assert_eq!(probe(&ram).unwrap_err(), Error::Overlap);
    assert_eq!(
        probe(&image(false, table(b"XSDT", 40))).unwrap_err(),
        Error::Signature
    );
    assert_eq!(
        probe(&image(false, table(b"FACP", 116))).unwrap_err(),
        Error::Format
    );
}

#[test]
fn mcfg_refuse_alias_et_depassement() {
    for (entries, error) in [
        (vec![(0x80000000, 0, 5, 4)], Error::Format),
        (vec![(0x80000001, 0, 0, 0)], Error::Format),
        (vec![(u64::MAX & !0xfffff, 1, 0, 0)], Error::Address),
        (
            vec![(0x80000000, 0, 0, 8), (0x90000000, 0, 7, 11)],
            Error::Overlap,
        ),
        (
            vec![(0x80000000, 0, 0, 2), (0x80000000, 1, 2, 3)],
            Error::Overlap,
        ),
    ] {
        assert_eq!(probe(&image(false, mcfg(&entries))).unwrap_err(), error);
    }
}

#[test]
fn madt_srat_verifient_les_sous_structures() {
    for (signature, begin, kind, length) in [(b"APIC", 44, 1, 12), (b"SRAT", 48, 1, 40)] {
        let mut t = table(signature, begin + length);
        if signature == b"SRAT" {
            put32(&mut t, 36, 1);
        }
        t[begin] = kind;
        t[begin + 1] = length as u8;
        assert!(probe(&image(false, t.clone())).is_ok());
        for len in [0, 1, length - 1, length + 1] {
            t[begin + 1] = len as u8;
            assert_eq!(probe(&image(false, t.clone())).unwrap_err(), Error::Length);
        }
    }
}

#[test]
fn dmar_portee_et_adresse_unite() {
    let mut dmar = table(b"DMAR", 48 + 16 + 8);
    dmar[36] = 47;
    dmar[50] = 24;
    put64(&mut dmar, 56, 0xfee00000);
    dmar[64] = 1;
    dmar[65] = 8;
    assert!(probe(&image(false, dmar.clone())).is_ok());
    dmar[65] = 0;
    assert_eq!(
        probe(&image(false, dmar.clone())).unwrap_err(),
        Error::Length
    );
    dmar[65] = 8;
    dmar[70] = 32;
    assert_eq!(
        probe(&image(false, dmar.clone())).unwrap_err(),
        Error::Format
    );
    put64(&mut dmar, 56, 0x123);
    assert_eq!(probe(&image(false, dmar)).unwrap_err(), Error::Address);
}

#[test]
fn iort_verifie_noeuds_et_references_de_sortie() {
    let mut t = table(b"IORT", 48 + 24 + 32 + 20);
    put32(&mut t, 36, 2);
    put32(&mut t, 40, 48);
    t[49] = 24;
    put32(&mut t, 64, 1);
    let root = 72;
    t[root] = 2;
    t[root + 1] = 52;
    put32(&mut t, root + 8, 1);
    put32(&mut t, root + 12, 32);
    put32(&mut t, root + 32 + 12, 48);
    assert!(probe(&image(false, t.clone())).is_ok());
    put32(&mut t, root + 32 + 12, 49);
    assert_eq!(probe(&image(false, t.clone())).unwrap_err(), Error::Format);
    put32(&mut t, root + 12, u32::MAX);
    assert_eq!(probe(&image(false, t)).unwrap_err(), Error::Length);
}

#[test]
fn slit_controle_matrice_et_fadt_taille_selon_revision() {
    let mut t = table(b"SLIT", 48);
    put64(&mut t, 36, 2);
    t[44..].copy_from_slice(&[10, 20, 20, 10]);
    assert!(probe(&image(false, t.clone())).is_ok());
    t[44] = 1;
    assert_eq!(probe(&image(false, t.clone())).unwrap_err(), Error::Format);
    put64(&mut t, 36, u64::MAX);
    assert_eq!(probe(&image(false, t)).unwrap_err(), Error::Length);
    let mut ram = image(false, table(b"SSDT", 40));
    ram[FADT + 8] = 6;
    sign(&mut ram[FADT..FADT + 116], 9);
    assert_eq!(probe(&ram).unwrap_err(), Error::Length);
}

#[test]
fn limites_entrees_et_table_inconnue() {
    let unknown = table(b"OEM1", 5000);
    assert_eq!(
        probe(&image(false, unknown)).unwrap().tables.last(),
        Some(b"OEM1")
    );
    let mut rsdp = image(false, table(b"SSDT", 40));
    let len = HEADER + 8 * (MAX_TABLES + 1);
    rsdp[ROOT..ROOT + len].fill(0);
    rsdp[ROOT..ROOT + 4].copy_from_slice(b"XSDT");
    put32(&mut rsdp, ROOT + 4, len as u32);
    sign(&mut rsdp[ROOT..ROOT + len], 9);
    assert_eq!(probe(&rsdp).unwrap_err(), Error::Limit);
}

#[test]
fn iort_cycle_indirect_et_portee_smmu() {
    let mut t = table(b"IORT", 48 + 2 * 52);
    put32(&mut t, 36, 2);
    put32(&mut t, 40, 48);
    for (at, other) in [(48, 100), (100, 48)] {
        t[at] = 2;
        t[at + 1] = 52;
        put32(&mut t, at + 8, 1);
        put32(&mut t, at + 12, 32);
        put32(&mut t, at + 44, other);
    }
    assert_eq!(probe(&image(false, t)).unwrap_err(), Error::Format);
    let mut t = table(b"IORT", 48 + 84);
    put32(&mut t, 36, 1);
    put32(&mut t, 40, 48);
    t[48] = 3;
    t[49] = 84;
    put32(&mut t, 48 + 40, 60); // Globaux dans [60,76).
    put32(&mut t, 48 + 44, 1);
    put32(&mut t, 48 + 48, 76);
    assert!(probe(&image(false, t.clone())).is_ok());
    put32(&mut t, 48 + 48, 68);
    assert_eq!(probe(&image(false, t.clone())).unwrap_err(), Error::Overlap);
    put32(&mut t, 48 + 48, 80);
    assert_eq!(probe(&image(false, t)).unwrap_err(), Error::Length);
}

#[test]
fn ecam_ne_peut_pas_designer_la_memoire_des_tables() {
    let mut t = image(false, mcfg(&[(0x100000, 0, 0, 0)]));
    put64(&mut t, ROOT + 36, 0x100000 + FADT as u64);
    put64(&mut t, ROOT + 44, 0x100000 + EXTRA as u64);
    sign(&mut t[ROOT..ROOT + 52], 9);
    put64(&mut t, RSDP + 24, 0x100000 + ROOT as u64);
    sign(&mut t[RSDP..RSDP + 36], 32);
    let mut high = vec![0; 0x100000];
    high.extend_from_slice(&t);
    let inv = discover(0x100000 + RSDP as u64, |addr, dst| {
        dst.copy_from_slice(&high[addr as usize..addr as usize + dst.len()]);
        true
    });
    assert_eq!(inv.unwrap_err(), Error::Overlap);
}

#[test]
fn iort_plages_reservees_descriptives_bornees() {
    let mut t = table(b"IORT", 48 + 68);
    put32(&mut t, 36, 1);
    put32(&mut t, 40, 48);
    t[48] = 6;
    t[49] = 68;
    put32(&mut t, 68, 2);
    put32(&mut t, 72, 28);
    put64(&mut t, 76, 0x30000);
    put64(&mut t, 84, 0x10000);
    put64(&mut t, 96, 0x20000);
    put64(&mut t, 104, 0x10000);
    assert!(probe(&image(false, t.clone())).is_ok());
    put64(&mut t, 104, 0x20000);
    assert_eq!(probe(&image(false, t.clone())).unwrap_err(), Error::Overlap);
    put32(&mut t, 72, u32::MAX);
    assert_eq!(probe(&image(false, t)).unwrap_err(), Error::Length);
}

fn dmar_q35(include_all: bool) -> Vec<u8> {
    let size = if include_all { 24 } else { 64 };
    let mut d = table(b"DMAR", 48 + size);
    d[36] = 47;
    d[50..52].copy_from_slice(&(size as u16).to_le_bytes());
    d[52] = u8::from(include_all);
    put64(&mut d, 56, 0xfed9_0000);
    d[64..72].copy_from_slice(&[3, 8, 0, 0, 0, 255, 0, 0]);
    if !include_all {
        for (s, (dev, fun)) in
            d[72..]
                .chunks_exact_mut(8)
                .zip([(0, 0), (6, 0), (31, 0), (31, 2), (31, 3)])
        {
            s.copy_from_slice(&[1, 8, 0, 0, 0, 0, dev, fun]);
        }
    }
    d
}

#[test]
fn drhd_q35_une_unite_portee_non_ambigue() {
    for include_all in [false, true] {
        let drhd = probe(&image(false, dmar_q35(include_all)))
            .unwrap()
            .q35_dmar
            .unwrap();
        assert_eq!(
            (drhd.base, drhd.host_bits, drhd.explicit),
            (0xfed9_0000, 48, !include_all)
        );
    }
    assert!(
        probe(&image(false, mcfg(&[(0xe0000000, 0, 0, 255)])))
            .unwrap()
            .q35_dmar
            .is_none()
    );
}

#[test]
fn drhd_hors_profil_ne_delivre_pas_de_registres() {
    for (offset, value) in [
        (8, 2),
        (36, 38),
        (37, 128),
        (38, 1),
        (52, 1),
        (53, 1),
        (54, 1),
        (59, 0xfd),
        (64, 4),
        (69, 0),
        (80, 2),
        (86, 0),
        (86, 5),
        (93, 1),
        (111, 4),
    ] {
        let mut d = dmar_q35(false);
        assert_ne!(d[offset], value);
        d[offset] = value;
        assert!(
            probe(&image(false, d)).map_or(true, |inv| inv.q35_dmar.is_none()),
            "offset {offset}"
        );
    }
}

#[test]
fn drhd_refuse_rmrr_unites_multiples_et_portee_manquante() {
    for n in [0u16, 1, 2, 0xfffe] {
        let mut d = dmar_q35(false);
        let mut record = vec![0u8; if n == 1 { 24 } else { 16 }];
        record[..2].copy_from_slice(&n.to_le_bytes());
        let l = record.len() as u16;
        record[2..4].copy_from_slice(&l.to_le_bytes());
        if n == 0 {
            put64(&mut record, 8, 0xfed9_1000);
        }
        d.extend_from_slice(&record);
        let len = d.len() as u32;
        put32(&mut d, 4, len);
        assert!(probe(&image(false, d)).map_or(true, |inv| inv.q35_dmar.is_none()));
    }
    let mut d = dmar_q35(false);
    d.drain(80..88);
    d[50] = 56;
    let n = d.len() as u32;
    put32(&mut d, 4, n);
    assert!(probe(&image(false, d)).unwrap().q35_dmar.is_none());
}

#[test]
fn visiteur_borne_avant_publication() {
    let ram = image(false, table(b"HPET", 56));
    let mut visites = Vec::new();
    let bilan = discover_with(RSDP as u64, |addr, dst| {
        let Some(src) = ram.get(addr as usize..).and_then(|s| s.get(..dst.len())) else {
            return false;
        };
        dst.copy_from_slice(src);
        true
    }, |sig, data| {
        assert_eq!(&data[..4], &sig);
        visites.push(sig);
        if &sig == b"HPET" { Err(Error::Limit) } else { Ok(()) }
    });
    assert_eq!(visites, [*b"FACP", *b"HPET"]);
    assert!(matches!(bilan, Err(Error::Limit)));
}
