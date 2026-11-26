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
    let origine=config(8) as usize;
    let lots = config(7) as usize;
    let profil=config(9);
    let mut echos=[[0u8;8];COMPTE];
    if profil>4 || !(1..=62500).contains(&lots) {return Err(REFUSED);}
    for lot in 0..lots {
    let milieu=lot!=0 && lot+1!=lots;
    let rx_seul=milieu && profil==2;
    let tx_seul=milieu && profil==1;
    if !rx_seul {
    for i in 0..COMPTE {
        let n = origine + lot * COMPTE + i;
        let len = if profil==4 {64} else if profil>0 {1514} else if n%2==0 {60} else {1514};
        trame[..6].copy_from_slice(&PAIR);
        trame[6..12].copy_from_slice(&NIC);
        trame[12..14].copy_from_slice(&[0x88, 0xb5]);
        trame[14] = n as u8;
        for (i, b) in trame[15..len].iter_mut().enumerate() {
            *b = octet(n, i);
        }
        if lots > 1 {trame[14..18].copy_from_slice(&(n as u32).to_be_bytes());}
        if profil==4 {trame[18..26].copy_from_slice(&echos[i]);}
        let len = virtio_net::encode_tx(&trame[..len], slot).map_err(|_| REFUSED)?;
        seq += 1;
        commande(handle, generation, seq, 2, len)?;
    }
    seq += 1;
    if commande(handle, generation, seq, 2, 1526) != Err(WOULD_BLOCK) {
        return Err(REFUSED);
    }
    }
    let mut recus = 0u16;
    for _ in 0..COMPTE {
        seq += 1;
        let taille = commande(handle,generation,seq,if tx_seul {7} else if rx_seul {8} else {3},0)?;
        if tx_seul {if taille!=0 {return Err(REFUSED);} continue;}
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
        let index=n.checked_sub(origine + lot * COMPTE).ok_or(REFUSED)?;
        let prefixe=if profil==4 {26} else if lots>1 {18} else {15};
        if index >= COMPTE
            || recus & (1 << index) != 0
            || data.len() != if profil==4 {64} else if profil>0 {1514} else if n%2==0 {60} else {1514}
            || data[prefixe..]
                .iter()
                .enumerate()
                .any(|(i, &b)| b != (octet(n, i+prefixe-15) ^ 0xa5))
        {
            return Err(REFUSED);
        }
        if profil==4 {echos[index].copy_from_slice(&data[18..26]);}
        recus |= 1 << index;
    }
    if !tx_seul && recus != u16::MAX {
        return Err(REFUSED);
    }
    seq+=1;
    commande(handle, generation, seq, if lot+1==lots {4} else {6}, 0)?;
    #[cfg(net_q35_fault_test)]
    if lot==0 && lots>1 && config(10)==1 {return Err(CANCELLED);}
    // Aucun appel d’E/S n’est en vol durant ce rendez-vous privé Q35.
    #[cfg(net_q35_fault_test)]
    if lot==0 && config(10)==2 {
        let limit=config(11).checked_mul(3).filter(|&v|v>0).ok_or(REFUSED)?;
        // Sur le profil Q35 de qualification, RDTSC est accessible en Ring 3.
        let start=unsafe {core::arch::x86_64::_rdtsc()};
        while config(12)!=generation {
            if unsafe {core::arch::x86_64::_rdtsc()}.wrapping_sub(start)>=limit {return Err(TIMED_OUT);}
            call(THREAD_YIELD,0,0,0);
        }
    }
    }
    Ok(())
}
