#![no_std]
#![no_main]
mod common;
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
struct Client {
    key: SigningKey,
    seq: u64,
}
impl Client {
    fn frame(&self, op: u8) -> [u8; 128] {
        let mut q = [0; 128];
        q[0] = op;
        q[8..40].copy_from_slice(self.key.verifying_key().as_bytes());
        q
    }
    fn send(&mut self, q: &[u8; 128]) -> rt::Result<[u8; 128]> {
        self.seq += 1;
        rpc(rt::config(7), rt::config(8), self.seq, q)
    }
    fn directory(
        &mut self,
        session: [u8; 32],
        op: u8,
        key: u64,
        rights: u8,
        request: u64,
    ) -> rt::Result<(u64, u64)> {
        let mut data = [0; 128];
        data[0] = op;
        data[1] = rights;
        data[8..16].copy_from_slice(&key.to_le_bytes());
        data[16..24].copy_from_slice(&request.to_le_bytes());
        let tag = self.seq.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_frame(rt::config(9), tag, &data, 0, false)?;
        let mut auth = self.frame(11);
        auth[40..72].copy_from_slice(&session);
        let result = self
            .send(&auth)
            .map_err(|e| if matches!(op, 6 | 7) { 22 } else { e })?;
        if number(&result) != 0 {
            return Ok((number(&result), 0));
        }
        let reply = remote(rt::config(10), tag)
            .map_err(|e| if matches!(op, 6 | 7) { 22 } else { e })?
            .payload;
        check(&reply[120..] == b"DIRREPLY")?;
        Ok((number(&reply), number(&reply[8..])))
    }
    fn challenge(&mut self) -> rt::Result<[u8; 128]> {
        let mut q = self.frame(1);
        q[40..56].copy_from_slice(&random()?[..16]);
        self.send(&q)
    }
    fn answer(&self, a: &[u8; 128]) -> [u8; 128] {
        let mut q = self.frame(2);
        q[40..48].copy_from_slice(&a[8..16]);
        q[48..112].copy_from_slice(&self.key.sign(&a[16..48]).to_bytes());
        q
    }
    fn login(&mut self) -> rt::Result<([u8; 16], [u8; 32], [u8; 128])> {
        let a = self.challenge()?;
        check(number(&a) == 0)?;
        let signed = self.answer(&a);
        let r = self.send(&signed)?;
        check(number(&r) == 0 && r[8..24] != [0; 16] && r[24..56] != [0; 32])?;
        Ok((
            r[8..24].try_into().unwrap(),
            r[24..56].try_into().unwrap(),
            signed,
        ))
    }
    fn session(&mut self, op: u8, id: [u8; 32], arg: [u8; 16]) -> rt::Result<[u8; 128]> {
        self.named_session(
            op,
            id,
            arg,
            if op == 5 {
                b"alice"
            } else if op == 8 {
                b"alice-b"
            } else {
                b""
            },
        )
    }
    fn named_session(
        &mut self,
        op: u8,
        id: [u8; 32],
        arg: [u8; 16],
        name: &[u8],
    ) -> rt::Result<[u8; 128]> {
        if name.len() > 24 {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut q = self.frame(op);
        q[40..72].copy_from_slice(&id);
        q[72..88].copy_from_slice(&arg);
        if op >= 5 {
            q[88..104].copy_from_slice(&random()?[..16]);
        }
        q[104..104 + name.len()].copy_from_slice(name);
        self.send(&q)
    }
}
fn from_manager(tag: u64, len: u32) -> rt::Result<[u8; 128]> {
    let m = rt::receive_for(rt::config(1), 20000)?;
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
    check(c.directory(session, 3, 1, 0, 0)?.0 == rt::DENIED)?;
    marker(b"NA3 D autre titulaire refuse");
    check(number(&c.session(3, second, [0; 16])?) == 0)?;
    check(number(&c.session(3, session, [0; 16])?) == 0)?;
    marker(b"NA3 trois sessions simultanees");
    check(number(&c.session(4, second, [0; 16])?) == 0)?;
    check(number(&c.session(3, second, [0; 16])?) == rt::DENIED)?;
    check(number(&c.session(3, session, [0; 16])?) == 0)?;
    marker(b"NA3 fermeture independante");
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
    from_manager(7, 1)?;
    const REQUEST: u64 = 0x4132303236;
    let (code, root_entries) = c.directory(id, 3, 1, 0, 0)?;
    check(code == 0 && root_entries == 0)?;
    let (again, second, _) = c.login()?;
    check(again == subject && c.directory(second, 3, 1, 0, 0)?.0 != 0)?;
    check(c.directory(id, 4, 1, 0, 0)?.0 == rt::DENIED)?;
    let (code, parent) = c.directory(id, 2, 1, 3, 0)?;
    check(code == 0)?;
    let (code, child) = c.directory(id, 2, parent, 3, 0)?;
    check(code == 0)?;
    let (code, ro) = c.directory(id, 2, 1, 1, 0)?;
    check(code == 0 && c.directory(id, 5, ro, 0, REQUEST)?.0 == rt::REVOKED)?;
    check(c.directory(id, 5, child, 0, REQUEST)?.0 == 0)?;
    check(c.directory(id, 9, parent, 0, 0)?.0 == 0)?;
    check(c.directory(id, 6, child, 0, REQUEST)?.0 == rt::REVOKED)?;
    check(
        c.directory(id, 3, parent, 0, 0)?.0 == rt::REVOKED
            && c.directory(id, 3, child, 0, 0)?.0 == rt::REVOKED,
    )?;
    check(c.directory(id, 8, ro, 0, REQUEST)? == (0, 0))?;
    check(c.directory(id, 5, 1, 0, REQUEST)?.0 == 0)?;
    check(c.directory(id, 7, 1, 0, REQUEST)?.0 == 22)?;
    check(c.directory(id, 8, ro, 0, REQUEST)? == (0, 1))?;
    check(c.directory(id, 9, 1, 0, 0)?.0 == 0)?;
    check(
        c.directory(id, 3, 1, 0, 0)?.0 == rt::REVOKED
            && c.directory(id, 3, ro, 0, 0)?.0 == rt::REVOKED,
    )?;
    check(c.directory(id, 1, 0, 3, 0)?.0 == rt::DENIED)?;
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
            marker(b"NA3 client echec");
            rt::exit(e)
        }
    }
}
