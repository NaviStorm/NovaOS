//! Pilote du seul modèle QEMU EDU. Le superviseur fournit trois handles locaux.
//! L'interface de commande utilise les trames IPC de NovaOS v1.
#![no_std]
#![no_main]
#[allow(dead_code)]
mod edu_protocol;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
use edu_protocol::*;
use runtime::*;

fn read_reg(offset: usize) -> u32 {
    // Sûreté : offsets fixes du témoin ; une lecture interdite termine ce processus.
    unsafe { ((PAGE + offset) as *const u32).read_volatile() }
}

fn send_on(handle: u64, op: u64, arg: u64) -> Result<()> {
    let mut payload = [0u8; 16];
    payload[..8].copy_from_slice(&config(2).to_le_bytes());
    payload[8..].copy_from_slice(&arg.to_le_bytes());
    let msg = SendV1 {
        version: VERSION,
        size: 160,
        timeout: 0,
        tag: op,
        len: 16,
        flags: 0,
        payload: {
            let mut buf = [0; 128];
            buf[..16].copy_from_slice(&payload);
            buf
        },
    };
    value(call(IPC_SEND, handle, (&raw const msg) as u64, 160))?;
    Ok(())
}
fn submit(op: u64, arg: u64) -> Result<()> {
    send_on(config(3), op, arg)
}
fn command(op: u64, arg: u64) -> Result<bool> {
    submit(op, arg)?;
    let response = receive(config(4))?;
    if response.kind != 2 || !matches!(response.aux, ACCEPTED | REJECTED) {
        return Err(REFUSED);
    }
    Ok(response.aux == ACCEPTED)
}
fn require(ok: bool) -> Result<()> {
    if ok { Ok(()) } else { Err(REFUSED) }
}
fn debug(s: &[u8]) -> Result<()> {
    value(call(DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0)).map(|_| ())
}
fn signal(i: usize) -> u64 {
    unsafe { core::sync::atomic::AtomicU64::from_ptr((0x401000 as *mut u64).add(i)) }
        .load(core::sync::atomic::Ordering::Acquire)
}
fn mark(word: usize, value: u64) {
    // Sûreté : mot aligné de DATA, propre au pilote ; le superviseur le lit atomiquement.
    unsafe { core::sync::atomic::AtomicU64::from_ptr((0x401000 as *mut u64).add(word)) }
        .store(value, core::sync::atomic::Ordering::Release);
}

fn test() -> Result<()> {
    require(signal(1) == 1)?;
    let mode = config(0);
    if mode == REFUSE {
        require(send_on(config(7), MASK, 0) == Err(DENIED))?;
        return Err(REFUSED);
    }
    require(read_reg(0) == 0x010000ed)?;
    // Les notifications ne sont pas forgeables par le port de réception.
    require(call(IPC_NOTIFY, config(5), 1, 0).status == DENIED)?;
    require(call(IPC_DUP, config(3), u64::from(SEND), 0).status == DENIED)?;
    require(call(DEBUG_WRITE, PAGE as u64, 4, 0).status == BAD_ADDRESS)?;
    if config(7) != 0 {
        require(send_on(config(7), MASK, 0) == Err(DENIED))?;
    }
    match mode {
        NORMAL => {
            let nonce = (config(2) as u32).wrapping_mul(0x45d9f3b);
            require(command(LIVE, u64::from(nonce))? && read_reg(4) == !nonce)?;
            require(!command(99, 0)? && !command(ACK, 0)?)?;
            for seq in 1..=2 {
                require(command(MASK, 0)? && command(RAISE, 0)? && command(RAISE, 0)?)?;
                require(
                    read_reg(0x24) == 1 && receive_for(config(5), 0).err() == Some(WOULD_BLOCK),
                )?;
                require(command(UNMASK, 0)?)?;
                let msg = receive(config(5))?;
                require(msg.kind == 2 && msg.aux == seq && !command(ACK, seq + 1)?)?;
                require(command(ACK, seq)? && read_reg(0x24) == 0 && !command(ACK, seq)?)?;
            }
            debug(b"EDU ID=010000ed liveness=ok IRQ=2 acquits=2")?;
        }
        WRITE_RO => unsafe { ((PAGE + 4) as *mut u32).write_volatile(0x1234) },
        OUTSIDE => {
            core::hint::black_box(read_reg(4096));
        }
        REVOKE | PENDING => {
            if mode == PENDING {
                require(command(UNMASK, 0)? && command(RAISE, 0)?)?;
                require(receive(config(5))?.aux == 1 && read_reg(0x24) == 1)?;
            }
            mark(6, 1);
            if mode == PENDING {
                // Le superviseur retire les trois handles avant de publier le signal.
                while signal(8) == 0 {
                    core::hint::spin_loop();
                }
                require(submit(MASK, 0) == Err(DENIED))?;
            }
            loop {
                core::hint::black_box(read_reg(0));
            }
        }
        ACK_FIRST | REVOKE_FIRST | IRQ_FAULT => {
            require(command(UNMASK, 0)? && command(RAISE, 0)?)?;
            let notice = receive(config(5))?;
            require(notice.kind == 2 && notice.aux == 1 && read_reg(0x24) == 1)?;
            if mode == IRQ_FAULT {
                core::hint::black_box(read_reg(4096));
                return Err(REFUSED);
            }
            submit(ACK, notice.aux)?;
            mark(6, 1);
            while signal(8) == 0 {
                core::hint::spin_loop();
            }
            require(submit(MASK, 0) == Err(DENIED))?;
            core::hint::black_box(read_reg(0));
        }
        IRQ_DURING_WAIT => {
            require(command(UNMASK, 0)?)?;
            mark(6, 1);
            let msg = receive(config(5))?;
            require(msg.kind == 2 && msg.aux == 1 && command(ACK, 1)? && read_reg(0x24) == 0)?;
        }
        SILENT => {
            mark(6, 1);
            loop {
                core::hint::spin_loop();
            }
        }
        _ => return Err(BAD_ARGUMENT),
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    match test() {
        Ok(()) => exit(0),
        Err(e) => exit(100 + e),
    }
}
