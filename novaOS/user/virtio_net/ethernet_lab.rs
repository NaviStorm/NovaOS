//! Instance privée du pilote. Seule la copie dans la page RW/NX est visible.
use super::{runtime::*, virtio_net};
const COMPTE: usize = 16;
const NIC: [u8; 6] = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
const PAIR: [u8; 6] = [0x02, 0x4e, 0x56, 0, 0, 1];

fn commande(handle: u64, generation: u64, seq: u64, tag: u64, len: usize) -> Result<usize> {
    let mut msg = SendV1 {
        version: VERSION,
        size: 160,
        timeout: 0,
        tag,
        len: 16,
        flags: 0,
        payload: [0; 128],
    };
    msg.payload[..8].copy_from_slice(&generation.to_le_bytes());
    msg.payload[8..16].copy_from_slice(&seq.to_le_bytes());
    // Page privée du titulaire. Le courtier borne la longueur lors de la copie.
    unsafe { (0x401030 as *mut u64).write_volatile(len as u64) };
    value(call(IPC_SEND, handle, (&raw const msg) as u64, 160))?;
    for _ in 0..100_000 {
        if config(3) == seq {
            let status = config(4);
            return if status == OK {
                Ok(config(5) as usize)
            } else {
                Err(status)
            };
        }
        call(THREAD_YIELD, 0, 0, 0);
    }
    Err(TIMED_OUT)
}
fn octet(n: usize, i: usize) -> u8 {
    (n.wrapping_mul(29).wrapping_add(i.wrapping_mul(17))) as u8
}
pub(super) fn executer(handle: u64, generation: u64) -> Result<()> {
    if handle == 0 || generation == 0 || call(IPC_DUP, handle, u64::from(SEND), 0).status != DENIED
    {
        return Err(REFUSED);
    }
    // L’image ELF de qualification s’exécute en mono-thread avec une page de données privée.
    let slot = unsafe { core::slice::from_raw_parts_mut(0x401100 as *mut u8, 1526) };
    let mut trame = [0u8; 1514];
    let mut seq = 1;
    commande(handle, generation, seq, 1, 0)?;
    let lots = config(7) as usize;
    if !(1..=6250).contains(&lots) {return Err(REFUSED);}
    for lot in 0..lots {
    for i in 0..COMPTE {
        let n = lot * COMPTE + i;
        let len = if n % 2 == 0 { 60 } else { 1514 };
        trame[..6].copy_from_slice(&PAIR);
        trame[6..12].copy_from_slice(&NIC);
        trame[12..14].copy_from_slice(&[0x88, 0xb5]);
        trame[14] = n as u8;
        for (i, b) in trame[15..len].iter_mut().enumerate() {
            *b = octet(n, i);
        }
        if lots > 1 {trame[14..18].copy_from_slice(&(n as u32).to_be_bytes());}
        let len = virtio_net::encode_tx(&trame[..len], slot).map_err(|_| REFUSED)?;
        seq += 1;
        commande(handle, generation, seq, 2, len)?;
    }
    seq += 1;
    if commande(handle, generation, seq, 2, 1526) != Err(WOULD_BLOCK) {
        return Err(REFUSED);
    }
    let mut recus = 0u16;
    for _ in 0..COMPTE {
        seq += 1;
        let taille = commande(handle, generation, seq, 3, 0)?;
        let data =
            virtio_net::decode_rx(slot.get(..taille).ok_or(REFUSED)?).map_err(|_| REFUSED)?;
        if data.len() < 60
            || data[..6] != NIC
            || data[6..12] != PAIR
            || data[12..14] != [0x88, 0xb5]
        {
            return Err(REFUSED);
        }
        let n = if lots==1 {usize::from(data[14])} else {u32::from_be_bytes(data[14..18].try_into().map_err(|_|REFUSED)?) as usize};
        let index=n.checked_sub(lot * COMPTE).ok_or(REFUSED)?;
        if index >= COMPTE
            || recus & (1 << index) != 0
            || data.len() != if n % 2 == 0 { 60 } else { 1514 }
            || data[if lots>1 {18} else {15}..]
                .iter()
                .enumerate()
                .any(|(i, &b)| b != (octet(n, i+if lots>1 {3} else {0}) ^ 0xa5))
        {
            return Err(REFUSED);
        }
        recus |= 1 << index;
    }
    if recus != u16::MAX {
        return Err(REFUSED);
    }
    seq+=1;
    commande(handle, generation, seq, if lot+1==lots {4} else {6}, 0)?;
    }
    Ok(())
}
