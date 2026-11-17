//! Signataire de qualification, joignable exclusivement par le vérificateur.
#![no_std]
#![no_main]
mod common;
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use common::*;
use ed25519_dalek::{Signer, SigningKey};
use runtime as rt;

fn serve() -> rt::Result<()> {
    let key = SigningKey::from_bytes(include_bytes!(env!("NOVA_LAB_ALICE")));
    let mut target = [0u8; 32];
    for (i, bytes) in target.chunks_exact_mut(8).enumerate() {
        bytes.copy_from_slice(&rt::config(9 + i).to_le_bytes());
    }
    if target == [0; 32] {
        return Err(rt::DENIED);
    }
    let mut last = 0;
    loop {
        let m = match rt::receive_for(rt::config(7), 1000) {
            Err(rt::TIMED_OUT) => continue,
            other => other?,
        };
        let mut reply = [0; 128];
        let status = if m.kind != 1
            || m.len != 128
            || m.tag <= last
            || m.payload[8..40] != target
            || m.payload[72..] != [0; 56]
        {
            rt::DENIED
        } else {
            last = m.tag;
            match &m.payload[..8] {
                b"NSHPUB01" if m.payload[40..72] == [0; 32] => {
                    reply[8..40].copy_from_slice(key.verifying_key().as_bytes());
                    rt::OK
                }
                b"NSHSIG01" => {
                    reply[8..72].copy_from_slice(&key.sign(&m.payload[40..72]).to_bytes());
                    rt::OK
                }
                _ => rt::BAD_ARGUMENT,
            }
        };
        reply[..8].copy_from_slice(&status.to_le_bytes());
        rt::send_for(rt::config(8), m.tag, &reply, 5000)?;
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
