//! Sonde de transport. Aucun secret d’authentification dans NovaFS.
use crate::{medium::Block, runtime as rt};
#[path = "cellule.rs"]
mod cellule;
use cellule::*;
#[path = "etat.rs"]
mod etat;

pub(super) fn run(block: &mut Block, sector: usize) -> rt::Result<()> {
    let maximum = etat::State {
        link: [255; 16],
        tx: u64::MAX,
        rx: u64::MAX,
        ack: u64::MAX - 1,
        sent: 447,
        length: 448,
        received: 448,
        closed: false,
    };
    let mut metadata = maximum.encode();
    if etat::State::decode(&metadata) != Ok(maximum)
        || etat::State::decode(&[0; 257]).is_ok()
        || etat::State::decode(&metadata[..55]).is_ok()
        || etat::State::decode(&etat::State::default().encode()) != Ok(etat::State::default())
    {
        return Err(rt::REFUSED);
    }
    metadata[4] = 2;
    if etat::State::decode(&metadata).is_ok() {
        return Err(rt::REFUSED);
    }
    let link = [43; 16];
    let mut page = [0u8; PAGE];
    let mut out = Cell {
        kind: OPEN,
        size: 0,
        link,
        sequence: 1,
        ack: 0,
    };
    out.encode(&mut page[..HEADER]).map_err(|_| rt::REFUSED)?;
    let min = Cell::decode(&page[..HEADER]).map_err(|_| rt::REFUSED)?;
    if min != out
        || Cell::decode(&page[..63]).is_ok()
        || Cell::decode(&page[..65]).is_ok()
        || Cell::decode(&page[..257]).is_ok()
    {
        return Err(rt::REFUSED);
    }
    page[4] = 2;
    if Cell::decode(&page[..HEADER]).is_ok() {
        return Err(rt::REFUSED);
    }
    page[4] = 1;
    let mut max = out;
    max.kind = DATA;
    max.size = MAX as u16;
    max.sequence = u64::MAX;
    max.ack = u64::MAX;
    max.encode(&mut page[..HEADER]).map_err(|_| rt::REFUSED)?;
    if decode_sector(&page[..sector]) != Ok(max) {
        return Err(rt::REFUSED);
    }
    max.size += 1;
    if max.encode(&mut page[..HEADER]).is_ok() {
        return Err(rt::REFUSED);
    }
    out.encode(&mut page[..HEADER]).map_err(|_| rt::REFUSED)?;
    let tx = 512 * PAGE as u64 / sector as u64;
    let rx = 513 * PAGE as u64 / sector as u64;
    let page = &mut page[..sector];
    block.transfer(page, tx, true)?;
    for _ in 0..8192 {
        block.transfer(page, rx, false)?;
        let input = decode_sector(page).map_err(|_| rt::REFUSED)?;
        if input.kind == BROKEN || input.link != link || input.ack != out.sequence {
            return Err(rt::REFUSED);
        }
        if input.kind == EMPTY {
            rt::call(rt::THREAD_YIELD, 0, 0, 0);
            continue;
        }
        if input.kind != DATA || input.sequence != out.ack + 1 {
            return Err(rt::REFUSED);
        }
        if &page[HEADER..HEADER + usize::from(input.size)] == b"NA24-pression" {
            block.transfer(page, rx, false)?;
            if decode_sector(page) != Ok(input) {
                return Err(rt::REFUSED);
            }
            out.ack = input.sequence;
            return saturate(block, tx, rx, page, out);
        }
        out.sequence += 1;
        out.ack = input.sequence;
        out.kind = DATA;
        out.size = input.size;
        out.encode(&mut page[..HEADER]).map_err(|_| rt::REFUSED)?;
        block.transfer(page, tx, true)?;
        // Une réponse NBD perdue peut faire relire la même cellule. Les octets ne doivent sortir qu'une fois.
        block.transfer(page, tx, true)?;
    }
    Err(rt::TIMED_OUT)
}

// L'autre extrémité attend le marqueur série avant de vider le socket noyau.
fn saturate(block: &mut Block, tx: u64, rx: u64, page: &mut [u8], mut out: Cell) -> rt::Result<()> {
    for index in 1u16..=1024 {
        page.fill(0);
        out.kind = DATA;
        out.size = MAX as u16;
        out.sequence += 1;
        out.encode(&mut page[..HEADER]).map_err(|_| rt::REFUSED)?;
        page[HEADER..HEADER + MAX].fill(index.to_le_bytes()[0]);
        block.transfer(page, tx, true)?;
        block.transfer(page, rx, false)?;
        let c = decode_sector(page).map_err(|_| rt::REFUSED)?;
        if c.kind != EMPTY || c.link != out.link {
            return Err(rt::REFUSED);
        }
        if c.ack == out.sequence {
            continue;
        }
        if c.ack != out.sequence - 1 {
            return Err(rt::REFUSED);
        }
        let message = alloc::format!("NAUTH saturation {index}");
        rt::call(
            rt::DEBUG_WRITE,
            message.as_ptr() as u64,
            message.len() as u64,
            0,
        );
        for _ in 0..8192 {
            block.transfer(page, rx, false)?;
            let c = decode_sector(page).map_err(|_| rt::REFUSED)?;
            if c.kind == BROKEN && c.size == 0 && c.link == [0; 16] {
                return Ok(());
            }
            if c.kind != EMPTY || !(out.sequence - 1..=out.sequence).contains(&c.ack) {
                return Err(rt::REFUSED);
            }
            rt::call(rt::THREAD_YIELD, 0, 0, 0);
        }
        break;
    }
    Err(rt::TIMED_OUT)
}
