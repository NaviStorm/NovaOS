//! Contrat du diagnostic privé NVMe. Aucun pilote, disque ou processus invité requis.
#[path = "../user/nvme_diagnostic.rs"]
mod nvme_diagnostic;

mod runtime {
    use std::sync::Mutex;
    pub const DEBUG_WRITE: u64 = 1;
    pub static DERNIERE_LIGNE: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    pub fn call(op: u64, ptr: u64, len: u64, reserved: u64) {
        assert_eq!((op, reserved), (DEBUG_WRITE, 0));
        // Cause::report conserve la ligne sur sa pile pendant cet appel synchrone.
        let line = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        *DERNIERE_LIGNE.lock().unwrap() = line.to_vec();
    }
}

use nvme_diagnostic::{Cause, Layer};

#[test]
fn conserve_cid_statut_et_etat_de_publication() {
    let bytes = *b"NCQ1\x1a\0\x80\x02";
    assert_eq!(Cause::new(26, 0x280).unwrap().encode(), bytes);
    let cause = Cause::decode(&bytes).unwrap();
    for (name, layer, code) in [
        ("bloc", Layer::Block, 26),
        ("p9", Layer::Files, 22),
        ("app", Layer::Application, 22),
    ] {
        cause.report(layer, code);
        let line = format!("NVME CQE {name} code={code:02x} cid=001a statut=0280");
        assert_eq!(
            runtime::DERNIERE_LIGNE.lock().unwrap().as_slice(),
            line.as_bytes()
        );
    }
    let maximum_cid = *b"NCQ1\xff\xff\x80\x42";
    assert_eq!(Cause::decode(&maximum_cid).unwrap().encode(), maximum_cid);
}

#[test]
fn rejette_les_trames_incompletes_et_les_causes_invalides() {
    for (cid, status) in [(0, 1), (1, 0), (1, 0x7000), (65536, 1), (1, 65536)] {
        assert!(Cause::new(cid, status).is_none());
    }
    let mut bytes = Cause::new(1, 0x280).unwrap().encode();
    for len in 0..bytes.len() {
        assert!(Cause::decode(&bytes[..len]).is_none());
    }
    bytes[3] = 0;
    assert!(Cause::decode(&bytes).is_none());
}
