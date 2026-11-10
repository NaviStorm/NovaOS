//! P3. Les octets applicatifs et les propriétaires physiques sont prédits
//! indépendamment du compteur de l'allocateur et du contenu de son arbre libre.
use novafs::device::{Event, Persistence, SimControl, SimDevice, SimFault, SimSnapshot};
use novafs::{Block, Device, Engine, Error, Ids, Mode, PairStatus, Result, VolumePolicy};
use std::collections::BTreeSet;

const N: usize = 4096;
const BLOCKS: u64 = 224;
const IDS: Ids = Ids {
    fs: [0xed; 16],
    lv: [0xbd; 16],
};
const MARKER: &[u8] = b"recu publie avant saturation";

fn le16(b: &[u8], offset: usize) -> usize {
    u16::from_le_bytes(b[offset..offset + 2].try_into().unwrap()) as usize
}
fn le64(b: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap())
}

struct Generation {
    number: u64,
    owners: BTreeSet<u64>,
    logical: u128,
}

// Parcours des seules images construites par le banc, avec les offsets du
// format P0. L'arbre d'espace n'est visité que pour ses propres blocs CoW.
// Ses valeurs de libre ou de différé n'alimentent jamais l'oracle.
fn generation(d: &SimDevice, slot: u64) -> Result<Generation> {
    let anchor = d.read(slot)?;
    let manifest = le64(&anchor, 136);
    let m = d.read(manifest)?;
    let mut owners = BTreeSet::from([0, 1, manifest]);
    let mut logical = 0;
    let mut work = Vec::new();
    for family in 1..=5 {
        let root = le64(&m, 160 + 48 * (family - 1));
        if root != 0 {
            work.push((root, family));
        }
    }
    while let Some((block, family)) = work.pop() {
        assert!(
            (2..d.blocks()).contains(&block) && owners.insert(block),
            "double propriétaire du bloc {block}"
        );
        let node = d.read(block)?;
        assert_eq!(le16(&node, 96), family);
        for i in 0..le16(&node, 100) {
            let cell = le16(&node, 104 + 2 * i);
            let value = cell + 4 + le16(&node, cell);
            if node[98] != 0 {
                work.push((le64(&node, value), family));
            } else if family == 1 && node[value] == 1 && node[value + 17] == 0 {
                logical += u128::from(le64(&node, value + 9));
            } else if family == 3 {
                let first = le64(&node, value);
                for b in first..first + le16(&node, value + 8) as u64 {
                    assert!(
                        (2..d.blocks()).contains(&b) && owners.insert(b),
                        "double attribution du bloc de données {b}"
                    );
                }
            }
        }
    }
    Ok(Generation {
        number: le64(&anchor, 56),
        owners,
        logical,
    })
}

fn partition(fs: &Engine<SimDevice>, ctl: &SimControl, reserve: u64) -> Result<BTreeSet<u64>> {
    let d = ctl.snapshot().device();
    let mut old = generation(&d, 0)?;
    let mut head = generation(&d, 1)?;
    if old.number > head.number {
        std::mem::swap(&mut old, &mut head);
    }
    assert_eq!(fs.generation(), head.number);
    let charged: BTreeSet<_> = old.owners.union(&head.owners).copied().collect();
    let count = fs.accounting()?;
    assert_eq!(count.capacity_blocks, d.blocks());
    assert_eq!(count.live_blocks, head.owners.len() as u64);
    assert_eq!(count.logical_bytes, head.logical);
    assert_eq!(
        count.recoverable_blocks,
        old.owners.difference(&head.owners).count() as u64
    );
    assert_eq!(count.allocated_blocks, charged.len() as u64);
    let free = d.blocks() - charged.len() as u64;
    assert_eq!(count.maintenance_reserved, free.min(reserve));
    assert_eq!(count.available_blocks, free.saturating_sub(reserve));
    assert_eq!(count.transaction_reserved, 0);
    Ok(charged)
}

fn policy(reserve: u64, quota: Option<u64>) -> VolumePolicy {
    VolumePolicy {
        maintenance_blocks: reserve,
        logical_quota_bytes: quota,
    }
}

fn pattern(page: usize) -> Block {
    [(page % 251 + 1) as u8; N]
}

fn check_data(fs: &Engine<SimDevice>, pages: usize) -> Result<()> {
    fs.verify()?;
    assert_eq!(fs.read("/garde", 0, N)?, MARKER);
    assert_eq!(fs.stat("/plein")?.size, (pages * N) as u64);
    for page in 0..pages {
        assert_eq!(fs.read("/plein", (page * N) as u64, N)?, pattern(page));
    }
    Ok(())
}

fn full(sector: u32) -> Result<(Engine<SimDevice>, SimControl, usize)> {
    let dev = SimDevice::new(BLOCKS, sector)?;
    let ctl = dev.control();
    let mut fs = Engine::format(dev, IDS)?;
    fs.create("/garde")?;
    fs.write("/garde", 0, MARKER)?;
    fs.create("/plein")?;
    for page in 0..BLOCKS as usize {
        let expected = fs.receipt();
        let original = ctl.snapshot();
        ctl.trace(None, None);
        match fs.write("/plein", (page * N) as u64, &pattern(page)) {
            Ok(()) => {}
            Err(Error::NoSpace) => {
                assert!(page >= 16);
                assert_eq!(fs.receipt(), expected);
                assert!(ctl.events().is_empty());
                assert_eq!(ctl.snapshot().bytes()?, original.bytes()?);
                check_data(&fs, page)?;
                partition(&fs, &ctl, 128)?;
                return Ok((fs, ctl, page));
            }
            Err(e) => return Err(e),
        }
    }
    panic!("écritures sans saturation dans un LV de {BLOCKS} blocs");
}

#[test]
fn p3_saturation_et_maintenance_preservent_les_deux_generations() -> Result<()> {
    for sector in [512, 4096] {
        let (fs, ctl, pages) = full(sector)?;
        let expected = fs.receipt();
        let mut fs = Engine::mount(fs.unmount()?, IDS, Mode::ReadWrite)?;
        assert_eq!(fs.receipt(), expected);
        check_data(&fs, pages)?;
        partition(&fs, &ctl, 128)?;
        let stats = fs.accounting()?;
        let reserve = stats.available_blocks + stats.maintenance_reserved;
        fs.set_volume_policy(policy(reserve, None))?;
        assert_eq!(fs.accounting()?.available_blocks, 0);
        ctl.trace(None, None);
        assert!(matches!(
            fs.write("/plein", 0, &pattern(160)),
            Err(Error::NoSpace)
        ));
        assert!(ctl.events().is_empty());
        check_data(&fs, pages)?;
        fs.unlink("/plein")?;
        assert_eq!(fs.accounting()?.logical_bytes, MARKER.len() as u128);
        partition(&fs, &ctl, reserve)?;
        let mut collected = 0;
        while fs.collect()? {
            collected += 1;
            assert!(collected <= pages);
            partition(&fs, &ctl, reserve)?;
        }
        fs.create("/reprise")?;
        fs.write("/reprise", 0, &[0xce; N * 2])?;
        let fs =
            Engine::mount_with_policy(fs.unmount()?, IDS, Mode::ReadOnly, policy(reserve, None))?;
        partition(&fs, &ctl, reserve)?;
        assert_eq!(fs.read("/garde", 0, N)?, MARKER);
        assert_eq!(fs.read("/reprise", 0, N * 2)?, [0xce; N * 2]);
        println!(
            "P3 saturation secteur={sector} pages={pages} reserve={reserve} collectes={collected}"
        );
    }
    Ok(())
}

#[test]
fn p3_reservation_exacte_annulation_et_arret_avant_publication() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(1024, sector)?;
        let ctl = dev.control();
        let mut fs = Engine::format(dev, IDS)?;
        fs.create("/f")?;
        fs.write("/f", 0, b"base")?;
        for len in [1, N + 1, 16 * N] {
            let delta = vec![len as u8; len];
            let before = ctl.snapshot().bytes()?;
            let receipt = fs.receipt();
            let charged = partition(&fs, &ctl, 128)?;
            let base = fs.accounting()?;
            for explicit in [false, true] {
                ctl.trace(None, None);
                let tx = fs.prepare_write("/f", 1, &delta)?;
                let cost = tx.cost();
                let reserved = cost.data_blocks + cost.metadata_blocks;
                assert_eq!(cost.data_blocks, (len + 1).div_ceil(N) as u64);
                assert!(cost.metadata_blocks >= 2);
                let stats = tx.accounting();
                assert_eq!(stats.transaction_reserved, reserved);
                assert_eq!(stats.available_blocks + reserved, base.available_blocks);
                assert_eq!(stats.allocated_blocks, base.allocated_blocks);
                assert!(ctl.events().is_empty());
                if explicit {
                    tx.cancel();
                } else {
                    drop(tx);
                }
                assert!(ctl.events().is_empty());
                assert_eq!(ctl.snapshot().bytes()?, before);
                assert_eq!(fs.receipt(), receipt);
                partition(&fs, &ctl, 128)?;
            }
            // Le client détient encore le plan. Aucun de ses blocs ne peut être
            // présent sur le support stable, même si la RAM du service disparaît.
            let tx = fs.prepare_write("/f", 1, &delta)?;
            ctl.power_cut(Persistence::Newest(usize::MAX))?;
            let cold = Engine::mount(ctl.snapshot().device(), IDS, Mode::ReadOnly)?;
            assert_eq!(cold.receipt(), receipt);
            assert_eq!(cold.accounting()?.transaction_reserved, 0);
            drop(tx);
            assert_eq!(ctl.snapshot().bytes()?, before);
            ctl.trace(None, None);
            let tx = fs.prepare_write("/f", 1, &delta)?;
            let cost = tx.cost();
            let (data, meta) = (cost.data_blocks, cost.metadata_blocks);
            tx.commit()?;
            let events = ctl.events();
            let mut issued = BTreeSet::new();
            for event in events
                .iter()
                .take_while(|e| !matches!(e, Event::FlushSubmitted(_)))
            {
                if let Event::BeforeWrite(b) = event {
                    assert!(
                        !charged.contains(b) && issued.insert(*b),
                        "réemploi prématuré de {b}"
                    );
                }
            }
            assert_eq!(issued.len() as u64, data + meta);
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, Event::FlushCompleted(_)))
                    .count(),
                2
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, Event::BeforeWrite(_)))
                    .count(),
                issued.len() + 1
            );
            assert_eq!(fs.read("/f", 1, len)?, delta);
            partition(&fs, &ctl, 128)?;
            println!(
                "P3 reservation secteur={sector} octets={len} donnees={data} metadonnees={meta}"
            );
        }
    }
    Ok(())
}

#[test]
fn p3_quota_apres_abaissement_et_trou_logique() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(BLOCKS, sector)?;
        let ctl = dev.control();
        let mut fs = Engine::format(dev, IDS)?;
        fs.create("/creux")?;
        let offset = BLOCKS * N as u64 + 37;
        fs.write("/creux", offset, b"Z")?;
        assert_eq!(fs.accounting()?.logical_bytes, u128::from(offset + 1));
        let fs = fs.unmount()?;
        let mut fs =
            Engine::mount_with_policy(fs, IDS, Mode::ReadWrite, policy(128, Some(2 * N as u64)))?;
        assert_eq!(fs.read("/creux", offset - 1, 2)?, b"\0Z");
        let receipt = fs.receipt();
        let before = ctl.snapshot().bytes()?;
        ctl.trace(None, None);
        assert!(matches!(
            fs.write("/creux", offset + 1, b"X"),
            Err(Error::QuotaExceeded)
        ));
        assert!(ctl.events().is_empty());
        assert_eq!(fs.receipt(), receipt);
        assert_eq!(ctl.snapshot().bytes()?, before);
        fs.write("/creux", offset, b"Q")?;
        assert_eq!(fs.read("/creux", offset, 1)?, b"Q");
        fs.unlink("/creux")?;
        assert_eq!(fs.accounting()?.logical_bytes, 0);
        while fs.collect()? {}
        fs.create("/a")?;
        fs.create("/b")?;
        fs.write("/a", N as u64 - 1, b"a")?;
        fs.write("/b", N as u64 - 1, b"b")?;
        assert_eq!(fs.accounting()?.logical_bytes, 2 * N as u128);
        ctl.trace(None, None);
        assert!(matches!(
            fs.write("/b", N as u64, b"c"),
            Err(Error::QuotaExceeded)
        ));
        assert!(ctl.events().is_empty());
        partition(&fs, &ctl, 128)?;
        for invalid in [0, 63, BLOCKS - 4, 4097, u64::MAX] {
            assert!(matches!(
                fs.set_volume_policy(policy(invalid, None)),
                Err(Error::Invalid(_))
            ));
        }
    }
    Ok(())
}

#[test]
fn p3_reservation_suit_les_scissions_a_taille_de_donnee_constante() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(512, sector)?;
        let ctl = disk.control();
        let mut fs = Engine::format(disk, IDS)?;
        fs.create("/clairseme")?;
        let mut costs = BTreeSet::new();
        for i in 0..80 {
            ctl.trace(None, None);
            let off = 2 * N * i;
            let tx = fs.prepare_write("/clairseme", off as u64, &[0x41])?;
            let cost = tx.cost();
            let (data, meta) = (cost.data_blocks, cost.metadata_blocks);
            assert_eq!(data, 1);
            assert_eq!(tx.accounting().transaction_reserved, data + meta);
            assert!(ctl.events().is_empty());
            tx.commit()?;
            let writes = ctl
                .events()
                .iter()
                .filter(|e| matches!(e, Event::BeforeWrite(_)))
                .count();
            assert_eq!(writes as u64, data + meta + 1);
            costs.insert(meta);
            partition(&fs, &ctl, 128)?;
        }
        assert!(
            costs.len() >= 3,
            "le coût doit distinguer feuille, montée et branche"
        );
        fs.verify()?;
        for i in 0..80 {
            let off = 2 * N * i;
            assert_eq!(fs.read("/clairseme", off as u64, 1)?, [0x41]);
            if i < 79 {
                assert_eq!(fs.read("/clairseme", (off + N) as u64, 1)?, [0]);
            }
        }
        println!("P3 scissions secteur={sector} ecritures=80 couts_metadonnees={costs:?}");
    }
    Ok(())
}

fn recovered(snapshot: &SimSnapshot, pages: usize, before: u64, published: bool) -> Result<()> {
    let dev = snapshot.device();
    let ctl = dev.control();
    let fs = Engine::mount(dev, IDS, Mode::ReadOnly)?;
    assert_eq!(fs.generation(), before + u64::from(published));
    fs.verify()?;
    partition(&fs, &ctl, 128)?;
    if published {
        assert_eq!(fs.list("/", None, 8)?, ["garde"]);
        assert_eq!(fs.read("/garde", 0, N)?, MARKER);
        // L'autre ancre doit encore restituer les pages de l'objet retiré.
        let old = Engine::salvage(snapshot.device(), IDS, before % 2)?;
        check_data(&old, pages)?;
    } else {
        check_data(&fs, pages)?;
    }
    Ok(())
}

#[test]
fn p3_coupures_et_eio_de_maintenance_sur_volume_sature() -> Result<()> {
    for sector in [512, 4096] {
        let (mut fs, ctl, pages) = full(sector)?;
        let base = ctl.snapshot();
        let g = fs.generation();
        ctl.trace(None, None);
        fs.unlink("/plein")?;
        let trace = ctl.events();
        let slot = (g + 1) % 2;
        let accepted = trace
            .iter()
            .position(|e| *e == Event::WriteSubmitted(slot))
            .unwrap();
        let stable = trace
            .iter()
            .position(|e| *e == Event::FlushEffect(2))
            .unwrap();
        let mut cuts = 0;
        for i in 0..trace.len() {
            for retain in [0, 1, usize::MAX] {
                let dev = base.device();
                let c = dev.control();
                let mut writer = Engine::mount(dev, IDS, Mode::ReadWrite)?;
                c.trace(Some(i), None);
                let error = writer.unlink("/plein").unwrap_err();
                assert!(matches!(error, Error::Io(_) | Error::Publication { .. }));
                assert!(
                    writer.accounting().is_err(),
                    "un écrivain gelé ne publie pas de compteurs fiables"
                );
                drop(writer);
                c.power_cut(Persistence::Newest(retain))?;
                let published = i >= stable || (i >= accepted && retain != 0);
                recovered(&c.snapshot(), pages, g, published)?;
                cuts += 1;
            }
        }
        for ordinal in [1, 2] {
            for persisted in [0, 1, usize::MAX] {
                let dev = base.device();
                let c = dev.control();
                let mut writer = Engine::mount(dev, IDS, Mode::ReadWrite)?;
                c.inject(SimFault::Flush {
                    ordinal,
                    persist_newest: persisted,
                })?;
                let error = writer.unlink("/plein").unwrap_err();
                if ordinal == 1 {
                    assert!(matches!(error, Error::Io(_)));
                } else {
                    assert!(matches!(error, Error::Publication { phase: "F2", .. }));
                }
                drop(writer);
                c.power_cut(Persistence::None)?;
                recovered(&c.snapshot(), pages, g, ordinal == 2 && persisted > 0)?;
            }
        }
        println!(
            "P3 panne secteur={sector} frontieres={} coupures={cuts} eio_flush=6",
            trace.len()
        );
    }
    Ok(())
}

#[test]
fn p3_corruption_combinee_a_la_saturation() -> Result<()> {
    for sector in [512, 4096] {
        let (fs, ctl, pages) = full(sector)?;
        let generation = fs.generation();
        let slot = (generation % 2) as usize;
        let image = ctl.snapshot().device();
        let c = image.control();
        let mut anchor = image.read(slot as u64)?;
        anchor[64] ^= 0x80;
        c.replace_stable(slot as u64, anchor)?;
        let before = c.snapshot().bytes()?;
        c.trace(None, None);
        let check = novafs::check_image(&image, IDS);
        assert!(check.slots[slot].issue.is_some());
        assert!(check.slots[1 - slot].issue.is_none());
        assert!(matches!(check.pair, PairStatus::Unavailable));
        assert!(matches!(
            Engine::mount(image, IDS, Mode::ReadWrite),
            Err(Error::Corrupt(_))
        ));
        assert!(c.events().is_empty());
        let survivor = Engine::salvage(c.snapshot().device(), IDS, (1 - slot) as u64)?;
        assert_eq!(survivor.generation(), generation - 1);
        check_data(&survivor, pages - 1)?;
        assert_eq!(c.snapshot().bytes()?, before);
    }
    Ok(())
}
