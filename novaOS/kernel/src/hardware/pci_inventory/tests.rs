use super::*;
use std::collections::BTreeMap;

const ROOT: BusRange = BusRange {
    segment: 3,
    first: 5,
    last: 8,
};

fn a(bus: u8, device: u8, function: u8) -> Address {
    Address {
        segment: ROOT.segment,
        bus,
        device,
        function,
    }
}

#[derive(Default)]
struct Fabric {
    registers: BTreeMap<(u8, u8, u8), [u32; 64]>,
    reads: Vec<(Address, u16)>,
    remove_after_probe: bool,
}
impl Fabric {
    fn endpoint(&mut self, a: Address) -> &mut [u32; 64] {
        let row = self
            .registers
            .entry((a.bus, a.device, a.function))
            .or_insert([0; 64]);
        row[0] = 0x9a49_8086;
        row[2] = 0x0300_0201;
        row
    }
    fn bridge(&mut self, a: Address, sec: u8, sub: u8) {
        let row = self.endpoint(a);
        row[2] = 0x0604_0001;
        row[3] = 1 << 16;
        row[6] = u32::from(a.bus) | u32::from(sec) << 8 | u32::from(sub) << 16;
    }
    fn run(&mut self) -> Vec<(Address, Result<Function, Error>)> {
        let mut events = Vec::new();
        let count = enumerate(self, ROOT, |a, r| events.push((a, r))).unwrap();
        assert_eq!(count, events.iter().filter(|(_, r)| r.is_ok()).count());
        events
    }
}
impl ConfigRead for Fabric {
    fn read32(&mut self, a: Address, off: u16) -> Option<u32> {
        assert_eq!(a.segment, ROOT.segment);
        assert!((ROOT.first..=ROOT.last).contains(&a.bus));
        assert!(a.device < 32 && a.function < 8 && off % 4 == 0 && off <= 0xfc);
        if self.remove_after_probe && off == 0 && self.reads.contains(&(a, off)) {
            return Some(u32::MAX);
        }
        self.reads.push((a, off));
        Some(
            self.registers
                .get(&(a.bus, a.device, a.function))
                .map_or(u32::MAX, |row| row[usize::from(off) / 4]),
        )
    }
}

#[test]
fn bus_vide_et_fenetre_invalide() {
    let mut f = Fabric::default();
    assert!(f.run().is_empty());
    assert_eq!(f.reads.len(), 32);
    assert!(
        f.reads
            .iter()
            .all(|(addr, off)| addr.bus == 5 && addr.function == 0 && *off == 0)
    );
    let range = BusRange {
        first: 8,
        last: 5,
        ..ROOT
    };
    assert_eq!(
        enumerate(&mut f, range, |_, _| unreachable!()),
        Err(Error::Range)
    );
    assert_eq!(f.reads.len(), 32);
}

#[test]
fn fonctions_multiples_identite_et_bar_sans_ecriture() {
    let mut f = Fabric::default();
    let zero = a(5, 2, 0);
    let seven = a(5, 2, 7);
    let row = f.endpoint(zero);
    row[3] = 0x80 << 16;
    row[1] = 4;
    row[4] = 0xc001;
    row[5] = 0x8000_0000;
    row[6] = 0x200c;
    row[7] = 0x1;
    row[8] = 0x1002;
    row[15] = 0x204;
    f.endpoint(seven);
    let table = f.registers.clone();
    let found = f.run();
    assert_eq!(f.registers, table);
    assert_eq!(found.len(), 2);
    assert_eq!(found[1].0, seven);
    let fun = found[0].1.unwrap();
    assert_eq!(
        (
            fun.vendor,
            fun.device,
            fun.class,
            fun.subclass,
            fun.interface
        ),
        (0x8086, 0x9a49, 3, 0, 2)
    );
    assert_eq!((fun.irq_pin, fun.irq_line, fun.bus_master), (2, 4, true));
    assert_eq!(fun.bars[0].unwrap().kind, BarKind::Io);
    assert_eq!(fun.bars[1].unwrap().base, 0x8000_0000);
    assert_eq!(
        fun.bars[2],
        Some(Bar {
            kind: BarKind::Memory64,
            base: 0x1_0000_2000,
            prefetchable: true
        })
    );
    assert_eq!(fun.bars[3], None);
    assert_eq!(fun.bars[4].unwrap().kind, BarKind::MemoryBelow1M);
    assert_eq!(fun.bars[5], None);
}

#[test]
fn ponts_imbriques_et_bornes_subordonnees() {
    let mut f = Fabric::default();
    f.bridge(a(5, 0, 0), 6, 7);
    f.bridge(a(6, 0, 0), 7, 7);
    f.endpoint(a(7, 2, 0));
    let r = f.run();
    assert_eq!(r.len(), 3);
    assert_eq!(r[0].1.unwrap().secondary, Some(6));
    assert_eq!(r[1].1.unwrap().secondary, Some(7));
    assert_eq!(r[2].0, a(7, 2, 0));
    assert!(!f.reads.iter().any(|(a, _)| a.bus == 8));

    let mut f = Fabric::default();
    f.bridge(a(5, 0, 0), 6, 7);
    f.bridge(a(6, 0, 0), 8, 8);
    assert_eq!(enumerate(&mut f, ROOT, |_, _| {}), Err(Error::Bridge));
    assert!(!f.reads.iter().any(|(a, _)| a.bus == 8));
}

#[test]
fn ponts_ambigus_et_boucles() {
    let mut f = Fabric::default();
    f.bridge(a(5, 0, 0), 6, 7);
    f.bridge(a(5, 1, 0), 7, 8);
    let mut seen = Vec::new();
    assert_eq!(
        enumerate(&mut f, ROOT, |a, r| seen.push((a, r))),
        Err(Error::AmbiguousBridge)
    );
    assert_eq!(seen[1].1, Err(Error::AmbiguousBridge));
    assert!(f.reads.iter().all(|(a, _)| a.bus == ROOT.first));

    let mut f = Fabric::default();
    f.bridge(a(5, 0, 0), 6, 7);
    f.bridge(a(6, 0, 0), 0, 0);
    f.bridge(a(6, 1, 0), 5, 7);
    let mut r = Vec::new();
    assert_eq!(
        enumerate(&mut f, ROOT, |a, x| r.push((a, x))),
        Err(Error::Bridge)
    );
    assert_eq!(r[1].1.unwrap().secondary, None);
    assert_eq!(r[2].1, Err(Error::Bridge));
    assert!(!f.reads.iter().any(|(a, _)| a.bus >= 7));
}

#[test]
fn liste_de_capacites_msi_msix_pcie() {
    let mut f = Fabric::default();
    let row = f.endpoint(a(5, 0, 0));
    row[1] = 1 << 20;
    row[13] = 0x40;
    row[16] = 0x5005;
    row[20] = 0x6011;
    row[21] = 0x1000;
    row[22] = 0x2000;
    row[24] = 0x0002_0010;
    let fun = f.run()[0].1.unwrap();
    assert!(fun.msi && fun.msix && fun.pcie);
}

#[test]
fn capacites_refusees_sans_depassement() {
    for (ptr, head) in [
        (0x40, 0x4001),
        (0x40, 0x4405),
        (0x40, 0x0040ff05),
        (0xf8, 0x11),
        (0xfc, 5),
        (0x41, 0),
        (0x38, 0),
        (0x40, 0x10),
        (0, 0),
    ] {
        let mut f = Fabric::default();
        let row = f.endpoint(a(5, 0, 0));
        row[1] = 1 << 20;
        row[13] = ptr;
        if ptr >= 0x40 {
            row[ptr as usize / 4] = head;
        }
        assert_eq!(
            f.run()[0].1,
            Err(Error::Capability),
            "ptr={ptr:#x} head={head:#x}"
        );
    }
}

#[test]
fn table_msix_bir_invalide_ou_zones_superposees() {
    for (bar, table, pba) in [
        (1, 0x1000, 0x2000),
        (4, 0x1001, 0x2001),
        (0, 0x1006, 0x2000),
        (0, 0, 0),
    ] {
        let mut f = Fabric::default();
        let row = f.endpoint(a(5, 0, 0));
        row[1] = 1 << 20;
        row[4] = bar;
        row[13] = 0x40;
        row[16] = 0x11;
        row[17] = table;
        row[18] = pba;
        assert_eq!(f.run()[0].1, Err(Error::Capability));
    }
}

#[test]
fn formats_bar_refuses() {
    for (slot, value) in [(4, 3), (4, u32::MAX), (4, 6), (4, 0x10_0002), (9, 4)] {
        let mut f = Fabric::default();
        f.endpoint(a(5, 0, 0))[slot] = value;
        assert_eq!(f.run()[0].1, Err(Error::Bar));
    }
}

#[test]
fn identite_changee_ou_non_prete_et_types_inconnus() {
    for (reg, value, error) in [
        (0, 0xffff_0001, Error::NotReady),
        (0, 0x1234_0000, Error::Identity),
        (3, 2 << 16, Error::UnsupportedHeader),
        (15, 5 << 8, Error::InterruptPin),
    ] {
        let mut f = Fabric::default();
        f.endpoint(a(5, 0, 0))[reg] = value;
        let mut seen = Vec::new();
        let verdict = enumerate(&mut f, ROOT, |_, result| seen.push(result));
        assert_eq!(seen, [Err(error)]);
        if error == Error::UnsupportedHeader {
            assert_eq!(verdict, Err(error));
        } else {
            assert_eq!(verdict, Ok(0));
        }
    }
    let mut f = Fabric::default();
    f.endpoint(a(5, 0, 0));
    f.remove_after_probe = true;
    assert_eq!(f.run()[0].1, Err(Error::Changed));
}

#[test]
fn transport_en_erreur_distingue_de_l_absence() {
    struct Fault;
    impl ConfigRead for Fault {
        fn read32(&mut self, _: Address, _: u16) -> Option<u32> {
            None
        }
    }
    assert_eq!(
        enumerate(&mut Fault, ROOT, |_, _| unreachable!()),
        Err(Error::Read)
    );
}

#[test]
fn fenetre_dense_limitee_en_lectures() {
    struct Dense;
    impl ConfigRead for Dense {
        fn read32(&mut self, a: Address, off: u16) -> Option<u32> {
            let bridge = a.device == 0 && a.function == 0 && a.bus < 255;
            Some(match off {
                0 => 0x0001_1234,
                8 if bridge => 0x0604_0000,
                12 => (0x80 | u32::from(bridge)) << 16,
                0x18 if bridge => u32::from(a.bus) | u32::from(a.bus + 1) << 8 | 0xff0000,
                _ => 0,
            })
        }
    }
    let mut functions = 0;
    assert_eq!(
        enumerate(
            &mut Dense,
            BusRange {
                segment: 0,
                first: 0,
                last: 255
            },
            |_, result| {
                result.unwrap();
                functions += 1;
            }
        ),
        Err(Error::Budget)
    );
    assert!(functions > 256 && functions < 65_536);
}

#[test]
fn pont_refuse_interdit_toute_lecture_aval() {
    for invalid_slot in [0, 1] {
        let mut f = Fabric::default();
        let bad = a(5, invalid_slot, 0);
        f.bridge(bad, 6, 7);
        f.bridge(a(5, 1 - invalid_slot, 0), 6, 6);
        f.endpoint(a(6, 0, 0));
        let row = f.registers.get_mut(&(5, invalid_slot, 0)).unwrap();
        row[1] = 1 << 20;
        row[13] = 0x40;
        row[16] = 0x4001; // Lien de capacités cyclique sur le pont.
        let mut errors = Vec::new();
        assert_eq!(
            enumerate(&mut f, ROOT, |a, r| if let Err(e) = r {
                errors.push((a, e))
            }),
            Err(Error::Capability)
        );
        assert_eq!(errors, [(bad, Error::Capability)]);
        assert!(f.reads.iter().all(|(a, _)| a.bus == ROOT.first));
    }

    let mut f = Fabric::default();
    f.bridge(a(5, 0, 0), 6, 7);
    f.endpoint(a(6, 0, 0));
    f.remove_after_probe = true;
    assert_eq!(enumerate(&mut f, ROOT, |_, _| {}), Err(Error::Changed));
    assert!(f.reads.iter().all(|(a, _)| a.bus == ROOT.first));
}

#[test]
fn pont_fenetres_32_64_et_io_32() {
    let mut fabric = Fabric::default();
    let b = a(5, 0, 0);
    fabric.bridge(b, 6, 6);
    let row = fabric.registers.get_mut(&(5, 0, 0)).unwrap();
    row[0x1c / 4] = 0x3121;
    row[0x20 / 4] = 0x8ff0_8000;
    row[0x24 / 4] = 0x0031_0011;
    row[0x28 / 4] = 2;
    row[0x2c / 4] = 2;
    row[0x30 / 4] = 0x1234_1234;
    let before = fabric.registers.clone();
    let function = fabric.run()[0].1.unwrap();
    let windows = function.bridge_windows.unwrap();
    use resources::Span;
    assert_eq!(
        windows.io,
        Some(Span {
            base: 0x1234_2000,
            last: 0x1234_3fff
        })
    );
    assert_eq!(
        windows.memory,
        Some(Span {
            base: 0x8000_0000,
            last: 0x8fff_ffff
        })
    );
    assert_eq!(
        windows.prefetch,
        Some(Span {
            base: 0x2_0010_0000,
            last: 0x2_003f_ffff
        })
    );
    assert_eq!(fabric.registers, before);
}

#[test]
fn pont_ferme_et_formats_incoherents() {
    let mut f = Fabric::default();
    f.bridge(a(5, 1, 0), 6, 6);
    let row = f.registers.get_mut(&(5, 1, 0)).unwrap();
    row[7] = 0xf0;
    row[8] = 0x0000_fff0;
    row[9] = 0x0000_fff0;
    let windows = f.run()[0].1.unwrap().bridge_windows.unwrap();
    assert_eq!(
        (windows.io, windows.memory, windows.prefetch),
        (None, None, None)
    );
    for (index, bad) in [(7, 0x0011), (8, 0x00010000), (9, 0x0010_0011)] {
        let row = f.registers.get_mut(&(5, 1, 0)).unwrap();
        row[index] = bad;
        assert_eq!(enumerate(&mut f, ROOT, |_, _| {}), Err(Error::Bridge));
        f.registers.get_mut(&(5, 1, 0)).unwrap()[index] = 0;
    }
}

#[test]
fn tailles_bar_excluent_les_depassements_et_les_frontieres() {
    use resources::Span;
    use resources::lab::extent;
    let bar = |kind, base| Bar {
        kind,
        base,
        prefetchable: false,
    };
    assert_eq!(
        extent(bar(BarKind::Memory64, u64::MAX - 4095), 4096),
        Ok(Span {
            base: u64::MAX - 4095,
            last: u64::MAX
        })
    );
    for (kind, base, size) in [
        (BarKind::Io, 0x1000, 2),
        (BarKind::Io, 1 << 32, 4),
        (BarKind::Memory32, 1 << 32, 16),
        (BarKind::Memory64, 0, 16),
        (BarKind::MemoryBelow1M, 1 << 20, 16),
        (BarKind::Memory32, 0x4000, 0),
        (BarKind::Memory64, 0x1000, 48),
        (BarKind::Memory64, 0x1000, 8192),
    ] {
        assert_eq!(extent(bar(kind, base), size), Err(Error::Bar));
    }
}

fn virt_lab() -> (Fabric, Vec<(Address, Function)>) {
    let mut fabric = Fabric::default();
    for (slot, id, class, pin) in [
        (0, 0x0008_1b36, 0x06000000, 0),
        (1, 0x1004_1af4, 0x01000000, 1),
        (6, 0x11e8_1234, 0x00ff0000, 1),
    ] {
        let row = fabric.endpoint(a(5, slot, 0));
        row[0] = id;
        row[2] = class;
        row[15] = pin << 8;
        if slot == 6 {
            row[4] = 0x1000_0000;
            row[1] = (1 << 20) | 4; // firmware propriétaire ; aucune écriture de BAR
            row[13] = 0x40;
            row[16] = 5;
        }
        if slot == 1 {
            row[5] = 0x1010_0000;
            row[8] = 0x0000_000c;
            row[9] = 0x80;
        }
    }
    let entries = fabric
        .run()
        .into_iter()
        .map(|(a, f)| {
            (
                Address {
                    segment: 0,
                    bus: 0,
                    ..a
                },
                f.unwrap(),
            )
        })
        .collect();
    (fabric, entries)
}

#[test]
fn profil_lecture_seule_taille_connue_et_bus_master_herite() {
    use resources::lab::{LabPlatform, qualify_edu};
    let (mut fabric, functions) = virt_lab();
    let before = fabric.registers.clone();
    let result = qualify_edu(LabPlatform::Virt, &functions).unwrap();
    assert_eq!(result.size, 1 << 20);
    assert_eq!(result.base, 0x1000_0000);
    assert_eq!(functions[2].1.command & 4, 4);
    assert_eq!(fabric.registers, before);
    let r = fabric.run();
    assert_eq!(r[1].1.unwrap().bars[0], None);
    // Aucun masque écrit durant le diagnostic d'une fonction déjà utilisée.
    assert_eq!(fabric.registers, before);
}

#[test]
fn profil_relu_refuse_recouvrements_rom_msi_et_objet_etranger() {
    use resources::lab::{LabPlatform, qualify_edu};
    let (_, mut functions) = virt_lab();
    let (_, f) = functions[1];
    let neighbor = f.bars[1].unwrap();
    functions[1].1.bars[1] = Some(Bar {
        base: 0x100f_f000,
        ..neighbor
    });
    assert_eq!(qualify_edu(LabPlatform::Virt, &functions), Err(Error::Bar));
    functions[1].1 = f;
    functions[2].1.msi_armed = true;
    assert_eq!(qualify_edu(LabPlatform::Virt, &functions), Err(Error::Bar));
    functions[2].1.msi_armed = false;
    functions[1].1.rom_enabled = true;
    assert_eq!(
        qualify_edu(LabPlatform::Virt, &functions),
        Err(Error::Identity)
    );
    functions[1].1.rom_enabled = false;
    functions[2].0.function = 1;
    assert_eq!(
        qualify_edu(LabPlatform::Virt, &functions),
        Err(Error::Identity)
    );
    functions[2].0.function = 0;
    functions.push(functions[2]);
    assert_eq!(
        qualify_edu(LabPlatform::Virt, &functions),
        Err(Error::Identity)
    );
}

#[test]
fn profil_refuse_taille_inconnue_et_bar_hors_aperture() {
    use resources::lab::{LabPlatform, qualify_edu};
    let (_, mut functions) = virt_lab();
    let mut bar = functions[2].1.bars[0].unwrap();
    functions[1].1.bars[2] = Some(bar);
    assert_eq!(qualify_edu(LabPlatform::Virt, &functions), Err(Error::Bar));
    functions[1].1.bars[2] = None;
    bar.base = 0x3000_0000;
    functions[2].1.bars[0] = Some(bar);
    assert_eq!(
        qualify_edu(LabPlatform::Virt, &functions),
        Err(Error::Range)
    );
}

#[test]
fn profil_nvme_q35_ferme_et_bar_64_du_firmware() {
    use resources::lab::{LabPlatform, qualify_edu, qualify_nvme};
    let mut fabric = Fabric::default();
    for (dev, fun, id, pin) in [
        (0, 0, 0x29c0_8086, 0),
        (6, 0, 0x0010_1b36, 1),
        (31, 0, 0x2918_8086, 0),
        (31, 2, 0x2922_8086, 1),
        (31, 3, 0x2930_8086, 1),
    ] {
        let row = fabric.endpoint(a(5, dev, fun));
        row[0] = id;
        row[15] = pin << 8;
        if dev == 31 && fun == 0 {
            row[3] |= 0x80 << 16;
        }
        if dev == 6 {
            row[1] = (1 << 20) | 7;
            row[2] = 0x01080202;
            row[4] = 4;
            row[5] = 0xc0;
            row[13] = 0x40;
            row[16] = 0x18011;
            row[17] = 0x2000;
            row[18] = 0x3000;
            row[32] = 0x926010;
            row[24] = 0x30001;
        }
    }
    let mut entries: Vec<_> = fabric
        .run()
        .into_iter()
        .map(|(a, f)| {
            (
                Address {
                    segment: 0,
                    bus: 0,
                    ..a
                },
                f.unwrap(),
            )
        })
        .collect();
    assert_eq!(qualify_nvme(&entries), Ok((0xc0_0000_0000, 7)));
    assert_eq!(
        qualify_edu(LabPlatform::Q35, &entries),
        Err(Error::Identity)
    );
    let saved = entries[1];
    entries[1].1.interface = 0;
    assert_eq!(qualify_nvme(&entries), Err(Error::Bar));
    entries[1] = saved;
    entries[1].1.command &= !2;
    assert_eq!(qualify_nvme(&entries), Err(Error::Bar));
    entries[1] = saved;
    entries[1].1.bars[0].as_mut().unwrap().base += 16384;
    assert_eq!(qualify_nvme(&entries), Err(Error::Range));
    entries[1] = saved;
    entries[1].1.device += 1;
    assert_eq!(qualify_nvme(&entries), Err(Error::Identity));
    entries[1] = saved;
    entries.pop();
    assert_eq!(qualify_nvme(&entries), Err(Error::Identity));
}
