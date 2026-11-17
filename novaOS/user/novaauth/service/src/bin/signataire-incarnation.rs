#![no_std]
#![no_main]
#[cfg(feature = "horloge-guest")]
#[path = "common/canal.rs"]
mod canal;
mod common;
#[path = "common/incarnation.rs"]
mod incarnation;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use ed25519_dalek::{Signer, SigningKey};
use runtime as rt;

#[cfg(feature = "horloge-guest")]
fn report_time(start: u64, until: u64, end: u64) {
    let mut line =
        *b"NAUTH TLS debut=0000000000000000 echeance=0000000000000000 fin=0000000000000000";
    for (slot, time) in [(16, start), (42, until), (63, end)] {
        for i in 0..16 {
            line[slot + i] = b"0123456789abcdef"[((time >> (60 - 4 * i)) & 15) as usize];
        }
    }
    common::marker(&line);
}
#[cfg(feature = "horloge-guest")]
fn exchange(key: &SigningKey, id: &[u8; 32]) -> rt::Result<()> {
    match canal::probe(key, id) {
        Ok(()) => common::marker(b"NAUTH canal authentifie"),
        Err(e) => {
            struct Log {
                b: [u8; 96],
                n: usize,
            }
            impl core::fmt::Write for Log {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    let n = (self.b.len() - self.n).min(s.len());
                    self.b[self.n..self.n + n].copy_from_slice(&s.as_bytes()[..n]);
                    self.n += n;
                    Ok(())
                }
            }
            use core::fmt::Write;
            let mut l = Log { b: [0; 96], n: 0 };
            let _ = write!(l, "NAUTH diagnostic {:?}", e);
            common::marker(&l.b[..l.n]);
            common::marker(b"NAUTH canal refuse");
            return Err(rt::REFUSED);
        }
    }
    Ok(())
}
fn serve() -> rt::Result<()> {
    let id = incarnation::identity();
    let port = rt::config(8);
    if id == [0; 32] || port == 0 {
        return Err(rt::REFUSED);
    }
    let key = SigningKey::from_bytes(include_bytes!(env!("NOVA_QEMU_SERVER_SEED")));
    // Une preuve de reprise au plus pour cette instance du signataire de qualification.
    let mut issued = false;
    #[cfg(feature = "horloge-guest")]
    let mut exchange_done = false;
    loop {
        let m = match rt::receive_for(port, 250) {
            Err(rt::TIMED_OUT) => match rt::receive_for(rt::config(1), 0) {
                Err(rt::WOULD_BLOCK) => continue,
                _ => return Ok(()),
            },
            other => other?,
        };
        let b = &m.payload;
        let mut result = [0u8; 128];
        let allowed = !issued
            && m.len == 128
            && m.kind == 1
            && m.tag == incarnation::TAG
            && b[..16] == incarnation::SERVER
            && b[16..32] == incarnation::DOMAIN
            && b[32..64] == id
            && b[64..96] != [0; 32]
            && b[96..] == [0; 32];
        #[cfg(feature = "horloge-guest")]
        if issued
            && !exchange_done
            && m.tag == incarnation::EXCHANGE
            && m.kind == 1
            && m.len == 128
            && b[..32] == id
            && b[32..] == [0; 96]
        {
            exchange_done = true;
            let status = exchange(&key, &id).err().unwrap_or(rt::OK);
            result[..8].copy_from_slice(&status.to_le_bytes());
            rt::send_for(port, m.tag, &result, 5000)?;
            continue;
        }
        let status = if allowed {
            let nonce = b[64..96].try_into().map_err(|_| rt::BAD_ARGUMENT)?;
            let sig = key.sign(&incarnation::transcript(&id, nonce));
            issued = true;
            result[8..72].copy_from_slice(&sig.to_bytes());
            rt::OK
        } else {
            rt::DENIED
        };
        result[..8].copy_from_slice(&status.to_le_bytes());
        rt::send_for(port, m.tag, &result, 5000)?;
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
