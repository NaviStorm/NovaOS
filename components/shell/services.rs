//! Accès de phase 3 au protocole de contrôle. Le nom de cette entrée de
//! maintenance appartient à nsh ; il ne fait pas partie du protocole v1.
use core::sync::atomic::{AtomicU64, Ordering};
use nova_cli::{console, runtime as rt, tty};
#[path = "../../novaOS/user/services/protocol.rs"]
mod wire;

static TAG: AtomicU64 = AtomicU64::new(0);

fn request(op: u64, id: u64, arg: u64) -> rt::Result<wire::Frame> {
    let slot = if matches!(op, wire::START | wire::STOP | wire::RESTART) { 14 } else { 11 };
    let (tx, rx) = (rt::config(slot), rt::config(slot + 1));
    if tx == 0 || rx == 0 { return Err(rt::DENIED); }
    let prev = TAG.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| rt::LIMIT)?;
    let tag = prev + 1;
    rt::send_for(tx, tag, &wire::encode(&wire::request(op, id, arg)), 15000)?;
    let reply = rt::receive_for(rx, 15000)?;
    if reply.kind != 1 || reply.len != 128 || reply.tag != tag { return Err(rt::REFUSED); }
    let frame = wire::decode(&reply.payload).ok_or(rt::BAD_ARGUMENT)?;
    if frame[0] != wire::VERSION { return Err(rt::BAD_VERSION); }
    if frame[1] != rt::OK { return Err(frame[1]); }
    Ok(frame)
}

fn show(frame: &wire::Frame) -> rt::Result<()> {
    console::write(tty::OUTPUT, &alloc::format!(
        "service={} etat={} essai={} pid={} instance={:016x}/{:016x}/{:016x}/{:016x} valeur={} retraits={}\n",
        frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8], frame[9], frame[10], frame[14]))
}

pub fn run(args: &[&str]) -> rt::Result<()> {
    fn number(s: &str) -> rt::Result<u64> { s.parse().map_err(|_| rt::BAD_ARGUMENT) }
    if args == ["list"] {
        for index in 0..=12 {
            match request(wire::LIST, 0, index) {
                Ok(data) if index < 12 => show(&data)?,
                Err(rt::REFUSED) => return Ok(()),
                Ok(_) => return Err(rt::LIMIT),
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }
    let (op, id, arg) = match args {
        ["status", id] => (wire::STATUS, *id, 0),
        ["info", id] => (wire::QUERY, *id, 0),
        ["start", id] => (wire::START, *id, 0),
        ["stop", id] => (wire::STOP, *id, 0),
        ["restart", id] => (wire::RESTART, *id, 0),
        ["dep", id, index] => (wire::DEP, *id, number(index)?),
        _ => return Err(rt::BAD_ARGUMENT),
    };
    show(&request(op, number(id)?, arg)?)
}
