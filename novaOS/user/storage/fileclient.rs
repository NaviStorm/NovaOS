#![no_std]
#![no_main]
#[path = "../../abi/files.rs"]
#[allow(dead_code)]
mod protocol;
mod runtime;
use protocol as w;
use runtime as rt;

mod fileapi;
mod names;
use fileapi::Client;
fn log(s: &[u8]) {
    rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}
fn ensure(v: bool) -> rt::Result<()> {
    if v { Ok(()) } else { Err(rt::REFUSED) }
}
fn perf(mut c: Client) -> rt::Result<()> {
    log(b"ech debut");
    c.mount()?;
    log(b"ech mount");
    c.info()?;
    log(b"ech info");
    let f = c.open(b"/sortie", 3)?;
    log(b"ech open");
    let data: [u8; 4231] = core::array::from_fn(|i| (i as u8).wrapping_mul(73) ^ (i >> 5) as u8);
    c.write(f, 0, &data)?;
    log(b"ech write");
    c.sync(f)?;
    log(b"ech sync");
    let mut actual = [0; 4231];
    ensure(c.read(f, 0, &mut actual)? == 4231 && actual == data)?;
    log(b"ech read");
    let mut chunk = [0; 64];
    for i in 0..16 {
        let off = (i * 1987) % 4100;
        ensure(c.read(f, off as u64, &mut chunk)? == 64 && chunk == data[off..off + 64])?;
    }
    log(b"ech read-rand");
    for i in 0..8 {
        let off = (i * 1987) % 4100;
        c.write(f, off as u64, &data[off..off + 64])?;
        c.sync(f)?;
    }
    log(b"ech write-rand");
    c.close(f)?;
    log(b"ech close");
    c.close(0)?;
    Ok(())
}
fn probe() -> rt::Result<()> {
    let mode = rt::config(1);
    let mut c = Client::new()?;
    if (29..=40).contains(&mode) {
        return names::probe(c, mode);
    }
    if mode == 25 {
        return perf(c);
    }
    c.mount()?;
    ensure(c.session != 0)?;
    log(b"FICHIERS MOUNT");
    let info = c.info()?;
    ensure(
        w::length(&info) == 83
            && matches!(info[w::PAYLOAD + 81], 0 | 9)
            && info[w::PAYLOAD + 82] == 1,
    )?;
    if mode == 26 {
        let mut id = *b"Volume 00000000000000000000000000000000";
        let digits = b"0123456789abcdef";
        for (i, byte) in info[w::PAYLOAD..w::PAYLOAD + 16].iter().enumerate() {
            id[7 + 2 * i] = digits[(byte >> 4) as usize];
            id[8 + 2 * i] = digits[(byte & 15) as usize];
        }
        log(&id);
        c.close(0)?;
        return Ok(());
    }
    if mode == 27 {
        let f = c.open(b"/sortie", 3)?;
        c.write(f, 0, b"prive")?;
        log(b"FICHIERS abandon pendant WRITE");
        rt::exit(rt::CANCELLED)
    }
    let mut malformed = w::frame(w::INFO, c.session, 0);
    malformed[6] = 1;
    ensure(c.request(malformed).err() == Some(rt::BAD_ARGUMENT))?;
    ensure(c.request(w::frame(w::INFO, c.session ^ 1, 0)).err() == Some(rt::DENIED))?;
    let source = c.open(b"/source", w::READABLE)?;
    let mut sample = [0xa5; 32];
    ensure(
        c.read(source, 0, &mut sample)? == 10
            && &sample[..10] == b"hote-Nova!"
            && sample[10] == 0xa5,
    )?;
    ensure(c.write(source, 0, b"x").err() == Some(rt::DENIED))?;
    c.close(source)?;
    ensure(c.read(source, 0, &mut sample).err() == Some(rt::REVOKED))?;
    ensure(c.open(b"/a/../b", 1).err() == Some(rt::BAD_ARGUMENT))?;
    log(b"FICHIERS INFO OPEN READ droits verifies");
    if (13..=24).contains(&mode) || mode == 28 {
        let verify = matches!(mode, 13 | 23);
        let f = c.open(b"/sortie", if verify { 1 } else { 3 })?;
        let next: [u8; 4231] =
            core::array::from_fn(|i| (i as u8).wrapping_mul(73) ^ (i >> 5) as u8);
        if !verify {
            c.write(f, 0, &next)?;
            c.sync(f)?;
            log(b"FICHIERS transaction confirmee");
        }
        let mut data = [0; 4232];
        let n = c.read(f, 0, &mut data)?;
        if n == 6 && &data[..n] == b"ancien" {
            log(b"FICHIERS oracle ancien");
        } else {
            ensure(n == next.len() && data[..n] == next)?;
            log(b"FICHIERS oracle nouveau");
        }
        c.close(f)?;
        c.close(0)?;
        return Ok(());
    }
    let writing = mode != 12;
    if !writing {
        ensure(c.open(b"/sortie", 3).err() == Some(rt::DENIED))?;
    }
    let f = c.open(b"/sortie", if writing { 7 } else { 1 })?;
    let pattern: [u8; 4231] = core::array::from_fn(|i| (i as u8).wrapping_mul(73) ^ (i >> 5) as u8);
    if writing {
        let disposable = c.open(b"/sortie", 2)?;
        let mut unfinished = w::frame(w::WRITE, c.session, disposable);
        unfinished[2] = w::START;
        w::set(&mut unfinished, 32, 4);
        w::size(&mut unfinished, 2);
        c.request(unfinished)?;
        ensure(c.simple(w::SYNC, disposable).err() == Some(w::BUSY))?;
        let mut discard = w::frame(w::CLOSE, c.session, disposable);
        discard[2] = w::DISCARD;
        c.request(discard)?;
        let mut bad = w::frame(w::READ, c.session, f);
        w::set(&mut bad, 24, u64::MAX);
        w::size(&mut bad, 2);
        ensure(c.request(bad).err() == Some(rt::BAD_ARGUMENT))?;
        // Deux handles sur le même fichier ont des droits et des vues distincts avant SYNC.
        let other = c.open(b"/sortie", w::READABLE | w::WRITABLE)?;
        let mut old = [0; 16];
        let published = c.read(other, 0, &mut old)?;
        c.write(f, 0, &pattern)?;
        ensure(c.write(other, 0, b"concurrent").err() == Some(w::BUSY))?;
        let mut probe = [0; 16];
        ensure(c.read(other, 0, &mut probe)? == published && probe == old)?;
        let mut staged = [0; 32];
        ensure(c.read(f, 4077, &mut staged)? == 32 && staged == pattern[4077..4109])?;
        ensure(c.close(f).err() == Some(w::BUSY))?;
        c.sync(f)?;
        ensure(c.read(other, 0, &mut probe)? == probe.len() && probe == pattern[..16])?;
        c.close(other)?;
        c.write(f, 4091, b"NovaFS-partiel!")?;
        c.sync(f)?;
        c.sync(f)?; // Barrière explicite sur une session déjà propre.
        log(b"FICHIERS SYNC confirme");
    }
    let mut expected = pattern;
    expected[4091..4106].copy_from_slice(b"NovaFS-partiel!");
    let mut actual = [0; 4232];
    ensure(c.read(f, 0, &mut actual)? == expected.len() && actual[..expected.len()] == expected)?;
    log(b"FICHIERS octets verifies");
    c.close(f)?;
    c.close(0)?;
    ensure(matches!(
        c.info(),
        Err(rt::REVOKED | rt::DENIED | rt::CLOSED)
    ))?;
    log(b"FICHIERS CLOSE");
    Ok(())
}
// Sûreté : point d’entrée privé appelé par le trampoline utilisateur.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(probe().err().unwrap_or(0))
}
