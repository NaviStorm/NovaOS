//! Contrat public des trames de découverte, compilé à côté des deux modules ABI candidats.
#![allow(dead_code)]
#[path = "block.rs"]
mod bloc;
#[path = "files.rs"]
mod fichier;

#[test]
fn bloc_version_sens_et_champs_reserves() {
    let connus = bloc::FEATURE_PAGE | bloc::FEATURE_BATCH;
    for bits in [0, bloc::FEATURE_PAGE, bloc::FEATURE_BATCH, connus, u32::MAX] {
        let q = bloc::discovery(bits, false);
        assert!(bloc::syntax(&q));
        assert_eq!(bloc::offered(&q, connus), None);
        let r = bloc::discovery(bits, true);
        assert_eq!(
            bloc::offered(&r, connus),
            (bits & !connus == 0).then_some(bits)
        );
        assert!(!bloc::syntax(&r));
        for i in 0..128 {
            if (40..44).contains(&i) {
                continue;
            }
            let mut invalide = q;
            invalide[i] ^= 0x80;
            assert!(!bloc::syntax(&invalide), "requête, octet {i}");
            let mut invalide = r;
            invalide[i] ^= 0x80;
            assert_eq!(bloc::offered(&invalide, connus), None, "réponse, octet {i}");
        }
    }
    assert_eq!(
        bloc::offered(&bloc::discovery(connus, true), bloc::FEATURE_PAGE),
        None
    );
}

#[test]
fn reponse_bloc_historique_et_reference_exacte() {
    let q = bloc::blank(bloc::INFO);
    assert!(bloc::syntax(&q));
    assert_eq!(bloc::offered(&q, u32::MAX), None);
    let mut q = [0u8; 128];
    q[0] = 1;
    q[1] = 7;
    q[32..36].copy_from_slice(b"NVBQ");
    q[36] = 1;
    q[40] = 3;
    assert_eq!(q, bloc::discovery(3, false));
    q[35] = b'A';
    assert_eq!(Some(3), bloc::offered(&q, 3));
}

#[test]
fn fichier_session_version_masque_et_anciens_opcodes() {
    let session = 0x0123_4567_89ab_cdefu64;
    let mut q = [0u8; 128];
    q[0] = 2;
    q[1] = 31;
    q[8..16].copy_from_slice(&session.to_le_bytes());
    q[24..28].copy_from_slice(b"NVFQ");
    q[28] = 1;
    q[32] = 1;
    assert_eq!(fichier::discovery(session, fichier::PLAN_V1, false), q);
    q[27] = b'A';
    assert_eq!(fichier::discovery(session, fichier::PLAN_V1, true), q);
    assert_ne!(fichier::frame(fichier::LOOKUP_AT, session, 0), q);
    assert_ne!(fichier::discovery(session + 1, fichier::PLAN_V1, true), q);
}

// Le transport simulé fournit une réponse au client vi réel. Il interdit tout appel
// de sauvegarde : la découverte doit être terminée avant la première mutation.
#[path = "vi_wire.rs"]
mod vi;
use fichier as protocol;

#[path = "native.rs"]
mod abi;

mod runtime {
    pub use super::abi::*;
    use std::cell::RefCell;
    pub type Result<T> = std::result::Result<T, u64>;
    thread_local! {
        static REPONSE: RefCell<Option<Result<ReceiveV1>>> = const { RefCell::new(None) };
    }
    pub fn fournir(message: Result<ReceiveV1>) {
        REPONSE.with(|slot| assert!(slot.replace(Some(message)).is_none()));
    }
    pub fn channels() -> Result<(u64, u64)> {
        Ok((3, 4))
    }
    pub fn send_for(cap: u64, tag: u64, bytes: &[u8], _: u64) -> Result<()> {
        assert_eq!((cap, tag), (3, 1));
        assert_eq!(bytes, b"\x0fNVVQ\x01\x07");
        Ok(())
    }
    pub fn receive_for(cap: u64, _: u64) -> Result<ReceiveV1> {
        assert_eq!(cap, 4);
        REPONSE.with(|slot| slot.take().expect("réponse attendue"))
    }
    pub fn call(_: u64, _: u64, _: u64, _: u64) {
        unreachable!()
    }
    pub fn exit(_: u64) -> ! {
        unreachable!()
    }
    pub mod bulk {
        use super::*;
        pub fn features(cap: u64, sens: u32) -> Result<u64> {
            assert_eq!((cap, sens), (3, SEND));
            Ok(IPC_FEATURE_PAGE)
        }
        pub fn put(_: u64, _: u64, _: &[u8]) -> Result<()> {
            unreachable!()
        }
        pub fn pret(_: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> Result<u64> {
            unreachable!()
        }
    }
}
fn reponse(octets: &[u8]) -> runtime::ReceiveV1 {
    let mut payload = [0; 128];
    payload[..octets.len()].copy_from_slice(octets);
    runtime::ReceiveV1 {
        version: 1,
        size: 0,
        timeout: 0,
        options: 0,
        reserved: 0,
        kind: 1,
        len: octets.len() as u32,
        tag: 1,
        handle: 0,
        aux: 0,
        payload,
    }
}
fn negocier(reponse: runtime::Result<runtime::ReceiveV1>) -> runtime::Result<(bool, u8)> {
    runtime::fournir(reponse);
    let mut lien = vi::Link::new().unwrap();
    let resultat = lien.negotiate();
    if resultat.is_err() {
        assert_eq!((lien.text_pages, lien.mode_plan), (false, 0));
    }
    resultat.map(|()| (lien.text_pages, lien.mode_plan))
}
#[test]
fn vi_masques_autorises_et_champs_invalides() {
    for bits in 0..=u8::MAX {
        let mut octets = *b"\0NVVA\x01\0";
        octets[6] = bits;
        let attendu = match bits {
            0..=5 => Ok((bits & 1 != 0, bits >> 1)),
            _ => Err(runtime::BAD_ARGUMENT),
        };
        assert_eq!(negocier(Ok(reponse(&octets))), attendu);
        for i in 1..6 {
            let mut mauvais = octets;
            mauvais[i] ^= 0x80;
            assert_eq!(negocier(Ok(reponse(&mauvais))), Err(runtime::BAD_ARGUMENT));
        }
    }
}
#[test]
fn vi_erreurs_historiques_erreurs_transport_et_revocation() {
    use runtime::*;
    assert_eq!(negocier(Ok(reponse(&[BAD_ARGUMENT as u8]))), Ok((false, 0)));
    for erreur in [DENIED, REVOKED, CLOSED, TIMED_OUT] {
        assert_eq!(negocier(Ok(reponse(&[erreur as u8]))), Err(erreur));
        assert_eq!(negocier(Err(erreur)), Err(erreur));
    }
    for octets in [
        &[][..],
        &[BAD_ARGUMENT as u8, 0],
        &[UNKNOWN_CALL as u8],
        b"\0NVVQ\x01\x01",
        b"\0NVVA\x01",
        b"\0NVVA\x01\x01\0",
    ] {
        assert_eq!(negocier(Ok(reponse(octets))), Err(BAD_ARGUMENT));
    }
    for (kind, tag) in [(0, 1), (1, 2), (2, 1)] {
        let mut mauvaise = reponse(b"\0NVVA\x01\x01");
        mauvaise.kind = kind;
        mauvaise.tag = tag;
        assert_eq!(negocier(Ok(mauvaise)), Err(BAD_ARGUMENT));
    }
}
