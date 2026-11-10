//! C01–C14. Les champs de la fixture sont modifiés à leurs offsets sur disque.
//! L'oracle fixe les contenus des générations avant d'appeler le vérificateur.
use novafs::device::{Persistence, SimDevice, SimFault};
use novafs::{
    BLOCK_BYTES as N, Block, Device, Engine, Error, Ids, Mode, PairStatus, Result, check_image,
};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::io::Write;
use std::ops::Range;
use std::process::Command;

const BLOCKS: usize = 256;
const IDS: Ids = Ids {
    fs: [0x27; 16],
    lv: [0xe1; 16],
};

fn fresh(sector: u32) -> SimDevice {
    SimDevice::new(BLOCKS as u64, sector).unwrap()
}

fn fixture(sector: u32) -> Vec<u8> {
    let dev = fresh(sector);
    let control = dev.control();
    let mut fs = Engine::format(dev, IDS).unwrap();
    fs.create("/x").unwrap();
    fs.write("/x", 0, &[0x17; 2 * N]).unwrap(); // slot 1, génération 3
    fs.write("/x", (3 * N) as u64, &[0x74; N]).unwrap(); // slot 0, génération 4
    control.snapshot().bytes().unwrap()
}

fn u16_at(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap()).into()
}
fn word(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn put(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn sign(image: &mut [u8], addr: usize) -> [u8; 32] {
    let page = &mut image[addr * N..(addr + 1) * N];
    page[64..96].fill(0);
    let mut hash = Sha256::new();
    hash.update(b"NovaFS.P0.meta\0");
    hash.update(&*page);
    let digest: [u8; 32] = hash.finalize().into();
    page[64..96].copy_from_slice(&digest);
    digest
}
fn manifest(image: &[u8], slot: usize) -> usize {
    word(image, slot * N + 136) as usize
}
fn relink_head(image: &mut [u8], slot: usize) {
    let digest = sign(image, manifest(image, slot));
    image[slot * N + 152..slot * N + 184].copy_from_slice(&digest);
    sign(image, slot);
}
/// Réécrire une feuille de la seule vue ciblée, avec des sommes valides jusque
/// dans son ancre. Les petites fixtures ont des racines de niveau zéro.
fn edit_leaf(image: &mut [u8], slot: usize, family: usize, edit: impl FnOnce(&mut [u8])) {
    let root_ref = manifest(image, slot) * N + 160 + 48 * family;
    let addr = word(image, root_ref) as usize;
    assert_eq!(image[addr * N + 98], 0);
    assert_eq!(u16_at(image, addr * N + 96), family + 1);
    edit(&mut image[addr * N..(addr + 1) * N]);
    let digest = sign(image, addr);
    image[root_ref + 16..root_ref + 48].copy_from_slice(&digest);
    relink_head(image, slot);
}
fn cell(node: &[u8], ordinal: usize) -> (Range<usize>, Range<usize>) {
    assert!(ordinal < u16_at(node, 100));
    let p = u16_at(node, 104 + 2 * ordinal);
    let key = p + 4..p + 4 + u16_at(node, p);
    let val = key.end..key.end + u16_at(node, p + 2);
    (key, val)
}
fn new_data(image: &[u8]) -> usize {
    let mut pages = image
        .chunks_exact(N)
        .enumerate()
        .filter(|(_, p)| p.iter().all(|v| *v == 0x74));
    let result = pages.next().unwrap().0;
    assert!(pages.next().is_none());
    result
}

#[derive(Clone, Copy)]
enum Pair {
    Head(usize),
    Absent,
    Divergent,
}

fn cli_image(bytes: &[u8], sector: u32, code: i32) -> serde_json::Value {
    let mut image = tempfile::NamedTempFile::new().unwrap();
    image.write_all(bytes).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_novafs"))
        .args([
            "check",
            &BLOCKS.to_string(),
            &sector.to_string(),
            &"27".repeat(16),
            &"e1".repeat(16),
        ])
        .stdin(std::fs::File::open(image.path()).unwrap())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(code), "{out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"], "novafs-check-1");
    assert_eq!(
        json["statut"],
        if code == 0 { "coherent" } else { "incoherent" }
    );
    assert_eq!(std::fs::read(image.path()).unwrap(), bytes);
    json
}

struct Observed<'a> {
    disk: &'a SimDevice,
    reads: Cell<usize>,
}

impl Device for Observed<'_> {
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }

    fn read(&self, block: u64) -> Result<Block> {
        let n = self.reads.get() + 1;
        // Une récursion sur la fixture cyclique échouerait avec un diagnostic
        // du test au lieu de bloquer la campagne. Cette borne est propre à la fixture.
        assert!(n <= 4096, "parcours non borné de la petite fixture");
        self.reads.set(n);
        self.disk.read(block)
    }

    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        panic!("écriture du vérificateur")
    }
    fn flush(&mut self) -> Result<()> {
        panic!("flush du vérificateur")
    }
}

/// Vérifie séparément l'éligibilité des deux slots, la décision de paire, les
/// contenus récupérables, zéro write/flush, puis l'égalité exacte du support.
fn assess(
    image: &[u8],
    sector: u32,
    generations: [Option<u64>; 2],
    pair: Pair,
    fault: Option<SimFault>,
    catalog: bool,
) -> [String; 2] {
    let dev = SimDevice::from_bytes(image, sector).unwrap();
    let control = dev.control();
    control.trace(None, None);
    if let Some(fault) = fault {
        control.inject(fault).unwrap();
    }
    let observed = Observed {
        disk: &dev,
        reads: Cell::new(0),
    };
    let report = check_image(&observed, IDS);
    match (pair, &report.pair) {
        (Pair::Head(wanted), PairStatus::Verified { slot }) => assert_eq!(wanted, *slot),
        (Pair::Absent, PairStatus::Unavailable) | (Pair::Divergent, PairStatus::Rejected(_)) => {}
        _ => panic!("décision de paire incorrecte"),
    }
    let mut messages = [String::new(), String::new()];
    for (i, expected) in generations.into_iter().enumerate() {
        let slot = &report.slots[i];
        match expected {
            None => {
                let issue = slot.issue.as_ref().expect("corruption passée sous silence");
                messages[i] = format!("{} {}", issue.stage, issue.error);
                if let Some(SimFault::Read { block }) = fault {
                    assert!(matches!(issue.error, Error::Io(_)));
                    assert_eq!(issue.block, Some(block));
                }
            }
            Some(g) => {
                assert!(slot.issue.is_none(), "slot {i} signalé à tort");
                assert_eq!(slot.anchor.as_ref().unwrap().manifest.birth, g);
                let ro = Engine::salvage(control.snapshot().device(), IDS, i as u64).unwrap();
                ro.verify().unwrap();
                if catalog {
                    assert_eq!(ro.list("/", None, 10).unwrap(), ["a"]);
                    assert!(ro.list("/a", None, 10).unwrap().is_empty());
                } else {
                    let mut bytes = vec![0x17; 2 * N];
                    if g == 4 {
                        bytes.resize(3 * N, 0);
                        bytes.extend_from_slice(&[0x74; N]);
                    }
                    assert_eq!(ro.list("/", None, 10).unwrap(), ["x"]);
                    assert_eq!(ro.read("/x", 0, 65536).unwrap(), bytes);
                }
            }
        }
    }
    assert!(
        control.events().is_empty(),
        "écriture/flush pendant la vérification"
    );
    assert_eq!(control.snapshot().bytes().unwrap().as_slice(), image);
    // Les deux exécutables voient réellement une image ordinaire ouverte RO.
    // L'EIO ponctuelle est une commande du simulateur et ne fait pas partie de l'image.
    if fault.is_none() {
        let expected = match pair {
            Pair::Head(_) => "verifiee",
            Pair::Absent => "indisponible",
            Pair::Divergent => "rejetee",
        };
        let json = cli_image(image, sector, i32::from(!matches!(pair, Pair::Head(_))));
        assert_eq!(json["paire"]["etat"], expected);
        for (slot, g) in generations.iter().enumerate() {
            assert_eq!(json["slots"][slot]["vue_verifiee"], g.is_some());
            if let Some(g) = g {
                assert_eq!(json["slots"][slot]["ancre"]["generation"], *g);
            } else {
                assert!(json["slots"][slot]["anomalie"]["raison"].is_string());
            }
        }
    }
    messages
}

#[test]
fn c01_a_c05_ancres_generation_et_references() {
    for sector in [512, 4096] {
        let base = fixture(sector);
        assess(
            &base,
            sector,
            [Some(4), Some(3)],
            Pair::Head(0),
            None,
            false,
        );
        for bad in 0..2 {
            let mut img = base.clone();
            img[bad * N] ^= 1;
            let mut good = [Some(4), Some(3)];
            good[bad] = None;
            assert!(
                assess(&img, sector, good, Pair::Absent, None, false)[bad].contains("signature")
            );
        }
        let mut img = base.clone();
        img[0] ^= 1;
        img[N + 64] ^= 1;
        let msg = assess(&img, sector, [None, None], Pair::Absent, None, false);
        assert!(msg[0].contains("signature") && msg[1].contains("checksum"));

        let mut img = base.clone();
        img[8] = 1; // version inconnue avec enveloppe intègre
        sign(&mut img, 0);
        assert!(
            assess(&img, sector, [None, Some(3)], Pair::Absent, None, false)[0].contains("version")
        );

        let mut img = base.clone();
        let m = N * manifest(&img, 0);
        img[m + 64] ^= 1;
        assert!(
            assess(&img, sector, [None, Some(3)], Pair::Absent, None, false)[0]
                .contains("checksum")
        );

        for (offset, value, reason) in [(96, 1, "filiation"), (160, BLOCKS as u64 + 5, "référence")]
        {
            let mut img = base.clone();
            put(&mut img, m + offset, value);
            relink_head(&mut img, 0);
            assert!(
                assess(&img, sector, [None, Some(3)], Pair::Absent, None, false)[0]
                    .contains(reason)
            );
        }
        // Chaque vue est complète mais les deux manifestes ne sont plus parents.
        let mut img = base.clone();
        img[m + 104] ^= 0x80;
        relink_head(&mut img, 0);
        assess(
            &img,
            sector,
            [Some(4), Some(3)],
            Pair::Divergent,
            None,
            false,
        );
    }
}

#[test]
fn c06_cycle_du_catalogue_aux_sommes_valides() {
    for sector in [512, 4096] {
        let dev = fresh(sector);
        let control = dev.control();
        let mut fs = Engine::format(dev, IDS).unwrap();
        fs.mkdir("/a").unwrap();
        fs.mkdir("/a/b").unwrap();
        let mut img = control.snapshot().bytes().unwrap();
        // Deux parents mutuels, avec deux entrées inverses valides et triées.
        edit_leaf(&mut img, 1, 0, |node| {
            let (key, v) = cell(node, 1);
            assert_eq!(word(node, key.start), 2);
            put(node, v.start + 18, 3);
        });
        edit_leaf(&mut img, 1, 1, |node| {
            for (i, parent, name, child) in [(0, 2, b'b', 3), (1, 3, b'a', 2)] {
                let (k, v) = cell(node, i);
                put(node, k.start, parent);
                node[k.start + 10] = name;
                put(node, v.start, child);
            }
        });
        let msg = assess(&img, sector, [Some(2), None], Pair::Absent, None, true);
        assert!(msg[1].contains("cycle"), "{}", msg[1]);
    }
}

#[test]
fn c07_a_c11_extents_comptes_volume_donnees_et_eio() {
    for sector in [512, 4096] {
        let base = fixture(sector);
        for bad in 0..2 {
            let mut good = [Some(4), Some(3)];
            good[bad] = None;
            assess(
                &base,
                sector,
                good,
                Pair::Absent,
                Some(SimFault::Read { block: bad as u64 }),
                false,
            );
        }
        for (case, reason) in [
            (7, "recouvrement"),
            (8, "partition"),
            (9, "identité"),
            (10, "données"),
            (11, "E/S"),
        ] {
            let mut img = base.clone();
            let mut fault = None;
            match case {
                7 => edit_leaf(&mut img, 0, 2, |page| {
                    let (key, _) = cell(page, 1);
                    assert_eq!(word(page, key.start + 8), 3);
                    put(page, key.start + 8, 1); // le premier extent couvre [0,2).
                }),
                8 => edit_leaf(&mut img, 0, 3, |page| {
                    let (_, v) = cell(page, u16_at(page, 100) - 1);
                    let n = word(page, v.start);
                    assert!(n > 1);
                    put(page, v.start, n - 1); // un bloc ni libre ni possédé.
                }),
                9 => {
                    img[32] ^= 1;
                    sign(&mut img, 0);
                }
                10 => {
                    let p = new_data(&img);
                    img[p * N + 127] ^= 4;
                }
                11 => {
                    fault = Some(SimFault::Read {
                        block: new_data(&img) as u64,
                    })
                }
                _ => unreachable!(),
            }
            let msg = assess(&img, sector, [None, Some(3)], Pair::Absent, fault, false);
            assert!(msg[0].contains(reason), "C{case}: {}", msg[0]);
        }
        let mut img = base.clone();
        edit_leaf(&mut img, 0, 2, |page| {
            let (_, first) = cell(page, 0);
            let (_, last) = cell(page, 1);
            let physical = word(page, first.start);
            put(page, last.start, physical);
        });
        assert!(
            assess(&img, sector, [None, Some(3)], Pair::Absent, None, false)[0].contains("possédé")
        );
    }
}

#[test]
fn c12_cli_refuse_image_tronquee_sans_modifier_les_octets() {
    for sector in [512, 4096] {
        let base = fixture(sector);
        for missing in [1, 512, N] {
            let truncated = &base[..base.len() - missing];
            let json = cli_image(truncated, sector, 1);
            assert!(json["raison"].as_str().unwrap().contains("longueur"));
        }
    }
}

#[test]
fn c13_bits_aleatoires_et_bloc_libre_temoin() {
    for sector in [512, 4096] {
        let base = fixture(sector);
        let page = new_data(&base);
        let mut seed = 0x4e56_5032_c01d_2026u64;
        for case in 0..64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut img = base.clone();
            let block = [0, 1, page, BLOCKS - 1][case % 4];
            let offset = (seed as usize >> 8) % N;
            img[block * N + offset] ^= 1 << (seed % 8);
            let good = match case % 4 {
                0 | 2 => [None, Some(3)],
                1 => [Some(4), None],
                _ => [Some(4), Some(3)], // dernier bloc de cette petite fixture inoccupé.
            };
            let pair = if case % 4 == 3 {
                Pair::Head(0)
            } else {
                Pair::Absent
            };
            assess(&img, sector, good, pair, None, false);
        }
    }
}

#[test]
fn c14_coupure_puis_corruption_de_la_derniere_ancre_confirmee() {
    for sector in [512, 4096] {
        let dev = SimDevice::from_bytes(&fixture(sector), sector).unwrap();
        let control = dev.control();
        let mut fs = Engine::mount(dev, IDS, Mode::ReadWrite).unwrap();
        control
            .inject(SimFault::Flush {
                ordinal: 2,
                persist_newest: 0,
            })
            .unwrap();
        assert!(matches!(
            fs.write("/x", 0, b"tentative"),
            Err(Error::Publication { phase: "F2", .. })
        ));
        drop(fs);
        control.power_cut(Persistence::None).unwrap();
        let mut img = control.snapshot().bytes().unwrap();
        assess(&img, sector, [Some(4), Some(3)], Pair::Head(0), None, false);
        img[64] ^= 1;
        assess(&img, sector, [None, Some(3)], Pair::Absent, None, false);
    }
}

#[test]
fn temoins_deux_historiques_integres_a_generation_egale() {
    for sector in [512, 4096] {
        let base = fixture(sector);
        let mut receipts = Vec::new();
        for value in [b'A', b'B'] {
            let dev = SimDevice::from_bytes(&base, sector).unwrap();
            let ctrl = dev.control();
            let mut fs = Engine::mount(dev, IDS, Mode::ReadWrite).unwrap();
            fs.write("/x", 0, &[value]).unwrap();
            let receipt = fs.receipt();
            assert_eq!(receipt.birth, 5);
            let dev = fs.unmount().unwrap();
            let before = ctrl.snapshot().bytes().unwrap();
            let checked = check_image(&dev, IDS);
            assert!(matches!(checked.pair, PairStatus::Verified { slot: 1 }));
            assert_eq!(checked.slots[1].anchor.as_ref().unwrap().manifest, receipt);
            assert_eq!(
                Engine::mount(dev, IDS, Mode::ReadOnly)
                    .unwrap()
                    .read("/x", 0, 1)
                    .unwrap(),
                [value]
            );
            assert_eq!(ctrl.snapshot().bytes().unwrap(), before);
            receipts.push(receipt.hash);
        }
        assert_ne!(receipts[0], receipts[1]);
    }
}
