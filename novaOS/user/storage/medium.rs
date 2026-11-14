//! Profil du service NovaFS lorsqu'un support est explicitement attribué.
//! Le diagnostic de persistance porte sur des secteurs réservés de l'image de test.
use crate::runtime::*;
#[allow(dead_code)]
#[path = "../../abi/block.rs"]
mod w;
use crate::runtime::bulk;
pub(crate) struct Block {
    tx: u64,
    rx: u64,
    tag: u64,
    pub(crate) features: u32,
    pub(crate) sector_size: usize,
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
            let status = u64::from(r.payload[0]);
            if buf[1] == w::DISCOVER && status == BAD_ARGUMENT {
                // Les anciens relais et pilotes rejettent une opération inconnue par cet octet.
                return Err(UNKNOWN_CALL);
            }
            return Err(if status == OK { BAD_ARGUMENT } else { status });
        }
        if r.len != 128 || buf[..3] != r.payload[..3] {
            return Err(BAD_ARGUMENT);
        }
        *buf = r.payload;
        Ok(())
    }
    fn negotiate(&mut self) -> Result<()> {
        let lane = config(10);
        if lane == 0 || bulk::features(lane, SEND)? & IPC_FEATURE_PAGE == 0
            || bulk::features(lane, RECEIVE)? & IPC_FEATURE_PAGE == 0
        {
            return Ok(());
        }
        let wanted = w::IMPLEMENTED;
        let mut frame = w::discovery(wanted, false);
        match self.exchange(&mut frame) {
            Ok(()) => self.features = w::offered(&frame, wanted).ok_or(REFUSED)?,
            Err(UNKNOWN_CALL) => (),
            Err(e) => return Err(e),
        }
        Ok(())
    }
    fn compat_page(&mut self, first: u64, data: &mut [u8; 4096], write: bool) -> Result<()> {
        if !matches!(self.sector_size, 512 | 4096) {
            return Err(BAD_ARGUMENT);
        }
        for (i, sector) in data.chunks_exact_mut(self.sector_size).enumerate() {
            self.transfer(sector, first.checked_add(i as u64).ok_or(BAD_ARGUMENT)?, write)?;
        }
        Ok(())
    }
    fn page_frame(op: u8, lba: u64) -> w::Wire {
        let mut frame = w::blank(op);
        frame[2] = w::PAGE_PHASE;
        w::put(&mut frame, 4, 4096);
        frame[8..16].copy_from_slice(&lba.to_le_bytes());
        frame
    }
    fn exchange_page(&mut self, op: u8, lba: u64) -> Result<()> {
        let expected = Self::page_frame(op, lba);
        let mut frame = expected;
        self.exchange(&mut frame)?;
        if frame != expected {
            return Err(BAD_ARGUMENT);
        }
        Ok(())
    }
    pub(crate) fn read_page(&mut self, lba: u64, cap: u64, out: &mut [u8; 4096]) -> Result<()> {
        if self.features & w::FEATURE_PAGE == 0 {
            return self.compat_page(lba, out, false);
        }
        self.exchange_page(w::READ, lba)?;
        bulk::take(cap, self.tag, out)
    }
    pub(crate) fn write_page(&mut self, lba: u64, cap: u64, data: &[u8; 4096]) -> Result<()> {
        if self.features & w::FEATURE_PAGE == 0 {
            return self.compat_page(lba, &mut *zeroize::Zeroizing::new(*data), true);
        }
        let tag = self.tag.checked_add(1).ok_or(LIMIT)?;
        bulk::put(cap, tag, data)?;
        self.exchange_page(w::WRITE, lba)
    }
    fn command(&mut self, op: u8) -> Result<w::Wire> {
        let mut b = w::blank(op);
        self.exchange(&mut b)?;
        Ok(b)
    }
    #[inline(never)]
    pub(crate) fn transfer(&mut self, bytes: &mut [u8], lba: u64, write: bool) -> Result<()> {
        let size = bytes.len();
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
                f[32..32 + n].copy_from_slice(&bytes[at..at + n]);
            }
            self.exchange(&mut f)?;
            if !write {
                if w::lba(&f) != lba
                    || w::number(&f, 4) != n as u32
                    || w::number(&f, 16) != at as u32
                {
                    return Err(BAD_FRAME as u64);
                }
                bytes[at..at + n].copy_from_slice(&f[32..32 + n]);
            }
        }
        if write {
            self.exchange(&mut base(w::END))?
        }
        Ok(())
    }
    pub(crate) fn close(&mut self) -> Result<()> {
        self.command(w::CLOSE).map(|_| ())
    }
    pub(crate) fn flush(&mut self) -> Result<()> {
        self.command(w::FLUSH).map(|_| ())
    }
    fn sector(&mut self, size: usize, lba: u64, write: bool) -> Result<()> {
        let mut page = [0; 4096];
        let sector = &mut page[..size];
        if write {
            for (i, b) in sector.iter_mut().enumerate() {
                *b = pattern(lba, i)
            }
        }
        self.transfer(sector, lba, write)?;
        if !write
            && sector
                .iter()
                .enumerate()
                .any(|(i, &b)| b != pattern(lba, i))
        {
            return Err(REFUSED);
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
    #[cfg(feature = "installation")]
    if matches!(mode, 43 | 44) {
        let rights = if mode == 43 { 15 } else { 9 };
        if w::number(&info, 16) != rights {
            return Err(DENIED);
        }
        let device =
            crate::device::IpcDevice::new(d, size as u32, count).map_err(crate::device::status)?;
        if mode == 44 {
            return crate::files::classer(&device);
        }
        return crate::files::initialiser(&device);
    }
    if matches!(mode, 9 | 10) {
        use novafs::Device;
        let mut device =
            &crate::device::IpcDevice::new(d, size as u32, count).map_err(crate::device::status)?;
        let logical = 2;
        if device.blocks() <= logical {
            return Err(BAD_ARGUMENT);
        }
        let expected = core::array::from_fn(|i| pattern(logical, i));
        if mode == 9 {
            device
                .write(logical, &expected)
                .map_err(crate::device::status)?;
            device.flush().map_err(crate::device::status)?;
            log(b"NVFS adaptateur FLUSH confirme");
        } else if device.read(logical).map_err(crate::device::status)? != expected {
            return Err(REFUSED);
        } else {
            log(b"NVFS adaptateur READ 4096 confirme");
        }
        return Ok(());
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
    let mut d = Block { tx, rx, tag: 0, features: 0, sector_size: 0 };
    if matches!(config(1), 41 | 42) {
        d.negotiate()?;
    }
    if (11..=42).contains(&config(1)) {
        let info = d.command(w::INFO)?;
        let writable = w::number(&info, 16) & 6 == 6;
        if !writable && !matches!(config(1), 12 | 13 | 23 | 26 | 30 | 37 | 38) {
            return Err(DENIED);
        }
        let dev = crate::device::IpcDevice::new(&mut d, w::number(&info, 4), w::lba(&info))
            .map_err(crate::device::status)?;
        return crate::files::serve(&dev, w::number(&info, 4), writable);
    }
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
