//! Pilote NVMe Q35 en Ring 3. La machine d’états ne manipule aucune adresse DMA.
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

fn refused(result: Result<[u8; hw::RESPONSE_SIZE]>) -> Result<()> {
    check(matches!(
        result,
        Err(runtime::DENIED | runtime::BAD_ARGUMENT | runtime::REFUSED | runtime::REVOKED)
    ))
}

fn initialize(broker: &mut Broker) -> Result<()> {
    check(config(hw::EPOCH) != 0 && config(hw::BROKER_SEND) != 0)?;
    check(
        runtime::call(
            runtime::IPC_DUP,
            config(hw::BROKER_SEND),
            runtime::SEND.into(),
            0,
        )
        .status
            == runtime::DENIED,
    )?;
    refused(broker.call_as(config(hw::EPOCH) - 1, hw::DISCOVER, [0; 3]))?;
    refused(broker.call(u64::MAX, [0; 3]))?;
    refused(broker.call(hw::REGISTER, [1, 0, 1]))?;
    let found = broker.call(hw::DISCOVER, [0; 3])?;
    check(hw::word(&found, 3) == 0x1b36 && hw::word(&found, 4) == 0x10)?;
    broker.call(hw::ENABLE, [0; 3])?;
    broker.wait(hw::READY)?;
    let ns = hw::NSID;
    for op in [
        hw::IDENTIFY_CONTROLLER,
        hw::LIST_NAMESPACES,
        hw::IDENTIFY_NAMESPACE,
        hw::NUMBER_QUEUES,
        hw::CREATE_CQ,
        hw::CREATE_SQ,
    ] {
        let mut args = [0; 3];
        if op == hw::IDENTIFY_NAMESPACE {
            refused(broker.call(op, [ns + 1, 0, 0]))?;
            args[0] = ns;
        }
        broker.call(op, args)?;
        let out = broker.wait(hw::POLL)?;
        match op {
            hw::IDENTIFY_CONTROLLER => {
                let serial = &out[hw::HEADER..hw::HEADER + 20];
                check(
                    hw::word(&out, 3) == 0x1b36
                        && hw::word(&out, 4) == 0x10400
                        && hw::word(&out, 6) == u64::from_le_bytes(*b"NOVA0190")
                        && u32::from_le_bytes(serial[8..12].try_into().unwrap())
                            == u32::from_le_bytes(*b"READ")
                        && serial[12..].iter().all(|&x| matches!(x, 0 | b' '))
                        && out[68..108].iter().any(|&x| x > b' '),
                )?;
            }
            hw::LIST_NAMESPACES => check(hw::word(&out, 3) == ns)?,
            hw::IDENTIFY_NAMESPACE => check(
                hw::word(&out, 3) == ns
                    && hw::word(&out, 4) > 0
                    && matches!(hw::word(&out, 5), 512 | 4096)
                    && hw::word(&out, 6) == u64::from_le_bytes(*b"NOVA0190"),
            )?,
            _ => (),
        }
    }
    debug(b"NVME pilote Ring3 Identify et files pretes")
}

fn serve() -> Result<()> {
    let mut broker = Broker::new();
    let rx = config(hw::DRIVER_REQUEST);
    let tx = config(hw::DRIVER_REPLY);
    let mut seq = 1;
    let (init, args) = next_request(rx, seq)?;
    check(init == INFO && args[0] | args[1] | args[2] == 0)?;
    initialize(&mut broker)?;
    send_reply(tx, seq, runtime::OK, [0; 3], &[])?;
    loop {
        seq = seq.checked_add(1).ok_or(runtime::LIMIT)?;
        let (op, [ticket, b, c]) = next_request(rx, seq)?;
        check(b == 0 && c == 0)?;
        match op {
            INVALID_TICKET => {
                refused(broker.call(hw::SUBMIT_READ, [ticket, 0, 0]))?;
                send_reply(tx, seq, runtime::OK, [0; 3], &[])?;
            }
            READ if ticket != 0 => {
                broker.call(hw::SUBMIT_READ, [ticket, 0, 0])?;
                if config(hw::MODE) == hw::DRIVER_DEATH {
                    return debug(b"NVME pilote sortie apres soumission");
                }
                if config(hw::MODE) == hw::REVOKE {
                    check(matches!(
                        broker.call(hw::POLL, [0; 3]),
                        Err(runtime::DENIED | runtime::REVOKED | runtime::CLOSED)
                    ))?;
                    return debug(b"NVME pilote POLL refuse apres revocation");
                }
                let status = match broker.wait(hw::POLL) {
                    Ok(result) => result,
                    Err(error) => {
                        send_reply(tx, seq, error, [0; 3], &[])?;
                        return Err(error);
                    }
                };
                let bytes = hw::word(&status, 3);
                check(bytes > 0 && bytes <= hw::MAX_TRANSFER)?;
                send_reply(tx, seq, runtime::OK, [bytes, 0, 0], &[])?;
            }
            STOP if ticket == 0 => {
                for op in [hw::DELETE_SQ, hw::DELETE_CQ] {
                    broker.call(op, [0; 3])?;
                    broker.wait(hw::POLL)?;
                }
                broker.call(hw::SHUTDOWN, [0; 3])?;
                broker.wait(hw::STOPPED)?;
                return send_reply(tx, seq, runtime::OK, [0; 3], &[]);
            }
            _ => return Err(runtime::BAD_ARGUMENT),
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    runtime::exit(serve().err().unwrap_or(0))
}
