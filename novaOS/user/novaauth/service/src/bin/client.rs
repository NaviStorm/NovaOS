#![no_std]
#![no_main]
mod common;
#[allow(dead_code)]
#[path = "../../../../../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../../../../abi/instances.rs"]
mod instances;
#[allow(dead_code)]
#[path = "../../../../../abi/service_process.rs"]
mod process;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
#[path = "../../../../../abi/tmp.rs"]
mod tmp;
use common::*;
use ed25519_dalek::{Signer, SigningKey};
use novaos_auth_session::{Factor, Possession};
use runtime as rt;
fn check(ok: bool) -> rt::Result<()> {
    ok.then_some(()).ok_or(rt::REFUSED)
}
include!("common/client_api.rs");
fn from_manager(tag: u64, len: u32) -> rt::Result<[u8; 128]> {
    let m = rt::receive_for(rt::config(1), 15000)?;
    check(m.kind == 1 && m.tag == tag && m.len == len)?;
    Ok(m.payload)
}
fn pause(ticks: u64) -> rt::Result<()> {
    check(matches!(
        rt::receive_for(rt::config(1), ticks),
        Err(rt::TIMED_OUT)
    ))
}
fn admin(c: &mut Client, other: [u8; 16]) -> rt::Result<()> {
    let (subject, session, signed) = c.login()?;
    let enrolled = c.session(10, session, [1; 16])?;
    check(number(&enrolled) == 0 && number(&enrolled[8..]) == 1)?;
    marker(b"NA3 client preuve valide");
    let (again, second, _) = c.login()?;
    check(subject == again && session != second)?;

    check(number(&c.send(&signed)?) == rt::DENIED)?;
    marker(b"NA3 client rejeu refuse");
    pause(12)?;
    let expired = c.challenge()?;
    check(number(&expired) == 0)?;
    pause(450)?;
    let q = c.answer(&expired);
    check(number(&c.send(&q)?) == rt::DENIED)?;
    marker(b"NA3 expiration refusee");
    pause(30)?;
    let a = c.challenge()?;
    check(number(&a) == 0)?;
    let mut q = c.answer(&a);
    q[60] ^= 1;
    check(number(&c.send(&q)?) == rt::DENIED)?;
    marker(b"NA3 signature incorrecte refusee");
    let a = c.session(5, session, other)?;
    check(number(&a) == 0)?;
    let alice: [u8; 16] = a[8..24].try_into().unwrap();
    rt::send_for(rt::config(0), 2, &alice, 5000)?;
    let other_session = from_manager(2, 32)?[..32].try_into().unwrap();
    check(number(&c.session(3, other_session, [0; 16])?) == rt::DENIED)?;
    check(other_session != session && other_session != second)?;
    let issued = from_manager(8, 8)?;
    check(c.directory(session, 3, number(&issued), 0, 0)?.0 == rt::DENIED)?;
    marker(b"NA3 D autre titulaire refuse");
    check(number(&c.session(3, second, [0; 16])?) == 0)?;
    check(number(&c.session(3, session, [0; 16])?) == 0)?;
    marker(b"NA3 trois sessions simultanees");
    check(number(&c.session(4, second, [0; 16])?) == 0)?;
    check(number(&c.session(3, second, [0; 16])?) == rt::DENIED)?;
    check(number(&c.session(3, session, [0; 16])?) == 0)?;
    marker(b"NA3 fermeture independante");
    rt::send_for(rt::config(0), 8, &[1], 5000)?;
    from_manager(9, 1)?;
    let (same, session, _) = c.login()?;
    check(same == subject)?;
    check(number(&c.session(8, session, alice)?) == 0)?;
    check(number(&c.session(9, session, alice)?) == 0)?;
    marker(b"NA3 renomme et groupe retire");
    check(number(&c.session(6, session, alice)?) == 0)?;
    marker(b"NA3 desactivation confirmee");
    rt::send_for(rt::config(0), 3, &[1], 5000)?;
    let done = from_manager(3, 16)?;
    check(number(&c.session(7, session, alice)?) == 0)?;
    let recreated = c.named_session(5, session, done[..16].try_into().unwrap(), b"alice-b")?;
    check(number(&recreated) == 0 && recreated[8..24] != alice && recreated[8..24] != [0; 16])?;
    check(number(&c.session(3, session, [0; 16])?) == 0)?;
    marker(b"NA3 nouveau titulaire distinct");
    rt::send_for(rt::config(0), 4, &recreated[8..24], 5000)?;
    Ok(())
}
fn ordinary(c: &mut Client, expected: [u8; 16]) -> rt::Result<()> {
    let (subject, id, _) = c.login()?;
    check(subject == expected)?;
    check(number(&c.session(6, id, subject)?) == rt::DENIED)?;
    marker(b"NA3 administration refusee");
    check(c.directory(id, 3, 1, 0, 0)?.0 == rt::DENIED)?;
    marker(b"NA3 D avant attribution refuse");
    rt::send_for(rt::config(0), 2, &id, 5000)?;
    let granted = from_manager(7, 8)?;
    let root = number(&granted);
    check(root != 0)?;
    const REQUEST: u64 = 0x4132303236;
    let (code, root_entries) = c.directory(id, 3, root, 0, 0)?;
    check(code == 0 && root_entries == 0)?;
    let (again, second, _) = c.login()?;
    check(again == subject && c.directory(second, 3, root, 0, 0)?.0 == rt::REVOKED)?;
    check(c.directory(id, 4, root, 0, 0)?.0 == rt::DENIED)?;
    let (code, parent) = c.directory(id, 2, root, 3, 0)?;
    check(code == 0)?;
    let (code, child) = c.directory(id, 2, parent, 3, 0)?;
    check(code == 0)?;
    let (code, ro) = c.directory(id, 2, root, 1, 0)?;
    check(code == 0 && c.directory(id, 5, ro, 0, REQUEST)?.0 == rt::DENIED)?;
    let cancelled = REQUEST - 1;
    check(c.directory(id, 5, child, 0, cancelled)?.0 == 0)?;
    check(c.directory(id, 9, parent, 0, 0)?.0 == 0)?;
    check(c.directory(id, 6, child, 0, cancelled)?.0 == rt::REVOKED)?;
    check(
        c.directory(id, 3, parent, 0, 0)?.0 == rt::REVOKED
            && c.directory(id, 3, child, 0, 0)?.0 == rt::REVOKED,
    )?;
    check(c.directory(id, 8, ro, 0, cancelled)? == (0, 0))?;
    check(c.directory(id, 5, root, 0, REQUEST)?.0 == 0)?;
    check(c.directory(id, 7, root, 0, REQUEST)?.0 == files::UNCERTAIN)?;
    check(c.directory(id, 8, ro, 0, REQUEST)? == (0, 1))?;
    let confirmed = REQUEST + 1;
    check(c.directory(id, 5, root, 0, confirmed)?.0 == 0)?;
    check(c.directory(id, 6, root, 0, confirmed)?.0 == 0)?;
    check(c.directory(id, 8, ro, 0, confirmed)? == (0, 1))?;
    check(c.directory(id, 9, root, 0, 0)?.0 == 0)?;
    check(
        c.directory(id, 3, root, 0, 0)?.0 == rt::REVOKED
            && c.directory(id, 3, ro, 0, 0)?.0 == rt::REVOKED,
    )?;
    check(c.directory(id, 1, 0, 3, 0)?.0 == rt::DENIED)?;
    check(number(&c.session(3, id, [0; 16])?) == 0)?;
    marker(b"NA3 D descendants retires temoin actif issue retrouvee");
    rt::send_for(rt::config(0), 7, &[1], 5000)?;
    from_manager(2, 1)?;
    check(
        number(&c.session(3, id, [0; 16])?) == rt::DENIED && number(&c.challenge()?) == rt::DENIED,
    )?;
    marker(b"NA3 session et facteur revoques");
    let mut fresh = Client {
        key: SigningKey::from_bytes(&*random()?),
        seq: c.seq,
    };
    let reference = Possession(fresh.key.verifying_key()).reference(DOMAIN);
    rt::send_for(rt::config(0), 3, &reference, 5000)?;
    let new = from_manager(3, 16)?;
    check(
        number(&c.session(3, id, [0; 16])?) == rt::DENIED && number(&c.challenge()?) == rt::DENIED,
    )?;
    pause(55)?;
    fresh.seq = c.seq;
    let (new_subject, _, _) = fresh.login()?;
    check(new_subject == new[..16] && new_subject != subject)?;
    marker(b"NA3 ancien facteur refuse apres recreation");
    Ok(())
}
fn exercise() -> rt::Result<()> {
    if rt::config(2) == 20 {
        let selection = from_manager(1, 128)?;
        check(
            &selection[..8] == b"NA3DIR01"
                && selection[8..24] != [0; 16]
                && selection[24..56] != [0; 32]
                && selection[56..].iter().all(|&v| v == 0),
        )?;
        rt::send_for(rt::config(0), 1, &selection, 5000)?;
        marker(b"NA3 provisionneur D attribue");
        check(from_manager(2, 1)?[0] == 0)?;
        return Ok(());
    }
    let mut c = Client {
        key: SigningKey::from_bytes(&*random()?),
        seq: 0,
    };
    check(rt::call(tmp::CAP, 0, 0, 0).status == rt::DENIED)?;
    check(rt::call(instances::SERVICE_PROCESS, process::NOW, 0, 0).status == rt::DENIED)?;
    let reference = Possession(c.key.verifying_key()).reference(DOMAIN);
    rt::send_for(rt::config(0), 1, &reference, 5000)?;
    let info = from_manager(1, 16)?[..16].try_into().unwrap();
    if rt::config(2) == 18 {
        admin(&mut c, info)?;
        marker(b"NA3 administrateur termine");
    } else {
        ordinary(&mut c, info)?;
        marker(b"NA3 client termine");
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    match exercise() {
        Ok(()) => rt::exit(0),
        Err(e) => {
            let mut reason = *b"NA3 client echec 000";
            for (i, b) in reason[17..].iter_mut().rev().enumerate() {
                *b = b'0' + ((e / 10u64.pow(i as u32)) % 10) as u8;
            }
            marker(&reason);
            rt::exit(e)
        }
    }
}
