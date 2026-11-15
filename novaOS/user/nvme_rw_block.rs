//! Backend NVMe du service bloc. Le moteur NovaFS parle les mêmes trames
//! versionnées et utilise la même voie de pages privées qu'avec VirtIO.
#![no_std]
#![no_main]
#[path = "storage/block_service.rs"]
mod block_service;
mod nvme_diagnostic;
#[allow(dead_code)]
mod nvme_ipc;
#[allow(dead_code)]
mod nvme_protocol;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
#[path = "../abi/block.rs"]
mod wire;

use nvme_diagnostic::Cause;
use nvme_ipc::{Broker, check, reply_message, send_words};
use nvme_protocol as hw;
use runtime::{Result, config};

struct Backend {
    broker: Broker,
    seq: u64,
    sector: u64,
    sectors: u64,
    lane: u64,
    failed_cqe: Option<Cause>,
}

impl Backend {
    fn driver(&mut self, op: u64, ticket: u64) -> Result<u64> {
        self.seq = self.seq.checked_add(1).ok_or(runtime::LIMIT)?;
        send_words(config(hw::DRIVER_REQUEST), self.seq, op, [ticket, 0, 0])?;
        let msg = reply_message(config(hw::DRIVER_REPLY), self.seq)?;
        check(msg.len as usize == hw::REQUEST_SIZE)?;
        let code = hw::word(&msg.payload, 1);
        if code != runtime::OK {
            let cid = hw::word(&msg.payload, 3);
            let raw = hw::word(&msg.payload, 4);
            if code == protocol::IO {
                self.failed_cqe = Some(Cause::new(cid, raw).ok_or(runtime::BAD_ARGUMENT)?);
            } else {
                check(cid == 0 && raw == 0)?;
            }
            return Err(code);
        }
        Ok(hw::word(&msg.payload, 2))
    }

    fn open(page: &mut [u8; 4096]) -> Result<Self> {
        let mut b = Self {
            broker: Broker::new(),
            seq: 0,
            sectors: 0,
            sector: 0,
            lane: config(hw::PAGE_LANE),
            failed_cqe: None,
        };
        check(b.lane != 0)?;
        b.driver(nvme_ipc::INFO, 0)?;
        let info = b.broker.call(hw::GEOMETRY, [0; 3])?;
        check(hw::word(&info, 3) == hw::NSID)?;
        b.sectors = hw::word(&info, 4);
        b.sector = hw::word(&info, 5);
        check(matches!(b.sector, 512 | 4096) && b.sectors > 0)?;
        b.check_grants(page)?;
        Ok(b)
    }

    fn check_grants(&mut self, page: &mut [u8; 4096]) -> Result<()> {
        let epoch = config(hw::EPOCH);
        let rd = config(hw::BROKER_SEND);
        let wr = config(hw::WRITE_CAP);
        let fl = config(hw::FLUSH_CAP);
        for (cap, generation, op, args) in [
            (rd, epoch, hw::REGISTER_WRITE, [hw::NSID, 0, 1]),
            (rd, epoch, hw::REGISTER_FLUSH, [hw::NSID, 0, 0]),
            (rd, epoch - 1, hw::REGISTER, [hw::NSID, 0, 1]),
            (rd, epoch, hw::REGISTER, [hw::NSID + 1, 0, 1]),
            (rd, epoch, hw::REGISTER, [hw::NSID, self.sectors, 1]),
        ] {
            check(matches!(
                self.broker.call_on(cap, generation, op, args),
                Err(runtime::DENIED | runtime::BAD_ARGUMENT | runtime::REVOKED)
            ))?;
        }
        if wr != 0 {
            for (op, args) in [
                (hw::REGISTER_FLUSH, [hw::NSID, 0, 0]),
                (hw::REGISTER_WRITE, [hw::NSID, self.sectors, 1]),
                (hw::REGISTER_WRITE, [hw::NSID + 1, 0, 1]),
                (hw::REGISTER_WRITE, [hw::NSID, 0, 1 + 4096 / self.sector]),
            ] {
                check(matches!(
                    self.broker.call_on(wr, epoch, op, args),
                    Err(runtime::DENIED | runtime::BAD_ARGUMENT)
                ))?;
            }
            let args = [hw::NSID, 0, 1];
            let ticket = hw::word(
                &self.broker.call_on(wr, epoch, hw::REGISTER_WRITE, args)?,
                3,
            );
            // Une adresse hors espace utilisateur et un crédit libéré sont refusés avant SQE.
            check(
                self.broker
                    .call_on(wr, epoch, hw::STAGE, [ticket, u64::MAX, self.sector])
                    .is_err(),
            )?;
            let source = [ticket, page.as_ptr() as u64, self.sector];
            page.fill(0x5a);
            self.broker.call_on(wr, epoch, hw::STAGE, source)?;
            page.fill(0xa5);
            check(matches!(
                self.broker.call_on(wr, epoch, hw::STAGE, source),
                Err(runtime::DENIED)
            ))?;
            self.broker.call(hw::CANCEL, [ticket, 0, 0])?;
            self.driver(nvme_ipc::INVALID_TICKET, ticket)?;
        }
        if fl != 0 {
            check(
                self.broker
                    .call_on(fl, epoch, hw::REGISTER_WRITE, [hw::NSID, 0, 1])
                    .is_err(),
            )?;
        }
        nvme_ipc::debug(b"NVME RW droits generation plage taille et adresse verifies")
    }

    fn work(&mut self, tag: u64, f: &mut wire::Wire, page: &mut [u8; 4096]) -> Result<()> {
        let wr = config(hw::WRITE_CAP);
        let fl = config(hw::FLUSH_CAP);
        let epoch = config(hw::EPOCH);
        let op = f[1];
        match op {
            wire::INFO => {
                wire::put(f, 4, self.sector as u32);
                f[8..16].copy_from_slice(&self.sectors.to_le_bytes());
                wire::put(
                    f,
                    16,
                    1 | 8 | if wr != 0 { 2 } else { 0 } | if fl != 0 { 4 } else { 0 },
                );
            }
            wire::DISCOVER => {
                *f = wire::discovery(wire::number(f, 40) & wire::FEATURE_PAGE, true);
            }
            wire::CLOSE => {
                self.driver(nvme_ipc::STOP, 0)?;
            }
            wire::FLUSH if fl != 0 => {
                let t = hw::word(
                    &self
                        .broker
                        .call_on(fl, epoch, hw::REGISTER_FLUSH, [hw::NSID, 0, 0])?,
                    3,
                );
                check(self.driver(hw::SUBMIT_FLUSH, t)? == 0)?;
                self.broker.call(hw::RELEASE, [t, 0, 0])?;
            }
            wire::READ | wire::WRITE if f[2] == wire::PAGE_PHASE => {
                let write = op == wire::WRITE;
                if write && wr == 0 {
                    return Err(runtime::DENIED);
                }
                let n = hw::MAX_TRANSFER / self.sector;
                let lba = wire::lba(f);
                if lba >= self.sectors || n > self.sectors - lba {
                    return Err(runtime::BAD_ARGUMENT);
                }
                let args = [hw::NSID, lba, n];
                let t = if write {
                    runtime::bulk::take(self.lane, tag, page)?;
                    let t = hw::word(
                        &self.broker.call_on(wr, epoch, hw::REGISTER_WRITE, args)?,
                        3,
                    );
                    check(
                        hw::word(
                            &self.broker.call_on(
                                wr,
                                epoch,
                                hw::STAGE,
                                [t, page.as_ptr() as u64, hw::MAX_TRANSFER],
                            )?,
                            3,
                        ) == hw::MAX_TRANSFER,
                    )?;
                    if config(hw::MODE) == 1019 {
                        // Le contrôleur doit lire la source déjà figée. Le client
                        // et la relecture après redémarrage vérifient les vrais octets.
                        page.fill(0x6d);
                    }
                    t
                } else {
                    hw::word(&self.broker.call(hw::REGISTER, args)?, 3)
                };
                let done = self.driver(
                    if write {
                        hw::SUBMIT_WRITE
                    } else {
                        nvme_ipc::READ
                    },
                    t,
                )?;
                check(done == hw::MAX_TRANSFER)?;
                if !write {
                    check(
                        hw::word(
                            &self.broker.call(
                                hw::COLLECT_PAGE,
                                [t, page.as_mut_ptr() as u64, hw::MAX_TRANSFER],
                            )?,
                            3,
                        ) == hw::MAX_TRANSFER,
                    )?;
                    runtime::bulk::put(self.lane, tag, page)?;
                }
                self.broker.call(hw::RELEASE, [t, 0, 0])?;
            }
            _ => return Err(runtime::DENIED),
        }
        Ok(())
    }
}

#[repr(align(4096))]
struct Page([u8; 4096]);

fn serve() -> Result<()> {
    let (rx, tx) = (config(hw::FRONT_REQUEST), config(hw::FRONT_REPLY));
    let mut page = Page([0; 4096]);
    let mut b = Backend::open(&mut page.0)?;
    let mut last = 0;
    loop {
        let mut m = block_service::request(rx, tx, last, false)?;
        last = m.tag;
        match b.work(last, &mut m.payload, &mut page.0) {
            Ok(()) => runtime::send(tx, last, &m.payload)?,
            Err(status) => {
                let code = u8::try_from(status).unwrap_or(runtime::REFUSED as u8);
                if let Some(cause) = b.failed_cqe.take() {
                    let mut error = [0; 1 + nvme_diagnostic::LEN];
                    error[0] = code;
                    error[1..].copy_from_slice(&cause.encode());
                    cause.report(nvme_diagnostic::Layer::Block, code);
                    runtime::send(tx, last, &error)?;
                } else {
                    runtime::send(tx, last, &[code])?;
                }
                return Err(status);
            }
        }
        if m.payload[1] == wire::CLOSE {
            return Ok(());
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    runtime::exit(serve().err().unwrap_or(0))
}
