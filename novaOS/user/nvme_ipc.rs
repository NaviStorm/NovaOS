//! Transport privé à une seule requête en vol sur des canaux directionnels.
//! Les images de qualification n'allouent ni tas ni page DMA en Ring 3.
use super::{nvme_protocol::*, runtime as rt};
use rt::{BAD_ARGUMENT, REFUSED, Result};

pub fn check(ok: bool) -> Result<()> {
    if ok { Ok(()) } else { Err(REFUSED) }
}

pub fn debug(bytes: &[u8]) -> Result<()> {
    rt::value(rt::call(
        rt::DEBUG_WRITE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    ))
    .map(|_| ())
}

pub const WAIT: u64 = 3000;
// Contrat client ↔ bloc et bloc ↔ pilote. Les tickets ne sortent pas du second lien.
pub const INFO: u64 = 1;
pub const READ: u64 = 2;
pub const STOP: u64 = 3;
pub const INVALID_TICKET: u64 = 4;
pub const WRITE_REFUSED: u64 = 5;
pub const FLUSH_REFUSED: u64 = 6;

fn header(first: u64, second: u64, args: [u64; 3]) -> [u8; REQUEST_SIZE] {
    let mut bytes = [0; REQUEST_SIZE];
    put(&mut bytes, 0, first);
    put(&mut bytes, 1, second);
    put(&mut bytes, 2, args[0]);
    put(&mut bytes, 3, args[1]);
    put(&mut bytes, 4, args[2]);
    bytes
}

pub fn send_words(cap: u64, seq: u64, op: u64, args: [u64; 3]) -> Result<()> {
    rt::send_for(cap, seq, &header(rt::config(EPOCH), op, args), WAIT)
}
pub fn next_request(cap: u64, seq: u64) -> Result<(u64, [u64; 3])> {
    let msg = rt::receive_for(cap, WAIT)?;
    check(
        msg.kind == 1
            && msg.len as usize == REQUEST_SIZE
            && msg.tag == seq
            && word(&msg.payload, 0) == rt::config(EPOCH),
    )?;
    Ok((
        word(&msg.payload, 1),
        [
            word(&msg.payload, 2),
            word(&msg.payload, 3),
            word(&msg.payload, 4),
        ],
    ))
}

// Les réponses des pairs portent l’époque et le statut, suivis de trois mots
// dont la signification dépend de la requête, puis d'une tranche éventuelle.
pub fn send_reply(cap: u64, seq: u64, status: u64, args: [u64; 3], data: &[u8]) -> Result<()> {
    if data.len() > CHUNK {
        return Err(BAD_ARGUMENT);
    }
    let mut frame = [0; RESPONSE_SIZE];
    frame[..REQUEST_SIZE].copy_from_slice(&header(rt::config(EPOCH), status, args));
    frame[REQUEST_SIZE..REQUEST_SIZE + data.len()].copy_from_slice(data);
    rt::send_for(cap, seq, &frame[..REQUEST_SIZE + data.len()], WAIT)
}
pub fn reply_message(cap: u64, seq: u64) -> Result<rt::ReceiveV1> {
    let m = rt::receive_for(cap, WAIT)?;
    check(
        m.kind == 1
            && m.tag == seq
            && (REQUEST_SIZE..=REQUEST_SIZE + CHUNK).contains(&(m.len as usize))
            && word(&m.payload, 0) == rt::config(EPOCH),
    )?;
    let status = word(&m.payload, 1);
    if status != rt::OK {
        check(m.len as usize == REQUEST_SIZE)?;
    }
    Ok(m)
}

pub fn reply(cap: u64, seq: u64) -> Result<rt::ReceiveV1> {
    let m = reply_message(cap, seq)?;
    let status = word(&m.payload, 1);
    if status != rt::OK { Err(status) } else { Ok(m) }
}

pub struct Broker {
    seq: u64,
    #[cfg(nvme_rw)]
    media_failure: Option<(u16, u16)>,
}
impl Broker {
    pub fn new() -> Self {
        Self {
            seq: 0,
            #[cfg(nvme_rw)]
            media_failure: None,
        }
    }

    pub fn error_words(&self) -> [u64; 3] {
        #[cfg(nvme_rw)]
        if let Some((cid, status)) = self.media_failure {
            return [0, u64::from(cid), u64::from(status)];
        }
        [0; 3]
    }

    pub fn call_as(&mut self, epoch: u64, op: u64, args: [u64; 3]) -> Result<[u8; RESPONSE_SIZE]> {
        self.call_on(rt::config(BROKER_SEND), epoch, op, args)
    }

    pub fn call_on(
        &mut self,
        cap: u64,
        epoch: u64,
        op: u64,
        args: [u64; 3],
    ) -> Result<[u8; RESPONSE_SIZE]> {
        #[cfg(nvme_rw)]
        {
            self.media_failure = None;
        }
        self.seq = self.seq.checked_add(1).ok_or(rt::LIMIT)?;
        let mut m = rt::SendV1 {
            version: rt::VERSION,
            size: 160,
            tag: op,
            len: REQUEST_SIZE as u32,
            flags: 0,
            timeout: 0,
            payload: [0; 128],
        };
        m.payload[..REQUEST_SIZE].copy_from_slice(&header(epoch, self.seq, args));
        rt::value(rt::call(rt::IPC_SEND, cap, (&raw const m) as u64, 160))?;
        let notice = rt::receive_for(rt::config(BROKER_REPLY), WAIT)?;
        check(notice.kind == 2 && notice.aux == self.seq)?;
        // Sûreté : la page DATA privée existe pendant toute la vie du processus.
        // Le courtier écrit le résultat, puis publie sa notification. Il attend
        // une nouvelle requête avant de pouvoir réutiliser cet emplacement.
        let out = unsafe { (RESPONSE_VA as *const [u8; RESPONSE_SIZE]).read_volatile() };
        check(word(&out, 0) == rt::config(EPOCH) && word(&out, 1) == self.seq)?;
        match word(&out, 2) {
            rt::OK => Ok(out),
            #[cfg(nvme_rw)]
            MEDIA_IO if op == POLL => {
                let cid = u16::try_from(word(&out, 4)).map_err(|_| rt::BAD_ARGUMENT)?;
                let status = u16::try_from(word(&out, 5)).map_err(|_| rt::BAD_ARGUMENT)?;
                if status == 0 {
                    return Err(rt::BAD_ARGUMENT);
                }
                self.media_failure = Some((cid, status));
                Err(MEDIA_IO)
            }
            error => Err(error),
        }
    }
    pub fn call(&mut self, op: u64, args: [u64; 3]) -> Result<[u8; RESPONSE_SIZE]> {
        self.call_as(rt::config(EPOCH), op, args)
    }
    pub fn wait(&mut self, op: u64) -> Result<[u8; RESPONSE_SIZE]> {
        // Chaque échange attend sur l’endpoint de réponse. L’échéance matérielle
        // monotone est contrôlée par le courtier même si le pilote se bloque.
        for _ in 0..3000 {
            match self.call(op, [0; 3]) {
                Err(rt::WOULD_BLOCK) => {
                    rt::value(rt::call(rt::THREAD_YIELD, 0, 0, 0))?;
                }
                outcome => return outcome,
            }
        }
        Err(rt::TIMED_OUT)
    }
}
