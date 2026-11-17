#![no_std]
#![no_main]
mod common;
#[path = "common/incarnation.rs"]
mod incarnation;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use ed25519_dalek::{Signature, VerifyingKey};
use runtime as rt;

fn qualify() -> rt::Result<()> {
    let public = VerifyingKey::from_bytes(include_bytes!(env!("NOVA_QEMU_SERVER_PUB")))
        .map_err(|_| rt::REFUSED)?;
    let id = incarnation::identity();
    if id == [0; 32] {
        return Err(rt::REFUSED);
    }
    // Le registre démarre avant le signataire. Le dernier mot publié est la feuille IPC.
    let port = loop {
        let key = rt::config(8);
        if key != 0 {
            break key;
        }
        match rt::receive_for(rt::config(1), 10) {
            Err(rt::TIMED_OUT) => (),
            _ => return Err(rt::CLOSED),
        }
    };
    let nonce = common::random()?;
    let mut req = [0; 128];
    req[..16].copy_from_slice(&incarnation::SERVER);
    req[16..32].copy_from_slice(&incarnation::DOMAIN);
    req[32..64].copy_from_slice(&id);
    req[64..96].copy_from_slice(&nonce[..]);
    for field in [0, 16, 32] {
        req[field] ^= 1;
        let response = common::rpc(port, port, incarnation::TAG, &req)?;
        if common::number(&response) != rt::DENIED || response[8..] != [0; 120] {
            return Err(rt::REFUSED);
        }
        req[field] ^= 1;
    }
    let response = common::rpc(port, port, incarnation::TAG, &req)?;
    if common::number(&response) != rt::OK {
        return Err(rt::DENIED);
    }
    let signature = Signature::from_slice(&response[8..72]).map_err(|_| rt::REFUSED)?;
    public
        .verify_strict(&incarnation::transcript(&id, &nonce), &signature)
        .map_err(|_| rt::REFUSED)?;
    let replay = common::rpc(port, port, incarnation::TAG, &req)?;
    if common::number(&replay) != rt::DENIED || replay[8..] != [0; 120] {
        return Err(rt::REFUSED);
    }
    common::marker(b"NAUTH incarnation preuve invitee OK substitutions=3 rejeu=refuse");
    #[cfg(feature = "horloge-guest")]
    {
        let mut frame = [0; 128];
        frame[..32].copy_from_slice(&id);
        rt::send_for(port, incarnation::EXCHANGE, &frame, 2000)?;
        let response = rt::receive_for(port, 12000)?;
        if response.kind != 1 || response.len != 128 || response.tag != incarnation::EXCHANGE {
            return Err(rt::REFUSED);
        }
        match common::number(&response.payload) {
            rt::OK => common::marker(b"NAUTH registre canal confirme"),
            rt::REFUSED => common::marker(b"NAUTH registre canal refuse"),
            _ => return Err(rt::REFUSED),
        }
    }
    loop {
        match rt::receive_for(rt::config(1), 500) {
            Err(rt::TIMED_OUT) => (),
            _ => return Ok(()),
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(qualify().err().unwrap_or(0))
}
