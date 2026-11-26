//! Client de qualification sans accès au bail, aux files matérielles ou aux IRQ.
#![no_std]
#![no_main]
mod canal;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
#[path = "../../tests/ethernet_v1/wire.rs"]
mod wire;
use canal::*;
use runtime::*;
struct Client {
    c: Canal,
    epoch: u64,
    corr: u64,
    generations: [u32; 8],
}
fn paquet(n: u32, rx: bool, data: &mut [u8]) -> usize {
    let size = match cfg(11) {
        10 => 60,
        11 => 1514,
        _ if n % 2 == 0 => 60,
        _ => 1514,
    };
    let nic = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
    let pair = [2, 0x4e, 0x56, 0, 0, 1];
    data[..6].copy_from_slice(if rx { &nic } else { &pair });
    data[6..12].copy_from_slice(if rx { &pair } else { &nic });
    data[12..14].copy_from_slice(&[0x88, 0xb5]);
    data[14] = n as u8;
    for (i, b) in data[15..size].iter_mut().enumerate() {
        *b = (n.wrapping_mul(29).wrapping_add(i as u32 * 17) as u8) ^ if rx { 0xa5 } else { 0 };
    }
    if n >= 16 {
        data[14..18].copy_from_slice(&n.to_be_bytes());
    }
    size
}
impl Client {
    fn envoyer(&self, op: u16, id: u64, count: u16, body: &[u8]) -> Result<()> {
        let h = wire::Header {
            op,
            reply: false,
            count,
            epoch: self.epoch,
            request: id,
        };
        let mut pkt = [0; 128];
        let n = h.encode(&mut pkt, body).map_err(|_| BAD_ARGUMENT)?;
        if op == wire::TX && cfg(58) == 1 {
            let addr = 0x401a00 + 8 * u64::from(count - 1);
            let msg = unsafe { &mut *(addr as *mut SendV1) };
            *msg = SendV1 {
                version: VERSION,
                size: 160,
                timeout: 2500,
                tag: CONTROL,
                len: n as u32,
                flags: 1,
                payload: pkt,
            };
            require(value(call(IPC_SEND, self.c.tx, addr, 160))? == n as u64)
        } else {
            self.c.envoyer(CONTROL, &pkt[..n])
        }
    }
    fn recu(&self, op: u16, id: u64) -> Result<ReceiveV1> {
        let r = if op == wire::ACK {
            loop {
                let msg = unsafe { &mut *(0x401b00 as *mut ReceiveV1) };
                *msg = ReceiveV1 {
                    version: VERSION,
                    size: 184,
                    timeout: 2500,
                    options: 0,
                    reserved: 0,
                    kind: 0,
                    len: 0,
                    tag: 0,
                    handle: 0,
                    aux: 0,
                    payload: [0; 128],
                };
                value(call(IPC_RECEIVE, self.c.rx, 0x401b00, 184))?;
                if msg.kind == 2 {
                    continue;
                }
                require(msg.kind == 1 && msg.tag == CONTROL)?;
                break unsafe { core::ptr::read(msg) };
            }
        } else {
            self.c.recevoir(CONTROL)?
        };
        require(
            r.len >= 32
                && r.payload[4..6] == op.to_le_bytes()
                && r.payload[16..24] == self.epoch.to_le_bytes()
                && r.payload[24..32] == id.to_le_bytes(),
        )?;
        if r.payload[6] == 3 {
            require(r.len == 40)?;
            return Ok(r);
        }
        let (h, _) =
            wire::Header::decode(&r.payload[..r.len as usize]).map_err(|_| BAD_ARGUMENT)?;
        require(h.reply)?;
        Ok(r)
    }
    fn ctrl(&mut self, op: u16, count: u16, body: &[u8]) -> Result<ReceiveV1> {
        self.corr += 1;
        self.envoyer(op, self.corr, count, body)?;
        self.recu(op, self.corr)
    }
    fn tx(&mut self, id: u64, count: usize, first: u32, invalide: Option<usize>) -> Result<()> {
        let mut desc = [0; 96];
        let mut data = [0; FRAME];
        for i in 0..count {
            let n = paquet(first + i as u32, false, &mut data);
            desc[i * 12..i * 12 + 12].copy_from_slice(&wire::descriptor(
                i as u16,
                n as u16,
                self.generations[i],
            ));
        }
        self.envoyer(wire::TX, id, count as u16, &desc[..count * 12])?;
        for i in 0..count {
            let n = paquet(first + i as u32, false, &mut data);
            if let Some(offset) = invalide
                && i + 1 == count
            {
                data[offset] ^= 1;
            }
            self.c.donnees(true, i as u64 + 1, &mut data[..n])?;
        }
        Ok(())
    }
    fn vague_rx(&mut self, first: u32, relacher: bool) -> Result<()> {
        let list = self.ctrl(wire::RX, 0, &[])?;
        require(list.len == 128 && list.payload[6] == 1)?;
        let mut release = [0; 96];
        for (i, d) in list.payload[32..].chunks_exact(12).enumerate() {
            let len = wire::word(d, 2) as usize;
            let mut data = [0; FRAME];
            let mut expected = [0; FRAME];
            require(len == paquet(first + i as u32, true, &mut expected))?;
            self.c
                .donnees(false, u64::from(wire::word(d, 0)) + 1, &mut data[..len])?;
            require(data[..len] == expected[..len])?;
            release[12 * i..12 * i + 12].copy_from_slice(d);
            release[12 * i + 2..12 * i + 4].fill(0);
        }
        if relacher {
            if first != 0 {
                let mut anciens = release;
                for desc in anciens.chunks_exact_mut(12) {
                    let generation = u32::from_le_bytes(desc[4..8].try_into().unwrap());
                    require(generation > 1)?;
                    desc[4..8].copy_from_slice(&(generation - 1).to_le_bytes());
                }
                require(self.ctrl(wire::RELEASE, 8, &anciens)?.payload[6] == 3)?;
            }
            require(self.ctrl(wire::RELEASE, 8, &release)?.len == 32)?;
            require(self.ctrl(wire::RELEASE, 8, &release)?.len == 32)?;
        }
        Ok(())
    }
    fn veille(&mut self) -> Result<()> {
        let v = self.ctrl(wire::ARM, 0, &0u64.to_le_bytes())?;
        let seen = &v.payload[32..40];
        require(self.ctrl(wire::ARM, 0, seen)?.payload[40] == 0)?;
        loop {
            let msg = unsafe { &mut *(0x401c00 as *mut ReceiveV1) };
            *msg = ReceiveV1 {
                version: VERSION,
                size: 184,
                timeout: 2500,
                options: 0,
                reserved: 0,
                kind: 0,
                len: 0,
                tag: 0,
                handle: 0,
                aux: 0,
                payload: [0; 128],
            };
            value(call(IPC_RECEIVE, self.c.rx, 0x401c00, 184))?;
            require(msg.kind == 2)?;
            if msg.aux & 2 != 0 {
                break;
            }
        }
        self.vague_rx(0, false)?;
        self.vague_rx(0, true)?;
        self.vague_rx(8, true)?;
        require(self.ctrl(wire::CLOSE, 0, &[])?.len == 32)?;
        self.c.envoyer(EVENT, &self.corr.to_le_bytes())
    }
    fn exercer(&mut self) -> Result<()> {
        self.c
            .envoyer(EVENT, &pair(1, if self.c.boite { 3 } else { 1 }))?;
        let (_, t) = fields(&self.c.recevoir(EVENT)?)?;
        require(t == if self.c.boite { 2 } else { 1 })?;
        require(self.ctrl(wire::NEGOTIATE, 0, &[0; 16])?.len == 48)?;
        let mut rights = [0; 8];
        rights[0] = 15;
        require(self.ctrl(wire::OPEN, 0, &rights)?.payload[32] == 1)?;
        rights[0] = if cfg(11) == 7 { 6 } else { 7 };
        let open = self.ctrl(wire::OPEN, 0, &rights)?;
        require(
            open.payload[32..36] == [rights[0], 8, 1, 1]
                && open.payload[38..44] == [0x52, 0x54, 0, 0x12, 0x34, 0x56],
        )?;
        require(self.ctrl(wire::LINK, 0, &[])?.len == 48)?;
        let no = call(IPC_FEATURES, cfg(10), u64::from(SEND), 1);
        require(no.status == DENIED)?;
        for (key, right) in [(self.c.tx, SEND), (self.c.rx, RECEIVE)] {
            require(call(IPC_DUP, key, right.into(), 0).status == DENIED)?;
        }
        self.epoch ^= 1 << 40;
        let stale = self.ctrl(wire::LINK, 0, &[])?;
        require(stale.payload[6] == 3)?;
        self.epoch ^= 1 << 40;
        if cfg(11) == 7 {
            self.tx(1, 1, 0, None)?;
            require(self.recu(wire::TX, 1)?.payload[32] == 1)?;
            return self.veille();
        }
        for invalid in [0, 6, 13] {
            self.tx(1, 2, 0, Some(invalid))?;
            require(self.recu(wire::TX, 1)?.payload[32] == 1)?;
        }
        let mut id = 1;
        let mut seen = 0u32;
        let mut sizes = 0u16;
        let mut pressure = false;
        let mut split = false;
        let total = cfg(12) as u32;

        while seen < total {
            let b = if matches!(cfg(11), 3 | 4 | 6) {
                8
            } else if cfg(11) == 5 && seen >= 144 {
                8.min((total - seen) as usize)
            } else {
                1 + (id as usize - 1) % 8
            };
            let b = b.min((total - seen) as usize);
            mark(58, u64::from(!matches!(cfg(11), 3 | 4 | 6)));
            self.tx(id, b, seen, None)?;
            mark(58, 0);
            if id == 2 && matches!(cfg(11), 1 | 2) {
                require(receive_for(self.c.rx, 2).err() == Some(TIMED_OUT))?;
                self.tx(id, b, seen, None)?;
                require(self.recu(wire::TX, id)?.payload[6] == 1)?;
            } else {
                let r = self.recu(wire::TX, id)?;
                require(r.payload[6] == 1 && r.payload[34..36] == 0u16.to_le_bytes())?;
            }
            if matches!(cfg(11), 3 | 4 | 6) {
                let q = self.ctrl(wire::QUERY, 0, &id.to_le_bytes())?;
                let stages = &q.payload[32..q.len as usize];
                require(
                    stages.len() == 96
                        && stages.chunks_exact(12).take(4).all(|d| d[2] == 5)
                        && stages.chunks_exact(12).skip(4).all(|d| d[2] == 4),
                )?;
                require(self.ctrl(wire::ACK, 0, &id.to_le_bytes())?.payload[6] == 3)?;
                require(self.ctrl(wire::CLOSE, 0, &[])?.len == 32)?;
                self.c.envoyer(EVENT, &self.corr.to_le_bytes())?;
                return Ok(());
            }
            for g in &mut self.generations[..b] {
                *g += 1;
            }
            if b == 8 && !pressure {
                self.tx(id + 1, 1, seen + b as u32, None)?;
                let denied = self.recu(wire::TX, id + 1)?;
                require(denied.payload[6] == 3 && denied.payload[32] == 6)?;
                pressure = true;
            }
            let mut complete = false;
            for _ in 0..10000 {
                let q = self.ctrl(wire::QUERY, 0, &id.to_le_bytes())?;
                require(q.payload[6] == 1 && q.len as usize == 32 + 12 * b)?;
                let stages = &q.payload[32..q.len as usize];
                split |= stages.chunks_exact(12).any(|x| x[2] == 1)
                    && stages.chunks_exact(12).any(|x| x[2] == 2);
                if stages.chunks_exact(12).all(|x| x[2] == 3) {
                    complete = true;
                    break;
                }
            }
            require(complete)?;
            let mut received = 0;
            while received < b {
                let list = self.ctrl(wire::RX, 0, &[])?;
                require(list.payload[6] == 1)?;
                let entries = &list.payload[32..list.len as usize];
                if entries.is_empty() {
                    continue;
                }
                let mut release = [0; 96];
                for (i, d) in entries.chunks_exact(12).enumerate() {
                    let slot = wire::word(d, 0);
                    let len = wire::word(d, 2) as usize;
                    let mut data = [0; FRAME];
                    let mut expected = [0; FRAME];
                    require(received < b && len <= FRAME)?;
                    self.c.donnees(false, slot as u64 + 1, &mut data[..len])?;
                    let n = paquet(seen + received as u32, true, &mut expected);
                    require(n == len && expected[..n] == data[..n])?;
                    release[12 * i..12 * i + 12].copy_from_slice(d);
                    release[12 * i + 2..12 * i + 4].fill(0);
                    received += 1;
                }
                let count = (entries.len() / 12) as u16;
                require(
                    self.ctrl(wire::RELEASE, count, &release[..entries.len()])?
                        .len
                        == 32,
                )?;
            }
            require(self.ctrl(wire::ACK, 0, &id.to_le_bytes())?.len == 32)?;

            sizes |= 1 << (b - 1);
            seen += b as u32;
            id += 1;
            mark(65, seen as u64);
        }
        if cfg(11) == 8 {
            require(self.ctrl(wire::CLOSE, 0, &[])?.len == 32)?;
            return self.c.envoyer(EVENT, &self.corr.to_le_bytes());
        }
        require(pressure && sizes == 255 && (split || cfg(11) == 0))?;
        let stats = self.ctrl(wire::STATS, 0, &[])?;
        for i in [0, 1, 2, 3, 5] {
            require(
                u64::from_le_bytes(stats.payload[32 + i * 8..40 + i * 8].try_into().unwrap())
                    == u64::from(total),
            )?;
        }
        mark(66, sizes as u64);
        let rev = self.ctrl(wire::ARM, 0, &0u64.to_le_bytes())?;
        let now = u64::from_le_bytes(rev.payload[32..40].try_into().unwrap());
        require(self.ctrl(wire::ARM, 0, &now.to_le_bytes())?.payload[40] == 0)?;
        let fin = self.ctrl(wire::CLOSE, 0, &[])?;
        require(fin.len == 32)?;
        let evt = receive_for(self.c.rx, 250)?;
        require(evt.kind == 2 && evt.aux & 8 != 0)?;
        self.envoyer(wire::CLOSE, self.corr, 0, &[])?;
        require(self.recu(wire::CLOSE, self.corr)?.len == 32)?;
        self.c.envoyer(EVENT, &self.corr.to_le_bytes())?;
        Ok(())
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let res = Canal::new(cfg(1), cfg(2), cfg(1)).and_then(|c| {
        Client {
            c,
            epoch: cfg(6),
            corr: 100000,
            generations: [1; 8],
        }
        .exercer()
    });
    exit(res.err().unwrap_or(0))
}
