#![no_std]
#![no_main]
mod runtime;
#[allow(dead_code)]
#[path = "../../abi/block.rs"]
mod wire;
use runtime::*;
struct Peer {
    tx: u64,
    rx: u64,
    id: u64,
}
impl Peer {
    #[inline(never)]
    fn exchange(&mut self, w: &wire::Wire) -> Result<wire::Wire> {
        self.id += 1;
        send(self.tx, self.id, w)?;
        let r = receive(self.rx)?;
        if r.tag != self.id || r.kind != 1 {
            return Err(BAD_ARGUMENT);
        }
        if r.len == 1 {
            return Err(r.payload[0] as u64);
        }
        if r.len != 128 || r.payload[..3] != w[..3] {
            return Err(BAD_ARGUMENT);
        }
        Ok(r.payload)
    }
}
fn request() -> Result<()> {
    let (tx, rx) = channels()?;
    let mut peer = Peer { tx, rx, id: 0 };
    // Même la connaissance de l'identifiant de test ne désigne pas un droit dans ce processus.
    if call(wire::PORTAL, config(2), 0, 128).status != DENIED {
        return Err(BAD_ARGUMENT);
    }
    let info = peer.exchange(&wire::blank(wire::INFO))?;
    let size = wire::number(&info, 4) as usize;
    if !matches!(size, 512 | 4096) || wire::lba(&info) <= 8192 / size as u64 {
        return Err(BAD_ARGUMENT);
    }
    let lba = 8192 / size as u64;
    let mut read = wire::blank(wire::READ);
    read[8..16].copy_from_slice(&lba.to_le_bytes());
    let mut invalid = read;
    invalid[3] = 0xff;
    if peer.exchange(&invalid).err() != Some(BAD_ARGUMENT) {
        return Err(BAD_ARGUMENT);
    }
    invalid = read;
    invalid[8..16].fill(255);
    if peer.exchange(&invalid).err() != Some(BAD_ARGUMENT) {
        return Err(BAD_ARGUMENT);
    }
    peer.exchange(&read)?;
    read[2] = wire::END;
    peer.exchange(&read)?;
    read[2] = wire::PIECE;
    for at in (0..size).step_by(wire::MAX) {
        let n = wire::MAX.min(size - at);
        wire::put(&mut read, 4, n as u32);
        wire::put(&mut read, 16, at as u32);
        let r = peer.exchange(&read)?;
        for j in 0..n {
            if r[32 + j] != 0x71 + ((at + j) % 83) as u8 {
                return Err(REFUSED);
            }
        }
    }
    peer.exchange(&wire::blank(wire::CLOSE))?;
    Ok(())
}
// Sûreté : point d'entrée privé du client NovaFS de qualification ; aucun format de disque interprété.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(request().err().unwrap_or(0))
}
