//! Port privé du signataire sur l'export de 514 pages. Le relais traite un fragment par appel.
use crate::{device::IpcDevice, runtime as rt};
#[path = "cellule.rs"]
mod cellule;
use cellule::*;

const IPC_TAG: u64 = u64::from_le_bytes(*b"NATUN001");
const CLOSE_TAG: u64 = u64::from_le_bytes(*b"NACLOSE1");
const PART: usize = 96;

#[derive(Default)]
pub(crate) struct Port {
    link: [u8; 16],
    tx: u64,
    rx: u64,
    offset: usize,
    broken: bool,
    closed_clean: bool,
}

impl Port {
    fn transfer(&self, dev: &IpcDevice<'_>, page: &mut [u8], write: bool) -> rt::Result<()> {
        dev.authority_cell(page, write)
    }
    fn send_cell(
        &mut self,
        dev: &IpcDevice<'_>,
        page: &mut [u8],
        kind: u8,
        bytes: &[u8],
    ) -> rt::Result<()> {
        self.tx = self.tx.checked_add(1).ok_or(rt::LIMIT)?;
        page.fill(0);
        Cell {
            kind,
            size: bytes.len().try_into().map_err(|_| rt::LIMIT)?,
            link: self.link,
            sequence: self.tx,
            ack: self.rx,
        }
        .encode(&mut page[..HEADER])
        .map_err(|_| rt::REFUSED)?;
        page[HEADER..HEADER + bytes.len()].copy_from_slice(bytes);
        self.transfer(dev, page, true)
    }
    fn inspect(&self, dev: &IpcDevice<'_>, page: &mut [u8]) -> rt::Result<Cell> {
        self.transfer(dev, page, false)?;
        let c = decode_sector(page).map_err(|_| rt::REFUSED)?;
        if c.link != self.link || !matches!(c.kind, DATA | EMPTY) || c.ack > self.tx {
            return Err(rt::CLOSED);
        }
        Ok(c)
    }
    fn close(&mut self, dev: &IpcDevice<'_>, sector: usize) -> rt::Result<()> {
        self.broken = true;
        if self.closed_clean {
            return Ok(());
        }
        if !matches!(sector, 512 | 4096) {
            return Err(rt::REFUSED);
        }
        if self.tx != 0 {
            let mut buffer = [0; PAGE];
            let page = &mut buffer[..sector];
            self.send_cell(dev, page, CLOSE, &[])?;
            self.transfer(dev, page, false)?;
            let closed = decode_sector(page).map_err(|_| rt::REFUSED)?;
            if closed.kind != BROKEN || closed.link != [0; 16] || closed.size != 0 {
                return Err(rt::REFUSED);
            }
        }
        *self = Self {
            broken: true,
            closed_clean: true,
            ..Self::default()
        };
        Ok(())
    }
    fn request(
        &mut self,
        dev: &IpcDevice<'_>,
        sector: usize,
        q: &[u8; 128],
        a: &mut [u8; 128],
    ) -> rt::Result<()> {
        if self.broken || !matches!(sector, 512 | 4096) {
            return Err(rt::CLOSED);
        }
        let mut block = [0; PAGE];
        let page = &mut block[..sector];
        let size = usize::from(q[1]);
        if q[2..32].iter().any(|&v| v != 0) || size > PART || q[32 + size..].iter().any(|&v| v != 0)
        {
            return Err(rt::BAD_ARGUMENT);
        }
        if self.tx == 0 {
            if q[0] != 1 || size != 16 || q[32..48] == [0; 16] {
                return Err(rt::DENIED);
            }
            self.link.copy_from_slice(&q[32..48]);
            return self.send_cell(dev, page, OPEN, &[]);
        }
        let c = self.inspect(dev, page)?;
        if c.ack != self.tx {
            return Err(rt::WOULD_BLOCK);
        }
        match q[0] {
            2 if size != 0 => self.send_cell(dev, page, DATA, &q[32..32 + size]),
            3 if size == 0 => {
                if c.kind == EMPTY {
                    return Err(rt::WOULD_BLOCK);
                }
                if self.rx.checked_add(1) != Some(c.sequence) || self.offset >= usize::from(c.size)
                {
                    return Err(rt::REFUSED);
                }
                let n = PART.min(usize::from(c.size) - self.offset);
                a[8] = n as u8;
                a[32..32 + n]
                    .copy_from_slice(&page[HEADER + self.offset..HEADER + self.offset + n]);
                self.offset += n;
                if self.offset == usize::from(c.size) {
                    self.rx = c.sequence;
                    self.offset = 0;
                    self.send_cell(dev, page, ACK, &[])?;
                }
                Ok(())
            }
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
    pub(crate) fn poll(&mut self, dev: &IpcDevice<'_>, sector: usize, port: u64) -> rt::Result<()> {
        if port == 0 {
            return Ok(());
        }
        let msg = match rt::receive_for(port, 0) {
            Ok(m) => m,
            Err(rt::WOULD_BLOCK) => return Ok(()),
            Err(rt::REVOKED | rt::CLOSED | rt::DENIED) => {
                let _ = self.close(dev, sector);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let mut a = [0; 128];
        let status =
            if msg.tag == CLOSE_TAG && msg.kind == 1 && msg.len == 128 && msg.payload == [0; 128] {
                self.close(dev, sector).err().unwrap_or(rt::OK)
            } else if msg.tag != IPC_TAG || msg.kind != 1 || msg.len != 128 {
                rt::BAD_ARGUMENT
            } else {
                self.request(dev, sector, &msg.payload, &mut a)
                    .err()
                    .unwrap_or(rt::OK)
            };
        if !matches!(status, rt::OK | rt::WOULD_BLOCK) {
            let _ = self.close(dev, sector);
            a.fill(0);
        }
        a[..8].copy_from_slice(&status.to_le_bytes());
        if rt::send_for(port, msg.tag, &a, 25).is_err() {
            let _ = self.close(dev, sector);
        }
        Ok(())
    }
}
const _: () = assert!(size_of::<Port>() <= 256);
