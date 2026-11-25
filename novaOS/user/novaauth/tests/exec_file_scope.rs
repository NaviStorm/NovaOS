#![cfg(feature = "recuperation-lab")]
use novaos_identity::{exec_file_scope::Probe, recuperation_view::seal_digest};

fn request() -> [u8; 128] {
    let mut frame = [0; 128];
    frame[..8].copy_from_slice(Probe::TAG);
    frame[8..24].fill(1);
    frame[24..40].fill(2);
    frame[40..56].fill(3);
    frame[56..72].fill(4);
    frame[72..80].copy_from_slice(&11u64.to_le_bytes());
    frame[80..88].copy_from_slice(&12u64.to_le_bytes());
    frame[88..120].fill(5);
    frame
}

#[test]
fn scope_facts_bind_the_exact_request_and_mounted_instance() {
    let frame = request();
    let probe = Probe::parse(&frame).unwrap();
    let mut seal = *b"NXScope1................................";
    seal[8..].copy_from_slice(&frame[56..88]);
    assert_eq!((probe.directory(), probe.file()), (11, 12));
    assert!(probe.verify_context([[2; 16], [3; 16]], [5; 32]).is_ok());
    assert!(probe.verify_context([[2; 16], [4; 16]], [5; 32]).is_err());
    assert!(probe.verify_context([[2; 16], [3; 16]], [8; 32]).is_err());
    let reply = probe.reply(&seal).unwrap();
    assert_eq!(probe.verify(&reply).unwrap(), seal_digest(&seal));

    for offset in [8, 24, 40, 56, 72, 80, 88] {
        let mut altered = frame;
        altered[offset] ^= 1;
        assert!(Probe::parse(&altered).unwrap().verify(&reply).is_err());
    }
    for offset in [0, 8, 16, 48, 80, 88, 104, 112, 120] {
        let mut altered = reply;
        altered[offset] ^= 1;
        assert!(probe.verify(&altered).is_err());
    }
    let mut unrelated_seal = seal;
    unrelated_seal[32] ^= 1;
    assert!(probe.reply(&unrelated_seal).is_err());
}

#[test]
fn malformed_scope_cannot_reach_the_file_effecter() {
    let frame = request();
    for (start, len) in [(0, 8), (8, 16), (24, 16), (40, 16),
                         (56, 16), (72, 8), (80, 8), (88, 32)] {
        let mut altered = frame;
        altered[start..start+len].fill(0);
        assert!(Probe::parse(&altered).is_err());
    }
    let mut alias = frame;
    alias[80..88].copy_from_slice(&frame[72..80]);
    assert!(Probe::parse(&alias).is_err());
    alias[87] = 0x80;
    assert!(Probe::parse(&alias).is_err());

    let mut padded = frame;
    padded[120] = 1;
    assert!(Probe::parse(&padded).is_err());
}

#[test]
fn mounted_p9_read_scope_requires_the_sealed_parent() {
    use novafs::{Engine, Ids, Kind, device::SimDevice, time::Atime};
    for sector in [512, 4096] {
        let mut fs = Engine::format_p9(SimDevice::new(1024, sector).unwrap(),
            Ids { fs: [2; 16], lv: [3; 16] }, Atime::No, None).unwrap();
        let root = fs.root_id();
        let directory = fs.create_at(root, "travail", Kind::Directory).unwrap().id;
        let file = fs.create_at(directory, "temoin", Kind::File).unwrap().id;
        let mut frame = request();
        frame[72..80].copy_from_slice(&directory.to_le_bytes());
        frame[80..88].copy_from_slice(&file.to_le_bytes());
        let probe = Probe::parse(&frame).unwrap();
        let mut seal = *b"NXScope1................................";
        seal[8..].copy_from_slice(&frame[56..88]);
        let reply = probe.inspect(&fs, [5; 32], &seal).unwrap();
        assert_eq!(probe.verify(&reply).unwrap(), seal_digest(&seal));

        let mut selection = frame;
        selection[..8].copy_from_slice(Probe::SELECT);
        selection[80..88].fill(0);
        let select = Probe::parse(&selection).unwrap();
        let response = select.inspect(&fs, [5; 32], &seal).unwrap();
        assert_eq!(select.verify_selection(&response, seal_digest(&seal)).unwrap().file, file);
        fs.rename_at(directory, "temoin", root, "ailleurs").unwrap();
        assert!(probe.inspect(&fs, [5; 32], &seal).is_err());
        assert!(select.inspect(&fs, [5; 32], &seal).is_err());
    }
}

#[test]
fn selection_requires_the_authenticated_seal_and_remains_distinct_from_exact_readback() {
    let exact = request();
    let mut selection = exact;
    selection[..8].copy_from_slice(Probe::SELECT);
    selection[80..88].fill(0);
    let query = Probe::parse(&selection).unwrap();
    let mut seal = *b"NXScope1................................";
    seal[8..].copy_from_slice(&exact[56..88]);
    let reply = query.reply(&seal).unwrap();
    let selected = query.verify_selection(&reply, seal_digest(&seal)).unwrap();
    assert_eq!((selected.directory, selected.file), (11, 12));
    assert!(query.verify(&reply).is_err());
    assert!(query.verify_selection(&reply, [0; 32]).is_err());
    assert!(Probe::parse(&exact).unwrap().verify_selection(&reply, seal_digest(&seal)).is_err());
    let mut alternative = seal;
    alternative[32] ^= 1;
    assert!(query.verify_selection(&query.reply(&alternative).unwrap(), seal_digest(&seal)).is_err());
    selection[80] = 12;
    assert!(Probe::parse(&selection).is_err());
}
