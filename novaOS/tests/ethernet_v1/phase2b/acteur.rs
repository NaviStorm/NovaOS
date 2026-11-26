use super::{DATA, abi::*, call, read, write};
#[allow(dead_code)]
#[path = "../wire.rs"]
mod wire;

type Result<T> = core::result::Result<T, u64>;
const CONTROL: usize = DATA + 1024;
const RECEIPT: usize = DATA + 1280;
const WAIT: u64 = 15000;

#[derive(Default)]
struct Counts {
    ipc: u64,
    pressure: u64,
    retries: u64,
    idle: u64,
}

fn ok(reply: Reply) -> Result<u64> {
    if reply.status == OK {
        Ok(reply.value)
    } else {
        Err(reply.status)
    }
}
fn check(test: bool) -> Result<()> {
    if test { Ok(()) } else { Err(BAD_ARGUMENT) }
}
fn sending(tag: u64, payload: &[u8]) -> SendV1 {
    let mut msg = SendV1 {
        version: 1,
        size: 160,
        timeout: 0,
        tag,
        len: payload.len() as u32,
        flags: 0,
        payload: [0; 128],
    };
    msg.payload[..payload.len()].copy_from_slice(payload);
    msg
}
fn receiving(timeout: u64) -> ReceiveV1 {
    ReceiveV1 {
        version: 1,
        size: 184,
        timeout,
        options: 0,
        reserved: 0,
        kind: 0,
        len: 0,
        tag: 0,
        handle: 0,
        aux: 0,
        payload: [0; 128],
    }
}
impl Counts {
    fn syscall(&mut self, id: u64, key: u64, addr: u64, size: u64) -> Result<u64> {
        self.ipc += 1;
        ok(call(id, key, addr, size))
    }
    fn send(&mut self, key: u64, msg: &mut SendV1) -> Result<()> {
        let ptr = msg as *mut SendV1 as u64;
        match self.syscall(IPC_SEND, key, ptr, 160) {
            Err(WOULD_BLOCK) => {
                self.pressure += 1;
                msg.flags = 1;
                msg.timeout = WAIT;
                let n = self.syscall(IPC_SEND, key, ptr, 160)?;
                check(n == u64::from(msg.len))
            }
            r => check(r? == u64::from(msg.len)),
        }
    }
    fn receive(&mut self, key: u64, msg: &mut ReceiveV1) -> Result<u64> {
        self.syscall(IPC_RECEIVE, key, msg as *mut ReceiveV1 as u64, 184)
    }
    fn page(&mut self, send: bool, key: u64, tag: u64, area: &mut [u8]) -> Result<()> {
        let desc = BulkV1 {
            version: 1,
            size: 32,
            tag,
            address: area.as_mut_ptr() as u64,
            length: area.len() as u64,
        };
        for _ in 0..16000 {
            let r = self.syscall(
                if send { IPC_PUT } else { IPC_TAKE },
                key,
                &desc as *const BulkV1 as u64,
                32,
            );
            match (send, r) {
                (true, Err(WOULD_BLOCK)) => self.pressure += 1,
                (false, Err(REFUSED)) => self.retries += 1,
                (_, r) => return check(r? == desc.length),
            }
            call(THREAD_YIELD, 0, 0, 0);
        }
        Err(TIMED_OUT)
    }
}
struct Session {
    mode: usize,
    profile: usize,
    batch: usize,
    len: usize,
    epoch: u64,
    input: u64,
    reply: u64,
    sync: u64,
    pages: [u64; 3],
}
impl Session {
    fn header(&self, seq: u64, reply: bool) -> wire::Header {
        wire::Header {
            op: wire::TX,
            reply,
            count: self.batch as u16,
            epoch: self.epoch,
            request: seq,
        }
    }
    fn control(&self, seq: u64, reply: bool) -> ([u8; 128], usize) {
        let mut body = [0; 96];
        for (i, d) in body.chunks_exact_mut(12).take(self.batch).enumerate() {
            if reply {
                d[..2].copy_from_slice(&(i as u16).to_le_bytes());
                d[4..].copy_from_slice(&seq.to_le_bytes());
            } else {
                d.copy_from_slice(&wire::descriptor(i as u16, self.len as u16, seq as u32));
            }
        }
        let mut data = [0; 128];
        let n = self
            .header(seq, reply)
            .encode(&mut data, &body[..12 * self.batch])
            .unwrap();
        (data, n)
    }
    fn payload(&self, c: &mut Counts, send: bool, seq: u64, bytes: &mut [u8]) -> Result<()> {
        if self.profile == 2 {
            if send {
                let mut ready = receiving(WAIT);
                check(
                    c.receive(self.reply, &mut ready)? == 1 && ready.tag == seq && ready.len == 0,
                )?;
            } else {
                c.send(self.reply, &mut sending(seq, &[]))?;
                let mut go = receiving(WAIT);
                check(c.receive(self.sync, &mut go)? == 2 && go.aux == 1)?;
            }
        }
        let mut staged = 0;
        if self.mode == 0 {
            for (i, frame) in bytes.chunks_exact_mut(self.len).enumerate() {
                for (part, area) in frame.chunks_mut(96).enumerate() {
                    let mut head = [0; 32];
                    head[..8].copy_from_slice(&seq.to_le_bytes());
                    head[8..10].copy_from_slice(&(i as u16).to_le_bytes());
                    head[10..12].copy_from_slice(&((part * 96) as u16).to_le_bytes());
                    head[12..14].copy_from_slice(&(self.len as u16).to_le_bytes());
                    if send {
                        let mut msg = sending(seq, &head);
                        msg.payload[32..32 + area.len()].copy_from_slice(area);
                        msg.len = (32 + area.len()) as u32;
                        c.send(self.input, &mut msg)?;
                        staged += 1;
                        if self.profile == 2 && staged == 8 {
                            check(
                                c.syscall(IPC_SEND, self.input, &msg as *const SendV1 as u64, 160)
                                    == Err(WOULD_BLOCK),
                            )?;
                            c.pressure += 1;
                            c.syscall(IPC_NOTIFY, self.sync, 1, 0)?;
                        }
                    } else {
                        let mut msg = receiving(WAIT);
                        check(
                            c.receive(self.input, &mut msg)? == 1
                                && msg.tag == seq
                                && msg.len as usize == 32 + area.len()
                                && msg.payload[..32] == head,
                        )?;
                        area.copy_from_slice(&msg.payload[32..32 + area.len()]);
                    }
                }
            }
        } else {
            for (i, page) in bytes.chunks_mut(4096).enumerate() {
                let key = self.pages[if self.mode == 2 { i } else { 0 }];
                c.page(send, key, seq * 4 + i as u64, page)?;
                let full = if self.mode == 1 {
                    i == 0
                } else {
                    i + 1 == (self.batch * self.len).div_ceil(4096)
                };
                if send && self.profile == 2 && full {
                    let d = BulkV1 {
                        version: 1,
                        size: 32,
                        tag: seq * 4 + i as u64,
                        address: page.as_ptr() as u64,
                        length: page.len() as u64,
                    };
                    check(
                        c.syscall(IPC_PUT, key, &d as *const BulkV1 as u64, 32) == Err(WOULD_BLOCK),
                    )?;
                    c.pressure += 1;
                    c.syscall(IPC_NOTIFY, self.sync, 1, 0)?;
                }
            }
        }
        Ok(())
    }
}
// L'adresse est dans la page DATA privée du processus et naturellement alignée.
fn ctrl_send() -> &'static mut SendV1 {
    unsafe { &mut *(CONTROL as *mut SendV1) }
}
fn ack_recv() -> &'static mut ReceiveV1 {
    unsafe { &mut *(RECEIPT as *mut ReceiveV1) }
}

fn seed(bytes: &mut [u8], len: usize, epoch: u64) {
    for (i, frame) in bytes.chunks_exact_mut(len).enumerate() {
        for (j, b) in frame.iter_mut().enumerate() {
            *b = (epoch as usize + i * 19 + j * 29) as u8;
        }
        frame[..14].copy_from_slice(&[2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x88, 0xb5]);
    }
}
fn sequence(bytes: &mut [u8], len: usize, seq: u64) {
    for f in bytes.chunks_exact_mut(len) {
        f[14..22].copy_from_slice(&seq.to_le_bytes());
    }
}
fn validate(msg: &ReceiveV1, s: &Session, seq: u64, reply: bool) -> Result<()> {
    let n = msg.len as usize;
    check(msg.kind == 1 && msg.tag == seq && n <= 128)?;
    let (h, _) = wire::Header::decode(&msg.payload[..n]).map_err(|_| BAD_ARGUMENT)?;
    let (expected, size) = s.control(seq, reply);
    check(h == s.header(seq, reply) && n == size && msg.payload[..n] == expected[..size])
}

fn query_bytes(s: &Session, seq: u64, reply: bool) -> Result<([u8; 128], usize)> {
    let correlation = seq | (1 << 63);
    let header = wire::Header {
        op: wire::QUERY,
        reply,
        count: if reply { s.batch as u16 } else { 0 },
        epoch: s.epoch,
        request: correlation,
    };
    let mut out = [0; 128];
    let n = if reply {
        let (receipt, n) = s.control(seq, true);
        header.encode(&mut out, &receipt[32..n])
    } else {
        header.encode(&mut out, &seq.to_le_bytes())
    }
    .map_err(|_| BAD_ARGUMENT)?;
    Ok((out, n))
}
fn matches_message(msg: &ReceiveV1, tag: u64, bytes: &[u8]) -> Result<()> {
    check(
        msg.kind == 1
            && msg.tag == tag
            && msg.len as usize == bytes.len()
            && msg.payload[..bytes.len()] == *bytes,
    )
}
fn answer_query(s: &Session, c: &mut Counts, msg: &ReceiveV1) -> Result<()> {
    let seq = read(24);
    check(seq != 0)?;
    let tag = seq | (1 << 63);
    let (req, n) = query_bytes(s, seq, false)?;
    matches_message(msg, tag, &req[..n])?;
    let (reply, n) = query_bytes(s, seq, true)?;
    c.send(s.reply, &mut sending(tag, &reply[..n]))
}
fn obtain_receipt(s: &Session, c: &mut Counts, seq: u64, timeout: u64) -> Result<()> {
    let ack = ack_recv();
    *ack = receiving(timeout);
    match c.receive(s.reply, ack) {
        Ok(1) => validate(ack, s, seq, true),
        Err(TIMED_OUT) => {
            let tag = seq | (1 << 63);
            let (req, n) = query_bytes(s, seq, false)?;
            c.send(s.input, &mut sending(tag, &req[..n]))?;
            let (expected, n) = query_bytes(s, seq, true)?;
            // Un reçu initial retardé peut précéder la réponse à QUERY.
            for _ in 0..2 {
                *ack = receiving(WAIT);
                check(c.receive(s.reply, ack)? == 1)?;
                if ack.tag == tag {
                    return matches_message(ack, tag, &expected[..n]);
                }
                validate(ack, s, seq, true)?;
            }
            Err(REFUSED)
        }
        Err(e) => Err(e),
        _ => Err(BAD_ARGUMENT),
    }
}
fn admit(seq: u64) -> Result<()> {
    check(read(24).checked_add(1) == Some(seq))?;
    write(24, seq);
    write(21, read(21) + 1);
    Ok(())
}

fn exchange(s: &Session, role: u64, rounds: u64) -> Result<u64> {
    let sender = role % 2 == 0;
    let background = role >= 2;
    let total = s.len * s.batch;
    let ptr = read(11) as *mut u8;
    // Le lanceur a réservé par HEAP_RESIZE le tampon et l’oracle privé du récepteur.
    let bytes = unsafe { core::slice::from_raw_parts_mut(ptr, total) };
    let expected = if sender {
        &mut [][..]
    } else {
        unsafe { core::slice::from_raw_parts_mut(ptr.add(total.next_multiple_of(4096)), total) }
    };
    if sender {
        seed(bytes, s.len, s.epoch);
    } else {
        seed(expected, s.len, s.epoch);
    }
    let mut c = Counts::default();
    let mut done = 0;
    let previous = read(24);
    loop {
        if !background && done == rounds {
            break;
        }
        if background && sender && read(13) != 0 {
            ok(call(IPC_NOTIFY, s.input, 1, 0))?;
            break;
        }
        let seq = previous + done + 1;
        if sender {
            if s.profile == 1 {
                let mut empty = receiving(1);
                check(c.receive(s.reply, &mut empty) == Err(TIMED_OUT))?;
                c.idle += 1;
            }
            sequence(bytes, s.len, seq);
            let (data, n) = s.control(seq, false);
            let msg = ctrl_send();
            *msg = sending(seq, &data[..n]);
            c.send(s.input, msg)?;
            s.payload(&mut c, true, seq, bytes)?;
            obtain_receipt(s, &mut c, seq, WAIT)?;
            write(24, seq);
        } else {
            let mut cmd = receiving(WAIT);
            let kind = c.receive(s.input, &mut cmd)?;
            if background && kind == 2 && cmd.aux == 1 {
                break;
            }
            if cmd.tag & (1 << 63) != 0 {
                answer_query(s, &mut c, &cmd)?;
                continue;
            }
            validate(&cmd, s, seq, false)?;
            s.payload(&mut c, false, seq, bytes)?;
            sequence(expected, s.len, seq);
            check(bytes == expected)?;
            admit(seq)?;
            let (data, n) = s.control(seq, true);
            c.send(s.reply, &mut sending(seq, &data[..n]))?;
        }
        done += 1;
        if background {
            write(14, done);
        }
    }
    for (i, value) in [c.ipc, c.pressure, c.retries, c.idle, done]
        .into_iter()
        .enumerate()
    {
        write(16 + i, value);
    }
    Ok(done)
}

// Essais à des points de protocole observables par les deux acteurs. Les deux
// portions restantes ne sont jamais publiées si le lot s'arrête après la première.
fn fault(s: &Session, role: u64) -> Result<u64> {
    let sender = role == 0;
    let mut c = Counts::default();
    let size = s.batch * s.len;
    let ptr = read(11) as *mut u8;
    let bytes = unsafe { core::slice::from_raw_parts_mut(ptr, size) };
    let mut cmd = receiving(WAIT);
    let (control, n) = s.control(1, false);
    if sender {
        seed(bytes, s.len, s.epoch);
        sequence(bytes, s.len, 1);
        c.send(s.input, &mut sending(1, &control[..n]))?;
    } else {
        check(c.receive(s.input, &mut cmd)? == 1)?;
        validate(&cmd, s, 1, false)?;
    }
    if s.profile == 7 {
        s.payload(&mut c, sender, 1, bytes)?;
        if sender {
            obtain_receipt(s, &mut c, 1, 2)?;
        } else {
            let expected = unsafe {
                core::slice::from_raw_parts_mut(ptr.add(size.next_multiple_of(4096)), size)
            };
            seed(expected, s.len, s.epoch);
            sequence(expected, s.len, 1);
            check(bytes == expected)?;
            admit(1)?;
            let mut retry = receiving(WAIT);
            check(c.receive(s.input, &mut retry)? == 1)?;
            answer_query(s, &mut c, &retry)?;
        }
        return Ok(1);
    }
    c.page(sender, s.pages[0], 4, &mut bytes[..4096])?;
    if sender {
        let mut checkpoint = receiving(WAIT);
        check(
            c.receive(s.reply, &mut checkpoint)? == 1
                && checkpoint.tag == 4
                && checkpoint.len == 1
                && checkpoint.payload[0] == 1,
        )?;
        if s.profile == 5 {
            call(PROCESS_EXIT, 23, 0, 0);
            unreachable!();
        }
        if s.profile == 6 {
            // Le pair peut encore se trouver entre l'envoi de son témoin et EXIT.
            loop {
                if ok(call(IPC_FEATURES, s.pages[1], SEND as u64, 1))? == 0 {
                    break;
                }
                call(THREAD_YIELD, 0, 0, 0);
            }
        } else {
            for key in s.pages {
                ok(call(
                    if s.profile == 4 {
                        IPC_CLOSE
                    } else {
                        IPC_REVOKE
                    },
                    key,
                    0,
                    0,
                ))?;
            }
        }
        for i in 0..3 {
            check(c.page(true, s.pages[i], 10 + i as u64, &mut bytes[..60]) == Err(DENIED))?;
        }
    } else {
        let expected =
            unsafe { core::slice::from_raw_parts_mut(ptr.add(size.next_multiple_of(4096)), size) };
        seed(expected, s.len, s.epoch);
        sequence(expected, s.len, 1);
        check(bytes[..4096] == expected[..4096] && bytes[4096..].iter().all(|&v| v == 0))?;
        c.send(s.reply, &mut sending(4, &[1]))?;
        if s.profile == 6 {
            call(PROCESS_EXIT, 24, 0, 0);
            unreachable!();
        }
        check(c.page(false, s.pages[1], 5, &mut bytes[4096..8192]) == Err(DENIED))?;
        check(bytes[4096..].iter().all(|&v| v == 0))?;
        bytes.fill(0);
        core::hint::black_box(&*bytes);
        check(bytes.iter().all(|&v| v == 0))?;
    }
    Ok(0)
}
pub(super) fn run(role: u64, rounds: u64, config: u64) -> Reply {
    let s = Session {
        mode: (config & 3) as usize,
        profile: ((config >> 2) & 15) as usize,
        batch: ((config >> 8) & 15) as usize,
        len: (config >> 16) as usize,
        epoch: read(12),
        input: read(6),
        reply: read(7),
        sync: read(22),
        pages: [read(8), read(9), read(10)],
    };
    let r = if s.profile >= 4 {
        fault(&s, role)
    } else {
        exchange(&s, role, rounds)
    };
    match r {
        Ok(n) => Reply {
            status: OK,
            value: n,
        },
        Err(status) => Reply { status, value: 0 },
    }
}
