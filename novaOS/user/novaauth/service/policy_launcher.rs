//! Scénario supervisé. Le lanceur transporte des messages, seuls les signataires
//! authentifiés autorisent l'édition de politique et attestent les sessions.
use super::*;
#[allow(dead_code)]
#[path = "../../../abi/files.rs"]
mod files;
type Bytes = [u8; 128];
fn word(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
fn check(ok: bool) -> rt::Result<()> {
    ok.then_some(()).ok_or(rt::REFUSED)
}
fn log(b: &[u8]) {
    rt::call(rt::DEBUG_WRITE, b.as_ptr() as u64, b.len() as u64, 0);
}
fn metric(prefix: &[u8], value: u64) {
    let mut msg = [0; 96];
    msg[..prefix.len()].copy_from_slice(prefix);
    let mut n = value;
    let mut digits = [0; 20];
    let mut i = digits.len();
    loop {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let end = prefix.len() + digits.len() - i;
    msg[prefix.len()..end].copy_from_slice(&digits[i..]);
    log(&msg[..end]);
}
fn receive(rx: u64, tag: u64, size: u32) -> rt::Result<Bytes> {
    let r = rt::receive_for(rx, 15000)?;
    check(r.kind == 1 && r.tag == tag && r.len == size)?;
    Ok(r.payload)
}
fn start(id: u64, install: &[u8; 16], hash: &[u8; 32]) -> rt::Result<Frame> {
    let mut w = [0; 16];
    w[0] = id;
    w[4] = 1;
    if id >= 21 {
        for (i, v) in hash
            .chunks_exact(8)
            .chain(install.chunks_exact(8))
            .enumerate()
        {
            w[7 + i] = word(v);
        }
    }
    w[0] = syscall(proc::START, &mut w)?;
    Ok(w)
}
struct Actor {
    p: Frame,
    seq: u64,
    next: u64,
}
impl Actor {
    fn new(p: Frame) -> Self {
        Self { p, seq: 0, next: 0 }
    }
    fn sign(&mut self, op: u8, origin: u8, rights: u8, request: u8) -> rt::Result<Bytes> {
        self.seq += 1;
        let mut q = [0; 128];
        q[0] = op;
        q[72] = origin;
        q[73] = rights;
        q[88..104].fill(request);
        rt::send_for(self.p[2], self.seq, &q, 5000)?;
        let r = receive(self.p[1], self.seq, 128)?;
        self.next = word(&r[112..]);
        Ok(r)
    }
    fn success(&mut self, op: u8, origin: u8, rights: u8, request: u8) -> rt::Result<Bytes> {
        let r = self.sign(op, origin, rights, request)?;
        if word(&r) != 0 {
            metric(b"NAP echec op=", op.into());
            metric(b"NAP statut=", word(&r));
        }
        check(word(&r) == 0)?;
        Ok(r)
    }
    fn submit(
        &mut self,
        app: Frame,
        op: u8,
        key: u64,
        rights: u8,
        request: u64,
    ) -> rt::Result<u64> {
        let mut q = [0; 128];
        q[0] = op;
        q[1] = rights;
        q[8..16].copy_from_slice(&key.to_le_bytes());
        q[16..24].copy_from_slice(&request.to_le_bytes());
        let tag = self.next;
        rt::send_for(app[2], tag, &q, 5000)?;
        check(receive(app[1], tag, 1)?[0] == 1)?;
        self.success(11, 0, 0, 0)?;
        Ok(tag)
    }
    fn data(
        &mut self,
        app: Frame,
        op: u8,
        key: u64,
        rights: u8,
        request: u64,
        status: u64,
    ) -> rt::Result<u64> {
        let tag = self.submit(app, op, key, rights, request)?;
        let r = receive(app[1], tag, 128)?;
        if word(&r) != status {
            metric(b"NAP echec VFS op=", op.into());
            metric(b"NAP statut=", word(&r));
        }
        check(word(&r) == status)?;
        Ok(word(&r[8..]))
    }
}
fn control(tag: u64, op: u8, id: &[u8; 16], hash: &[u8; 32]) -> rt::Result<Bytes> {
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NA4START");
    q[8] = op;
    q[16..32].copy_from_slice(id);
    q[32..64].copy_from_slice(hash);
    rt::send_for(rt::config(5), tag, &q, 5000)?;
    receive(rt::config(6), tag, 128)
}
fn collect(p: Frame) -> rt::Result<()> {
    let end = now()? + 15000;
    loop {
        let mut w = [0; 16];
        w[0] = p[0];
        match syscall(proc::POLL, &mut w) {
            Ok(0) if w[1] == 0 => return Ok(()),
            Err(rt::WOULD_BLOCK) if now()? < end => {
                rt::call(rt::THREAD_YIELD, 0, 0, 0);
            }
            _ => return Err(rt::REFUSED),
        }
    }
}
fn measurement(offset: usize) -> [u8; 32] {
    let mut hash = [0; 32];
    for (i, b) in hash.chunks_exact_mut(8).enumerate() {
        let v = unsafe {
            (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(offset + i))
                .load(core::sync::atomic::Ordering::Acquire)
        };
        b.copy_from_slice(&v.to_le_bytes());
    }
    hash
}
fn admit(id: u64, installation: &[u8; 16], hash: &[u8; 32]) -> rt::Result<Frame> {
    check(word(&control(20 + id, 1, installation, hash)?[16..]) == 0)?;
    start(id, installation, hash)
}
fn finish(actors: &[Frame], verifier: Frame) -> rt::Result<()> {
    for p in actors {
        rt::send_for(p[2], 10000, &[0; 128], 5000)?;
        collect(*p)?;
    }
    rt::send_for(verifier[2], 1, &[0], 5000)?;
    collect(verifier)
}
pub(super) fn launch() -> rt::Result<()> {
    let base_hash = measurement(20);
    let mut hash = base_hash;
    let h = control(1, 0, &[0; 16], &hash)?;
    if word(&h[16..]) != 0 {
        log(b"NAP registre refuse avant lancement");
        return Ok(());
    }
    let id = h[24..40].try_into().unwrap();
    let stage = word(&h[40..]);
    check(matches!(stage, 5 | 7 | 8 | 10 | 11))?;
    if stage == 7 {
        check(word(&control(10, 1, &id, &base_hash)?[16..]) == rt::DENIED)?;
        hash = measurement(24);
        check(hash != base_hash)?;
    }
    if stage == 8 {
        let old = h[48..64].try_into().unwrap();
        check(old != id && word(&control(11, 1, &old, &hash)?[16..]) == rt::DENIED)?;
    }
    let mut bad = hash;
    bad[0] ^= 1;
    check(word(&control(2, 1, &id, &bad)?[16..]) == rt::DENIED)?;
    check(word(&control(3, 1, &[0; 16], &hash)?[16..]) == rt::DENIED)?;
    check(matches!(start(21, &id, &bad), Err(rt::DENIED)))?;
    check(word(&control(4, 1, &id, &hash)?[16..]) == 0)?;
    let verifier = start(17, &id, &hash)?;
    let mut alice = Actor::new(start(18, &id, &hash)?);
    let mut bob = Actor::new(start(19, &id, &hash)?);
    let a = alice.success(1, 0, 0, 0)?;
    let b = bob.success(1, 0, 0, 0)?;
    check(a[16..32] != b[16..32])?;
    log(b"NAP deux titulaires authentifies");
    if matches!(stage, 7 | 8) {
        let app = admit(if stage == 7 { 23 } else { 21 }, &id, &hash)?;
        if stage == 7 {
            let lease = alice.data(app, 1, 0, 1, 0, 0)?;
            alice.data(app, 3, lease, 0, 0, 0)?;
            log(b"NAP mise a jour attestee lecture autorisee");
        } else {
            alice.data(app, 1, 0, 1, 0, rt::DENIED)?;
            log(b"NAP reinstallation meme ELF sans droits");
        }
        alice.success(4, 0, 0, 0)?;
        bob.success(4, 0, 0, 0)?;
        finish(&[app, alice.p, bob.p], verifier)?;
        log(b"NAP cycle installation termine");
        return Ok(());
    }
    if stage == 5 {
        check(alice.success(12, 0, 0, 0)?[8] == 0)?;
        alice.success(13, 0, 1, 6)?;
        check(alice.success(12, 0, 0, 0)?[8] == 1)?;
        alice.success(13, 0, 0, 7)?;
        check(alice.success(12, 0, 0, 0)?[8] == 0)?;
        alice.success(13, 0, 1, 8)?;
        alice.success(13, 1, 2, 9)?;
        alice.success(14, 1, 0, 10)?;
        alice.success(4, 0, 0, 0)?;
        alice.success(1, 0, 0, 0)?;
        bob.success(4, 0, 0, 0)?;
        bob.success(1, 0, 0, 0)?;
        log(b"NAP politique modifiee application arretee");
    }
    check(word(&bob.sign(13, 0, 1, 77)?) == rt::DENIED)?;
    let app_a = admit(21, &id, &hash)?;
    let app_b = admit(22, &id, &hash)?;
    log(b"NAP meme image admise deux instances");
    let direct = alice.data(app_a, 1, 0, 1, 0, 0)?;
    alice.data(app_a, 3, direct, 0, 0, 0)?;
    alice.data(app_a, 4, direct, 0, 0, rt::DENIED)?;
    bob.data(app_b, 1, 0, 1, 0, rt::DENIED)?;
    bob.data(app_b, 10, 0, 2, 0, rt::DENIED)?;
    if stage != 11 {
        let root = alice.data(app_a, 10, 0, 2, 0, 0)?;
        let child = alice.data(app_a, 2, root, 2, 0, 0)?;
        if stage == 10 {
            let grandchild = alice.data(app_a, 2, child, 2, 0, 0)?;
            let blocked = alice.submit(app_a, 11, grandchild, 0, 37)?;
            let debut = now()?;
            alice.success(15, 1, 0, 11)?;
            metric(b"NAP retrait ticks=", now()? - debut);
            check(word(&receive(app_a[1], blocked, 128)?) == rt::CANCELLED)?;
            for key in [root, child, grandchild] {
                alice.data(app_a, 5, key, 0, 38, rt::REVOKED)?;
            }
            alice.data(app_a, 6, grandchild, 0, 37, rt::REVOKED)?;
            check(alice.data(app_a, 8, direct, 0, 37, 0)? == 0)?;
            log(b"NAP retrait groupe descendants refuses lecture conservee");
        } else {
            alice.data(app_a, 5, child, 0, 35, 0)?;
            alice.data(app_a, 7, child, 0, 35, files::UNCERTAIN)?;
            check(alice.data(app_a, 8, direct, 0, 35, 0)? == 1)?;
            let waiting = alice.submit(app_a, 11, child, 0, 36)?;
            alice.success(4, 0, 0, 0)?;
            let r = receive(app_a[1], waiting, 128)?;
            check(word(&r) == rt::CANCELLED)?;
            alice.success(1, 0, 0, 0)?;
            alice.data(app_a, 3, direct, 0, 0, rt::REVOKED)?;
            let fresh = alice.data(app_a, 1, 0, 1, 0, 0)?;
            check(alice.data(app_a, 8, fresh, 0, 36, 0)? == 0)?;
            bob.success(3, 0, 0, 0)?;
            log(b"NAP fermeture session reveil certain ecriture perdue reconciliee");
        }
    } else {
        alice.data(app_a, 10, 0, 2, 0, rt::DENIED)?;
        log(b"NAP retrait persistant apres reauthentification");
    }
    if stage != 5 {
        check(alice.data(app_a, 3, direct, 0, 0, 0)? == 1)?;
    }
    alice.success(4, 0, 0, 0)?;
    bob.success(4, 0, 0, 0)?;
    let counts = control(100, 2, &id, &hash)?;
    check(word(&counts[16..]) == 0 && counts[24..27] == [0; 3])?;
    log(b"NAP zero handle preparation attente");
    finish(&[app_a, app_b, alice.p, bob.p], verifier)?;
    log(b"NAP cinq processus collectes");
    Ok(())
}
