//! Profil du service NovaFS lorsqu'un support est explicitement attribué.
//! Le diagnostic de persistance porte sur des secteurs réservés de l'image de test.
use crate::runtime::*;
#[allow(dead_code)]
#[path = "../../abi/block.rs"]
mod w;
struct Block {
    tx: u64,
    rx: u64,
    tag: u64,
}
impl Block {
    #[inline(never)]
    fn exchange(&mut self, buf: &mut w::Wire) -> Result<()> {
        self.tag = self.tag.checked_add(1).ok_or(LIMIT)?;
        send(self.tx, self.tag, buf)?;
        let r = receive(self.rx)?;
        if r.tag != self.tag || r.kind != 1 {
            return Err(BAD_ARGUMENT);
        }
        if r.len == 1 {
            return Err(r.payload[0] as u64);
        }
        if r.len != 128 || buf[..3] != r.payload[..3] {
            return Err(BAD_ARGUMENT);
        }
        *buf = r.payload;
        Ok(())
    }
    fn command(&mut self, op: u8) -> Result<w::Wire> {
        let mut b = w::blank(op);
        self.exchange(&mut b)?;
        Ok(b)
    }
    #[inline(never)]
    fn sector(&mut self, size: usize, lba: u64, write: bool) -> Result<()> {
        let op = if write { w::WRITE } else { w::READ };
        let base = |phase| {
            let mut f = w::blank(op);
            f[2] = phase;
            f[8..16].copy_from_slice(&lba.to_le_bytes());
            f
        };
        self.exchange(&mut base(w::BEGIN))?;
        if !write {
            self.exchange(&mut base(w::END))?
        }
        for at in (0..size).step_by(w::MAX) {
            let n = w::MAX.min(size - at);
            let mut f = base(w::PIECE);
            w::put(&mut f, 4, n as u32);
            w::put(&mut f, 16, at as u32);
            if write {
                for i in 0..n {
                    f[32 + i] = pattern(lba, at + i)
                }
            }
            self.exchange(&mut f)?;
            if !write {
                for i in 0..n {
                    if f[32 + i] != pattern(lba, at + i) {
                        return Err(REFUSED);
                    }
                }
            }
        }
        if write {
            self.exchange(&mut base(w::END))?
        }
        Ok(())
    }
}
fn pattern(block: u64, at: usize) -> u8 {
    (block as u8)
        .wrapping_mul(29)
        .wrapping_add((at as u8).wrapping_mul(37))
        ^ ((at >> 5) as u8)
        ^ 0xb7
}
fn log(s: &[u8]) {
    let _ = call(DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}
fn exercise(d: &mut Block, mode: u64) -> Result<()> {
    let info = d.command(w::INFO)?;
    let size = w::number(&info, 4) as usize;
    let count = w::lba(&info);
    if !matches!(size, 512 | 4096) || count == 0 {
        return Err(BAD_ARGUMENT);
    }
    if mode == 3 {
        log(b"NVFS INFO support accessible");
        return Ok(());
    }
    let base = 8192 / size as u64;
    if base >= count || (mode == 6 && base + 63 >= count) {
        return Err(BAD_ARGUMENT);
    }
    match mode {
        4 | 8 => {
            d.sector(size, base, true)?;
            log(b"NVFS WRITE accepte");
            d.command(w::FLUSH)?;
            log(b"NVFS FLUSH confirme");
        }
        5 => {
            d.sector(size, base, false)?;
            log(b"NVFS READ octets compares");
        }
        6 => {
            for random in [false, true] {
                for write in [true, false] {
                    for i in 0..32u64 {
                        let n = if random { (i * 13 + 7) % 32 } else { i };
                        d.sector(size, base + 32 + n, write)?;
                    }
                    if write {
                        d.command(w::FLUSH)?;
                    }
                    log(match (random, write) {
                        (false, true) => b"NVFS seq write",
                        (false, false) => b"NVFS seq read",
                        (true, true) => b"NVFS rand write",
                        (true, false) => b"NVFS rand read",
                    });
                }
            }
        }
        _ => return Err(BAD_ARGUMENT),
    }
    Ok(())
}
pub fn serve() -> Result<()> {
    let (tx, rx) = channels()?;
    let mut d = Block { tx, rx, tag: 0 };
    let (front, back) = (config(4), config(5));
    let mode = config(1);
    for (id, op) in [(1, INFO), (2, FIN)] {
        let r = receive_for(front, 5000)?;
        if r.kind != 1
            || r.tag != id
            || r.len != 8
            || r.payload[..8] != [WIRE_VERSION, op, 0, 0, 0, 0, 0, 0]
        {
            return Err(BAD_ARGUMENT);
        }
        let status = if op == INFO {
            exercise(&mut d, mode)
        } else {
            d.command(w::CLOSE).map(|_| ())
        };
        let mut result = [0u8; 12];
        result[0] = 1;
        result[4] = 1;
        result[8..].copy_from_slice(IDENTITY);
        if let Err(e) = status {
            result[1] = e as u8
        }
        send(back, id, &result)?;
        status?;
    }
    Ok(())
}
