//! Pilote du banc VT-d. Seuls les handles privés transmis à ce PID permettent
//! de demander les deux transferts. Le message ne transporte aucune adresse.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
use runtime::{Result, call, config, exit, receive, value};
mod edu_dma_protocol;
use edu_dma_protocol as wire;

fn require(ok: bool) -> Result<()> {
    if ok { Ok(()) } else { Err(runtime::REFUSED) }
}

fn debug(bytes: &[u8]) -> Result<()> {
    value(call(
        runtime::DEBUG_WRITE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    ))
    .map(|_| ())
}

fn submit(epoch: u64, op: u64) -> Result<()> {
    let mut msg = runtime::SendV1 {
        version: runtime::VERSION,
        size: 160,
        timeout: 0,
        tag: op,
        len: 16,
        flags: 0,
        payload: [0; 128],
    };
    msg.payload[..8].copy_from_slice(&epoch.to_le_bytes());
    value(call(
        runtime::IPC_SEND,
        config(2),
        (&raw const msg) as u64,
        160,
    ))
    .map(|_| ())
}

fn response(expected: u64) -> Result<()> {
    let msg = receive(config(3))?;
    require(msg.kind == 2 && msg.aux == expected)
}

fn witness_valid() -> bool {
    // Sûreté : la seule image WITNESS reçoit cette page RO du lanceur.
    (0..4096)
        .all(|offset| unsafe { ((0x600000usize + offset) as *const u8).read_volatile() == 0xc3 })
}

fn run_driver() -> Result<()> {
    let mode = config(0);
    let epoch = config(1);
    if mode == wire::REFUSED {
        require(submit(epoch, wire::READ) == Err(runtime::DENIED))?;
        return debug(b"EDU DMA Ring3 sans capacite refuse");
    }
    require(epoch != 0 && config(2) != 0 && config(3) != 0)?;
    require(call(runtime::IPC_DUP, config(2), runtime::SEND.into(), 0).status == runtime::DENIED)?;
    require(call(runtime::IPC_NOTIFY, config(3), 1, 0).status == runtime::DENIED)?;
    if mode == wire::CONTENDER {
        submit(epoch, wire::PREPARE)?;
        response(wire::REJECTED)?;
        return debug(b"EDU DMA Ring3 domaine concurrent refuse");
    }
    if mode != wire::WITNESS {
        submit(epoch, wire::PREPARE)?;
        response(wire::ACCEPTED)?;
    }
    match mode {
        wire::NORMAL => {
            submit(epoch, wire::PREPARE)?;
            response(wire::REJECTED)?;
            submit(epoch, u64::MAX)?;
            response(wire::REJECTED)?;
            submit(epoch - 1, wire::READ)?;
            response(wire::REJECTED)?;
            submit(epoch, wire::READ)?;
            response(wire::ACCEPTED)?;
            submit(epoch, wire::WRITE)?;
            response(wire::ACCEPTED)?;
            debug(b"EDU DMA Ring3 lecture et ecriture confirmees")
        }
        wire::LATE => {
            submit(epoch, wire::READ)?;
            // Le banc confirme l'extraction et attend notre sortie avant le doorbell.
            response(wire::ACCEPTED)?;
            Ok(())
        }
        wire::DEATH => {
            submit(epoch, wire::START_INFLIGHT)?;
            response(wire::ACCEPTED)?;
            debug(b"EDU DMA Ring3 sortie volontaire apres soumission")
        }
        wire::REVOKE => {
            submit(epoch, wire::START_INFLIGHT)?;
            response(wire::ACCEPTED)?;
            submit(epoch, wire::RETIRE_READY)?;
            response(wire::DRAINED)?;
            require(submit(epoch, wire::READ) == Err(runtime::DENIED))?;
            debug(b"EDU DMA Ring3 ancienne capacite refusee")
        }
        wire::WITNESS => {
            require(witness_valid())?;
            submit(epoch, wire::PROBE_READY)?;
            response(wire::ACCEPTED)?;
            require(witness_valid())?;
            debug(b"EDU DMA Ring3 page temoin privee 4096 octets intacts")
        }
        wire::FAULT_PROBE => {
            submit(epoch, wire::PROBE_READY)?;
            response(wire::ACCEPTED)?;
            debug(b"EDU DMA Ring3 sonde terminee")
        }
        _ => Err(runtime::BAD_ARGUMENT),
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    match run_driver() {
        Ok(()) => exit(0),
        Err(error) => exit(100 + error),
    }
}
