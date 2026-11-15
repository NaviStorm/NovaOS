//! Client du volume témoin. Il compare chaque octet reçu avant de demander l’arrêt.
#![no_std]
#![no_main]
#[allow(dead_code)]
mod nvme_ipc;
#[allow(dead_code)]
mod nvme_protocol;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
use nvme_ipc::*;
use nvme_protocol as hw;
use runtime::{Result, config};

fn expected(i: u64) -> u8 {
    (i.wrapping_mul(37) + (i >> 8) * 13 + (i >> 16) * 17 + 91) as u8
}

fn fingerprint(value: u64) -> Result<()> {
    let mut line = *b"NVME READ fnv64=0000000000000000";
    let digits = b"0123456789abcdef";
    let start = line.len() - 16;
    for i in 0..16 {
        line[start + i] = digits[((value >> (4 * (15 - i))) & 15) as usize];
    }
    debug(&line)
}

fn qualify() -> Result<()> {
    check(config(hw::BROKER_SEND) == 0 && config(hw::BROKER_REPLY) == 0)?;
    let mut forbidden = Broker::new();
    check(matches!(
        forbidden.call(hw::DISCOVER, [0; 3]),
        Err(runtime::DENIED)
    ))?;
    let (tx, rx) = (config(hw::FRONT_REQUEST), config(hw::FRONT_REPLY));
    let mut seq = 1u64;
    send_words(tx, seq, INFO, [0; 3])?;
    let info = reply(rx, seq)?;
    let [ns, sectors, sector] = [2, 3, 4].map(|i| hw::word(&info.payload, i));
    check(ns == hw::NSID && matches!(sector, 512 | 4096) && sectors == 1_048_576 / sector)?;
    for (op, args, status) in [
        (WRITE_REFUSED, [ns, 0, 1], runtime::DENIED),
        (FLUSH_REFUSED, [ns, 0, 0], runtime::DENIED),
        (READ, [ns + 1, 0, 1], runtime::BAD_ARGUMENT),
        (READ, [ns, sectors, 1], runtime::BAD_ARGUMENT),
        (READ, [ns, 0, 0], runtime::BAD_ARGUMENT),
        (READ, [ns, 0, 4096 / sector + 1], runtime::BAD_ARGUMENT),
        (READ, [ns, u64::MAX, 2], runtime::BAD_ARGUMENT),
    ] {
        seq += 1;
        send_words(tx, seq, op, args)?;
        check(hw::word(&reply_message(rx, seq)?.payload, 1) == status)?;
    }
    for (lba, count) in [(0, 1), (sectors - 1, 1), (sectors / 2, 4096 / sector)] {
        seq += 1;
        send_words(tx, seq, READ, [ns, lba, count])?;
        if config(hw::MODE) == hw::CLIENT_DEATH {
            return debug(b"NVME client sortie apres requete de lecture");
        }
        if config(hw::MODE) == hw::TIMEOUT_TEST {
            check(hw::word(&reply_message(rx, seq)?.payload, 1) == runtime::TIMED_OUT)?;
            return debug(b"NVME client lecture expiree aucun octet livre");
        }
        let mut offset = 0u64;
        let mut hash = 0xcbf29ce484222325u64;
        let bytes = count * sector;
        while offset < bytes {
            let msg = reply(rx, seq)?;
            let n = hw::word(&msg.payload, 4);
            check(
                hw::word(&msg.payload, 2) == bytes
                    && hw::word(&msg.payload, 3) == offset
                    && n > 0
                    && n <= hw::CHUNK as u64
                    && n <= bytes - offset
                    && msg.len as u64 == hw::REQUEST_SIZE as u64 + n,
            )?;
            for (j, &v) in msg.payload[hw::REQUEST_SIZE..hw::REQUEST_SIZE + n as usize]
                .iter()
                .enumerate()
            {
                hash = (hash ^ u64::from(v)).wrapping_mul(0x100000001b3);
                if v != expected(sector * lba + offset + j as u64) {
                    return Err(runtime::REFUSED);
                }
            }
            offset += n;
        }
        fingerprint(hash)?;
    }
    debug(b"NVME client READ premier dernier page octets exacts refus ecriture namespace bornes")?;
    seq += 1;
    send_words(tx, seq, STOP, [0; 3])?;
    reply(rx, seq)?;
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    runtime::exit(qualify().err().unwrap_or(0))
}
