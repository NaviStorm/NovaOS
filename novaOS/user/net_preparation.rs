//! Instance EL0 de pré-attribution du banc réseau. Seule la préparation est demandée.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
use runtime::*;
#[cfg(net_ethernet_lab)]
#[allow(dead_code)]
mod virtio_net;
#[cfg(net_ethernet_lab)]
#[path = "virtio_net/ethernet_lab.rs"]
mod ethernet_lab;

fn demande(handle: u64, generation: u64) -> u64 {
    let mut msg = SendV1 {
        version: VERSION,
        size: 160,
        timeout: 0,
        tag: 1,
        len: 16,
        flags: 0,
        payload: [0; 128],
    };
    msg.payload[..8].copy_from_slice(&generation.to_le_bytes());
    msg.payload[8..16].copy_from_slice(&1u64.to_le_bytes());
    call(IPC_SEND, handle, (&raw const msg) as u64, 160).status
}
fn essai() -> Result<()> {
    let handle = config(1);
    let generation = config(2);
    #[cfg(net_ethernet_lab)]
    if config(0) == 3 {
        return ethernet_lab::executer(handle, generation);
    }
    if config(0) == 2 {
        return if demande(handle, generation) == DENIED {
            Ok(())
        } else {
            Err(REFUSED)
        };
    }
    if handle == 0 || generation == 0 || call(IPC_DUP, handle, u64::from(SEND), 0).status != DENIED
    {
        return Err(REFUSED);
    }
    let ancien = demande(handle, generation - 1);
    if ancien != OK {
        return Err(ancien);
    }
    for phase in 1..=2 {
        // Synchronisation sur le mot de retour privé. Le superviseur conserve
        // la décision et sa séquence, même si l'utilisateur écrit ses propres données.
        let mut termine = false;
        for _ in 0..100_000 {
            if config(3) == phase {
                termine = true;
                break;
            }
            call(THREAD_YIELD, 0, 0, 0);
        }
        if !termine {
            return Err(TIMED_OUT);
        }
        if phase == 1 {
            if config(4) != DENIED {
                return Err(REFUSED);
            }
            let r = demande(handle, generation);
            if r != OK {
                return Err(r);
            }
        } else if config(4) != OK {
            return Err(REFUSED);
        }
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(essai().err().unwrap_or(0))
}
