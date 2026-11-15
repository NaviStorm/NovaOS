//! Service bloc de lecture. Seul ce processus peut faire enregistrer un ticket
//! ou recevoir les données du courtier. Le pilote ne transmet que son avancement.
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

struct Driver {
    seq: u64,
}
impl Driver {
    fn ask(&mut self, op: u64, arg: u64) -> Result<u64> {
        self.seq = self.seq.checked_add(1).ok_or(runtime::LIMIT)?;
        send_words(config(hw::DRIVER_REQUEST), self.seq, op, [arg, 0, 0])?;
        let m = reply(config(hw::DRIVER_REPLY), self.seq)?;
        check(m.len as usize == hw::REQUEST_SIZE)?;
        Ok(hw::word(&m.payload, 2))
    }
}

fn serve() -> Result<()> {
    let mut broker = Broker::new();
    let mut driver = Driver { seq: 0 };
    let (rx, tx) = (config(hw::FRONT_REQUEST), config(hw::FRONT_REPLY));
    driver.ask(INFO, 0)?;
    let info = broker.call(hw::GEOMETRY, [0; 3])?;
    let (ns, sectors, sector) = (hw::word(&info, 3), hw::word(&info, 4), hw::word(&info, 5));
    check(ns == hw::NSID && sectors > 0 && matches!(sector, 512 | 4096))?;

    // Une annulation avant le doorbell ferme le ticket sans modifier les files.
    let ticket = hw::word(&broker.call(hw::REGISTER, [ns, 0, 1])?, 3);
    check(ticket != 0)?;
    broker.call(hw::CANCEL, [ticket, 0, 0])?;
    driver.ask(INVALID_TICKET, ticket)?;
    let mut seq = 0u64;
    loop {
        seq = seq.checked_add(1).ok_or(runtime::LIMIT)?;
        let (op, args) = next_request(rx, seq)?;
        let [id, lba, count] = args;
        match op {
            INFO if args == [0; 3] => send_reply(tx, seq, runtime::OK, [ns, sectors, sector], &[])?,
            STOP if args == [0; 3] => {
                driver.ask(STOP, 0)?;
                return send_reply(tx, seq, runtime::OK, [0; 3], &[]);
            }
            READ if id == ns
                && lba < sectors
                && count > 0
                && count <= sectors - lba
                && count <= hw::MAX_TRANSFER / sector =>
            {
                let bytes = count * sector;
                let t = hw::word(&broker.call(hw::REGISTER, args)?, 3);
                check(t != 0)?;
                match driver.ask(READ, t) {
                    Ok(done) => check(done == bytes)?,
                    Err(error) => {
                        send_reply(tx, seq, error, [0; 3], &[])?;
                        return Err(error);
                    }
                }
                let mut at = 0;
                while at < bytes {
                    let n = (bytes - at).min(hw::CHUNK as u64);
                    let data = broker.call(hw::COLLECT, [t, at, n])?;
                    check(hw::word(&data, 3) == n)?;
                    // Libérer le tampon avant la dernière réponse. Aucune donnée de
                    // cette requête ne dépend alors d’une demande ultérieure du client.
                    if at + n == bytes {
                        broker.call(hw::RELEASE, [t, 0, 0])?;
                    }
                    send_reply(
                        tx,
                        seq,
                        runtime::OK,
                        [bytes, at, n],
                        &data[hw::HEADER..hw::HEADER + n as usize],
                    )?;
                    at += n;
                }
            }
            READ => send_reply(tx, seq, runtime::BAD_ARGUMENT, [0; 3], &[])?,
            _ => send_reply(tx, seq, runtime::DENIED, [0; 3], &[])?,
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    runtime::exit(serve().err().unwrap_or(0))
}
