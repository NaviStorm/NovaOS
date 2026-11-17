//! Courtier de remise du profil QEMU, limité à ses deux canaux privés et à la supervision.
#![no_std]
#![no_main]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use runtime as rt;

fn serve() -> rt::Result<()> {
    let rx = rt::config(5);
    let tx = rt::config(6);
    if rx == 0 || tx == 0 {
        return Err(rt::DENIED);
    }
    // Ces appels font aussi partie du témoin dans la véritable instance.
    if rt::call(rt::IPC_CREATE, 0, 0, 0).status != rt::DENIED
        || rt::call(rt::IPC_DUP, rx, rt::RECEIVE.into(), 0).status != rt::DENIED
    {
        return Err(rt::REFUSED);
    }
    if [b"NXPKVIEW", b"NXPKBEG1", b"NSHSIG01"].iter().any(|op| {
        rt::send_for(tx, 1, *op, 200) != Err(rt::DENIED)
    }) {
        return Err(rt::REFUSED);
    }
    let marker = b"NXRELAY aucun droit de creation ou duplication";
    rt::call(
        rt::DEBUG_WRITE,
        marker.as_ptr() as u64,
        marker.len() as u64,
        0,
    );
    loop {
        match rt::receive_for(rt::config(1), 0) {
            Ok(m) if m.kind == 1 && m.len == 1 && m.payload[0] == 0 => return Ok(()),
            Err(rt::WOULD_BLOCK | rt::TIMED_OUT) => (),
            Ok(_) => return Err(rt::BAD_ARGUMENT),
            Err(e) => return Err(e),
        }
        let m = match rt::receive_for(rx, 10) {
            Err(rt::TIMED_OUT | rt::WOULD_BLOCK) => continue,
            r => r?,
        };
        // Le reçu est lié à cette instance par le noyau. Le seul champ présenté
        // par le courtier est la séquence; aucun PID, masque de droit ni handle.
        if option_env!("NOVA_RELAY_FAULT") == Some("avant") {
            rt::exit(41);
        }
        let decision = if m.kind == 1 && m.len == 8 && &m.payload[..8] == b"NXRELAY1" {
            rt::call(rt::IPC_NOTIFY, tx, m.tag, 0)
        } else {
            rt::Reply {
                status: rt::BAD_ARGUMENT,
                value: 0,
            }
        };
        if option_env!("NOVA_RELAY_FAULT") == Some("apres") {
            rt::exit(42);
        }
        // Seule la première présentation peut transférer une feuille. La
        // seconde retrouve la décision ou sa révocation si la session s'est fermée.
        if decision.status == rt::OK {
            let replay = rt::call(rt::IPC_NOTIFY, tx, m.tag, 0);
            if replay.status == rt::OK && replay.value != decision.value {
                return Err(rt::REFUSED);
            }
        }
        let mut response = [0; 16];
        response[..8].copy_from_slice(&decision.status.to_le_bytes());
        response[8..].copy_from_slice(&decision.value.to_le_bytes());
        rt::send_for(tx, m.tag, &response, 250)?;
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
