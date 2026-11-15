//! Utilise le module embarqué et le moteur P9 réel sur un périphérique de laboratoire.
extern crate alloc;
#[allow(dead_code)]
#[path = "../user/novafs/files/config_store.rs"]
mod config_store;
#[path = "../user/services/journal_wire.rs"]
mod schema;
#[path = "../user/novafs/files/journal_store.rs"]
mod store;
use novafs::{
    Device, Engine, Error, Ids, Kind, Mode, Result, VolumePolicy,
    device::{Event, FileImage, Persistence, SimDevice, SimFault},
    time::Atime,
};
use store::{Journal, wire};
const IDS: Ids = Ids {
    fs: [49; 16],
    lv: [75; 16],
};

fn event(n: u64) -> wire::Event {
    schema::bytes(&[
        n,
        n,
        1,
        2,
        3,
        4,
        5,
        6,
        schema::meta(
            9,
            schema::INFO,
            schema::SERVICE,
            schema::PRODUCER,
            schema::NOTICE,
            schema::MONOTONIC,
        ),
        n * 7,
        1,
        2,
    ])
}
fn add<D: Device>(fs: &mut Engine<D>, n: u64) -> Result<u64> {
    Journal::load(fs)?.append(fs, n, &event(n))
}
fn admin<D: Device>(fs: &mut Engine<D>, op: u64, n: u64) -> Result<u64> {
    let j = Journal::load(fs)?;
    let rev = j.status().revision;
    j.administer(fs, op, rev, n)
}
fn history<D: Device>(fs: &Engine<D>) -> Result<(wire::Status, Vec<wire::Event>)> {
    let j = Journal::load(fs)?;
    let status = j.status();
    Ok((
        status,
        (status.first..status.next)
            .map(|i| j.read(i))
            .collect::<Result<_>>()?,
    ))
}
fn normal(sector: u32) -> Result<()> {
    let mut fs = Engine::format_p9(SimDevice::new(1024, sector)?, IDS, Atime::No, None)?;
    assert!(store::hidden(store::NAME));
    assert!(store::hidden("\x1fNovaOS-journal-new"));
    assert!(!store::hidden("journal"));
    let mut elapsed = Vec::new();
    for i in 1..=129 {
        let start = std::time::Instant::now();
        assert_eq!(add(&mut fs, i)?, i);
        elapsed.push(start.elapsed().as_nanos());
        if i == 128 {
            let s = Journal::load(&fs)?.status();
            assert_eq!(
                (s.first, s.next, s.segments, s.bytes, s.quota),
                (1, 129, 8, 12288, 12288)
            );
        }
    }
    let mut fs = Engine::mount_current(fs.unmount()?, IDS, Mode::ReadWrite)?;
    let (s, records) = history(&fs)?;
    assert_eq!((s.first, s.next, s.segments), (17, 130, 8));
    assert_eq!(records, (17..130).map(event).collect::<Vec<_>>());
    assert!(matches!(Journal::load(&fs)?.read(1), Err(Error::NotFound)));
    let receipt = fs.receipt();
    let mut same = event(129);
    wire::put(&mut same, 1, 130);
    assert_eq!(Journal::load(&fs)?.append(&mut fs, 130, &same)?, 129);
    same[80] ^= 1;
    assert!(matches!(
        Journal::load(&fs)?.append(&mut fs, 130, &same),
        Err(Error::Busy)
    ));
    let mut invalid = event(130);
    invalid[64] = 0;
    assert!(matches!(
        Journal::load(&fs)?.append(&mut fs, 130, &invalid),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(add(&mut fs, 131), Err(Error::Busy)));
    assert!(matches!(
        Journal::load(&fs)?.administer(&mut fs, wire::ROTATE, s.revision - 1, 0),
        Err(Error::Busy)
    ));
    assert_eq!(receipt, fs.receipt());
    let view = Journal::load(&fs)?;
    let rotate = std::time::Instant::now();
    assert_eq!(admin(&mut fs, wire::ROTATE, 0)?, s.revision + 1);
    let rotate_ns = rotate.elapsed().as_nanos();
    assert_eq!(view.read(17)?, event(17));
    assert!(matches!(
        view.append(&mut fs, 130, &event(130)),
        Err(Error::Busy)
    ));
    assert!(matches!(Journal::load(&fs)?.read(17), Err(Error::NotFound)));
    let rev = admin(&mut fs, wire::RETAIN, 1)?;
    assert_eq!(admin(&mut fs, wire::ROTATE, 0)?, rev);
    let s = Journal::load(&fs)?.status();
    assert_eq!((s.first, s.next, s.segments, s.retention), (130, 130, 1, 1));
    add(&mut fs, 130)?;
    admin(&mut fs, wire::PURGE, 0)?;
    let s = Journal::load(&fs)?.status();
    assert_eq!((s.first, s.next, s.segments, s.bytes), (131, 131, 1, 0));
    assert!(matches!(add(&mut fs, 130), Err(Error::Busy)));
    add(&mut fs, 131)?;
    // Les noms imprimables ou échappés ne peuvent devenir un alias du fichier interne.
    let root = fs.root_id();
    let private = fs.lookup_at(root, store::NAME)?.id;
    let protected = history(&fs)?;
    for name in ["NovaOS-journal", "␟NovaOS-journal", r"\x1fNovaOS-journal"] {
        assert!(matches!(fs.lookup_at(root, name), Err(Error::NotFound)));
        let public = fs.create_at(root, name, Kind::File)?;
        assert_ne!(public.id, private);
        fs.prepare_write_id(public.id, 0, b"fichier utilisateur")?
            .commit()?;
        assert_eq!(history(&fs)?, protected);
    }
    let mut p8 = Engine::format(SimDevice::new(512, sector)?, IDS)?;
    let receipt = p8.receipt();
    assert!(matches!(add(&mut p8, 1), Err(Error::Invalid(_))));
    assert_eq!(receipt, p8.receipt());
    println!(
        "secteur={sector} contrôle P8/P9, rotation 8×16, quota, rétention, purge, CAS concurrent, vue antérieure, EID, réouverture et octets : OK"
    );
    let sum = elapsed.iter().sum::<u128>();
    println!(
        "SimDevice P9 hôte secteur={sector} ns_par_append={elapsed:?} débit_événements_s={} rotation_manuelle_pleine_ns={rotate_ns}",
        elapsed.len() as f64 * 1e9 / sum as f64
    );
    Ok(())
}
fn saturation(sector: u32) -> Result<()> {
    for initial in [true, false] {
        let mut fs = Engine::format_p9(SimDevice::new(512, sector)?, IDS, Atime::No, None)?;
        if !initial {
            add(&mut fs, 1)?;
        }
        let old = history(&fs)?;
        let a = fs.accounting()?;
        fs.set_volume_policy(VolumePolicy {
            maintenance_blocks: a.available_blocks + a.maintenance_reserved - 15,
            logical_quota_bytes: None,
        })?;
        assert!(fs.accounting()?.available_blocks <= 15);
        let receipt = fs.receipt();
        assert!(matches!(
            add(&mut fs, old.0.next),
            Err(Error::NoSpace) | Err(Error::MaintenanceExhausted)
        ));
        if !initial {
            assert!(matches!(
                admin(&mut fs, wire::ROTATE, 0),
                Err(Error::NoSpace) | Err(Error::MaintenanceExhausted)
            ));
        }
        assert_eq!(receipt, fs.receipt());
        assert_eq!(history(&fs)?, old);
        assert!(fs.accounting()?.available_blocks <= 15);
    }
    println!(
        "secteur={sector} saturation : premier append refusé, append/rotation refusés, compteur et octets préservés"
    );
    Ok(())
}
fn corruption(sector: u32) -> Result<()> {
    let mut fs = Engine::format_p9(FileImage::create(512, sector)?, IDS, Atime::No, None)?;
    let root = fs.root_id();
    let st = fs.create_at(root, "\x1fNovaOS-journal-new", Kind::File)?;
    fs.prepare_write_id(st.id, 0, b"incomplet")?.commit()?;
    assert_eq!(Journal::load(&fs)?.status().revision, 0);
    add(&mut fs, 1)?;
    let st = fs.lookup_at(root, store::NAME)?;
    fs.prepare_write_id(st.id, 96 + 13, &[0xff])?.commit()?;
    let mut fs = Engine::mount_current(fs.unmount()?, IDS, Mode::ReadWrite)?;
    let r = fs.receipt();
    assert!(matches!(Journal::load(&fs), Err(Error::Corrupt(_))));
    assert!(matches!(add(&mut fs, 2), Err(Error::Corrupt(_))));
    assert_eq!(r, fs.receipt());
    println!(
        "secteur={sector} préparation partielle ignorée; enveloppe modifiée malgré CoW P9 valide détectée, refus sans écriture"
    );
    Ok(())
}
#[derive(Copy, Clone, Debug)]
enum Action {
    First,
    Append,
    Rotate,
    Purge,
}
fn act(fs: &mut Engine<SimDevice>, action: Action) -> Result<u64> {
    match action {
        Action::First => add(fs, 1),
        Action::Append => add(fs, 3),
        Action::Rotate => admin(fs, wire::ROTATE, 0),
        Action::Purge => admin(fs, wire::PURGE, 0),
    }
}
fn interruptions(sector: u32, action: Action) -> Result<()> {
    let d = SimDevice::new(512, sector)?;
    let c = d.control();
    let mut fs = Engine::format_p9(d, IDS, Atime::No, None)?;
    if !matches!(action, Action::First) {
        add(&mut fs, 1)?;
        add(&mut fs, 2)?;
    }
    let old = history(&fs)?;
    drop(fs);
    let baseline = c.snapshot();
    let dev = baseline.device();
    let c = dev.control();
    let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
    c.trace(None, None);
    act(&mut fs, action)?;
    let events = c.events();
    let new = history(&fs)?;
    assert_ne!(old, new);
    let mut stops = 0;
    for (idx, e) in events.iter().enumerate() {
        let dev = baseline.device();
        let c = dev.control();
        let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        c.trace(Some(idx), None);
        let result = act(&mut fs, action);
        drop(fs);
        c.power_cut(Persistence::None)?;
        let fs = Engine::mount_current(c.snapshot().device(), IDS, Mode::ReadOnly)?;
        let after = history(&fs)?;
        assert!(after == old || after == new, "coupure {e:?}/{idx}");
        if result.is_ok() {
            assert_eq!(after, new);
        }
        if matches!(e, Event::FlushCompleted(_)) {
            stops += 1;
        }
    }
    assert_eq!(
        stops,
        if matches!(action, Action::First) {
            6
        } else {
            2
        }
    );
    for ordinal in 1..=stops {
        for newest in [0, 1] {
            let dev = baseline.device();
            let c = dev.control();
            let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
            c.trace(None, None);
            c.inject(SimFault::Flush {
                ordinal,
                persist_newest: newest,
            })?;
            let err = act(&mut fs, action).unwrap_err();
            let unknown = matches!(err, Error::Publication { .. } | Error::UnknownOutcome);
            assert_eq!(unknown, ordinal == stops, "{err}");
            drop(fs);
            c.power_cut(Persistence::None)?;
            let fs = Engine::mount_current(c.snapshot().device(), IDS, Mode::ReadOnly)?;
            assert_eq!(
                history(&fs)?,
                if unknown && newest == 1 {
                    new.clone()
                } else {
                    old.clone()
                }
            );
        }
    }
    println!(
        "secteur={sector} action={action:?} coupures avant/pendant/après écritures et barrières={} F1/F2={stops} erreurs flush={} résultat incertain seulement pour la dernière barrière, historique complet au remontage",
        events.len(),
        stops * 2
    );
    Ok(())
}
// Déchirer physiquement une commande du périphérique, y compris l'ancre P9.
// Une ancre ambiguë doit conserver le comportement de refus de montage normal.
fn torn(sector: u32, action: Action) -> Result<()> {
    let d = SimDevice::new(512, sector)?;
    let c = d.control();
    let mut fs = Engine::format_p9(d, IDS, Atime::No, None)?;
    add(&mut fs, 1)?;
    add(&mut fs, 2)?;
    let old = history(&fs)?;
    drop(fs);
    let baseline = c.snapshot();
    let dev = baseline.device();
    let c = dev.control();
    let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
    c.trace(None, None);
    act(&mut fs, action)?;
    let events = c.events();
    let new = history(&fs)?;
    let mut data_cases = 0;
    let mut refused = 0;
    let mut changed = 0;
    let mut recovered_anchor = 0;
    for (idx, e) in events.iter().enumerate() {
        let Event::WriteCompleted(block) = *e else {
            continue;
        };
        for bytes in [1, 96, 512, 2048] {
            let dev = baseline.device();
            let c = dev.control();
            let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
            c.trace(Some(idx), None);
            assert!(act(&mut fs, action).is_err());
            drop(fs);
            let before = c.snapshot().bytes()?;
            c.power_cut(Persistence::Prefix { block, bytes })?;
            let snap = c.snapshot();
            let after = snap.bytes()?;
            let at = usize::try_from(block).unwrap() * 4096;
            changed += usize::from(before[at..at + bytes] != after[at..at + bytes]);
            assert_eq!(
                &before[at + bytes..at + 4096],
                &after[at + bytes..at + 4096]
            );
            let mounted = Engine::mount_current(snap.device(), IDS, Mode::ReadOnly);
            if block < 2 {
                match mounted {
                    Ok(fs) => {
                        let s = history(&fs)?;
                        assert!(s == old || s == new);
                        recovered_anchor += 1;
                    }
                    Err(Error::Corrupt(_)) => {
                        assert_eq!(bytes, 96);
                        refused += 1;
                    }
                    Err(err) => panic!("erreur de montage non attendue {err}"),
                }
            } else {
                assert_eq!(history(&mounted?)?, old);
                data_cases += 1;
            }
        }
    }
    assert!(changed > 0 && data_cases > 0 && refused > 0 && recovered_anchor > 0);
    println!(
        "secteur={sector} déchirure {action:?} par préfixes 1/96/512/2048 : données et métadonnées CoW ancien journal={data_cases}, ancre complète ancienne/nouvelle={recovered_anchor}, refus de montage P9 après ancre déchirée={refused}, préfixes physiquement différents={changed}"
    );
    Ok(())
}
fn main() -> Result<()> {
    for sector in [512, 4096] {
        normal(sector)?;
        saturation(sector)?;
        corruption(sector)?;
        for op in [Action::Append, Action::Rotate] {
            torn(sector, op)?;
        }
        for op in [Action::First, Action::Append, Action::Rotate, Action::Purge] {
            interruptions(sector, op)?;
        }
    }
    Ok(())
}
