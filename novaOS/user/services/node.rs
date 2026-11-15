//! Boucle des ELF supervisés. Chaque message de cycle est lié aux 32 octets
//! d'identité du processus et à l'essai remis par le lanceur.
use crate::{protocol as p, runtime as rt};
#[allow(dead_code)]
#[path = "../../abi/files.rs"]
mod fs;
#[allow(dead_code)]
#[path = "../../abi/instances.rs"]
mod instance;
#[allow(dead_code)]
#[path = "../../abi/tmp.rs"]
mod tmp;

fn root_empty(cap: u64) -> rt::Result<()> {
    let mut dir = fs::frame(fs::READDIR, 1, 0);
    rt::value(rt::call(
        tmp::CALL,
        cap,
        dir.as_mut_ptr() as u64,
        dir.len() as u64,
    ))?;
    if fs::length(&dir) == 0 {
        Ok(())
    } else {
        Err(rt::REFUSED)
    }
}

pub fn worker(mode: u64) -> rt::Result<()> {
    let cap = rt::value(rt::call(tmp::CAP, 0, 0, 0));
    match (mode, cap) {
        (100, Err(rt::DENIED)) => Ok(()),
        (101, Ok(cap)) => root_empty(cap),
        _ => Err(rt::REFUSED),
    }
}

fn verify_delegation(identity: &[u8; 32]) -> rt::Result<()> {
    for (call, mode) in [(instance::WORKER, 100), (tmp::WORKER, 101)] {
        let child = rt::value(rt::call(call, identity.as_ptr() as u64, mode, 32))?;
        let mut result = [0u64; 4];
        rt::value(rt::call(
            rt::PROCESS_WAIT,
            child,
            result.as_mut_ptr() as u64,
            32,
        ))?;
        if result[2] != 0 || result[3] != 0 {
            return Err(rt::REFUSED);
        }
    }
    Ok(())
}

pub fn crash() -> ! {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("ud2", options(noreturn));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("brk #0", options(noreturn));
    }
}

pub fn serve(storage: bool, mut info: impl FnMut(u64, &[u8]) -> [u8; 12]) -> rt::Result<()> {
    let (tx, rx, id, attempt) = (rt::config(0), rt::config(1), rt::config(2), rt::config(4));
    if attempt == 0
        || !(1..=9).contains(&id)
        || storage != (id == 9)
        || rt::value(rt::call(rt::IPC_BOOTSTRAP, 0, 0, 0))? != rx
    {
        return Err(rt::DENIED);
    }
    let cap = rt::value(rt::call(tmp::CAP, 0, 0, 0))?;
    root_empty(cap)?;
    let mut identity = [0u8; 32];
    rt::value(rt::call(
        instance::INFO,
        2,
        identity.as_mut_ptr() as u64,
        32,
    ))?;
    if id == 2 {
        verify_delegation(&identity)?;
        let msg = b"SVC tmp prive ordinaire refuse et delegation verifiee";
        rt::call(rt::DEBUG_WRITE, msg.as_ptr() as u64, msg.len() as u64, 0);
    }
    let mut stamp = p::request(p::ANNOUNCE, id, attempt);
    for (out, bytes) in stamp[4..8].iter_mut().zip(identity.chunks_exact(8)) {
        *out = u64::from_le_bytes(bytes.try_into().unwrap());
    }
    if rt::call(rt::IPC_DUP, tx, rt::SEND.into(), 0).status != rt::DENIED
        || rt::call(rt::IPC_DUP, rx, rt::RECEIVE.into(), 0).status != rt::DENIED
        || rt::call(
            instance::SERVICE_PROCESS,
            crate::service_process::DRAIN,
            0,
            0,
        )
        .status
            != rt::DENIED
    {
        return Err(rt::REFUSED);
    }
    let mut outside = fs::frame(fs::OPEN_AT, 1, 0);
    let name = b"/documents/exemple.txt";
    fs::size(&mut outside, name.len());
    outside[2] = fs::READABLE;
    outside[fs::PAYLOAD..fs::PAYLOAD + name.len()].copy_from_slice(name);
    if rt::call(
        tmp::CALL,
        cap,
        outside.as_mut_ptr() as u64,
        outside.len() as u64,
    )
    .status
        != rt::DENIED
    {
        return Err(rt::REFUSED);
    }
    if id == 1 {
        rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0))?;
    }
    if storage {
        for (tag, operation) in [(1, rt::INFO), (2, rt::FIN)] {
            let reply = info(tag, &[rt::WIRE_VERSION, operation, 0, 0, 0, 0, 0, 0]);
            if reply[1] != 0 || &reply[8..12] != rt::IDENTITY {
                return Err(rt::REFUSED);
            }
        }
    }
    let emit = |op, arg, status, tag| {
        let mut frame = stamp;
        frame[1] = op;
        frame[8] = arg;
        frame[9] = status;
        rt::send_frame(tx, tag, &p::encode(&frame), 0, false)
    };
    // Ces deux identifiants ne sont disponibles que dans le catalogue de qualification.
    if id != 4 {
        emit(p::ANNOUNCE, 0, 0, 1)?;
    }
    if id == 5 {
        crash();
    }
    let mut silent = false;
    let mut delayed_sample = None;
    loop {
        let msg = match rt::receive_for(rx, 25) {
            Ok(msg) => msg,
            Err(rt::TIMED_OUT) => {
                if let Some(tag) = delayed_sample.take() {
                    let status = root_empty(cap).err().unwrap_or(rt::OK);
                    emit(p::VALUE, 0, status, tag)?;
                }
                if id != 4 && !silent {
                    if rt::call(tmp::CAP, 0, 0, 0).status != rt::OK {
                        emit(p::LOST, 0, 0, 1)?;
                        return Err(rt::REVOKED);
                    }
                    match emit(p::PULSE, 0, 0, 1) {
                        Ok(()) | Err(rt::WOULD_BLOCK) => (),
                        Err(e) => return Err(e),
                    }
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        if msg.kind != 1 {
            continue;
        }
        if storage && msg.len == 8 {
            let ans = info(msg.tag, &msg.payload[..8]);
            rt::send_frame(tx, msg.tag, &ans, 0, false)?;
            continue;
        }
        let Some(frame) = p::decode(&msg.payload[..msg.len as usize]) else {
            continue;
        };
        if frame[0] != p::VERSION || frame[2..8] != stamp[2..8] {
            continue;
        }
        match frame[1] {
            p::HALT if id != 6 => return Ok(()),
            // Le témoin rend l'échantillon à l'échéance suivante, ce qui exerce
            // une réponse différée sur un vrai canal, même avec un seul vCPU.
            p::SAMPLE if id == 7 => delayed_sample = Some(msg.tag),
            p::SAMPLE => {
                let result = if id == 1 {
                    rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0))
                } else {
                    root_empty(cap).map(|()| 0)
                };
                let (value, status) = match result {
                    Ok(n) => (n, 0),
                    Err(e) => (0, e),
                };
                emit(p::VALUE, value, status, msg.tag)?;
            }
            p::DISTURB if (3..=8).contains(&id) => match frame[8] {
                1 => return Ok(()),
                2 => crash(),
                3 => {
                    rt::value(rt::call(rt::IPC_CLOSE, rx, 0, 0))?;
                    let mut lost = stamp;
                    lost[1] = p::LOST;
                    rt::send_for(tx, 1, &p::encode(&lost), 250)?;
                    return Err(rt::REVOKED);
                }
                4 => silent = true,
                _ => (),
            },
            _ => (),
        }
    }
}
