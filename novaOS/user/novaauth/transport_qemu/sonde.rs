//! Échange opaque préalable au raccord de sécurité. Aucun reçu ni droit émis.
use crate::{medium::Block, runtime as rt};

fn tx(block: &mut Block, page: &mut [u8], seq: u64, kind: u8, bytes: &[u8]) -> rt::Result<()> {
    page.fill(0);
    page[..4].copy_from_slice(b"NA24");
    page[4] = 1;
    page[5] = kind;
    page[6..8].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
    // Incarnation de témoin sur un export neuf. Sans fonction cryptographique.
    page[8..24].fill(43);
    page[24..32].copy_from_slice(&seq.to_le_bytes());
    page[64..64 + bytes.len()].copy_from_slice(bytes);
    block.transfer(page, 512 * 4096 / page.len() as u64, true)
}

pub(super) fn run(block: &mut Block, sector: usize) -> rt::Result<()> {
    let mut page = [0u8; 4096];
    let page = &mut page[..sector];
    tx(block, page, 1, 1, &[])?;
    tx(block, page, 2, 2, b"aller\x00\x09\xff")?;
    for _ in 0..32 {
        block.transfer(page, 513 * 4096 / sector as u64, false)?;
        if &page[..4] != b"NA24" || page[4] != 1 || page[8..24] != [43;16] {
            return Err(rt::REFUSED);
        }
        if page[5] == 2 && u16::from_le_bytes([page[6], page[7]]) == 9
            && &page[64..73] == b"retour\xff\x09\0"
            && u64::from_le_bytes(page[32..40].try_into().unwrap()) == 2 {
            return Ok(());
        }
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
    Err(rt::TIMED_OUT)
}
